//! TS2352 — `Conversion of type '{0}' to type '{1}' may be a mistake…`
//!
//! `checkAssertionWorker` (`checker.go`): an `as` or `<T>` assertion errors when
//! **neither** type is comparable to the other.
//!
//! # `isTypeComparableTo` — §913: the relation exists now
//!
//! This header used to read *"`isTypeComparableTo` is unported, and this uses
//! assignability instead"*, and justified the substitution: comparable is
//! *weaker* than assignable, so "not assignable in either direction" is a
//! **superset** of upstream's condition, saved from over-reporting only by
//! `relate_ternary` answering `Unknown` on pairs it cannot decide.
//!
//! **[`Relation::Comparable`] has existed since §750**, where
//! `narrow_type_by_discriminant` uses it. The substitution's justification was
//! sound while it held and the claim above it had simply gone stale. This is now
//! upstream's own relation, in both directions, as `checkAssertionWorker` writes
//! it.
//!
//! The error node is the whole assertion expression.

use tsr_ast::{Node, NodeId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, relater::Relation, relater::Ternary};

impl Checker<'_, '_> {
    /// The overlap check for one `as` or `<T>` assertion.
    pub(crate) fn check_assertion_overlap(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (expression, annotation) = match self.node_map.get(node) {
            Some(Node::AsExpression(assertion)) => (assertion.expression, assertion.r#type),
            Some(Node::TypeAssertion(assertion)) => (assertion.expression, assertion.r#type),
            _ => return,
        };
        let (Some(expression), Some(annotation)) = (expression, annotation) else { return };
        // `as const` is `isConstTypeReference` and is not a conversion at all.
        let Some(annotation_id) = annotation.node_id() else { return };
        if self.nodes.kind(annotation_id) == tsr_ast::SyntaxKind::TypeReference
            && matches!(
                self.node_map.get(annotation_id),
                Some(Node::TypeReferenceNode(reference))
                    if reference
                        .type_name
                        .and_then(|name| name.node_id())
                        .and_then(|id| self.node_map.get(id))
                        .is_some_and(|name| matches!(name, Node::Identifier(i) if i.text == "const"))
            )
        {
            return;
        }
        let target = self.get_type_from_type_node(annotation);
        // `getBaseTypeOfLiteralType` on the **expression**
        // (`checker.go:12317`), which is what makes `"foo" as "bar"` compare
        // `string` against `"bar"` and report nothing. Ported literally rather
        // than approximated by a primitive-family test — §45 found the proxy
        // was being read as a statement about the *relation*, which it is not.
        // checkAssertionDeferred regularizes object literals before comparing
        // them (checker.go:12317); assertion overlap does not check excess
        // properties on the fresh expression type.
        let expression_type = self.check_expression(expression);
        let source = self.get_base_type_of_literal_type(expression_type);
        let source = self.get_regular_type_of_object_literal(source);
        let widened = self.widen_object_literal_freshness(source);
        // Unions and intersections are the relater's own union/intersection arms
        // under the comparable relation (`relater.go:181` tries each side's
        // simple arms both ways); `fooOrBar as "baz"`
        // (`stringLiteralsWithTypeAssertions01`) relates there.
        if !self.pair_is_reportable(source, target) {
            return;
        }
        // Both directions, both confident. `Unknown` on either side is silence.
        if self.relate_ternary(target, widened, Relation::Comparable) != Ternary::NotRelated
            || self.relate_ternary(source, target, Relation::Comparable) != Ternary::NotRelated
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CONVERSION_OF_TYPE_0_TO_TYPE_1_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OVERLAPS_WITH_THE_OTHER_IF_THIS_WAS_INTENTIONAL_CONVERT_THE_EXPRESSION_TO_UNKNOWN_FIRST,
                span,
                [source_text, target_text],
            ),
        );
    }
}
