//! Reports made while `createJsxAttributesTypeFromAttributesProperty`
//! (`checker/jsx.go:709`) builds an element's attributes type.
//!
//! Upstream reports these from inside the type function; this port reports
//! from the per-node walk on the `JsxAttributes` node, which visits each
//! element once — the split `docs/parity/notes/jsx.md` §2 records for
//! `checkJsxExpression`.

use tsr_ast::{JsxAttributeLike, JsxAttributeName, Node, NodeId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// TS2783 — `'{0}' is specified more than once, so this usage will be
    /// overwritten.`
    ///
    /// `createJsxAttributesTypeFromAttributesProperty`'s `allAttributesTable`
    /// (`jsx.go:710-713`, `:758`, `:782`): under `strictNullChecks` every
    /// attribute enters the table by name, a later one of the same name
    /// replacing the earlier, and each spread whose reduced type
    /// `isValidSpreadType` runs `checkSpreadPropOverrides`
    /// (`checker.go:13371`) against the table as filled so far — no
    /// `tryMergeUnionOfObjectTypeAndEmptyObject`, unlike the object-literal
    /// arm in `crate::spread_overrides`.
    ///
    /// The report lands on the attribute (`left.ValueDeclaration`). Two
    /// spreads overwriting one attribute produce one line: upstream's two
    /// diagnostics differ only in related information, which the baseline
    /// folds into one (`jsxSpreadOverwritesAttributeStrict`, line 24).
    pub(crate) fn check_jsx_spread_property_overrides(&mut self, node: NodeId) {
        if !self.strict_null_checks {
            return;
        }
        let Some(Node::JsxAttributes(attributes)) = self.node_map.get(node) else { return };
        let mut table: Vec<(String, NodeId)> = Vec::new();
        let mut reports: Vec<(NodeId, String)> = Vec::new();
        for attribute in attributes.properties {
            match attribute {
                JsxAttributeLike::JsxAttribute(attribute) => {
                    let Some(at) = attribute.node_id else { continue };
                    let name = match attribute.name {
                        Some(JsxAttributeName::Identifier(name)) => name.text.to_string(),
                        Some(JsxAttributeName::JsxNamespacedName(name)) => {
                            match (name.namespace, name.name) {
                                (Some(namespace), Some(name)) => {
                                    format!("{}:{}", namespace.text, name.text)
                                }
                                _ => continue,
                            }
                        }
                        None => continue,
                    };
                    if let Some(entry) = table.iter_mut().find(|(seen, _)| *seen == name) {
                        entry.1 = at;
                    } else {
                        table.push((name, at));
                    }
                }
                JsxAttributeLike::JsxSpreadAttribute(spread) => {
                    let Some(operand) = spread.expression else { continue };
                    // `getReducedType` has no port; it only collapses an
                    // intersection with disjoint discriminants to `never`,
                    // which `isValidSpreadType` then rejects either way.
                    let operand_type = self.check_expression(operand);
                    if self.is_error(operand_type) || !self.is_valid_spread_type(operand_type) {
                        continue;
                    }
                    // `getPropertiesOfType` reads the reduced apparent type,
                    // so a spread of `T extends { x: number }` overwrites `x`
                    // (`tsxGenericAttributesType1`).
                    let apparent = self.apparent_type(operand_type);
                    let Some(required) = self.spread_required_property_names(apparent) else {
                        continue;
                    };
                    for name in required {
                        if let Some((_, declaration)) = table.iter().find(|(seen, _)| *seen == name)
                            && !reports.iter().any(|(at, seen)| at == declaration && *seen == name)
                        {
                            reports.push((*declaration, name));
                        }
                    }
                }
            }
        }
        for (at, name) in reports {
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                    span,
                    [name],
                ),
            );
        }
    }

    /// TS2710 — `'{0}' are specified twice. The attribute named '{0}' will be
    /// overwritten.`
    ///
    /// `createJsxAttributesTypeFromAttributesProperty` (`jsx.go:819-826`): an
    /// attribute named as the children property, on an element whose body has
    /// semantic children, reports on the attributes node — unless a spread of
    /// type `any` made the whole attributes type `any` (`hasSpreadAnyType`;
    /// `IsTypeAny` is true of `errorType` too). Only explicit attributes
    /// count: a `children` arriving through a spread is not warned about.
    pub(crate) fn check_jsx_children_specified_twice(&mut self, node: NodeId) {
        let Some(Node::JsxAttributes(attributes)) = self.node_map.get(node) else { return };
        let Some(opening) = self.nodes.parent(node) else { return };
        let Some(Node::JsxOpeningElement(_)) = self.node_map.get(opening) else { return };
        let Some(Node::JsxElement(element)) =
            self.nodes.parent(opening).and_then(|parent| self.node_map.get(parent))
        else {
            return;
        };
        if element.opening_element.and_then(|o| o.node_id) != Some(opening)
            || !element.children.iter().any(crate::jsx_intrinsic::semantic_jsx_child)
        {
            return;
        }
        let Some(children) = self.jsx_children_name(opening) else { return };
        let mut explicit = false;
        for attribute in attributes.properties {
            match attribute {
                JsxAttributeLike::JsxAttribute(attribute) => {
                    if let Some(JsxAttributeName::Identifier(name)) = attribute.name
                        && name.text == children
                    {
                        explicit = true;
                    }
                }
                JsxAttributeLike::JsxSpreadAttribute(spread) => {
                    let Some(operand) = spread.expression else { continue };
                    let ty = self.check_expression(operand);
                    if self.is_error(ty)
                        || self.store.get(ty).flags.intersects(crate::flags::TypeFlags::ANY)
                    {
                        return;
                    }
                }
            }
        }
        if !explicit {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_0_WILL_BE_OVERWRITTEN,
                span,
                [children],
            ),
        );
    }
}
