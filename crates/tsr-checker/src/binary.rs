//! The binary operators.
//!
//! Ported from `Checker.checkBinaryExpression` / `checkBinaryLikeExpression`
//! (`checker.go:12331`, `:12336`). Split out of [`crate::checker`] because
//! upstream's worker is a dozen unrelated rules behind one node kind and they
//! land at different times — the logical operators are still a gap and belong
//! to unions (`bd tsr-4sc.9`).

use tsr_ast::{Expression, SyntaxKind};

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

impl Checker<'_, '_> {
    /// Ported from `Checker.checkBinaryExpression` / `checkBinaryLikeExpression`
    /// (`checker.go:12331`, `:12336`).
    ///
    /// # What is here and what is not
    ///
    /// Upstream's worker is a dozen unrelated rules behind one node kind, and
    /// they do not become available at the same time. Ported: assignment, the
    /// arithmetic/bitwise/shift family, `+`, the relational and equality
    /// families, `in`, `instanceof` and the comma operator. Not ported: the
    /// logical operators — which need `getTypeFacts` and `strictNullChecks`
    /// rather than unions, see the arm below (`bd tsr-5s2`) — and destructuring
    /// assignment, whose left-hand side is an object or array literal pattern
    /// (`bd tsr-4sc.13`). Both yield `errorType`.
    ///
    /// Nothing here reports a diagnostic: upstream's arms are mostly error
    /// reporting, and the result type is computed independently of it. That is
    /// why this is a small function against a large one, rather than an
    /// abbreviation of it. `bd tsr-5e7.6`.
    pub(crate) fn check_binary_expression(
        &mut self,
        node: &tsr_ast::BinaryExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(left), Some(operator_token), Some(right)) =
            (node.left, node.operator_token, node.right)
        else {
            return error;
        };
        let operator = operator_token.kind;

        // `[a, b] = c` is a destructuring assignment, and upstream leaves this
        // function before checking either operand as an expression
        // (`checker.go:12338`). Taking the right-hand type would be *nearly*
        // right and would silently mistype the pattern itself.
        //
        // **This test is unobservable today**, and that is stated rather than
        // covered by a test that would not bite: an array or object literal is
        // itself unported, so both paths reach `errorType` and deleting the test
        // turns nothing red. It becomes load-bearing the moment those literals
        // are checked, at which point the fall-through would answer the
        // right-hand type for a pattern. Same reasoning as the symbol-flags test
        // in `get_type_of_symbol`.
        if operator == SyntaxKind::EqualsToken
            && matches!(
                left,
                Expression::ObjectLiteralExpression(_) | Expression::ArrayLiteralExpression(_)
            )
        {
            return error;
        }

        let left_type = self.check_expression(left);
        let right_type = self.check_expression(right);

