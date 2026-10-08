//! checkImportCallExpression argument diagnostics (pinned checker.go:8267).
use tsr_ast::{Expression, Node, NodeId, ObjectLiteralElementLike, PropertyName, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
};

impl Checker<'_, '_> {
    /// Check arguments in native order: specifier, options, extra arguments,
    /// specifier relation, options relation, then obsolete assertion property.
    pub(crate) fn check_import_call_specifier(&mut self, node: NodeId) {
        let Some(Node::CallExpression(call)) = self.node_map.get(node) else { return };
        if !matches!(call.expression, Some(Expression::KeywordExpression(keyword))
            if keyword.kind == SyntaxKind::ImportKeyword)
        {
            return;
        }
        // Native grammar precedes argument checking but does not suppress it.
        // ES2015 module rejection is already owned by check.rs and has priority.
        if !self.file_has_parse_errors && self.module_kind != tsr_core::ModuleKind::ES2015 {
            let supports_options = matches!(
                self.module_kind,
                tsr_core::ModuleKind::ESNext
                    | tsr_core::ModuleKind::Preserve
                    | tsr_core::ModuleKind::Node16
                    | tsr_core::ModuleKind::Node18
                    | tsr_core::ModuleKind::Node20
                    | tsr_core::ModuleKind::NodeNext
            );
            let error = if !supports_options && call.arguments.len() > 1 {
                call.arguments[1].node_id().map(|at| (at,
                    &messages::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NODE16_NODE18_NODE20_NODENEXT_OR_PRESERVE))
            } else if call.arguments.is_empty() || call.arguments.len() > 2 {
                Some((node, &messages::DYNAMIC_IMPORTS_CAN_ONLY_ACCEPT_A_MODULE_SPECIFIER_AND_AN_OPTIONAL_SET_OF_ATTRIBUTES_AS_ARGUMENTS))
            } else {
                call.arguments.iter().find_map(|argument| match argument {
                    Expression::SpreadElement(spread) => spread.node_id.map(|at| {
                        (at, &messages::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT)
                    }),
                    _ => None,
                })
            };
            if let Some((at, message)) = error
                && let Some(file) = self.source_file_of_for_diagnostics(at)
            {
                self.report(file, Diagnostic::new(message, self.error_span(at)));
            }
        }
        let Some(&specifier) = call.arguments.first() else { return };
        let specifier_type = self.check_expression(specifier);
        let options = call.arguments.get(1).copied();
        let options_type = options.map(|options| self.check_expression(options));
        for &argument in call.arguments.iter().skip(2) {
            self.check_expression(argument);
        }
        if !self.is_gap(specifier_type)
            && (self.store.get(specifier_type).flags.intersects(TypeFlags::NULLABLE)
                || self.relate_ternary(
                    specifier_type,
                    self.intrinsics.string,
                    Relation::Assignable,
                ) == Ternary::NotRelated)
            && let Some(at) = specifier.node_id()
            && let Some(file) = self.source_file_of_for_diagnostics(at)
        {
            let printed = self.type_to_string(specifier_type);
            self.report(file, Diagnostic::with_args(
                &messages::DYNAMIC_IMPORT_S_SPECIFIER_MUST_BE_OF_TYPE_STRING_BUT_HERE_HAS_TYPE_0,
                self.error_span(at), [printed]));
        }
        let (Some(options), Some(options_type)) = (options, options_type) else { return };
        if !self.is_gap(options_type)
            && let Some(symbol) = self.global_type_symbol_with_arity("ImportCallOptions", 0)
        {
            let target = self.get_declared_type_of_symbol(symbol);
            if !self.is_gap(target) && target != self.intrinsics.empty_object {
                let target = self.get_union_type(&[target, self.intrinsics.undefined]);
                if self.relate_ternary(options_type, target, Relation::Assignable)
                    == Ternary::NotRelated
                    && let Some(at) = options.node_id()
                {
                    self.report_relation_failure(
                        at,
                        self.error_span(at),
                        options.node_id(),
                        options_type,
                        target,
                        None,
                    );
                }
            }
        }
        if let Expression::ObjectLiteralExpression(object) = options {
            for property in object.properties {
                if let ObjectLiteralElementLike::PropertyAssignment(property) = property
                    && let PropertyName::Identifier(name) = property.name
                    && name.text == "assert"
                    && let Some(at) = name.node_id
                    && let Some(file) = self.source_file_of_for_diagnostics(at)
                {
                    self.report(file, Diagnostic::new(
                        &messages::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_ASSERT,
                        self.error_span(at)));
                    break;
                }
            }
        }
    }
}
