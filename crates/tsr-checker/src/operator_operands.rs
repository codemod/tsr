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
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    types::TypeId,
};

impl Checker<'_, '_> {
    /// The operand check for one `+`, `+=`, `<`, `>`, `<=`, `>=` or equality
    /// operator.
    pub(crate) fn check_operator_operands(&mut self, node: NodeId, ambient: bool) {
        // **No `file_has_parse_errors` gate**, for the reason §892 removed the
        // one below: upstream's `checkBinaryLikeExpression` runs regardless, and
        // `1 > > 2` is TS2365 **and** TS1109 upstream while this port emitted
        // only the second — declining the semantic diagnostic *because it had
        // emitted the parse error*. §895 measured the class as a whole at
        // −1/+13 and closed it; this is one rule, measured on its own, which is
        // the route it left open. §896.
        if ambient {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        let Some(operator) = binary.operator_token.map(|token| token.kind) else { return };
        // The equality arm's literal and NaN rules run in JS too (TS2839 only
        // for `===`/`!==` there), so they precede the JS gate.
        if matches!(
            operator,
            SyntaxKind::EqualsEqualsToken
                | SyntaxKind::ExclamationEqualsToken
                | SyntaxKind::EqualsEqualsEqualsToken
                | SyntaxKind::ExclamationEqualsEqualsToken
        ) {
            if let (Some(left), Some(right)) = (binary.left, binary.right) {
                self.check_equality_operator(node, operator, left, right);
            }
            return;
        }
        if self.in_js_file(node) {
            return;
        }
        // `+=` is upstream's arm too, and it is declined here with a named
        // owner. A compound assignment's left operand goes through the
        // assignment-target checks first, and those answer `errorType` when
        // they fail — `f += 1` on a class is TS2629 and `IsTypeAny(errorType)`
        // then supplies a result type, so TS2365 never fires. This port models
        // neither TS2629 nor TS2364, so its left operand keeps a real type and
        // the rule invents a diagnostic. `arithAssignTyping` (7 lines) and
        // `parserStrictMode5` are exactly that — `checker-notes-diag2.md` §49.
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        if matches!(
            operator,
            SyntaxKind::LessThanToken
                | SyntaxKind::GreaterThanToken
                | SyntaxKind::LessThanEqualsToken
                | SyntaxKind::GreaterThanEqualsToken
        ) {
            self.check_relational_operator(node, operator, left, right);
            return;
        }
        if operator != SyntaxKind::PlusToken {
            return;
        }
        let source = self.check_expression(left);
        let target = self.check_expression(right);
        // `checkNonNullType` runs before both arms (`checker.go:12419`,
        // `:12467`) and reports TS2531/TS2533 in place of this code, so a
        // nullish operand is a different diagnostic rather than a missing one.
        // …and a union *containing* `undefined` is the same case one level in:
        // `C1M4A2?: number` is `number | undefined`, `checkNonNullType` strips
        // it and reports TS18048 instead. `optionalParamArgsTest` is three
        // lines of that.
        //
        // **…and only under `strictNullChecks`.** `checkNonNullType` strips and
        // reports nothing when the flag is off, so the addition arm sees `null`
        // directly and TS2365 is upstream's answer — `null.ts` sets
        // `@strict: false` and expects it. A guard justified by another rule's
        // behaviour inherits that rule's preconditions, including its flags.
        // §429.
        if self.strict_null_checks
            && (self.operand_is_nullish(source) || self.operand_is_nullish(target))
        {
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
        if !self.addition_operands_have_no_result(source, target) {
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
                [token_text(operator).to_string(), source_text, target_text],
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
    /// The arithmetic, shift and bitwise arm of
    /// `checkBinaryLikeExpressionWorker` (`checker.go:12358`), diagnostics
    /// only — the arm's type is [`Checker::check_binary_expression`]'s.
    ///
    /// ```go
    /// leftType = c.checkNonNullType(leftType, left)
    /// rightType = c.checkNonNullType(rightType, right)
    /// if both boolean-like and getSuggestedBooleanOperator(op) != Unknown { TS2447; return numberType }
    /// leftOk := c.checkArithmeticOperandType(left, leftType, TS2362, true)
    /// rightOk := c.checkArithmeticOperandType(right, rightType, TS2363, true)
    /// … number / bigint / reportOperatorError(…, bothAreBigIntLike) …
    /// if leftOk && rightOk { c.checkAssignmentOperator(…) … }
    /// ```
    ///
    /// Returns `leftOk && rightOk` — whether the caller goes on to
    /// `checkAssignmentOperator`, which is TS2364's site. Every relation is
    /// asked as a [`Ternary`] and a diagnostic needs a confident
    /// `NotRelated` (`docs/parity/notes/operators.md` §3).
    pub(crate) fn check_arithmetic_operand_types(&mut self, node: NodeId, ambient: bool) -> bool {
        // **No `file_has_parse_errors` gate.** Upstream runs regardless.
        // §892.
        if ambient || self.in_js_file(node) {
            return true;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return true };
        let Some(operator_token) = binary.operator_token else { return true };
        let operator = operator_token.kind;
        if !is_arithmetic_operator(operator) {
            return true;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return true };
        let left_type = self.assignment_operand_type(left, operator.is_assignment_operator());
        let right_type = self.check_expression(right);
        let left_type = self.check_non_null_type_reporting(left_type, left);
        let right_type = self.check_non_null_type_reporting(right_type, right);
        // A helpful suggestion for two boolean operands (`checker.go:12372`),
        // on the operator token, and `return c.numberType` before either
        // operand check.
        if self.type_of(left_type).flags.intersects(TypeFlags::BOOLEAN_LIKE)
            && self.type_of(right_type).flags.intersects(TypeFlags::BOOLEAN_LIKE)
            && let Some(suggested) = suggested_boolean_operator(operator)
        {
            if let Some(at) = operator_token.node_id
                && let Some(file) = self.source_file_of_for_diagnostics(node)
            {
                let span = self.nodes.span(at);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::THE_0_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_1_INSTEAD,
                        span,
                        [token_text(operator).to_string(), token_text(suggested).to_string()],
                    ),
                );
            }
            return false;
        }
        let left_ok = self.check_arithmetic_operand_type(
            left,
            left_type,
            &messages::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
        );
        let right_ok = self.check_arithmetic_operand_type(
            right,
            right_type,
            &messages::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
        );
        // The result-type cascade: `number` when both are any-like or neither
        // may be bigint-like; `bigint` when both are bigint-like (where `>>>`
        // is TS2365 and `**` below ES2016 is TS2791); otherwise TS2365 on the
        // pair.
        let any_or_unknown = |checker: &Self, id: TypeId| {
            checker.type_of(id).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        };
        let numeric = (any_or_unknown(self, left_type) && any_or_unknown(self, right_type))
            || (!self.maybe_type_of_kind(left_type, TypeFlags::BIG_INT_LIKE)
                && !self.maybe_type_of_kind(right_type, TypeFlags::BIG_INT_LIKE));
        if !numeric {
            match self.both_are_bigint_like(left_type, right_type) {
                Ternary::Related => match operator {
                    SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                    | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => {
                        self.report_operator_error(
                            left_type,
                            operator,
                            right_type,
                            node,
                            OperatorRelation::None,
                        );
                    }
                    SyntaxKind::AsteriskAsteriskToken | SyntaxKind::AsteriskAsteriskEqualsToken
                        if self.language_version < tsr_core::ScriptTarget::ES2016 =>
                    {
                        if let Some(file) = self.source_file_of_for_diagnostics(node) {
                            let span = self.error_span(node);
                            self.report(
                                file,
                                Diagnostic::new(
                                    &messages::EXPONENTIATION_CANNOT_BE_PERFORMED_ON_BIGINT_VALUES_UNLESS_THE_TARGET_OPTION_IS_SET_TO_ES2016_OR_LATER,
                                    span,
                                ),
                            );
                        }
                    }
                    _ => {}
                },
                Ternary::NotRelated => self.report_operator_error(
                    left_type,
                    operator,
                    right_type,
                    node,
                    OperatorRelation::BothBigIntLike,
                ),
                Ternary::Unknown => {}
            }
        }
        left_ok && right_ok
    }

    /// An operand's type as the operator arms see it. `checkIdentifier`'s
    /// assignment arms end `return c.errorType` (`checker.go:11093`,
    /// `:11101`) for an assignment target that is not a writable variable
    /// (TS2539/TS2629/TS2630/… and TS2540 report there), and an error-typed
    /// operand is `any` to every operator rule. `is_target` is
    /// `getAssignmentTargetKind(operand) != None`: the left of a compound
    /// assignment, or the operand of `++`/`--`. §253, §282.
    fn assignment_operand_type(
        &mut self,
        operand: tsr_ast::Expression<'_>,
        is_target: bool,
    ) -> TypeId {
        let is_error_target = is_target
            && operand.node_id().is_some_and(|id| {
                let tsr_ast::Expression::Identifier(identifier) = operand else { return false };
                self.assignment_target_symbol(id, identifier.text).is_some_and(|(symbol, flags)| {
                    !flags.intersects(SymbolFlags::VARIABLE) || self.is_readonly_symbol(symbol)
                })
            });
        if is_error_target { self.intrinsics.error } else { self.check_expression(operand) }
    }

    /// `checkArithmeticOperandType` (`checker.go:12799`):
    /// `!isTypeAssignableTo(t, numberOrBigIntType)` reports `diagnostic` on
    /// the operand. The await suggestion is related information only.
    fn check_arithmetic_operand_type(
        &mut self,
        operand: tsr_ast::Expression<'_>,
        ty: TypeId,
        diagnostic: &'static tsr_diagnostics::Message,
    ) -> bool {
        if self.assignable_to_number_or_bigint(ty) != Ternary::NotRelated {
            return true;
        }
        if let Some(at) = operand.node_id()
            && let Some(file) = self.source_file_of_for_diagnostics(at)
        {
            let span = self.error_span(at);
            self.report(file, Diagnostic::new(diagnostic, span));
        }
        false
    }

    /// `bothAreBigIntLike` (`checker.go:12776`): `isTypeAssignableToKind`
    /// (not strict) to `BigIntLike` on both sides.
    fn both_are_bigint_like(&mut self, left: TypeId, right: TypeId) -> Ternary {
        let left = self.is_type_assignable_to_kind(left, TypeFlags::BIG_INT_LIKE, false);
        if left == Ternary::NotRelated {
            return left;
        }
        ternary_and(left, self.is_type_assignable_to_kind(right, TypeFlags::BIG_INT_LIKE, false))
    }

    /// `isTypeAssignableToKindEx` (`checker.go:27645`) for the kinds the
    /// operator arms ask, as a [`Ternary`].
    pub(crate) fn is_type_assignable_to_kind(
        &mut self,
        source: TypeId,
        kind: TypeFlags,
        strict: bool,
    ) -> Ternary {
        let flags = self.type_of(source).flags;
        if flags.intersects(kind) {
            return Ternary::Related;
        }
        if strict
            && flags.intersects(
                TypeFlags::ANY_OR_UNKNOWN
                    | TypeFlags::VOID
                    | TypeFlags::UNDEFINED
                    | TypeFlags::NULL,
            )
        {
            return Ternary::NotRelated;
        }
        let mut answer = Ternary::NotRelated;
        for (bit, primitive) in [
            (TypeFlags::NUMBER_LIKE, self.intrinsics.number),
            (TypeFlags::BIG_INT_LIKE, self.intrinsics.bigint),
            (TypeFlags::STRING_LIKE, self.intrinsics.string),
        ] {
            if kind.intersects(bit) {
                answer = ternary_or(
                    answer,
                    self.relate_ternary(source, primitive, Relation::Assignable),
                );
            }
        }
        answer
    }
}

impl Checker<'_, '_> {
    /// The diagnostics of `checkPrefixUnaryExpression` (`checker.go:10855`)
    /// and `checkPostfixUnaryExpression` (`checker.go:10914`).
    ///
    /// - `+`, `-`, `~`: `checkNonNullType` with its reporter, TS2469 for an
    ///   operand that may be a symbol, and for `+` TS2736 for one that may
    ///   be bigint-like. A numeric or bigint literal operand of `-`/`+`
    ///   returns before any of it.
    /// - `++`, `--` (prefix and postfix): `checkArithmeticOperandType` on
    ///   the non-null operand type (TS2356).
    ///
    /// Returns upstream's `ok` — whether `checkReferenceExpression` runs
    /// (TS2357), *"to avoid reporting cascading errors"*. `true` for the
    /// operators that never reach it, whose caller does not ask.
    pub(crate) fn check_unary_operator_operands(&mut self, node: NodeId, ambient: bool) -> bool {
        if ambient || self.in_js_file(node) {
            return true;
        }
        let (operator, operand) = match self.node_map.get(node) {
            Some(Node::PrefixUnaryExpression(unary)) => (unary.operator.kind, unary.operand),
            Some(Node::PostfixUnaryExpression(unary)) => (unary.operator.kind, unary.operand),
            _ => return true,
        };
        let Some(operand) = operand else { return true };
        let prefix = self.nodes.kind(node) == SyntaxKind::PrefixUnaryExpression;
        let operand_type = self.assignment_operand_type(
            operand,
            matches!(operator, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken),
        );
        if prefix {
            // The literal arms answer a fresh literal before the operator
            // switch (`checker.go:10861`).
            match operand {
                tsr_ast::Expression::NumericLiteral(_)
                    if matches!(operator, SyntaxKind::MinusToken | SyntaxKind::PlusToken) =>
                {
                    return true;
                }
                tsr_ast::Expression::BigIntLiteral(_) if operator == SyntaxKind::MinusToken => {
                    return true;
                }
                _ => {}
            }
            if matches!(
                operator,
                SyntaxKind::PlusToken | SyntaxKind::MinusToken | SyntaxKind::TildeToken
            ) {
                self.check_non_null_type_reporting(operand_type, operand);
                let Some(at) = operand.node_id() else { return true };
                let Some(file) = self.source_file_of_for_diagnostics(at) else { return true };
                let span = self.error_span(at);
                if self.maybe_type_of_kind_considering_base_constraint(
                    operand_type,
                    TypeFlags::ES_SYMBOL_LIKE,
                ) {
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::THE_0_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                            span,
                            [token_text(operator).to_string()],
                        ),
                    );
                }
                if operator == SyntaxKind::PlusToken
                    && self.maybe_type_of_kind_considering_base_constraint(
                        operand_type,
                        TypeFlags::BIG_INT_LIKE,
                    )
                {
                    let base = self.get_base_type_of_literal_type(operand_type);
                    let text = self.type_to_string(base);
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::OPERATOR_0_CANNOT_BE_APPLIED_TO_TYPE_1,
                            span,
                            [token_text(operator).to_string(), text],
                        ),
                    );
                }
                return true;
            }
            if !matches!(operator, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken) {
                return true;
            }
        }
        let non_null = self.check_non_null_type_reporting(operand_type, operand);
        self.check_arithmetic_operand_type(
            operand,
            non_null,
            &messages::AN_ARITHMETIC_OPERAND_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT_OR_AN_ENUM_TYPE,
        )
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

