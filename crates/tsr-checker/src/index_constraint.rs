//! TS2411 — `Property '{0}' of type '{1}' is not assignable to '{2}' index type
//! '{3}'.`
//!
//! `checkIndexConstraints` (`checker.go`), run from `checkClassDeclaration` and
//! `checkInterfaceDeclaration`: every **named** property of a type carrying an
//! index signature must be assignable to that signature's type.
//!
//! Error node the property's own name — `classIndexer3.ts(9,5)` is the `y` of
//! `y: string;` in a class extending one that declares `[s: string]: number`.
//!
//! # Why the index signature is found by walking rather than by asking the type
//!
//! The constraining signature can be **inherited**: `classIndexer3`'s is on the
//! base and the offending property is on the derived class. This port has no
//! resolved index-signature table on a type, so the signature is found the same
//! way `crate::members` finds a property — own declarations first, then the
//! `extends` chain, with `base_symbols_of`'s "any base I cannot follow makes the
//! whole answer a miss" contract doing the safety work.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolId;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, relater::Relation, relater::Ternary, types::TypeId};

/// Which index signature a property is constrained by.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IndexKind {
    /// `[k: string]: T` — constrains **every** named property.
    String,
    /// `[k: number]: T` — constrains only numeric-named ones.
    Number,
}

impl IndexKind {
    fn printed(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
        }
    }

    /// Does this signature constrain a property called `name`?
    ///
    /// A numeric index signature applies only to a property whose name *is* a
    /// number (`isNumericLiteralName`), which is what keeps `y: string` legal
    /// beside `[n: number]: number`.
    fn constrains(self, name: &str) -> bool {
        match self {
            Self::String => true,
            Self::Number => is_numeric_literal_name(name),
        }
    }
}

impl Checker<'_, '_> {
    /// `get_type_from_type_node` reached from a [`NodeId`].
    ///
    /// ADR-0013's read-drop-recurse: the typed node is `Copy`, so the immutable
    /// borrow of `node_map` ends with the statement and the `&mut self` call is
    /// free. Collecting `TypeNode`s into a `Vec` instead would hold that borrow
    /// across the whole comparison loop.
    fn type_from_type_node_id(&mut self, node: NodeId) -> Option<TypeId> {
        let typed = tsr_ast::TypeNode::try_from(self.node_map.get(node)?).ok()?;
        Some(self.get_type_from_type_node(typed))
    }

    /// A member's **written** name, for the three spellings a declared member
    /// can carry: an identifier, a string literal, or a numeric literal.
    ///
    /// §28 collected identifiers alone, which dropped exactly the population a
    /// *numeric* index signature constrains — `public "1": string` beside
    /// `[i: number]: number` (`checker-notes-diag2.md` §62). A computed name is
    /// still `None`: its text is not the property's key.
    fn written_member_name(&self, name: NodeId) -> Option<String> {
        match self.node_map.get(name)? {
            Node::Identifier(written) => Some(written.text.to_string()),
            Node::StringLiteral(written) => Some(written.text.to_string()),
            Node::NumericLiteral(written) => Some(written.text.to_string()),
            _ => None,
        }
    }

