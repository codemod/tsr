//! The JSON tree a config file denotes, and how it is read out of the AST.
//!
//! Ported from `internal/tsoptions/tsconfigparsing.go` (`convertToObject`,
//! `convertToJson`, `convertObjectLiteralExpressionToJson`,
//! `convertPropertyValueToJson`) at the pinned commit.
//!
//! # Why a value tree exists at all, next to the AST
//!
//! Upstream keeps both: the *raw* config as `OrderedMap[string, any]` and the
//! typed `CompilerOptions` it was converted into. That is not duplication —
//! `files`, `include`, `exclude`, `extends` and `references` are read from the
//! raw map and are not compiler options, and the raw map is what
//! `tsc --showConfig` prints back.
//!
//! Order is preserved throughout because it is observable: `include` order
//! decides which of two files differing only in extension enters the program.
//!
//! # Why every function here takes the node table
//!
//! `true`, `false` and `null` are all `KeywordExpression` nodes that differ only
//! in the kind recorded in [`NodeTable`] — the tree has no back-edges and no
//! per-node kind field ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)).
//! So the table is an input to reading a value, not an optional extra.

use tsr_ast::{
    Expression, Node, NodeTable, ObjectLiteralElementLike, PropertyName, SourceFile, Statement,
    SyntaxKind,
};
use tsr_core::{OrderedMap, Span};

/// A value in a config file.
///
/// Upstream's `any`, made explicit. `Null` is distinct from absent: `"extends":
/// null` is an error rather than a default.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    /// `true` or `false`.
    Bool(bool),
    /// A number. JSON has one numeric type and so does this.
    Number(f64),
    /// A string.
    String(String),
    /// An array, in order.
    List(Vec<ConfigValue>),
    /// An object, in declaration order.
    Map(OrderedMap<ConfigValue>),
    /// An explicit `null`, or a value that is not valid JSON syntax.
    Null,
}

impl ConfigValue {
    /// The string, if this is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// The list, if this is one.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(values) => Some(values),
            _ => None,
        }
    }

    /// The map, if this is one.
    #[must_use]
    pub fn as_map(&self) -> Option<&OrderedMap<Self>> {
        match self {
            Self::Map(entries) => Some(entries),
            _ => None,
        }
    }

    /// Whether a list option would silently drop this entry.
    ///
    /// Upstream's `filteredValues` filter: `"types": ["", "node"]` is
    /// `["node"]`, which is why an empty string never becomes an empty path
    /// mapping.
    #[must_use]
    pub fn is_falsy(&self) -> bool {
        match self {
            Self::Bool(value) => !value,
            Self::String(value) => value.is_empty(),
            Self::Number(value) => *value == 0.0,
            Self::Null => true,
            Self::List(_) | Self::Map(_) => false,
        }
    }
}

/// One property of a config object.
pub struct ConfigProperty<'a> {
    /// The property name.
    pub name: String,
    /// Its value.
    pub value: ConfigValue,
    /// The value expression, so an option diagnostic can point at the offending
    /// value rather than at the file, and so a list option can walk elements.
    pub value_expression: Option<Expression<'a>>,
    /// The span of the value, or of the property when it has none.
    pub span: Span,
}

/// The top-level properties of a parsed config file, in order
/// (`convertToObject` over the root expression).
///
/// `None` when the root is not an object literal. Upstream reports a diagnostic
/// and carries on with an empty config; the diagnostic is not reproduced here
/// because nothing consumes config diagnostics yet, and inventing one would be
/// worse than the gap.
#[must_use]
pub fn root_properties<'a>(
    source_file: &'a SourceFile<'a>,
    nodes: &NodeTable,
) -> Option<Vec<ConfigProperty<'a>>> {
    let Some(Statement::ExpressionStatement(statement)) = source_file.statements.first() else {
        return None;
    };
    let Some(Expression::ObjectLiteralExpression(object)) = statement.expression else {
        return None;
    };
    Some(properties_of(object, nodes))
}

/// The properties of an object literal, in order
/// (`convertObjectLiteralExpressionToJson`).
#[must_use]
pub fn properties_of<'a>(
    object: &'a tsr_ast::ObjectLiteralExpression<'a>,
    nodes: &NodeTable,
) -> Vec<ConfigProperty<'a>> {
    object
        .properties
        .iter()
        .filter_map(|element| {
            // Only `name: value` means anything in a config. A shorthand, a
            // spread, or a method is a syntax error upstream reports and skips.
            let ObjectLiteralElementLike::PropertyAssignment(property) = element else {
                return None;
            };
            let name = property_name(property.name)?;
            let value_expression = property.initializer;
            let value = value_expression.map_or(ConfigValue::Null, |value| value_of(value, nodes));
            let span = value_expression
                .and_then(|value| span_of(Node::from(value), nodes))
                .or_else(|| property.node_id.map(|id| nodes.span(id)))
                .unwrap_or_default();
            Some(ConfigProperty { name, value, value_expression, span })
        })
        .collect()
}