        match operator {
            // Assignment yields the right-hand type — including its *freshness*,
            // which is why `x = "a"` is `"a"` and not `string`. The rest of
            // upstream's arm is `checkAssignmentOperator`, which reports rather
            // than computes.
            SyntaxKind::EqualsToken | SyntaxKind::CommaToken => right_type,

            SyntaxKind::AsteriskToken
            | SyntaxKind::AsteriskAsteriskToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::MinusToken
            | SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
            | SyntaxKind::BarToken
            | SyntaxKind::CaretToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::AsteriskAsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::CaretEqualsToken
            | SyntaxKind::AmpersandEqualsToken => {
                self.check_arithmetic_operation(left_type, right_type)
            }

            SyntaxKind::PlusToken | SyntaxKind::PlusEqualsToken => {
                self.check_addition(left_type, right_type)
            }

            // Every comparison is `boolean` **whatever the operands are** —
            // upstream computes the operand types only to report on them and
            // returns `booleanType` unconditionally (`checker.go:12460`, `:12474`,
            // and `checkInExpression`). So these answer even where an operand is
            // a gap, and that is a computed answer rather than a guess.
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken
            | SyntaxKind::EqualsEqualsEqualsToken
            | SyntaxKind::ExclamationEqualsEqualsToken
            | SyntaxKind::InKeyword
            // `instanceof` reaches `checkInstanceOfExpression`
            // (`checker.go:12492`), which consults `Symbol.hasInstance` and then
            // returns `booleanType` as well. The result does not depend on that
            // lookup, so it joins the arm rather than getting one of its own.
            | SyntaxKind::InstanceOfKeyword => self.intrinsics.boolean,

            // The logical operators build a union of the operands, and unions
            // now exist — but the union each one builds is *not* determined by
            // the operand types alone. `&&` unions
            // `extractDefinitelyFalsyTypes(getBaseTypeOfLiteralType(right))` with
            // the right type when `strictNullChecks` is off and
            // `extractDefinitelyFalsyTypes(left)` with it when it is on
            // (`checker.go:12495`); `||` and `??` reduce with
            // `UnionReductionSubtype`, which needs assignability. Both also need
            // `getTypeFacts` (`checker.go:30982`), whose every arm branches on
            // `strictNullChecks`. This port has no compiler options, so answering
            // would be a wrong line rather than a missing one. `bd tsr-5s2`.
            //
            // `errorType`, not the left type, which would be right only when the
            // left operand is never falsy.
            _ => error,
        }
    }

    /// The arithmetic, bitwise and shift arm of `checkBinaryLikeExpression`
    /// (`checker.go:12358`).
    ///
    /// Upstream: `number` when both operands are any-like or neither is
    /// bigint-like, `bigint` when both are bigint-like, and `errorType`
    /// otherwise.
    ///
    /// # A deliberate deviation: a gap in is a gap out
    ///
    /// `errorType` carries `TypeFlagsAny`, so upstream's first test — *"if both
    /// are any or unknown, assume the operation resolves to `number`"* — accepts
    /// it and answers `number`. In upstream that is sound: `errorType` appears
    /// only where a real error was already reported, and the operand genuinely
    /// could be anything.
    ///
    /// In this port `errorType` also means **an unported form**, and there the
    /// same rule would convert a gap into a claim: `someUnportedThing * 2` would
    /// read `number` whether or not the operand is a `bigint`, and the
    /// `checker_types` histogram could no longer tell the two apart. So an
    /// `errorType` operand propagates. Upstream itself does exactly this in the
    /// `+` arm (`checker.go:12452`), which is the precedent.
    ///
    /// **How this would be shown wrong:** when the unported expression forms
    /// land, the operands stop being `errorType` and the deviation stops
    /// applying to anything. If a measurable population of lines still reaches
    /// here with an `errorType` operand at that point, this should return
    /// `number` as upstream does — the honest reading of that would be that the
    /// gap is not the operand's form but something else.
    fn check_arithmetic_operation(&mut self, left: TypeId, right: TypeId) -> TypeId {
        if self.is_error(left) || self.is_error(right) {
            return self.intrinsics.error;
        }
        let left_flags = self.store.get(left).flags;
        let right_flags = self.store.get(right).flags;
        if left_flags.intersects(TypeFlags::BIG_INT_LIKE)
            || right_flags.intersects(TypeFlags::BIG_INT_LIKE)
        {
            if left_flags.intersects(TypeFlags::BIG_INT_LIKE)
                && right_flags.intersects(TypeFlags::BIG_INT_LIKE)
            {
                return self.intrinsics.bigint;
            }
            // Mixing `bigint` with anything else is an error, and upstream's
            // answer for it is `errorType` rather than a best guess.
            return self.intrinsics.error;
        }
        self.intrinsics.number
    }

    /// The `+` arm of `checkBinaryLikeExpression` (`checker.go:12403`).
    ///
    /// Upstream's order is load-bearing and is kept: both number-like → `number`,
    /// both bigint-like → `bigint`, *either* string-like → `string`, either any
    /// → `any` unless either is `errorType`, in which case `errorType`.
    ///
    /// **The order is upstream's and is currently unobservable here**, which is
    /// stated rather than dressed up as a test: with only primitive types, a
    /// type is string-like or number-like and never both, so swapping the two
    /// arms turns nothing red. It starts to matter with the types that are
    /// assignable to both kinds — enums, and unions of them — and it is kept in
    /// upstream's order so that it is already right when they arrive.
    fn check_addition(&mut self, left: TypeId, right: TypeId) -> TypeId {
        if self.is_error(left) || self.is_error(right) {
            return self.intrinsics.error;
        }
        let left_flags = self.store.get(left).flags;
        let right_flags = self.store.get(right).flags;
        let both = |kind: TypeFlags| left_flags.intersects(kind) && right_flags.intersects(kind);
        if both(TypeFlags::NUMBER_LIKE) {
            return self.intrinsics.number;
        }
        if both(TypeFlags::BIG_INT_LIKE) {
            return self.intrinsics.bigint;
        }
        if left_flags.intersects(TypeFlags::STRING_LIKE)
            || right_flags.intersects(TypeFlags::STRING_LIKE)
        {
            return self.intrinsics.string;
        }
        if left_flags.intersects(TypeFlags::ANY) || right_flags.intersects(TypeFlags::ANY) {
            return self.intrinsics.any;
        }
        // Upstream reports and answers `any` here; without diagnostics the honest
        // answer is that nothing was computed.
        self.intrinsics.error
    }
}