/// Which of upstream's `isRelated` callbacks `reportOperatorError` was handed
/// (`checker.go:12718`). Upstream passes a closure; the port names the
/// closures it passes so the relation they ask stays upstream's.
#[derive(Clone, Copy)]
pub(crate) enum OperatorRelation {
    /// `nil`: no await probe and no base-type widening.
    None,
    /// `bothAreBigIntLike` (`checker.go:12776`).
    BothBigIntLike,
    /// The relational arm's closure (`checker.go:12469`).
    Relational,
}

/// Kleene conjunction over the port's three-valued relation answers.
fn ternary_and(a: Ternary, b: Ternary) -> Ternary {
    match (a, b) {
        (Ternary::NotRelated, _) | (_, Ternary::NotRelated) => Ternary::NotRelated,
        (Ternary::Related, Ternary::Related) => Ternary::Related,
        _ => Ternary::Unknown,
    }
}

/// Kleene disjunction.
fn ternary_or(a: Ternary, b: Ternary) -> Ternary {
    match (a, b) {
        (Ternary::Related, _) | (_, Ternary::Related) => Ternary::Related,
        (Ternary::NotRelated, Ternary::NotRelated) => Ternary::NotRelated,
        _ => Ternary::Unknown,
    }
}

/// Kleene negation.
fn ternary_not(a: Ternary) -> Ternary {
    match a {
        Ternary::Related => Ternary::NotRelated,
        Ternary::NotRelated => Ternary::Related,
        Ternary::Unknown => Ternary::Unknown,
    }
}

