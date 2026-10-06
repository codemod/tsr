//! TS2783 — `'{0}' is specified more than once, so this usage will be
//! overwritten.`
//!
//! `checkSpreadPropOverrides` (`checker.go:13371`), plus the object-literal
//! and JSX spread validity reports (TS2698) and the object rest report
//! (TS2700) that share its operands.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::types::TypeId;

impl Checker<'_, '_> {
    /// `checkObjectLiteral`'s `allPropertiesTable` walk (`checker.go:13158`):
    /// under `strictNullChecks`, every property assignment, shorthand and
    /// method enters the table by name (the binder-merged symbol, whose value
    /// declaration is the first member of that name), and each valid spread
    /// runs `checkSpreadPropOverrides` (`checker.go:13371`) against the table
    /// as filled so far.
    pub(crate) fn check_spread_property_overrides(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) || !self.strict_null_checks {
            return;
        }
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else {
            return;
        };
        if self.assignment_target_kind(node) != crate::expressions::AssignmentTargetKind::None {
            return;
        }
        let mut members = Vec::new();
        for property in literal.properties {
            let Some(at) = property.node_id() else { continue };
            let name = match self.node_map.get(at) {
                Some(Node::PropertyAssignment(assignment)) => assignment.name,
                Some(Node::ShorthandPropertyAssignment(shorthand)) => shorthand.name,
                Some(Node::MethodDeclaration(method)) => method.name,
                Some(Node::SpreadAssignment(spread)) => {
                    if let Some(operand) = spread.expression {
                        members.push((at, None, Some(operand)));
                    }
                    continue;
                }
                _ => continue,
            };
            let text = match name {
                tsr_ast::PropertyName::Identifier(name) => name.text.to_string(),
                tsr_ast::PropertyName::StringLiteral(name) => name.text.to_string(),
                tsr_ast::PropertyName::NumericLiteral(name) => {
                    crate::printing::normalise_number(name.text)
                }
                _ => continue,
            };
            members.push((at, Some(text), None));
        }
        let mut table: Vec<(String, NodeId)> = Vec::new();
        let mut reports: Vec<(NodeId, String)> = Vec::new();
        for (at, name, operand) in members {
            if let Some(name) = name {
                if !table.iter().any(|(seen, _)| *seen == name) {
                    table.push((name, at));
                }
                continue;
            }
            let Some(operand) = operand else { continue };
            let operand_type = self.check_expression(operand);
            if self.is_error(operand_type) || !self.is_valid_spread_type(operand_type) {
                continue;
            }
            let merged = self.try_merge_union_of_object_type_and_empty_object(operand_type);
            let Some(required) = self.spread_required_property_names(merged) else { continue };
            for name in required {
                if let Some((_, declaration)) = table.iter().find(|(seen, _)| *seen == name) {
                    reports.push((*declaration, name));
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

    /// `getPropertiesOfType(t)` filtered to properties that are neither
    /// `SymbolFlagsOptional` nor `CheckFlagsPartial`: a union property is
    /// partial unless every constituent declares it, and optional when any
    /// constituent's is. `None` when a constituent's members are unresolved.
    pub(crate) fn spread_required_property_names(&mut self, ty: TypeId) -> Option<Vec<String>> {
        if self.store.get(ty).flags.intersects(TypeFlags::ANY) {
            return None;
        }
        let parts = match &self.store.get(ty).data {
            crate::types::TypeData::Union { types, .. } => types.clone(),
            _ => vec![ty],
        };
        let mut required: Option<Vec<String>> = None;
        for part in parts {
            let (properties, _) = self.spread_properties(part, false)?;
            let names: Vec<String> = properties
                .into_iter()
                .filter(|property| !property.optional)
                .map(|property| property.name)
                .collect();
            required = Some(match required {
                None => names,
                Some(previous) => {
                    previous.into_iter().filter(|name| names.contains(name)).collect()
                }
            });
        }
        required
    }

    /// TS2700 — `Rest types may only be created from object types.`
    ///
    /// `getBindingElementTypeFromParentType`'s object rest arm
    /// (`checker.go:17723`): an `unknown` or non-spreadable parent reports on
    /// the rest element. An `any` parent returns before it.
    pub(crate) fn check_object_rest_of_non_object_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        if element.dot_dot_dot_token.is_none() {
            return;
        }
        let Some(pattern_id) = self.nodes.parent(node) else { return };
        if self.nodes.kind(pattern_id) != SyntaxKind::ObjectBindingPattern {
            return;
        }
        // A contextually typed parameter's parent can be this port's
        // in-progress inference image (an instantiable or unknown stand-in),
        // which is not evidence about the final parent type.
        if self.binding_root_is_unannotated_parameter(node) {
            return;
        }
        let Some(holder) = self.nodes.parent(pattern_id) else { return };
        let parent_type = self.get_type_for_binding_element_parent(holder);
        if parent_type == self.intrinsics.any || self.is_error(parent_type) {
            return;
        }
        let parent_type = self.destructuring_parent_adjusted(node, holder, parent_type);
        if !self.store.get(parent_type).flags.intersects(crate::flags::TypeFlags::UNKNOWN)
            && self.is_valid_spread_type(parent_type)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(&messages::REST_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES, span),
        );
    }

    /// TS2698 for a JSX spread attribute: `createJsxAttributesTypeFromAttributesProperty`
    /// (`jsx.go:785`) reports on the attribute's expression.
    pub(crate) fn check_jsx_spread_of_non_object_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::JsxSpreadAttribute(attribute)) = self.node_map.get(node) else { return };
        let Some(operand) = attribute.expression else { return };
        let Some(operand_id) = operand.node_id() else { return };
        let operand_type = self.check_expression(operand);
        if self.is_error(operand_type) || self.is_valid_spread_type(operand_type) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(operand_id) else { return };
        let span = self.error_span(operand_id);
        self.report(
            file,
            Diagnostic::new(&messages::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES, span),
        );
    }

    /// TS2698 — `Spread types may only be created from object types.`
    ///
    /// `checkObjectLiteral`'s spread arm (`checker.go:13291`): the operand's
    /// type must pass `isValidSpreadType` (`checker.go:13304`), reported on
    /// the spread assignment.
    pub(crate) fn check_spread_of_non_object_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else {
            return;
        };
        // A destructuring target is checkDestructuringAssignment's, not
        // checkObjectLiteral's; its spread is a rest element.
        if self.assignment_target_kind(node) != crate::expressions::AssignmentTargetKind::None {
            return;
        }
        let mut operands = Vec::new();
        for property in literal.properties {
            let Some(at) = property.node_id() else { continue };
            let Some(Node::SpreadAssignment(spread)) = self.node_map.get(at) else { continue };
            let Some(operand) = spread.expression else { continue };
            operands.push((at, operand));
        }
        for (at, operand) in operands {
            let operand_type = self.check_expression(operand);
            if self.is_error(operand_type) || self.is_valid_spread_type(operand_type) {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.nodes.span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
                    span,
                ),
            );
        }
    }
}
