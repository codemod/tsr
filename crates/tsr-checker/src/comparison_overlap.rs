//! TS2367 — `This comparison appears to be unintentional because the types
//! '{0}' and '{1}' have no overlap.`
//!
//! `checkBinaryLikeExpression`'s equality arm (`checker.go:12487`) reports
//! unless `isTypeEqualityComparableTo` holds in **either** direction, and
//! `isTypeEqualityComparableTo` (`checker.go:12861`) is
//!
//! ```go
//! return (target.flags&TypeFlagsNullable) != 0 || c.isTypeComparableTo(source, target)
//! ```
//!
//! # The nullable disjunct comes first, and it costs no relation at all
//!
//! Because the predicate is asked in both directions, a `null` or `undefined`
//! on *either* side takes the flag disjunct and the comparison is never
//! reported. `x === null` and `null === x` are both silence, and this is a flag
//! test rather than a relation.
//!
//! # What is left is `crate::assertion_overlap`'s substitution
//!
//! `crate::relater` has no comparable relation, so assignability stands in for
//! it — sound only because `relate_ternary` answers `Unknown` for a pair this
//! port cannot decide, so the rule fires only where both directions are a
//! confident negative. The two places the relations part company are named and
//! measured in `checker-notes-diag2.md` §31, and
//! [`Checker::same_primitive_family`] is reused here **unchanged**: whether the
//! substitution needs a third decline at this site is the question §45 is
//! measuring.
//!
//! The error node is the whole binary expression (`checker.go:12333` passes
//! `node`), not the operator and not either operand.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
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
        // `(target.flags&TypeFlagsNullable) != 0`, asked in both directions —
        // so a `null` or `undefined` on either side is silence before any
        // relation runs.
        if self.type_of(source).flags.intersects(TypeFlags::NULLABLE)
            || self.type_of(target).flags.intersects(TypeFlags::NULLABLE)
        {
            return;
        }
        // **Not** `same_primitive_family`. That decline is
        // `check_assertion_overlap`'s stand-in for `getBaseTypeOfLiteralType`,
        // which `checkAssertionDeferred` (`checker.go:12317`) applies to the
        // expression before comparing — `"foo" as "bar"` compares `string`
        // against `"bar"`. The equality arm applies no such widening, so
        // `"foo" === "bar"` **is** TS2367 and declining it costs the whole
        // `stringLiteralsWithEqualityChecks0*` family. §45 records the
        // measurement. What is shared is the composite decline.
        if !self.pair_is_reportable(source, target) || self.either_is_composite(source, target) {
            return;
        }
        // An earlier `if` in the same statement list that tests the same name
        // has already narrowed it upstream, and every narrowing this port does
        // not perform leaves the operand **wider** — which in a no-overlap
        // check is the direction that invents a diagnostic. `const x = 1;
        // if (x == 1) { break; } if (x == 2)` reads `x : never` upstream at the
        // second test (equality narrowing plus the unreachable `break`) and
        // `never` overlaps everything; here it is still `1`, and `1` against
        // `2` is a confident negative. Four cases, 36 wrong lines, all of the
        // rule's — `checker-notes-diag2.md` §45.
        if self.narrowed_away_by_an_earlier_test(node, left, source)
            || self.narrowed_away_by_an_earlier_test(node, right, target)
        {
            return;
        }
        // Both directions, both confident. `Unknown` on either side is silence.
        if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
            || self.relate_ternary(target, source, Relation::Assignable) != Ternary::NotRelated
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
                &messages::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_0_AND_1_HAVE_NO_OVERLAP,
                span,
                [source_text, target_text],
            ),
        );
    }

    /// Is `operand` an identifier that an **earlier statement in the same
    /// list** already tested in an `if` condition?
    ///
    /// A syntactic over-approximation of "upstream narrowed this before the
    /// comparison", in the same spirit as
    /// `Checker::reference_is_guarded_by_a_condition_on` (§8) and for the same
    /// reason: declining costs a missing diagnostic, which is the direction
    /// this rule may fail in, while reporting on an un-narrowed operand costs a
    /// confident wrong line.
    fn narrowed_away_by_an_earlier_test(
        &mut self,
        comparison: NodeId,
        operand: tsr_ast::Expression<'_>,
        operand_type: crate::types::TypeId,
    ) -> bool {
        // Only a **unit** type can be narrowed to `never` by a failed equality
        // test, and `never` is what makes upstream's comparison overlap. A
        // `let y = 0` reads `number` here and no equality test empties it, so
        // `capturedLetConstInLoop8`'s repeated `if (y == 1)` stays reportable —
        // which is what separates that case from `…Loop6`'s `const x = 1`.
        if !self.type_of(operand_type).flags.intersects(TypeFlags::LITERAL) {
            return false;
        }
        let tsr_ast::Expression::Identifier(identifier) = operand else { return false };
        let text = identifier.text;
        // The statement this comparison belongs to, and the block holding it.
        let mut statement = comparison;
        let mut depth = 0u32;
        let block = loop {
            depth += 1;
            if depth > 64 {
                return false;
            }
            let Some(parent) = self.nodes.parent(statement) else { return false };
            match self.node_map.get(parent) {
                Some(Node::Block(block)) => break block,
                Some(Node::SourceFile(_)) | None => return false,
                _ => statement = parent,
            }
        };
        for element in block.statements {
            let Some(id) = tsr_ast::Node::from(*element).node_id() else { continue };
            if id == statement {
                return false;
            }
            if let Some(Node::IfStatement(preceding)) = self.node_map.get(id)
                && let Some(condition) = preceding.expression.and_then(|e| e.node_id())
                && self.equality_test_empties(condition, text, operand_type)
            {
                return true;
            }
        }
        false
    }

    /// Does `condition` empty `operand_type` on its false branch?
    ///
    /// `x == 1` narrows `x : 1` to `never` where it fails, and only there: the
    /// compared literal must be the operand's own unit type.
    /// `capturedLetConstInLoop8`'s `y : 0` against `if (y == 1)` keeps `0` and
    /// stays reportable, which is what separates the two families.
    fn equality_test_empties(
        &mut self,
        condition: NodeId,
        text: &str,
        operand_type: crate::types::TypeId,
    ) -> bool {
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(condition) else {
            return false;
        };
        if !binary.operator_token.is_some_and(|token| {
            matches!(
                token.kind,
                SyntaxKind::EqualsEqualsToken
                    | SyntaxKind::ExclamationEqualsToken
                    | SyntaxKind::EqualsEqualsEqualsToken
                    | SyntaxKind::ExclamationEqualsEqualsToken
            )
        }) {
            return false;
        }
        let sides = [binary.left, binary.right];
        if !sides
            .into_iter()
            .flatten()
            .any(|side| matches!(side, tsr_ast::Expression::Identifier(name) if name.text == text))
        {
            return false;
        }
        let regular = self.get_regular_type_of_literal_type(operand_type);
        sides.into_iter().flatten().any(|side| {
            !matches!(side, tsr_ast::Expression::Identifier(name) if name.text == text) && {
                let compared = self.check_expression(side);
                self.get_regular_type_of_literal_type(compared) == regular
            }
        })
    }
}