/// One value expression as a [`ConfigValue`] (`convertPropertyValueToJson`).
///
/// Anything that is not valid JSON syntax becomes [`ConfigValue::Null`], which
/// is how upstream's `nil` return propagates: the option is then treated as
/// absent rather than as wrong.
#[must_use]
pub fn value_of(expression: Expression<'_>, nodes: &NodeTable) -> ConfigValue {
    match Node::from(expression) {
        Node::StringLiteral(literal) => ConfigValue::String(literal.text.to_string()),
        Node::NumericLiteral(literal) => {
            literal.text.parse().map_or(ConfigValue::Null, ConfigValue::Number)
        }
        Node::KeywordExpression(keyword) => {
            match keyword.node_id.map(|id| nodes.kind(id)) {
                Some(SyntaxKind::TrueKeyword) => ConfigValue::Bool(true),
                Some(SyntaxKind::FalseKeyword) => ConfigValue::Bool(false),
                // `null`, and anything else a keyword expression could be.
                _ => ConfigValue::Null,
            }
        }
        // `-1`. Any other prefix operator is not JSON.
        Node::PrefixUnaryExpression(unary) => match (unary.operator.kind, unary.operand) {
            (SyntaxKind::MinusToken, Some(Expression::NumericLiteral(literal))) => literal
                .text
                .parse::<f64>()
                .map_or(ConfigValue::Null, |value| ConfigValue::Number(-value)),
            _ => ConfigValue::Null,
        },
        Node::ObjectLiteralExpression(object) => ConfigValue::Map(
            properties_of(object, nodes)
                .into_iter()
                .map(|property| (property.name, property.value))
                .collect(),
        ),
        Node::ArrayLiteralExpression(array) => ConfigValue::List(
            array.elements.iter().map(|element| value_of(*element, nodes)).collect(),
        ),
        _ => ConfigValue::Null,
    }
}

/// A property key's text. Only a string or an identifier can be one.
fn property_name(name: PropertyName<'_>) -> Option<String> {
    match name {
        PropertyName::StringLiteral(literal) => Some(literal.text.to_string()),
        PropertyName::Identifier(identifier) => Some(identifier.text.to_string()),
        _ => None,
    }
}

pub(crate) fn span_of(node: Node<'_>, nodes: &NodeTable) -> Option<Span> {
    node.node_id().map(|id| nodes.span(id))
}

#[cfg(test)]
mod tests {
    use tsr_core::Arena;
    use tsr_parser::{ParseOptions, ScriptKind, parse_with_options};

    use super::*;

    fn read(source: &str) -> Vec<(String, ConfigValue)> {
        let arena = Arena::new();
        let parsed = parse_with_options(
            &arena,
            source,
            ParseOptions { script_kind: ScriptKind::Json, ..Default::default() },
        );
        root_properties(parsed.source_file, &parsed.nodes)
            .expect("the root is an object")
            .into_iter()
            .map(|property| (property.name, property.value))
            .collect()
    }

    #[test]
    fn every_json_shape_reads_back() {
        let read = read(
            r#"{
                "s": "text", "n": 1.5, "neg": -2, "t": true, "f": false, "nil": null,
                "list": [1, "a"], "map": { "inner": true }
            }"#,
        );
        assert_eq!(read[0], ("s".into(), ConfigValue::String("text".into())));
        assert_eq!(read[1], ("n".into(), ConfigValue::Number(1.5)));
        assert_eq!(read[2], ("neg".into(), ConfigValue::Number(-2.0)));
        assert_eq!(read[3], ("t".into(), ConfigValue::Bool(true)));
        assert_eq!(read[4], ("f".into(), ConfigValue::Bool(false)));
        assert_eq!(read[5], ("nil".into(), ConfigValue::Null));
        assert_eq!(
            read[6].1,
            ConfigValue::List(vec![ConfigValue::Number(1.0), ConfigValue::String("a".into())])
        );
        let ConfigValue::Map(inner) = &read[7].1 else { panic!("expected a map") };
        assert_eq!(inner.get("inner"), Some(&ConfigValue::Bool(true)));
    }

    #[test]
    fn property_order_is_preserved() {
        // Not cosmetic: `include` order decides which of two files differing
        // only in extension enters the program.
        let names: Vec<String> =
            read(r#"{ "z": 1, "a": 2, "m": 3 }"#).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["z", "a", "m"]);
    }

    #[test]
    fn an_unquoted_key_is_still_a_key() {
        // Not valid JSON; valid `tsconfig.json`, and the corpus contains it.
        assert_eq!(read(r"{ compilerOptions: {} }")[0].0, "compilerOptions");
    }
}
