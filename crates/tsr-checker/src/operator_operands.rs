//! TS2365 — `Operator '{0}' cannot be applied to types '{1}' and '{2}'.`
//!
//! `reportOperatorError`'s default branch (`checker.go:12734`), reached from
//! two of `checkBinaryLikeExpression`'s arms: `+` / `+=`
//! (`checker.go:12452`) and the relational operators `<`, `>`, `<=`, `>=`
//! (`checker.go:12465`). The equality arm is the third caller and is
//! `crate::comparison_overlap`.
//!
//! # The direction trap, and it is the mirror of §25's
//!
//! Here the relation is consulted to decide **not** to report. A
//! `Ternary::Unknown` collapsed into `false` by the binary projection would
//! therefore *manufacture* a diagnostic, where §25's collapse suppressed one.
//! Every positive test below reads "not a confident negative" rather than "a
//! confident positive", which is why they compare `Ternary::NotRelated`
//! explicitly instead of going through a boolean helper.
//!
//! `docs/architecture/checker-notes-diag2.md` §49.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    types::TypeId,
};

impl Checker<'_, '_> {
    /// The operand check for one `+`, `+=`, `<`, `>`, `<=` or `>=`.
    pub(crate) fn check_operator_operands(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        let Some(operator) = binary.operator_token.map(|token| token.kind) else { return };
        // `+=` is upstream's arm too, and it is declined here with a named
        // owner. A compound assignment's left operand goes through the
        // assignment-target checks first, and those answer `errorType` when
        // they fail — `f += 1` on a class is TS2629 and `IsTypeAny(errorType)`
        // then supplies a result type, so TS2365 never fires. This port models
        // neither TS2629 nor TS2364, so its left operand keeps a real type and
        // the rule invents a diagnostic. `arithAssignTyping` (7 lines) and
        // `parserStrictMode5` are exactly that — `checker-notes-diag2.md` §49.
        let addition = operator == SyntaxKind::PlusToken;
        let relational = matches!(
            operator,
            SyntaxKind::LessThanToken
                | SyntaxKind::GreaterThanToken
                | SyntaxKind::LessThanEqualsToken
                | SyntaxKind::GreaterThanEqualsToken
        );
        if !addition && !relational {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        let mut source = self.check_expression(left);
        let mut target = self.check_expression(right);
        // `checkNonNullType` runs before both arms (`checker.go:12419`,
        // `:12467`) and reports TS2531/TS2533 in place of this code, so a
        // nullish operand is a different diagnostic rather than a missing one.
        // …and a union *containing* `undefined` is the same case one level in:
        // `C1M4A2?: number` is `number | undefined`, `checkNonNullType` strips
        // it and reports TS18048 instead. `optionalParamArgsTest` is three
        // lines of that.
        if self.operand_is_nullish(source) || self.operand_is_nullish(target) {
            return;
        }
        if !self.pair_is_reportable(source, target) {
            return;
        }
        // A **type parameter** on either side. `areTypesComparable` and
        // `isTypeAssignableTo` both reach a type parameter through its
        // constraint (`getBaseConstraintOfType`), which this relater does not
        // follow, so every unconstrained-looking `t < a` reads as a confident
        // negative here and is not one upstream.
        // `comparisonOperatorWithNoRelationshipTypeParameter` was 48 of this
        // rule's first 58 wrong lines — `checker-notes-diag2.md` §49.
        if self.type_of(source).flags.intersects(TypeFlags::TYPE_PARAMETER)
            || self.type_of(target).flags.intersects(TypeFlags::TYPE_PARAMETER)
        {
            return;
        }
        // `checkForDisallowedESSymbolOperand` runs before both arms
        // (`checker.go:12442`, `:12461`) and reports TS2469 — *"the '{0}'
        // operator cannot be applied to type 'symbol'"* — in place of this
        // code. `symbolType8` is five lines of it.
        if self
            .type_of(source)
            .flags
            .intersects(TypeFlags::ES_SYMBOL.union(TypeFlags::UNIQUE_ES_SYMBOL))
            || self
                .type_of(target)
                .flags
                .intersects(TypeFlags::ES_SYMBOL.union(TypeFlags::UNIQUE_ES_SYMBOL))
        {
            return;
        }
        let reportable = if addition {
            self.addition_operands_have_no_result(source, target)
        } else {
            // `getBaseTypeOfLiteralTypeForComparison` (`checker.go:12468`) —
            // the relational arm compares `string` and `number`, not `"a"` and
            // `1`.
            source = self.get_base_type_of_literal_type(source);
            target = self.get_base_type_of_literal_type(target);
            self.relational_operands_are_incomparable(source, target)
        };
        if !reportable {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::OPERATOR_0_CANNOT_BE_APPLIED_TO_TYPES_1_AND_2,
                span,
                [operator_text(operator).to_string(), source_text, target_text],
            ),
        );
    }

