//! The binary operators.
//!
//! Ported from `Checker.checkBinaryExpression` / `checkBinaryLikeExpression`
//! (`checker.go:12331`, `:12336`). Split out of [`crate::checker`] because
//! upstream's worker is a dozen unrelated rules behind one node kind and they
//! land at different times — `||` and `??` are still gaps and belong to
//! assignability (`bd tsr-5s2`).

use tsr_ast::{Expression, SyntaxKind};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    flow::TypeFacts,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// Ported from `Checker.checkBinaryExpression` / `checkBinaryLikeExpression`
    /// (`checker.go:12331`, `:12336`).
    ///
    /// # What is here and what is not
    ///
    /// Upstream's worker is a dozen unrelated rules behind one node kind, and
    /// they do not become available at the same time. Ported: assignment, the
    /// arithmetic/bitwise/shift family, `+`, the relational and equality
    /// families, `in`, `instanceof`, the comma operator, and `&&`
    /// ([`Checker::check_logical_and`]). Not ported: `||`, `??` and their
    /// compound forms — which need `UnionReductionSubtype` and therefore
    /// assignability, see the arm below (`bd tsr-5s2`) — and destructuring
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
            // §365: the test became load-bearing exactly as predicted above —
            // literals are checked now, so the fall-through would mistype the
            // pattern. `checkDestructuringAssignment` (checker.go:12683)
            // checks the pattern against the right type and ANSWERS THE RIGHT
            // TYPE: `[a, b] = new FooIterator` is `FooIterator`
            // (`iterableArrayPattern3`). The pattern's own line comes from
            // the walker visiting the literal, which the array road answers
            // as a tuple in this position.
            let right_type = self.check_expression(right);
            if right_type == error {
                return error;
            }
            return right_type;
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

            SyntaxKind::AmpersandAmpersandToken => {
                self.check_logical_and(left_type, right_type)
            }

            // `||` and `??` were refused whole on `UnionReductionSubtype`
            // (`bd tsr-5s2`); `checker-notes-assign.md` §7–§8 re-measured the
            // ground — the reduction question is a property of the constituent
            // PAIR — and the reduction-free slice ships. The remaining pairs
            // keep declining inside [`Checker::check_logical_or_coalescing`].
            //
            // The compound forms `&&=`, `||=` and `??=` additionally reach
            // `checkAssignmentOperator`, and are left with the family they
            // belong to rather than split off for the sake of three lines
            // corpus-wide.
            SyntaxKind::BarBarToken => {
                self.check_logical_or_coalescing(left_type, right_type, false)
            }
            SyntaxKind::QuestionQuestionToken => {
                self.check_logical_or_coalescing(left_type, right_type, true)
            }
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
    /// # ~~A deliberate deviation: a gap in is a gap out~~ RETIRED — §271
    ///
    /// This arm used to propagate an `errorType` operand instead of taking
    /// upstream's any-like → `number` rule, on the argument that this port's
    /// `errorType` also means "unported form" and `number` would be a claim.
    /// The deviation carried its own falsifier: *"if a measurable population
    /// of lines still reaches here with an `errorType` operand, this should
    /// return `number` as upstream does."*
    ///
    /// §269 fired it — `@param`-typed receivers made `u.a - u.b.length`
    /// reach here with error operands where upstream computes `number`
    /// (`jsdocTemplateTag3`) — and the §271 measurement settled it: **31
    /// lines right (12 GAP→RIGHT, 19 WRONG→RIGHT), zero adverse in any
    /// direction.** The feared population — a gap turning into a wrong
    /// confident `number` — measured EMPTY: every line the corpus routes
    /// here with an error operand is one upstream also answers `number` on.
    /// The `+` arm is different (upstream itself yields errorType there,
    /// `checker.go:12452`) and keeps its propagation.
    fn check_arithmetic_operation(&mut self, left: TypeId, right: TypeId) -> TypeId {
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
            // §313's tail — upstream's ORDER, not a deviation: the string
            // test precedes the any/error fallthrough
            // (`checker.go:12430-12437`), so an error-typed operand beside a
            // string-like one is STRING — `f += ''` on a class-named target
            // records `>f += '' : string` beside `>f : any`
            // (`concatClassAndString`, `arithAssignTyping`). Only a
            // string-free pair keeps the error.
            let other = if self.is_error(left) { right } else { left };
            let other_is_string_like = !self.is_error(other)
                && (self.store.get(other).flags.intersects(TypeFlags::STRING_LIKE)
                    || matches!(
                        &self.store.get(other).data,
                        crate::types::TypeData::Union { types, .. }
                            if !types.is_empty() && types.iter().all(|&c| {
                                self.store.get(c).flags.intersects(TypeFlags::STRING_LIKE)
                            })
                    ));
            if other_is_string_like {
                return self.intrinsics.string;
            }
            return self.intrinsics.error;
        }
        // §179 (`checker-notes-narrow.md`): upstream's `+` arm asks
        // `isTypeAssignableToKind`, not `flags & Kind`, and assignability
        // consults a type PARAMETER's constraint — `n + 1` for
        // `n: T extends number` is `number`. This port tests raw flags, and
        // a type parameter carries none, so such an operand fell through to
        // the error tail. Only TYPE PARAMETERS are mapped to their
        // constraint here: §178 measured that reading EVERY operand through
        // its apparent type costs 4,400 lines, because a primitive's
        // apparent form is an interface that carries no kind flags either.
        let kind_source = |checker: &mut Self, id: TypeId| -> TypeId {
            if checker.store.get(id).flags.intersects(TypeFlags::TYPE_PARAMETER) {
                checker.type_parameter_constraint(id).unwrap_or(id)
            } else {
                id
            }
        };
        let (left, right) = (kind_source(self, left), kind_source(self, right));
        // §258. `isTypeAssignableToKind` descends a UNION — every constituent
        // must match the kind — where a raw flags test sees only the union's
        // own flags, which carry no kind at all. That is why `a + b` on two
        // enum-typed operands gapped: `Choice.Yes | Choice.No` is a `UNION`,
        // and `both(NUMBER_LIKE)` asked the wrong node.
        //
        //     enum Choice { Unknown, Yes, No }
        //     var a: Choice, b: Choice;
        //     var x = a + b;
        //     >a + b : number
        //
        // Witness `conformance/enumLiteralTypes1`, whose eleven blocked lines
        // §257 measured as the population its `any` fallback would have
        // answered WRONGLY — this computes them instead, which is the repair
        // that refusal named as its own reopening condition.
        //
        // Only unions are descended, and only one level. An intersection needs
        // ANY constituent to match rather than all, which is a different rule
        // and a different witness; it stays on flags until it has one.
        let has_kind = |checker: &mut Self, id: TypeId, kind: TypeFlags| -> bool {
            if checker.store.get(id).flags.intersects(kind) {
                return true;
            }
            if let crate::types::TypeData::Union { types, .. } = &checker.store.get(id).data {
                let constituents = types.clone();
                return !constituents.is_empty()
                    && constituents.iter().all(|&c| checker.store.get(c).flags.intersects(kind));
            }
            false
        };
        // §266 (renumbered from §265, which checker-2's resolve_name filter
        // took in the same hour): upstream's first move in the `+` arm
        // (`checker.go:12418`) —
        // when NEITHER operand is string-like, both are put through
        // `checkNonNullType` before classification. That is where
        // `(number | undefined) + 1` becomes `number`: the TS2532 report is
        // the diagnostic half, but the TYPE proceeds on the stripped operand.
        // Without this, §263's now-correct `number | undefined` element reads
        // turned their enclosing `+=` expressions from `number` to `any`.
        // String-likeness is tested on the UNSTRIPPED types, as upstream does.
        //
        // A strip that EMPTIES an operand keeps the original instead: upstream's
        // `checkNonNullType(undefinedType)` yields errorType, which *prints* as
        // `any` in a baseline, but this port's errorType means "unported" and is
        // scored as a gap — the first draft returned it and turned 16
        // previously-right lines into gaps/wrongs (`ANY + ANY1` under `!`,
        // `logicalNotOperatorWithAnyOtherType`), because the port's `!`
        // propagates a gap where upstream's returns `boolean` regardless. With
        // the original kept, an `any` operand still answers through the `any`
        // arm below, and a bare `undefined + 1` falls to the tail's honest
        // decline rather than a confident answer either way.
        let strip = |checker: &mut Self, id: TypeId| -> TypeId {
            let stripped = checker.check_non_null_type(id);
            if checker.is_error(stripped) { id } else { stripped }
        };
        let (left, right) = if !has_kind(self, left, TypeFlags::STRING_LIKE)
            && !has_kind(self, right, TypeFlags::STRING_LIKE)
        {
            (strip(self, left), strip(self, right))
        } else {
            (left, right)
        };
        let left_flags = self.store.get(left).flags;
        let right_flags = self.store.get(right).flags;
        let both = |kind: TypeFlags| left_flags.intersects(kind) && right_flags.intersects(kind);
        if both(TypeFlags::NUMBER_LIKE)
            || (has_kind(self, left, TypeFlags::NUMBER_LIKE)
                && has_kind(self, right, TypeFlags::NUMBER_LIKE))
        {
            return self.intrinsics.number;
        }
        // §260, and this is corollary 30 applied to §258 itself: §258 wired the
        // union descent into ONE of this function's three kind tests, because
        // `NUMBER_LIKE` was the one its witness needed. The other two ask the
        // same question of the same nodes and were left on raw flags — the
        // half-a-predicate shape, committed one commit after writing the rule
        // against it.
        if both(TypeFlags::BIG_INT_LIKE)
            || (has_kind(self, left, TypeFlags::BIG_INT_LIKE)
                && has_kind(self, right, TypeFlags::BIG_INT_LIKE))
        {
            return self.intrinsics.bigint;
        }
        if left_flags.intersects(TypeFlags::STRING_LIKE)
            || right_flags.intersects(TypeFlags::STRING_LIKE)
            || has_kind(self, left, TypeFlags::STRING_LIKE)
            || has_kind(self, right, TypeFlags::STRING_LIKE)
        {
            return self.intrinsics.string;
        }
        if left_flags.intersects(TypeFlags::ANY) || right_flags.intersects(TypeFlags::ANY) {
            return self.intrinsics.any;
        }
        // Upstream reports and answers `any` here; without diagnostics the honest
        // answer is that nothing was computed.
        //
        // §257, ATTEMPTED TWICE AND REVERTED — the conclusion holds and the
        // stated reason is not why. Upstream's line really is `return
        // c.anyType` (`checker.go:12455`), deliberate error recovery with its
        // own comment ("Otherwise, the result is of type Any"), so porting it
        // is NOT ADR-0038's forbidden gap-wearing-`any`. The premise about
        // diagnostics is beside the point. It still must not be ported, for a
        // reason only measurement gives:
        //
        //   whole fallback -> any     +4 cases   GAP->RIGHT 10  WRONG->RIGHT 12
        //                                        **GAP->WRONG 57**
        //   narrowed to non-literal   +0 cases   WRONG->RIGHT 12
        //                             operands   **GAP->WRONG 32**
        //
        // The adverse population is LITERAL and ENUM-LITERAL arithmetic
        // (`enumLiteralTypes1/2` 11 lines each, `numericLiteralTypes1/2`,
        // `stringLiteralTypesWithVariousOperators01`), where upstream computes
        // a real `number`/`string` and this port cannot yet. Every line that
        // reaches this fallback from that population is a gap THIS PORT OWNS,
        // and answering `any` replaces it with a confident wrong answer.
        //
        // So the `error` here is load-bearing, and what it is bearing is not
        // "we have no diagnostics" but **"the arithmetic above this line is
        // incomplete"**. It should be revisited when literal arithmetic lands —
        // at which point the adverse population stops reaching here at all and
        // the port becomes free — and not before. Narrowing by flags does not
        // rescue it: the second attempt kept 32 adverse lines and bought
        // nothing, because enum-literal operands do not carry the flags the
        // narrowing tested for.
        self.intrinsics.error
    }

    /// The `&&` arm of `checkBinaryLikeExpression` (`checker.go:12496`).
    ///
    /// ```go
    /// resultType := leftType
    /// if c.hasTypeFacts(leftType, TypeFactsTruthy) {
    ///     t := leftType
    ///     if !c.strictNullChecks { t = c.getBaseTypeOfLiteralType(rightType) }
    ///     resultType = c.getUnionType([]*Type{c.extractDefinitelyFalsyTypes(t), rightType})
    /// }
    /// return resultType
    /// ```
    ///
    /// # Why this arm lands and `||` and `??` do not
    ///
    /// This module previously recorded all three as one gap, on the grounds
    /// that `extractDefinitelyFalsyTypes` reaches `getTypeFacts`. **It does
    /// not**, and the correction is what separated them
    /// (`docs/architecture/checker-notes-armsplit.md` §3.1, `bd tsr-rmi`).
    /// Taken with `grep -n` on the declarations:
    ///
    /// - `extractDefinitelyFalsyTypes` (`checker.go:29110`) is
    ///   `mapType(t, getDefinitelyFalsyPartOfType)`, and
    ///   [`Checker::get_definitely_falsy_part_of_type`] (`checker.go:29114`) is
    ///   a pure switch on `TypeFlags` consulting no table at all.
    /// - It is `removeDefinitelyFalsyTypes` (`checker.go:29106`), the **`||`**
    ///   arm, that calls `hasTypeFacts(Truthy)` through `filterType`.
    /// - `&&` unions with plain `getUnionType`. `||` and `??` use
    ///   `getUnionTypeEx(…, UnionReductionSubtype, …)`, which is `removeSubtypes`
    ///   and needs assignability.
    ///
    /// The one facts bit `&&` does need — the gate — this port already had:
    /// [`Checker::get_type_facts`] has carried `TRUTHY`/`FALSY` since narrowing
    /// landed. It gained a union arm for this caller, because narrowing only
    /// ever asks it about a *constituent*.
    ///
    /// # Two branches of upstream's that are not written here
    ///
    /// The `!strictNullChecks` branch is dead: this crate assumes
    /// `strictNullChecks` **on** throughout (`crate::unions`,
    /// `crate::array_literals`). Writing it would be writing a line no corpus
    /// case can reach, which this project treats as worse than omitting it.
    ///
    /// **A gap in is a gap out**, as in [`Checker::check_arithmetic_operation`]
    /// and for the same reason: `errorType` carries `TypeFlags::ANY`, so
    /// upstream's `AnyOrUnknown` case in the falsy switch would map it to
    /// itself and `getUnionType` would answer `error` anyway — but only by
    /// accident of the flag. The test is written explicitly, on **identity**,
    /// so that a genuine `any` operand still answers `any` (56 corpus lines are
    /// `any && true`) while an unported form still gaps.
    /// The `||` and `??` arms (`checker.go:12509`, `:12519`), restricted to
    /// the union pairs whose reduction is decidable without assignability —
    /// `checker-notes-assign.md` §8.
    ///
    /// - `||`: a never-falsy left answers `left` unchanged; otherwise the
    ///   union of `nonNullable(removeDefinitelyFalsyTypes(left))` and the
    ///   right.
    /// - `??`: a never-nullish left answers `left`; otherwise the union of
    ///   `nonNullable(left)` and the right.
    ///
    /// The union ships only when the pair is reduction-agnostic: both sides
    /// reduction-free, or identical after freshness-stripping, or either side
    /// `any` (absorption). Everything else stays a gap — the surviving core of
    /// `bd tsr-5s2`'s refusal.
    fn check_logical_or_coalescing(
        &mut self,
        left: TypeId,
        right: TypeId,
        coalescing: bool,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if self.is_error(left) {
            return error;
        }
        // The non-strict half of every `Base*Facts` aggregate
        // (`checker.go:467` and siblings): with `strictNullChecks` off a
        // non-nullable type may still hold `undefined`/`null` at runtime, so
        // upstream adds `EQUndefined | EQNull | EQUndefinedOrNull | Falsy` to
        // each strict set. The delta is applied HERE, to the whole-operand
        // question only, and deliberately not inside `get_type_facts`: a
        // global application leaked into truthiness narrowing (`if (!x)` kept
        // a `"foo"` constituent because the literal had gained `FALSY`) and
        // lost 2 right lines — the §8 bar's leg 4, honoured by this
        // placement. The constituent-level filter below stays strict, which
        // keeps a falsy literal a falsy literal.
        let mut facts = self.get_type_facts(left);
        if !self.strict_null_checks {
            facts |= TypeFacts::FALSY | TypeFacts::EQ_UNDEFINED_OR_NULL;
        }
        if coalescing {
            if !facts.contains(TypeFacts::EQ_UNDEFINED_OR_NULL) {
                return left;
            }
        } else if !facts.contains(TypeFacts::FALSY) {
            return left;
        }
        if self.is_error(right) {
            return error;
        }
        let filtered = if coalescing {
            self.get_non_nullable_type(left)
        } else {
            let truthy = self.remove_definitely_falsy_types(left);
            self.get_non_nullable_type(truthy)
        };
        let pair = [filtered, right];
        let regular = [
            self.get_regular_type_of_literal_type(filtered),
            self.get_regular_type_of_literal_type(right),
        ];
        let any = self.intrinsics.any;
        if regular[0] == regular[1]
            || pair.contains(&any)
            || self.non_null_refinement_bases.get(&filtered).is_some_and(|&base| base == right)
        {
            return self.get_union_type(&pair);
        }
        // Other non-identical generic/unknown pairs still require the broader
        // subtype reducer. Admit identity and the known refinement relation
        // above before applying this existing capability boundary to the pair.
        let undecidable = TypeFlags::TYPE_PARAMETER | TypeFlags::UNKNOWN;
        if pair.iter().any(|&id| {
            self.store.get(id).flags.intersects(undecidable)
                || self.type_reference_targets.get(&id).is_some_and(|(_, arguments)| {
                    arguments.iter().any(|&a| self.store.get(a).flags.intersects(undecidable))
                })
        }) {
            return error;
        }
        if pair.iter().all(|&id| self.is_subtype_reduction_free(id)) {
            return self.get_union_type(&pair);
        }
        // The non-agnostic pairs run the decidability-gated `removeSubtypes`
        // (`checker-notes-assign.md` §9); an undecidable pair stays a gap.
        self.union_with_subtype_reduction(&pair).unwrap_or(error)
    }

    /// `removeDefinitelyFalsyTypes` (`checker.go:29106`) — `filterType` by
    /// `TypeFactsTruthy` per constituent.
    pub(crate) fn remove_definitely_falsy_types(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        let TypeData::Union { types, .. } = &ty.data else {
            return if self.get_type_facts(id).contains(TypeFacts::TRUTHY) {
                id
            } else {
                self.intrinsics.never
            };
        };
        let constituents = types.clone();
        let kept: Vec<TypeId> = constituents
            .into_iter()
            .filter(|&c| self.get_type_facts(c).contains(TypeFacts::TRUTHY))
            .collect();
        if kept.is_empty() {
            return self.intrinsics.never;
        }
        self.get_union_type(&kept)
    }

    fn check_logical_and(&mut self, left: TypeId, right: TypeId) -> TypeId {
        if self.is_error(left) {
            return self.intrinsics.error;
        }
        if !self.get_type_facts(left).contains(TypeFacts::TRUTHY) {
            // The left operand can never be truthy, so the right is never
            // evaluated and the result is the left type unchanged.
            return left;
        }
        if self.is_error(right) {
            return self.intrinsics.error;
        }
        // §54 (`checker-notes-narrow.md`): the falsy SOURCE splits on
        // strictness (`checker.go:12500`) — non-strict extracts from the
        // RIGHT's literal base, which is what makes `boolean && string`
        // answer `string` there.
        let source =
            if self.strict_null_checks { left } else { self.get_base_type_of_literal_type(right) };
        let falsy = self.extract_definitely_falsy_types(source);
        self.get_union_type(&[falsy, right])
    }

    /// `Checker.extractDefinitelyFalsyTypes` (`checker.go:29110`) —
    /// `mapType(t, getDefinitelyFalsyPartOfType)`.
    ///
    /// `mapType` (`checker.go:25561`) returns `never` unchanged, applies the
    /// function directly to a non-union, and otherwise maps each constituent
    /// and re-unions. The `origin` branch of `mapTypeEx` (`checker.go:25574`)
    /// is not ported: this port builds no denormalised union origins — the one
    /// place upstream would want one, `union_type_worker` answers `errorType`
    /// instead (`crate::unions`), so there is no origin to read.
    fn extract_definitely_falsy_types(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        if ty.flags.contains(TypeFlags::NEVER) {
            return id;
        }
        let TypeData::Union { types, .. } = &ty.data else {
            return self.get_definitely_falsy_part_of_type(id);
        };
        let constituents = types.clone();
        let mapped: Vec<TypeId> =
            constituents.into_iter().map(|c| self.get_definitely_falsy_part_of_type(c)).collect();
        self.get_union_type(&mapped)
    }

    /// `getDefinitelyFalsyPartOfType` (`checker.go:29114`).
    ///
    /// The falsy *value* a type can hold, or `never` when it can hold none.
    /// Upstream's order is kept: the three wide primitives first, then the
    /// types that are already definitely falsy and map to themselves.
    ///
    /// The zero literals are interned **regular**, not fresh — upstream's
    /// `emptyStringType`, `zeroType` and `zeroBigIntType` are created by
    /// `getStringLiteralType`/`getNumberLiteralType`, which produce the regular
    /// form. A fresh `0` here would print identically and compare unequal, and
    /// the first thing to notice would be a union failing to dedupe.
    fn get_definitely_falsy_part_of_type(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        let flags = ty.flags;
        if flags.contains(TypeFlags::STRING) {
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(String::new()),
                false,
            );
        }
        if flags.contains(TypeFlags::NUMBER) {
            return self.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral("0".to_owned()),
                false,
            );
        }
        if flags.contains(TypeFlags::BIG_INT) {
            return self.store.intern_literal(
                TypeFlags::BIG_INT_LITERAL,
                TypeData::BigIntLiteral("0".to_owned()),
                false,
            );
        }
        // The already-falsy set. `AnyOrUnknown` is upstream's and is why
        // `any && x` is `any`: `any`'s falsy part is `any`, and the union
        // absorbs the right operand.
        let definitely_falsy = flags
            .intersects(TypeFlags::VOID | TypeFlags::NULLABLE | TypeFlags::ANY_OR_UNKNOWN)
            || match &ty.data {
                TypeData::BooleanLiteral(value) => !*value,
                TypeData::StringLiteral(value)
                | TypeData::EnumLiteral {
                    value: crate::types::EnumLiteralValue::String(value),
                    ..
                } => value.is_empty(),
                TypeData::NumberLiteral(value)
                | TypeData::EnumLiteral {
                    value: crate::types::EnumLiteralValue::Number(value),
                    ..
                } => matches!(value.as_str(), "0" | "-0"),
                TypeData::BigIntLiteral(value) => value == "0",
                _ => false,
            };
        if definitely_falsy { id } else { self.intrinsics.never }
    }
}