impl Checker<'_, '_> {
    /// The `<`, `>`, `<=`, `>=` arm of `checkBinaryLikeExpressionWorker`
    /// (`checker.go:12460`), diagnostics only — the arm's type is
    /// `booleanType` and [`Checker::check_binary_expression`] answers it.
    ///
    /// ```go
    /// if c.checkForDisallowedESSymbolOperand(left, right, leftType, rightType, operator) {
    ///     leftType = c.getBaseTypeOfLiteralTypeForComparison(c.checkNonNullType(leftType, left))
    ///     rightType = c.getBaseTypeOfLiteralTypeForComparison(c.checkNonNullType(rightType, right))
    ///     c.reportOperatorErrorUnless(leftType, operator, rightType, errorNode, …)
    /// }
    /// ```
    ///
    /// The predicate is evaluated in Kleene logic over [`Ternary`] and the
    /// diagnostic is reported only on a confident `NotRelated`
    /// (`docs/parity/notes/operators.md`).
    fn check_relational_operator(
        &mut self,
        node: NodeId,
        operator: SyntaxKind,
        left: tsr_ast::Expression<'_>,
        right: tsr_ast::Expression<'_>,
    ) {
        let left_type = self.check_expression(left);
        let right_type = self.check_expression(right);
        if !self
            .check_for_disallowed_es_symbol_operand(left, right, left_type, right_type, operator)
        {
            return;
        }
        let left_type = self.check_non_null_type_reporting(left_type, left);
        let left_type = self.get_base_type_of_literal_type_for_comparison(left_type);
        let right_type = self.check_non_null_type_reporting(right_type, right);
        let right_type = self.get_base_type_of_literal_type_for_comparison(right_type);
        if self.operator_types_related(OperatorRelation::Relational, left_type, right_type)
            != Ternary::NotRelated
        {
            return;
        }
        self.report_operator_error(
            left_type,
            operator,
            right_type,
            node,
            OperatorRelation::Relational,
        );
    }