    /// `checkNonNullType` (`checker.go:12419`, `:12467`) runs before both arms
    /// and reports TS2531/TS2533/TS18048 in place of this code, so a nullish
    /// operand is a *different* diagnostic rather than a missing one — at the
    /// top level or inside a union.
    fn operand_is_nullish(&self, operand: TypeId) -> bool {
        if self.type_of(operand).flags.intersects(TypeFlags::NULLABLE) {
            return true;
        }
        matches!(
            &self.store.get(operand).data,
            crate::types::TypeData::Union { types, .. }
                if types.iter().any(|&constituent| {
                    self.type_of(constituent).flags.intersects(TypeFlags::NULLABLE)
                })
        )
    }

    /// `resultType == nil` for `+` (`checker.go:12422`): neither both
    /// number-like, nor both bigint-like, nor either string-like, nor either
    /// `any`.
    fn addition_operands_have_no_result(&mut self, source: TypeId, target: TypeId) -> bool {
        for side in [source, target] {
            if self.type_of(side).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
                return false;
            }
        }
        let both = |checker: &mut Self, kind: TypeFlags, to: TypeId| {
            checker.assignable_to_kind(source, kind, to)
                && checker.assignable_to_kind(target, kind, to)
        };
        let number = self.intrinsics.number;
        let bigint = self.intrinsics.bigint;
        let string = self.intrinsics.string;
        if both(self, TypeFlags::NUMBER_LIKE, number) || both(self, TypeFlags::BIG_INT_LIKE, bigint)
        {
            return false;
        }
        // `||` and not `&&`: one string operand makes the whole expression a
        // concatenation.
        if self.assignable_to_kind(source, TypeFlags::STRING_LIKE, string)
            || self.assignable_to_kind(target, TypeFlags::STRING_LIKE, string)
        {
            return false;
        }
        true
    }

    /// The relational arm's predicate, negated (`checker.go:12466`): `any` on
    /// either side, or both operands numeric, or neither numeric **and** the
    /// two comparable.
    fn relational_operands_are_incomparable(&mut self, source: TypeId, target: TypeId) -> bool {
        for side in [source, target] {
            if self.type_of(side).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
                return false;
            }
        }
        let number = self.intrinsics.number;
        let bigint = self.intrinsics.bigint;
        let numeric = |checker: &mut Self, side: TypeId| {
            checker.assignable_to_kind(side, TypeFlags::NUMBER_LIKE, number)
                || checker.assignable_to_kind(side, TypeFlags::BIG_INT_LIKE, bigint)
        };
        let (left, right) = (numeric(self, source), numeric(self, target));
        if left && right {
            return false;
        }
        if left != right {
            return true;
        }
        // Neither is numeric: `areTypesComparable`, for which this port
        // substitutes assignability behind §45's composite decline.
        if self.either_is_composite(source, target) {
            return false;
        }
        self.relate_ternary(source, target, Relation::Assignable) == Ternary::NotRelated
            && self.relate_ternary(target, source, Relation::Assignable) == Ternary::NotRelated
    }

    /// `isTypeAssignableToKindEx(source, kind, strict)`
    /// (`checker.go:27645`): the flag test first, then assignability to the
    /// kind's primitive — and **not a confident negative** rather than a
    /// confident positive, per this module's header.
    fn assignable_to_kind(&mut self, source: TypeId, kind: TypeFlags, primitive: TypeId) -> bool {
        if self.type_of(source).flags.intersects(kind) {
            return true;
        }
        // The `strict` short-circuit: `any`, `unknown`, `void`, `undefined`
        // and `null` are never assignable to a kind they do not carry.
        if self.type_of(source).flags.intersects(
            TypeFlags::ANY_OR_UNKNOWN
                .union(TypeFlags::VOID)
                .union(TypeFlags::UNDEFINED)
                .union(TypeFlags::NULL),
        ) {
            return false;
        }
        // §29's definite negative, asked here for the same reason it is asked
        // there: `is_related_to` answers `Unknown` for an object source
        // against a primitive target because its structural arm was never
        // reached, and nothing structured is assignable to `number` or
        // `string` whatever its shape turns out to be.
        // `additionOperatorWithInvalidOperands`' `Object`, `Number`, `C`, `E`
        // and function operands are twelve of its nineteen lines.
        if self.object_against_primitive(source, primitive) {
            return false;
        }
        self.relate_ternary(source, primitive, Relation::Assignable) != Ternary::NotRelated
    }
}

