//! TS1360 — `Type '{0}' does not satisfy the expected type '{1}'.`
//!
//! `checkSatisfiesExpressionWorker` (`checker.go:10741`, `checkSatisfiesExpression`
//! at the pinned commit): the expression's type must be assignable to the
//! written type, checked by `checkTypeAssignableToAndOptionallyElaborate` with
//! the satisfies node as the error node, the expression as the elaboration
//! source and TS1360 as the head message. The expression's own type is the
//! result, which `expressions.rs` already answers; this module is the report.

use tsr_ast::{Node, NodeId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

impl<'a> Checker<'a, '_> {
    /// The assignability check of one `expression satisfies Type`.
    pub(crate) fn check_satisfies_expression(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::SatisfiesExpression(satisfies)) = self.node_map.get(node) else { return };
        let (Some(expression), Some(annotation)) = (satisfies.expression, satisfies.r#type) else {
            return;
        };
        let Some(expression_id) = expression.node_id() else { return };
        let span = self.satisfies_keyword_span(node, expression_id);
        self.check_satisfies_worker(node, expression, annotation, span);
    }

    /// `checkSatisfiesExpressionWorker` proper: `expression` against
    /// `annotation`, the head message at `span`. Shared with a JS file's
    /// reparsed `@satisfies` cast (`jsdoc_annotations.rs`), whose span is the
    /// tag name.
    pub(crate) fn check_satisfies_worker(
        &mut self,
        node: NodeId,
        expression: tsr_ast::Expression<'a>,
        annotation: tsr_ast::TypeNode<'a>,
        span: tsr_core::Span,
    ) {
        let Some(expression_id) = expression.node_id() else { return };
        let source = self.check_expression(expression);
        let target = self.get_type_from_type_node(annotation);
        if self.is_error(target) {
            return;
        }
        // The relation's own excess-property arm (hasExcessProperties,
        // relater.go) reports TS2353 at the property and fails the relation
        // before any head message is written.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, expression_id);
        if self.diagnostics.len() != before {
            return;
        }
        // The same arm, for targets whose declared member table the syntactic
        // check above declines (mapped `Record`/`Partial` instantiations): when
        // the relation fails and its excess verdict holds, that names the
        // failure, so the head message must not be written in its place.
        if self.fresh_object_literal_types.contains(&source)
            && self.fresh_literal_has_excess_property(source, target)
            && self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                == crate::relater::Ternary::NotRelated
        {
            self.report_first_excess_property(expression_id, target);
            return;
        }
        self.report_relation_failure(
            node,
            span,
            Some(expression_id),
            source,
            target,
            Some(&messages::TYPE_0_DOES_NOT_SATISFY_THE_EXPECTED_TYPE_1),
        );
    }

    /// `hasExcessProperties` (relater.go) for a non-union target: the first
    /// written member (`getPropertiesOfType(source)` order) that
    /// `isKnownProperty` rejects is TS2353 at its name. A near miss would be
    /// TS2561, which this port does not speak, and silence is kept there and
    /// for any shape whose names cannot be read.
    fn report_first_excess_property(&mut self, literal_node: NodeId, target: TypeId) {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(literal_node) else {
            return;
        };
        if self.type_of(target).flags.intersects(TypeFlags::UNION) {
            return;
        }
        for property in literal.properties {
            let name = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name.node_id()
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    shorthand.name.node_id()
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    method.name.node_id()
                }
                _ => return,
            };
            let Some(name) = name.and_then(|id| self.identifier_text(id)).map(str::to_string)
            else {
                return;
            };
            if self.get_type_of_property_of_type(target, &name).is_some() {
                continue;
            }
            let key = self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(name.clone()),
                false,
            );
            if self.get_applicable_index_info(target, key).is_some() {
                continue;
            }
            let candidates = self.get_property_names_of_type(target).unwrap_or_default();
            let candidates: Vec<&str> = candidates.iter().map(String::as_str).collect();
            if crate::check::spelling_suggestion(&name, &candidates).is_some() {
                return;
            }
            let Some(at) = self.excess_property_name_node(literal, &name) else { return };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
            let span = self.error_span(at);
            let printed = self.type_to_string(target);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                    span,
                    [name, printed],
                ),
            );
            return;
        }
    }

    /// `getErrorSpanForNode`'s satisfies arm (`scanner.go:2625`): the token
    /// after the expression, `GetRangeOfTokenAtPosition(SkipTrivia(text,
    /// expression.End()))` — the `satisfies` keyword. Without source text the
    /// whole node is the span.
    fn satisfies_keyword_span(&self, node: NodeId, expression: NodeId) -> tsr_core::Span {
        let whole = self.nodes.span(node);
        let start = self.nodes.span(expression).end;
        let token = self
            .source_file_of_for_diagnostics(node)
            .and_then(|file| self.module_host?.source_text(file, self.nodes))
            .and_then(|text| text.get(start as usize..whole.end as usize))
            .map(|rest| tsr_scanner::Scanner::new(rest).scan().span);
        token.map_or(whole, |span| tsr_core::Span::new(start + span.start, start + span.end))
    }
}
