//! TS2352 — `Conversion of type '{0}' to type '{1}' may be a mistake…`
//!
//! `checkAssertionWorker` (`checker.go`): an `as` or `<T>` assertion errors when
//! **neither** type is comparable to the other.
//!
//! # `isTypeComparableTo` is unported, and this uses assignability instead
//!
//! `crate::relater` has `Assignable`, `Subtype` and `StrictSubtype` and no
//! comparable relation. Comparable is *weaker* than assignable, so
//! "not assignable in either direction" is a **superset** of upstream's
//! condition — which would over-report.
//!
//! What makes the substitution sound here is `relate_ternary`: the two relations
//! differ exactly where one of them decides a pair the other does not, and a pair
//! this port cannot decide answers `Unknown` rather than `NotRelated`. So the
//! rule fires only where *both* directions are a confident negative, and the
//! measurement in `checker-notes-diag2.md` is what says whether that is enough.
//!
//! The error node is the whole assertion expression.

use tsr_ast::{Node, NodeId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, relater::Relation, relater::Ternary, types::TypeId};

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
        let source = self.check_expression(expression);
        if !self.pair_is_reportable(source, target) || self.same_primitive_family(source, target) {
            return;
        }
        // Both directions, both confident. `Unknown` on either side is silence.
        if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
            || self.relate_ternary(target, source, Relation::Assignable) != Ternary::NotRelated
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
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

    /// Are both sides the same primitive family — both string-like, both
    /// number-like, both boolean-like, both bigint-like?
    ///
    /// **This is where `isTypeComparableTo` and `isTypeAssignableTo` part
    /// company**, and it is the whole of what the substitution above costs.
    /// `"foo" as "bar"` is not assignable in either direction and *is* comparable
    /// (both reduce to `string`), so upstream reports nothing.
    /// `stringLiteralsWithTypeAssertions01` and
    /// `stringLiteralsAssertionsInEqualityComparisons02` were 6 of this rule's
    /// first 7 wrong lines and its only loss.
    pub(crate) fn same_primitive_family(&self, source: TypeId, target: TypeId) -> bool {
        let family = |flags: crate::flags::TypeFlags| -> Option<u8> {
            use crate::flags::TypeFlags as F;
            if flags.intersects(F::STRING.union(F::STRING_LITERAL)) {
                Some(0)
            } else if flags.intersects(F::NUMBER.union(F::NUMBER_LITERAL)) {
                Some(1)
            } else if flags.intersects(F::BOOLEAN.union(F::BOOLEAN_LITERAL)) {
                Some(2)
            } else if flags.intersects(F::BIG_INT.union(F::BIG_INT_LITERAL)) {
                Some(3)
            } else {
                None
            }
        };
        if self.either_is_composite(source, target) {
            return true;
        }
        let (Some(left), Some(right)) =
            (family(self.type_of(source).flags), family(self.type_of(target).flags))
        else {
            return false;
        };
        left == right
    }

    /// A **union or intersection** on either side, which every rule that
    /// substitutes assignability for comparability must decline.
    ///
    /// `fooOrBar as "baz"` with `fooOrBar: "foo" | "bar"` is comparable
    /// upstream — the constituents and the target are one primitive family —
    /// and this port's union carries `UNION` rather than its constituents'
    /// flags, so no flag test can see through it.
    /// `stringLiteralsWithTypeAssertions01` lines 7 and 8 are that, and they
    /// were `check_assertion_overlap`'s last two wrong lines and its only loss.
    ///
    /// Shared with [`Checker::check_comparison_overlap`], which needs **this**
    /// decline and not the literal-family one above —
    /// `checker-notes-diag2.md` §45.
    pub(crate) fn either_is_composite(&self, source: TypeId, target: TypeId) -> bool {
        let composite = crate::flags::TypeFlags::UNION.union(crate::flags::TypeFlags::INTERSECTION);
        self.type_of(source).flags.intersects(composite)
            || self.type_of(target).flags.intersects(composite)
    }
}