    /// The closures `checkBinaryLikeExpressionWorker` hands to
    /// `reportOperatorError(Unless)`, as a [`Ternary`].
    fn operator_types_related(
        &mut self,
        relation: OperatorRelation,
        left: TypeId,
        right: TypeId,
    ) -> Ternary {
        match relation {
            OperatorRelation::None => Ternary::NotRelated,
            OperatorRelation::BothBigIntLike => self.both_are_bigint_like(left, right),
            // ```go
            // if IsTypeAny(left) || IsTypeAny(right) { return true }
            // leftAssignableToNumber := c.isTypeAssignableTo(left, c.numberOrBigIntType)
            // rightAssignableToNumber := c.isTypeAssignableTo(right, c.numberOrBigIntType)
            // return leftAssignableToNumber && rightAssignableToNumber ||
            //     !leftAssignableToNumber && !rightAssignableToNumber && c.areTypesComparable(left, right)
            // ```
            OperatorRelation::Relational => {
                if self.is_type_any(left) || self.is_type_any(right) {
                    return Ternary::Related;
                }
                let left_numeric = self.assignable_to_number_or_bigint(left);
                let right_numeric = self.assignable_to_number_or_bigint(right);
                let both = ternary_and(left_numeric, right_numeric);
                if both == Ternary::Related {
                    return both;
                }
                let neither = ternary_and(ternary_not(left_numeric), ternary_not(right_numeric));
                if neither == Ternary::NotRelated {
                    return ternary_or(both, neither);
                }
                let comparable = ternary_or(
                    self.relate_ternary(left, right, Relation::Comparable),
                    self.relate_ternary(right, left, Relation::Comparable),
                );
                ternary_or(both, ternary_and(neither, comparable))
            }
        }
    }

