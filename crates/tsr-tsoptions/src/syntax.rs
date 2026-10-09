//! The property syntax of a `tsconfig.json`, for diagnostics positioned on it.
//!
//! Ported from the syntax walkers `Program.verifyCompilerOptions`
//! (`internal/compiler/program.go:751`) positions its option diagnostics with:
//! `ForEachTsConfigPropArray` and `getTsConfigObjectLiteralExpression`
//! (`tsoptions/tsconfigparsing.go:1505`, `:1576`), `ForEachPropertyAssignment`
//! (`:1560`) and `CreateDiagnosticForNodeInSourceFile` (`tsoptions/errors.go:92`).
//!
//! Upstream walks the config's retained `SourceFile`. This port's
//! [`crate::ParsedCommandLine`] does not retain one, so the caller hands the
//! config text back and it is parsed once into this owned view: the root
//! object's property assignments, each with its name, the error ranges of its
//! name and initializer, and the nested object properties or array elements a
//! diagnostic can point into. Nothing here reads a value; the options come from
//! the parsed command line.

use tsr_ast::{Expression, Node, NodeTable, ObjectLiteralElementLike, PropertyName, Statement};
use tsr_core::Span;
use tsr_parser::{ParseOptions, ScriptKind, parse_with_options};

/// One `name: initializer` property assignment of a config object.
#[derive(Debug, Clone)]
pub struct PropertySyntax {
    /// `TryGetTextOfPropertyName(property.Name())`; `None` when the name has no
    /// text (a computed name), so no key matches it.
    pub name: Option<String>,
    /// The error range of `property.Name()`.
    pub name_span: Span,
    /// The initializer, or `None` when the assignment has none.
    pub initializer: Option<InitializerSyntax>,
}

/// A property initializer: its error range, and the children a diagnostic can
/// be positioned on.
#[derive(Debug, Clone)]
pub struct InitializerSyntax {
    /// The error range of the initializer expression.
    pub span: Span,
    /// What the expression is.
    pub kind: InitializerKind,
}

/// The shapes of [`InitializerSyntax`] upstream's walkers distinguish.
#[derive(Debug, Clone)]
pub enum InitializerKind {
    /// `ast.IsObjectLiteralExpression`: its property assignments, in order.
    Object(Vec<PropertySyntax>),
    /// `ast.IsArrayLiteralExpression`: the error range of each element.
    Array(Vec<Span>),
    /// Any other expression.
    Other,
}

impl InitializerSyntax {
    /// The object literal's properties, if this is one.
    #[must_use]
    pub fn as_object(&self) -> Option<&[PropertySyntax]> {
        match &self.kind {
            InitializerKind::Object(properties) => Some(properties),
            _ => None,
        }
    }
}

/// A config file's root object (`getTsConfigObjectLiteralExpression`).
#[derive(Debug, Clone, Default)]
pub struct ConfigSyntax {
    root: Option<Vec<PropertySyntax>>,
}

impl ConfigSyntax {
    /// Parse `text` as a JSON config and keep its root object's syntax.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let arena = tsr_core::Arena::new();
        let parsed = parse_with_options(
            &arena,
            text,
            ParseOptions { script_kind: ScriptKind::Json, ..ParseOptions::default() },
        );
        let root = match parsed.source_file.statements.first() {
            Some(Statement::ExpressionStatement(statement)) => match statement.expression {
                Some(Expression::ObjectLiteralExpression(object)) => {
                    Some(properties(object, &parsed.nodes, text))
                }
                _ => None,
            },
            _ => None,
        };
        Self { root }
    }

    /// The root object literal's properties; `None` when the root is not an
    /// object literal.
    #[must_use]
    pub fn root(&self) -> Option<&[PropertySyntax]> {
        self.root.as_deref()
    }

    /// `ForEachTsConfigPropArray(sourceFile, key, core.Identity)`: the first root
    /// property named `key`.
    #[must_use]
    pub fn root_property(&self, key: &str) -> Option<&PropertySyntax> {
        for_each_property_assignment(self.root(), key, None)
    }
}

/// `ForEachPropertyAssignment(objectLiteral, key, identity, key2)`: the first
/// property assignment whose name is `key` or `key2`.
#[must_use]
pub fn for_each_property_assignment<'s>(
    object: Option<&'s [PropertySyntax]>,
    key: &str,
    key2: Option<&str>,
) -> Option<&'s PropertySyntax> {
    object?.iter().find(|property| {
        property.name.as_deref().is_some_and(|name| name == key || key2 == Some(name))
    })
}