impl Checker<'_, '_> {
    /// TS2362 / TS2363 — `checkArithmeticOperandType` (`checker.go:12799`),
    /// called once per operand of an arithmetic or bitwise operator with
    /// `!isTypeAssignableTo(t, numberOrBigIntType)` as its whole predicate.
    ///
    /// Error node the **operand**. `docs/architecture/checker-notes-diag2.md`
    /// §65.
    pub(crate) fn check_arithmetic_operand_types(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if !binary.operator_token.is_some_and(|token| is_arithmetic_operator(token.kind)) {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        let left_type = self.check_expression(left);
        let right_type = self.check_expression(right);
        // `checkNonNullType` runs first at this site and reports TS18050 /
        // TS18048 in place of these (§50.3).
        if self.operand_is_nullish(left_type) || self.operand_is_nullish(right_type) {
            return;
        }
        // Two boolean operands are **TS2447** on the operator token, reported
        // before the operand check and returning (`checker.go:12372`).
        if self.type_of(left_type).flags.intersects(TypeFlags::BOOLEAN_LIKE)
            && self.type_of(right_type).flags.intersects(TypeFlags::BOOLEAN_LIKE)
        {
            return;
        }
        for (operand, operand_type, message) in [
            (
                left,
                left_type,
                &messages::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
            ),
            (
                right,
                right_type,
                &messages::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
            ),
        ] {
            let Some(at) = operand.node_id() else { continue };
            if !self.operand_is_definitely_not_numeric(operand_type) {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(file, Diagnostic::new(message, span));
        }
    }

    /// `!isTypeAssignableTo(t, numberOrBigIntType)`, read as a **confident**
    /// negative (§52's direction): the rule reports because a relation failed,
    /// so `Unknown` is silence. An enum answers `Unknown` here and is
    /// assignable upstream, which is the safe direction — the message itself
    /// names enums.
    fn operand_is_definitely_not_numeric(&mut self, operand: TypeId) -> bool {
        if self
            .type_of(operand)
            .flags
            .intersects(TypeFlags::NUMBER_LIKE.union(TypeFlags::BIG_INT_LIKE))
        {
            return false;
        }
        if !self.pair_is_reportable(operand, self.intrinsics.number) {
            return false;
        }
        if self.either_is_composite(operand, operand) {
            return false;
        }
        let number = self.intrinsics.number;
        let bigint = self.intrinsics.bigint;
        // §29's definite negative first: nothing structured is assignable to
        // `number` whatever its shape turns out to be.
        if self.object_against_primitive(operand, number) {
            return true;
        }
        self.relate_ternary(operand, number, Relation::Assignable) == Ternary::NotRelated
            && self.relate_ternary(operand, bigint, Relation::Assignable) == Ternary::NotRelated
    }
}

impl Checker<'_, '_> {
    /// TS2356 — `An arithmetic operand must be of type 'any', 'number',
    /// 'bigint' or an enum type.`
    ///
    /// `checkArithmeticOperandType` again (`checker.go:10899` and `:10915`),
    /// this time at the operand of `++` or `--`.
    ///
    /// **Only those two.** Unary `+`, `-` and `~` take a different arm
    /// (`checker.go:10875`) which reports TS2469 for a `symbol` operand and
    /// nothing about numerics — `-"a"` is not this diagnostic.
    /// `docs/architecture/checker-notes-diag2.md` §66.
    pub(crate) fn check_increment_operand_type(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let operand = match self.node_map.get(node) {
            Some(Node::PrefixUnaryExpression(unary))
                if matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) =>
            {
                unary.operand
            }
            Some(Node::PostfixUnaryExpression(unary)) => unary.operand,
            _ => return,
        };
        let Some(operand) = operand else { return };
        let Some(at) = operand.node_id() else { return };
        // An operand naming something that is **not a variable** is
        // `checkIdentifier`'s assignment-target arm (`checker.go:11080`),
        // which reports TS2628 for an enum, TS2629 for a class, TS2631 for a
        // namespace, TS2630 for a function and TS2632 for an import — and
        // reaches them *before* the operand type is looked at. `++ENUM` was
        // sixteen wrong TS2356 lines (§66).
        if let Some(Node::Identifier(identifier)) = self.node_map.get(at)
            && self
                .binder
                .resolve_name(
                    self.nodes,
                    self.node_map,
                    at,
                    identifier.text,
                    tsr_binder::SymbolFlags::VALUE,
                )
                .is_some_and(|symbol| {
                    !self
                        .binder
                        .symbols()
                        .get(symbol)
                        .flags
                        .intersects(tsr_binder::SymbolFlags::VARIABLE)
                })
        {
            return;
        }
        let operand_type = self.check_expression(operand);
        // `checkNonNullType` wraps the argument (`checker.go:10899`) and
        // reports TS18050 / TS18048 in place of this one.
        if self.operand_is_nullish(operand_type)
            || !self.operand_is_definitely_not_numeric(operand_type)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::new(
                &messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
                span,
            ),
        );
    }
}

/// The operators whose operands `checkArithmeticOperandType` guards —
/// arithmetic, shift and bitwise, with their compound forms. `+` is excluded:
/// it is overloaded with concatenation and has its own arm (§49).
fn is_arithmetic_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::MinusToken
            | SyntaxKind::AsteriskToken
            | SyntaxKind::AsteriskAsteriskToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::BarToken
            | SyntaxKind::CaretToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::AsteriskAsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::CaretEqualsToken
    )
}

/// `scanner.TokenToString` for the six operators this rule reports on.
fn operator_text(operator: SyntaxKind) -> &'static str {
    match operator {
        SyntaxKind::PlusToken => "+",
        SyntaxKind::PlusEqualsToken => "+=",
        SyntaxKind::LessThanToken => "<",
        SyntaxKind::GreaterThanToken => ">",
        SyntaxKind::LessThanEqualsToken => "<=",
        _ => ">=",
    }
}