    /// `IsTypeAny` (`checker.go`): `flags & Any`. This port's error type
    /// carries `ANY` as upstream's does; an unresolved-reference mint answers
    /// [`Checker::is_error`] without being that intrinsic and is any-like for
    /// the same reason.
    pub(crate) fn is_type_any(&self, id: TypeId) -> bool {
        self.is_error(id) || self.type_of(id).flags.intersects(TypeFlags::ANY)
    }

    /// `isTypeAssignableTo(t, numberOrBigIntType)`. A number, bigint (or
    /// their literals) or any-flagged source is `isSimpleTypeRelatedTo`'s
    /// first answer (`relater.go`), asked here before building the union so
    /// the hot arithmetic operand does not pay for the relation walk.
    fn assignable_to_number_or_bigint(&mut self, ty: TypeId) -> Ternary {
        if self.type_of(ty).flags.intersects(
            TypeFlags::NUMBER
                | TypeFlags::NUMBER_LITERAL
                | TypeFlags::BIG_INT
                | TypeFlags::BIG_INT_LITERAL
                | TypeFlags::ANY,
        ) {
            return Ternary::Related;
        }
        let number_or_bigint = self.number_or_bigint_type();
        self.relate_ternary(ty, number_or_bigint, Relation::Assignable)
    }