    /// The index-constraint check for one class or interface declaration.
    pub(crate) fn check_index_constraints(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let members: Vec<(String, NodeId, NodeId)> = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => {
                if !class.type_parameters.is_empty() {
                    return;
                }
                class
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        tsr_ast::ClassElement::PropertyDeclaration(property) => {
                            Some((property.name, property.r#type?.node_id()?))
                        }
                        _ => None,
                    })
                    .filter_map(|(name, annotation)| {
                        let id = name.node_id()?;
                        Some((self.written_member_name(id)?, id, annotation))
                    })
                    .collect()
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                if !interface.type_parameters.is_empty() {
                    return;
                }
                interface
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        tsr_ast::TypeElement::PropertySignatureDeclaration(property) => {
                            Some((property.name, property.r#type?.node_id()?))
                        }
                        _ => None,
                    })
                    .filter_map(|(name, annotation)| {
                        let id = name.node_id()?;
                        Some((self.written_member_name(id)?, id, annotation))
                    })
                    .collect()
            }
            _ => return,
        };
        if members.is_empty() {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let mut visiting = Vec::new();
        let Some(signatures) = self.index_signatures_of(symbol, &mut visiting, 0) else { return };
        if signatures.is_empty() {
            return;
        }

        for (name, at, annotation) in members {
            let Some(property) = self.type_from_type_node_id(annotation) else { continue };
            for (kind, index_annotation) in signatures.clone() {
                if !kind.constrains(&name) {
                    continue;
                }
                let Some(index) = self.type_from_type_node_id(index_annotation) else { continue };
                if !self.pair_is_reportable(property, index) {
                    continue;
                }
                if self.relate_ternary(property, index, Relation::Assignable) != Ternary::NotRelated
                {
                    continue;
                }
                let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
                let span = self.error_span(at);
                let property_text = self.type_to_string(property);
                let index_text = self.type_to_string(index);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::PROPERTY_0_OF_TYPE_1_IS_NOT_ASSIGNABLE_TO_2_INDEX_TYPE_3,
                        span,
                        [name.clone(), property_text, kind.printed().to_string(), index_text],
                    ),
                );
                break;
            }
        }
    }

    /// The index signatures a symbol has, own and inherited — `None` where the
    /// walk cannot be completed.
    fn index_signatures_of(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
        depth: u32,
    ) -> Option<Vec<(IndexKind, NodeId)>> {
        if depth > 16 || visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let declarations = self.binder.symbols().get(owner).declarations.to_vec();
        let mut found = Vec::new();
        for declaration in declarations {
            let signatures: Vec<&tsr_ast::IndexSignatureDeclaration<'_>> =
                match self.node_map.get(declaration) {
                    Some(Node::ClassDeclaration(class)) => class
                        .members
                        .iter()
                        .filter_map(|member| match member {
                            tsr_ast::ClassElement::IndexSignatureDeclaration(index) => Some(*index),
                            _ => None,
                        })
                        .collect(),
                    Some(Node::InterfaceDeclaration(interface)) => interface
                        .members
                        .iter()
                        .filter_map(|member| match member {
                            tsr_ast::TypeElement::IndexSignatureDeclaration(index) => Some(*index),
                            _ => None,
                        })
                        .collect(),
                    // A merged namespace, an enum, a variable: the member list
                    // is assembled somewhere this walk does not read.
                    _ => return None,
                };
            for signature in signatures {
                let parameter = signature.parameters.first()?;
                let kind = match parameter.r#type.and_then(|t| t.node_id()) {
                    Some(id) => match self.node_map.get(id) {
                        Some(Node::KeywordTypeNode(keyword))
                            if keyword.kind == SyntaxKind::StringKeyword =>
                        {
                            IndexKind::String
                        }
                        Some(Node::KeywordTypeNode(keyword))
                            if keyword.kind == SyntaxKind::NumberKeyword =>
                        {
                            IndexKind::Number
                        }
                        // A template-literal or union index parameter is
                        // upstream's `getIndexInfosOfType` and is not ported.
                        _ => return None,
                    },
                    None => return None,
                };
                found.push((kind, signature.r#type?.node_id()?));
            }
        }
        for base in self.base_symbols_of(owner)? {
            found.extend(self.index_signatures_of(base, visiting, depth + 1)?);
        }
        Some(found)
    }
}

/// `isNumericLiteralName` (`utilities.go:898`) — *"we test whether
/// `ToString(ToNumber(name))` is exactly equal to `name`"*.
///
/// The round-trip is the point, and it is why `"0xF00D"` is **not** a numeric
/// name: indexing with `0xF00D` indexes with `"61453"`. `Infinity`,
/// `-Infinity` and `NaN` are accepted deliberately — upstream's comment says so
/// — because indexing with them really does index with those strings.
///
/// `parse::<f64>()` is not this predicate: it accepts `"+1"`, `"inf"` and
/// `"nan"`, none of which round-trips. Every character outside `[0-9.eE+-]` is
/// rejected up front, which covers the hex, octal and whitespace spellings the
/// corpus tests without needing JS's full `ToNumber`.
fn is_numeric_literal_name(name: &str) -> bool {
    if matches!(name, "Infinity" | "-Infinity" | "NaN") {
        return true;
    }
    if name.is_empty()
        || !name.bytes().all(|byte| byte.is_ascii_digit() || b".eE+-".contains(&byte))
    {
        return false;
    }
    name.parse::<f64>().is_ok_and(|value| js_number_to_string(value) == name)
}

/// `Number#toString` for a finite value — decimal in JS's `[1e-6, 1e21)` band
/// and exponential outside it, with the `+` JS writes on a non-negative
/// exponent.
fn js_number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    if !(1e-6..1e21).contains(&magnitude) {
        let exponential = format!("{value:e}");
        return match exponential.split_once('e') {
            Some((mantissa, exponent)) if !exponent.starts_with('-') => {
                format!("{mantissa}e+{exponent}")
            }
            _ => exponential,
        };
    }
    format!("{value}")
}
