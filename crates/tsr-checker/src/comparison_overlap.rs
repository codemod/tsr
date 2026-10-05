//! TS2367 — `This comparison appears to be unintentional because the types
//! '{0}' and '{1}' have no overlap.`
//!
//! `checkBinaryLikeExpression`'s equality arm (`checker.go:12475`) reports
//! through `reportOperatorErrorUnless` unless `isTypeEqualityComparableTo`
//! holds in **either** direction, and `isTypeEqualityComparableTo`
//! (`checker.go:12861`) is
//!
//! ```go
//! return (target.flags&TypeFlagsNullable) != 0 || c.isTypeComparableTo(source, target)
//! ```
//!
//! # The nullable disjunct comes first, and it costs no relation at all
//!
//! Because the predicate is asked in both directions, a `null` or `undefined`
//! on *either* side takes the flag disjunct and the comparison is never
//! reported. `x === null` and `null === x` are both silence.
//!
//! # Upstream's comparable relation, not a substitute
//!
//! This rule used to substitute assignability for comparability and carried
//! three declines to keep the substitution from over-reporting: a
//! union/intersection veto, an enum veto, and a syntactic "an earlier `if`
//! already narrowed this name" test. [`Relation::Comparable`] exists
//! (`checker-notes-diag2.md` §750, already used by TS2352), so the rule now
//! asks it directly. Measured at the switch (`docs/parity/notes/flow.md`):
//! every TS2367 extra in the corpus (175 lines with TS2678) disappeared, 69
//! missing lines appeared, and dropping the narrowing test changed no case's
//! output at all — the operands' flow types already carry that narrowing.
//!
//! `relate_ternary`'s `Unknown` is still silence: both directions must be a
//! confident `NotRelated`.
//!
//! The error node is the whole binary expression; the printed pair is
//! `getBaseTypesIfUnrelated`'s (`checker.go:12745`).

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    types::TypeId,
};

impl Checker<'_, '_> {
    /// The overlap check for one `==`, `!=`, `===` or `!==`.
    pub(crate) fn check_comparison_overlap(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if !binary.operator_token.is_some_and(|token| {
            matches!(
                token.kind,
                SyntaxKind::EqualsEqualsToken
                    | SyntaxKind::ExclamationEqualsToken
                    | SyntaxKind::EqualsEqualsEqualsToken
                    | SyntaxKind::ExclamationEqualsEqualsToken
            )
        }) {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        let source = self.check_expression(left);
        let target = self.check_expression(right);
        // An error type is `any` upstream and relates to everything; here it
        // is a type the port could not build, so it is silence. No enum veto:
        // the relater's enum arms are ported (`assignreport.rs`).
        if !self.assignability_pair_is_reportable(source, target) {
            return;
        }
        // `isTypeEqualityComparableTo` both ways (`checker.go:12487`), under
        // upstream's own comparable relation. Both directions must be a
        // confident `NotRelated`; `Unknown` on either side is silence.
        if !self.equality_incomparable(source, target) {
            return;
        }
        // `reportOperatorError` (`checker.go:12718`): `getBaseTypesIfUnrelated`
        // prints the widened pair when the widened pair is unrelated too, so
        // `"foo" === "bar"` reads `'"foo"'` and `'"bar"'` only when `string`
        // against `string` relates.
        let (source, target) = {
            let left_base = self.get_base_type_of_literal_type(source);
            let right_base = self.get_base_type_of_literal_type(target);
            if self.equality_incomparable(left_base, right_base) {
                (left_base, right_base)
            } else {
                (source, target)
            }
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_0_AND_1_HAVE_NO_OVERLAP,
                span,
                [source_text, target_text],
            ),
        );
    }

    /// `!(isTypeEqualityComparableTo(l, r) || isTypeEqualityComparableTo(r, l))`
    /// with both relations a confident `NotRelated`. The nullable disjunct is a
    /// flag test asked in both directions, so it runs before any relation.
    fn equality_incomparable(&mut self, left: TypeId, right: TypeId) -> bool {
        if self.type_of(left).flags.intersects(TypeFlags::NULLABLE)
            || self.type_of(right).flags.intersects(TypeFlags::NULLABLE)
        {
            return false;
        }
        self.relate_ternary(left, right, Relation::Comparable) == Ternary::NotRelated
            && self.relate_ternary(right, left, Relation::Comparable) == Ternary::NotRelated
    }
}