    /// `numberOrBigIntType` (`checker.go:1012`).
    fn number_or_bigint_type(&mut self) -> TypeId {
        let (number, bigint) = (self.intrinsics.number, self.intrinsics.bigint);
        self.get_union_type(&[number, bigint])
    }

    /// `getBaseTypeOfLiteralTypeForComparison` (`checker.go:25454`): like
    /// `getBaseTypeOfLiteralType`, but an enum or enum literal reads as the
    /// primitive it holds rather than as its enum base type.
    pub(crate) fn get_base_type_of_literal_type_for_comparison(&mut self, id: TypeId) -> TypeId {
        let flags = self.type_of(id).flags;
        if flags.intersects(
            TypeFlags::STRING_LITERAL | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING,
        ) {
            return self.intrinsics.string;
        }
        if flags.intersects(TypeFlags::NUMBER_LITERAL | TypeFlags::ENUM) {
            return self.intrinsics.number;
        }
        if flags.intersects(TypeFlags::BIG_INT_LITERAL) {
            return self.intrinsics.bigint;
        }
        if flags.intersects(TypeFlags::BOOLEAN_LITERAL) {
            return self.intrinsics.boolean;
        }
        if flags.intersects(TypeFlags::UNION)
            && let crate::types::TypeData::Union { types, .. } = &self.store.get(id).data
        {
            let constituents = types.clone();
            let mapped: Vec<TypeId> = constituents
                .into_iter()
                .map(|constituent| self.get_base_type_of_literal_type_for_comparison(constituent))
                .collect();
            return self.get_union_type(&mapped);
        }
        id
    }

    /// `checkForDisallowedESSymbolOperand` (`checker.go:12812`): TS2469 on the
    /// first operand that may be a symbol, considering its base constraint.
    /// Answers `true` when there was no error.
    pub(crate) fn check_for_disallowed_es_symbol_operand(
        &mut self,
        left: tsr_ast::Expression<'_>,
        right: tsr_ast::Expression<'_>,
        left_type: TypeId,
        right_type: TypeId,
        operator: SyntaxKind,
    ) -> bool {
        let offending = if self
            .maybe_type_of_kind_considering_base_constraint(left_type, TypeFlags::ES_SYMBOL_LIKE)
        {
            left
        } else if self
            .maybe_type_of_kind_considering_base_constraint(right_type, TypeFlags::ES_SYMBOL_LIKE)
        {
            right
        } else {
            return true;
        };
        if let Some(at) = offending.node_id()
            && let Some(file) = self.source_file_of_for_diagnostics(at)
        {
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THE_0_OPERATOR_CANNOT_BE_APPLIED_TO_TYPE_SYMBOL,
                    span,
                    [token_text(operator).to_string()],
                ),
            );
        }
        false
    }

    /// `maybeTypeOfKindConsideringBaseConstraint` (`checker.go:27620`).
    fn maybe_type_of_kind_considering_base_constraint(
        &mut self,
        id: TypeId,
        kind: TypeFlags,
    ) -> bool {
        if self.maybe_type_of_kind(id, kind) {
            return true;
        }
        let base = self.base_constraint_or_type(id);
        self.maybe_type_of_kind(base, kind)
    }

    /// `reportOperatorError` (`checker.go:12718`), the TS2365 branch. The
    /// equality operators' TS2367 branch is `crate::comparison_overlap`.
    ///
    /// The await suggestion changes only the printed pair here: upstream's
    /// related "Did you forget to use 'await'?" information is not part of
    /// the code/position this port reports.
    pub(crate) fn report_operator_error(
        &mut self,
        left: TypeId,
        operator: SyntaxKind,
        right: TypeId,
        error_node: NodeId,
        relation: OperatorRelation,
    ) {
        let has_relation = !matches!(relation, OperatorRelation::None);
        let mut would_work_with_await = false;
        if has_relation {
            let awaited_left = self.awaited_type_no_alias(left);
            let awaited_right = self.awaited_type_no_alias(right);
            if let (Some(awaited_left), Some(awaited_right)) = (awaited_left, awaited_right)
                && !(awaited_left == left && awaited_right == right)
            {
                would_work_with_await =
                    self.operator_types_related(relation, awaited_left, awaited_right)
                        == Ternary::Related;
            }
        }
        let (mut effective_left, mut effective_right) = (left, right);
        if has_relation && !would_work_with_await {
            // `getBaseTypesIfUnrelated` (`checker.go:12745`).
            let left_base = self.get_base_type_of_literal_type(left);
            let right_base = self.get_base_type_of_literal_type(right);
            if self.operator_types_related(relation, left_base, right_base) == Ternary::NotRelated {
                effective_left = left_base;
                effective_right = right_base;
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(error_node) else { return };
        let span = self.error_span(error_node);
        let left_text = self.type_to_string(effective_left);
        let right_text = self.type_to_string(effective_right);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::OPERATOR_0_CANNOT_BE_APPLIED_TO_TYPES_1_AND_2,
                span,
                [token_text(operator).to_string(), left_text, right_text],
            ),
        );
    }
}

