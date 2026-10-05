//! TS2367 — `This comparison appears to be unintentional because the types
//! '{0}' and '{1}' have no overlap.`
//!
//! `checkBinaryLikeExpression`'s equality arm (`checker.go:12487`) calls
//! `reportOperatorErrorUnless` with
//!
//! ```go
//! c.isTypeEqualityComparableTo(left, right) || c.isTypeEqualityComparableTo(right, left)
//! ```
//!
//! and `isTypeEqualityComparableTo` (`checker.go:12861`) is
//!
//! ```go
//! return (target.flags&TypeFlagsNullable) != 0 || c.isTypeComparableTo(source, target)
//! ```
//!
//! Both operand types are `checkExpression` results, so each is the
//! flow-narrowed type at the comparison.
//!
//! # Three-valued
//!
//! [`Relation::Comparable`] answers `Unknown` for a pair this relater cannot
//! decide, so the rule fires only where both directions are a confident
//! `NotRelated`.
//!
//! The error node is the whole binary expression (`checker.go:12333` passes
//! `node`), not the operator and not either operand.

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
        let left_type = self.check_expression(left);
        let right_type = self.check_expression(right);
        if self.is_error(left_type) || self.is_error(right_type) {
            return;
        }
        if self.equality_comparable_either_way(left_type, right_type) != Ternary::NotRelated {
            return;
        }
        // `reportOperatorError` → `getBaseTypesIfUnrelated` (`checker.go:12745`):
        // display the literal bases when they are unrelated too. The
        // `wouldWorkWithAwait` related-information suggestion is not ported;
        // it changes neither the code nor the span.
        let left_base = self.get_base_type_of_literal_type(left_type);
        let right_base = self.get_base_type_of_literal_type(right_type);
        let (left_shown, right_shown) =
            if self.equality_comparable_either_way(left_base, right_base) == Ternary::NotRelated {
                (left_base, right_base)
            } else {
                (left_type, right_type)
            };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let left_text = self.type_to_string(left_shown);
        let right_text = self.type_to_string(right_shown);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_0_AND_1_HAVE_NO_OVERLAP,
                span,
                [left_text, right_text],
            ),
        );
    }

    /// `isTypeEqualityComparableTo(left, right) ||
    /// isTypeEqualityComparableTo(right, left)` (`checker.go:12861`) as a
    /// Kleene disjunction: `Related` if either direction holds, `NotRelated`
    /// only if both are confident negatives.
    fn equality_comparable_either_way(&mut self, left: TypeId, right: TypeId) -> Ternary {
        if self.type_of(left).flags.intersects(TypeFlags::NULLABLE)
            || self.type_of(right).flags.intersects(TypeFlags::NULLABLE)
        {
            return Ternary::Related;
        }
        let forward = self.relate_ternary(left, right, Relation::Comparable);
        if forward == Ternary::Related {
            return Ternary::Related;
        }
        match (forward, self.relate_ternary(right, left, Relation::Comparable)) {
            (_, Ternary::Related) => Ternary::Related,
            (Ternary::NotRelated, Ternary::NotRelated) => Ternary::NotRelated,
            _ => Ternary::Unknown,
        }
    }
}