fn properties(
    object: &tsr_ast::ObjectLiteralExpression<'_>,
    nodes: &NodeTable,
    text: &str,
) -> Vec<PropertySyntax> {
    object
        .properties
        .iter()
        .filter_map(|element| {
            let ObjectLiteralElementLike::PropertyAssignment(property) = element else {
                return None;
            };
            Some(PropertySyntax {
                name: text_of_property_name(property.name),
                name_span: error_range(property.name.node_id(), nodes, text),
                initializer: property
                    .initializer
                    .map(|initializer| initializer_syntax(initializer, nodes, text)),
            })
        })
        .collect()
}

fn initializer_syntax(
    expression: Expression<'_>,
    nodes: &NodeTable,
    text: &str,
) -> InitializerSyntax {
    let node = Node::from(expression);
    let kind = match node {
        Node::ObjectLiteralExpression(object) => {
            InitializerKind::Object(properties(object, nodes, text))
        }
        Node::ArrayLiteralExpression(array) => InitializerKind::Array(
            array
                .elements
                .iter()
                .map(|element| error_range(Node::from(*element).node_id(), nodes, text))
                .collect(),
        ),
        _ => InitializerKind::Other,
    };
    InitializerSyntax { span: error_range(node.node_id(), nodes, text), kind }
}

/// `ast.TryGetTextOfPropertyName` for the names a JSON object can carry.
fn text_of_property_name(name: PropertyName<'_>) -> Option<String> {
    Some(
        match name {
            PropertyName::Identifier(n) => n.text,
            PropertyName::PrivateIdentifier(n) => n.text,
            PropertyName::StringLiteral(n) => n.text,
            PropertyName::NumericLiteral(n) => n.text,
            PropertyName::BigIntLiteral(n) => n.text,
            PropertyName::NoSubstitutionTemplateLiteral(n) => n.text,
            PropertyName::ComputedPropertyName(_) => return None,
        }
        .to_string(),
    )
}

/// `CreateDiagnosticForNodeInSourceFile`'s range: `SkipTrivia(text, node.Pos())`
/// to `node.End()`.
fn error_range(id: Option<tsr_ast::NodeId>, nodes: &NodeTable, text: &str) -> Span {
    let Some(id) = id else { return Span::default() };
    let span = nodes.span(id);
    Span::new(skip_trivia(text, span.start).min(span.end), span.end)
}

/// `scanner.SkipTrivia` over JSON text: whitespace and comments.
fn skip_trivia(text: &str, start: u32) -> u32 {
    let bytes = text.as_bytes();
    let mut pos = start as usize;
    while pos < bytes.len() {
        match bytes[pos] {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => pos += 1,
            b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                pos += 2;
                pos += text[pos..]
                    .find(['\r', '\n', '\u{2028}', '\u{2029}'])
                    .unwrap_or(text.len() - pos);
            }
            b'/' if bytes.get(pos + 1) == Some(&b'*') => {
                pos += 2;
                pos = text[pos..].find("*/").map_or(text.len(), |end| pos + end + 2);
            }
            byte if !byte.is_ascii() => {
                let Some(ch) = text[pos..].chars().next() else { break };
                if !is_white_space_like(ch) {
                    break;
                }
                pos += ch.len_utf8();
            }
            _ => break,
        }
    }
    u32::try_from(pos).unwrap_or(u32::MAX)
}

/// `stringutil.IsWhiteSpaceLike` for a non-ASCII character.
fn is_white_space_like(ch: char) -> bool {
    matches!(
        ch,
        '\u{0085}' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200B}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_ranges_skip_trivia_and_find_the_first_matching_key() {
        let text = "{\n  // c\n  \"compilerOptions\": { /* x */ \"outDir\": \"a\", \"outDir\": \"b\",\n    \"paths\": { \"*\": [\"x\", \"y\"] } }\n}";
        let syntax = ConfigSyntax::parse(text);
        let options = syntax.root_property("compilerOptions").unwrap();
        assert_eq!(
            &text[options.name_span.start as usize..options.name_span.end as usize],
            "\"compilerOptions\""
        );
        let object = options.initializer.as_ref().unwrap().as_object();
        let out_dir = for_each_property_assignment(object, "outDir", None).unwrap();
        let value = out_dir.initializer.as_ref().unwrap().span;
        assert_eq!(&text[value.start as usize..value.end as usize], "\"a\"");
        let paths = for_each_property_assignment(object, "missing", Some("paths")).unwrap();
        let star = for_each_property_assignment(
            paths.initializer.as_ref().unwrap().as_object(),
            "*",
            None,
        )
        .unwrap();
        let InitializerKind::Array(elements) = &star.initializer.as_ref().unwrap().kind else {
            panic!("array")
        };
        assert_eq!(&text[elements[1].start as usize..elements[1].end as usize], "\"y\"");
    }
}