impl Checker<'_, '_> {
    /// The two operand rules of the equality arm of
    /// `checkBinaryLikeExpressionWorker` (`checker.go:12479`) that precede
    /// `reportOperatorErrorUnless`: TS2839 for an object, array, regex,
    /// function or class literal operand, then `checkNaNEquality` (TS2845).
    /// The comparability report (TS2367) that follows them is
    /// `crate::comparison_overlap`'s.
    fn check_equality_operator(
        &mut self,
        node: NodeId,
        operator: SyntaxKind,
        left: tsr_ast::Expression<'_>,
        right: tsr_ast::Expression<'_>,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let strict = matches!(
            operator,
            SyntaxKind::EqualsEqualsEqualsToken | SyntaxKind::ExclamationEqualsEqualsToken
        );
        let equality =
            matches!(operator, SyntaxKind::EqualsEqualsToken | SyntaxKind::EqualsEqualsEqualsToken);
        let always = if equality { "false" } else { "true" };
        // `isLiteralExpressionOfObject` (`utilities.go:1064`) — the operand
        // as written, parentheses not skipped; only `===`/`!==` in JS.
        let literal_of_object = |operand: tsr_ast::Expression<'_>| {
            matches!(
                operand,
                tsr_ast::Expression::ObjectLiteralExpression(_)
                    | tsr_ast::Expression::ArrayLiteralExpression(_)
                    | tsr_ast::Expression::RegularExpressionLiteral(_)
                    | tsr_ast::Expression::FunctionExpression(_)
                    | tsr_ast::Expression::ClassExpression(_)
            )
        };
        if (literal_of_object(left) || literal_of_object(right))
            && (!self.in_js_file(node) || strict)
        {
            let span = self.error_span(node);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_0_SINCE_JAVASCRIPT_COMPARES_OBJECTS_BY_REFERENCE_NOT_VALUE,
                    span,
                    [always.to_string()],
                ),
            );
        }
        // `checkNaNEquality` (`checker.go:12827`). The `Did you mean
        // 'Number.isNaN(…)'?` related information is not part of the
        // code/position this port reports.
        if self.is_global_nan(left) || self.is_global_nan(right) {
            let span = self.error_span(node);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_0,
                    span,
                    [always.to_string()],
                ),
            );
        }
    }

    /// `isGlobalNaN` (`checker.go:12853`): an identifier `NaN` (parentheses
    /// skipped) that resolves to the global `NaN` value symbol.
    fn is_global_nan(&mut self, expression: tsr_ast::Expression<'_>) -> bool {
        let mut expression = expression;
        while let tsr_ast::Expression::ParenthesizedExpression(parenthesized) = expression {
            let Some(inner) = parenthesized.expression else { return false };
            expression = inner;
        }
        let tsr_ast::Expression::Identifier(identifier) = expression else { return false };
        if identifier.text != "NaN" {
            return false;
        }
        let Some(id) = identifier.node_id else { return false };
        let Some(global) = self.binder.globals().get("NaN").copied() else { return false };
        if !self.binder.symbols().get(global).flags.intersects(SymbolFlags::VALUE) {
            return false;
        }
        self.binder
            .resolve_name(self.nodes, self.node_map, id, identifier.text, SymbolFlags::VALUE)
            .is_some_and(|resolved| {
                self.binder.merged_symbol(resolved) == self.binder.merged_symbol(global)
            })
    }
}

impl Checker<'_, '_> {
    /// TS2731 from `checkTemplateExpression` (`checker.go:7976`): a span
    /// expression that may be a symbol, considering its base constraint, is
    /// an implicit string conversion that throws at runtime. Reported on the
    /// span's expression. Kept with the operator rules because it is the
    /// template form of `+`'s `checkForDisallowedESSymbolOperand`
    /// (`docs/parity/notes/operators.md` §7).
    pub(crate) fn check_template_span_symbol_conversion(&mut self, span: NodeId) {
        let Some(Node::TemplateSpan(template_span)) = self.node_map.get(span) else { return };
        // A tagged template's spans are arguments, checked by call resolution
        // (`checkTaggedTemplateExpression`, `checker.go:10034`) rather than by
        // `checkTemplateExpression`.
        let Some(template) = self.nodes.parent(span) else { return };
        if self
            .nodes
            .parent(template)
            .is_some_and(|tag| self.nodes.kind(tag) == SyntaxKind::TaggedTemplateExpression)
        {
            return;
        }
        let Some(expression) = template_span.expression else { return };
        let Some(at) = expression.node_id() else { return };
        let ty = self.check_expression(expression);
        if !self.maybe_type_of_kind_considering_base_constraint(ty, TypeFlags::ES_SYMBOL_LIKE) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::new(
                &messages::IMPLICIT_CONVERSION_OF_A_SYMBOL_TO_A_STRING_WILL_FAIL_AT_RUNTIME_CONSIDER_WRAPPING_THIS_EXPRESSION_IN_STRING,
                span,
            ),
        );
    }
}

/// `getSuggestedBooleanOperator` (`checker.go:12780`).
fn suggested_boolean_operator(operator: SyntaxKind) -> Option<SyntaxKind> {
    match operator {
        SyntaxKind::BarToken | SyntaxKind::BarEqualsToken => Some(SyntaxKind::BarBarToken),
        SyntaxKind::CaretToken | SyntaxKind::CaretEqualsToken => {
            Some(SyntaxKind::ExclamationEqualsEqualsToken)
        }
        SyntaxKind::AmpersandToken | SyntaxKind::AmpersandEqualsToken => {
            Some(SyntaxKind::AmpersandAmpersandToken)
        }
        _ => None,
    }
}

/// `scanner.TokenToString` for the binary and unary operator tokens.
pub(crate) fn token_text(operator: SyntaxKind) -> &'static str {
    match operator {
        SyntaxKind::PlusToken => "+",
        SyntaxKind::PlusEqualsToken => "+=",
        SyntaxKind::MinusToken => "-",
        SyntaxKind::MinusEqualsToken => "-=",
        SyntaxKind::AsteriskToken => "*",
        SyntaxKind::AsteriskEqualsToken => "*=",
        SyntaxKind::AsteriskAsteriskToken => "**",
        SyntaxKind::AsteriskAsteriskEqualsToken => "**=",
        SyntaxKind::SlashToken => "/",
        SyntaxKind::SlashEqualsToken => "/=",
        SyntaxKind::PercentToken => "%",
        SyntaxKind::PercentEqualsToken => "%=",
        SyntaxKind::LessThanLessThanToken => "<<",
        SyntaxKind::LessThanLessThanEqualsToken => "<<=",
        SyntaxKind::GreaterThanGreaterThanToken => ">>",
        SyntaxKind::GreaterThanGreaterThanEqualsToken => ">>=",
        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => ">>>",
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => ">>>=",
        SyntaxKind::AmpersandToken => "&",
        SyntaxKind::AmpersandEqualsToken => "&=",
        SyntaxKind::BarToken => "|",
        SyntaxKind::BarEqualsToken => "|=",
        SyntaxKind::CaretToken => "^",
        SyntaxKind::CaretEqualsToken => "^=",
        SyntaxKind::LessThanToken => "<",
        SyntaxKind::GreaterThanToken => ">",
        SyntaxKind::LessThanEqualsToken => "<=",
        SyntaxKind::GreaterThanEqualsToken => ">=",
        SyntaxKind::PlusPlusToken => "++",
        SyntaxKind::MinusMinusToken => "--",
        SyntaxKind::TildeToken => "~",
        SyntaxKind::ExclamationToken => "!",
        SyntaxKind::AmpersandAmpersandToken => "&&",
        SyntaxKind::BarBarToken => "||",
        SyntaxKind::QuestionQuestionToken => "??",
        SyntaxKind::ExclamationEqualsEqualsToken => "!==",
        _ => "",
    }
}
