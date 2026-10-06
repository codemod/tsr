# Lane notes: operators (`tsr-2zk.12`)

Binary and unary operator checks — `checkBinaryLikeExpressionWorker`,
`checkPrefixUnaryExpression`, `checkPostfixUnaryExpression`,
`checkArithmeticOperandType`, `checkNonNullType`, `reportOperatorError`
(`vendor/typescript-go/internal/checker/checker.go` @ `5b1047d`).

## 1. The relational arm is a port, not a substitute (TS2365, TS2469)

**Forcing constraint.** At `06f25e0` the lane's TS2365 was both missing (114)
and extra (153). The extras were all the relational operators on object
operands — `comparisonOperatorWithNoRelationshipObjectsOn*Signature` alone were
128 of them — because `relational_operands_are_incomparable` substituted
*assignability* in both directions for upstream's `areTypesComparable`, and
`{ fn<T>(x: T): T }` is comparable to `{ fn(): string }` (comparable erases
generics, `relater.go:4492`) without being assignable. The missing ones were a
blanket "either side is a type parameter → silence" gate, written when the
relater could not follow a type parameter's constraint.

**What upstream does** (`checker.go:12460`):

```go
if c.checkForDisallowedESSymbolOperand(left, right, leftType, rightType, operator) {
    leftType = c.getBaseTypeOfLiteralTypeForComparison(c.checkNonNullType(leftType, left))
    rightType = c.getBaseTypeOfLiteralTypeForComparison(c.checkNonNullType(rightType, right))
    c.reportOperatorErrorUnless(leftType, operator, rightType, errorNode, func(left, right *Type) bool {
        if IsTypeAny(left) || IsTypeAny(right) { return true }
        l := c.isTypeAssignableTo(left, c.numberOrBigIntType)
        r := c.isTypeAssignableTo(right, c.numberOrBigIntType)
        return l && r || !l && !r && c.areTypesComparable(left, right)
    })
}
```

`check_relational_operator` (`operator_operands.rs`) is that, line for line:
TS2469 first (`maybeTypeOfKindConsideringBaseConstraint`, via the base
constraint), then `checkNonNullType` with its reporter
(`check_non_null_type_reporting`, which took the relational operators over from
`check_nullable_operand`), then `getBaseTypeOfLiteralTypeForComparison` (enum →
`number`, unlike `getBaseTypeOfLiteralType`), then the closure against
`Relation::Assignable` and `Relation::Comparable`. `reportOperatorError` is
ported with its await probe and `getBaseTypesIfUnrelated` printing.

**The one deviation: three-valued logic.** The closure is evaluated in Kleene
logic over `Ternary` (`ternary_and/or/not`), and TS2365 is reported only when
it is a confident `NotRelated`. The relater still answers `Unknown` for pairs
it cannot decide, and an `Unknown` read as `false` would manufacture a
diagnostic. Consequence accepted: where the relater says `Unknown` and upstream
says `false`, the diagnostic stays missing — `t < u` with two unconstrained
type parameters is one (the comparable carve-out at `relater.go:3435` is not
reached by this port's relater). The removed gates (type-parameter veto, enum
veto, composite decline, `either_is_composite`) are no longer needed because
the relation now answers what they guessed at.

**Measured** (full corpus, against `9c1e3c2`): diagnostics +10 cases
(8 WRONG→RIGHT, 2 EMPTY_WRONG→EMPTY_RIGHT), zero losses in either dump; lane
EXTRA TS2365 153 → 1, MISSING TS2365 114 → 82, MISSING TS2469 31 → 21.

## 2. TS18046 / TS2571 are not reported yet — the owner is inference

`checkNonNullType` reports `'x' is of type 'unknown'` for an `unknown` operand
under `strictNullChecks`. Wiring it measured two EMPTY_RIGHT → EMPTY_WRONG
losses: `compiler/mapGroupBy` and `compiler/nonInferrableTypePropagation2`.
Both are the same defect outside this lane: a context-sensitive arrow argument
of a generic call (`declare function gb<K, T>(items: Iterable<T>, f: (item: T)
=> K): …; gb([0], x => x < 5)`) has its parameter read as `unknown` by the
diagnostics walk, while the type dump answers `number` for `mapGroupBy` — the
`node_types` entry for `x` is written during an inference pass with `T`
unfixed (`expressions.rs` `check_expression` cache). `nonInferrableType
Propagation2` is already a WRONG `unknown` in the type dump.

So `check_non_null_type_reporting` answers `errorType` for `unknown` (which is
upstream's *type* answer, and silences TS2365 as upstream's `IsTypeAny` does)
and reports nothing. **Falsifier / reopening condition:** when the calls /
inference owner stops publishing the speculative parameter type, wiring the
report should convert the lane's TS18046 lines (17 missing) with no loss.

## 3. The arithmetic arm is a port (TS2362, TS2363, TS2447, TS2365, TS18050)

**Forcing constraint.** At `590ac64` the lane missed 83 TS2362 and 48 TS2363.
Two causes, neither a logic gap in the rule: the walk's numeric dispatch
(`is_numeric_binary_operator`, `check.rs`) admitted only four of the eleven
compound arithmetic operators, so `**=`, `<<=`, `>>=`, `>>>=`, `&=`, `|=`, `^=`
never reached `checkArithmeticOperandType`; and the rule replaced
`!isTypeAssignableTo(t, numberOrBigIntType)` with a hand-built
"definitely not numeric" predicate (flag tests, a syntactic "declared as an
unconstrained type parameter" lookup, composite/enum declines).

**What changed.** `check_arithmetic_operand_types` is now
`checker.go:12358` in order: `checkNonNullType` with its reporter on each side,
TS2447 on the operator token for two boolean-like operands (`return`, so no
operand check and no `checkAssignmentOperator`), `checkArithmeticOperandType`
per operand against `numberOrBigIntType`, then the result cascade —
`number` when both are any/unknown or neither may be bigint-like; `bigint`
when `bothAreBigIntLike` (`>>>` is TS2365 with no related closure); otherwise
`reportOperatorError(…, bothAreBigIntLike)`. `leftOk && rightOk` gates
`checkAssignmentOperator` (TS2364) as before. `checkIdentifier`'s
`errorType` for a non-variable assignment target is modelled as an error
*left type* — the right operand is still checked, which the old early return
skipped. The dispatch now takes all eleven compound forms, and the numeric arm
also runs the assignment arm's `check_private_accessor_is_writable` for them.

Not ported: TS2791 (`**` on bigint below ES2016) — the checker does not hold
`target`, and adding a field is outside this lane's files; the `>= 32` shift
suggestion (an error only inside an enum member).

**Perf.** The operand relation is preceded by `isSimpleTypeRelatedTo`'s own
first answer (number/bigint/literal/any source → related) so a plain numeric
operand does not build `number | bigint`. The harness self-ratio on
domain-model read 1.039–1.05 at 9/21 samples, but identical binaries read
0.975–1.016 and the unchanged binary 0.994–1.05 in the same hour; 25
alternating direct runs gave base median 0.1654 s, new 0.1631 s.

**Measured** (against `590ac64`): diagnostics +8 cases (WRONG→RIGHT), zero
losses; lane MISSING TS2362 83 → 0, TS2363 48 → 0, TS18050 39 → 19, TS2447
6 → 0; EXTRA TS2364 18 → 0.

## 4. Prefix and postfix operators are one port (TS2356, TS2357, TS2469, TS2736)

`check_unary_operator_operands` replaces `check_increment_operand_type` and the
prefix half of `check_nullable_operand`, following `checkPrefixUnaryExpression`
(`checker.go:10855`) and `checkPostfixUnaryExpression` (`:10914`): a numeric
literal under `-`/`+` (bigint literal under `-`) returns before any check;
`+`/`-`/`~` run `checkNonNullType` with its reporter, TS2469 for a maybe-symbol
operand (base constraint considered) and, for `+`, TS2736 for a
maybe-bigint operand printed through `getBaseTypeOfLiteralType`; `++`/`--`
run `checkArithmeticOperandType` (TS2356) on the non-null type and gate
`checkReferenceExpression` (TS2357) on its `ok`.

Removed with it: the `file_has_parse_errors` bail (upstream has none), the
"operand names a non-variable" early return (that operand's `checkIdentifier`
answers `errorType`, which the relation already treats as assignable), and the
flag-based `operand_is_definitely_not_numeric`. The last caller of
`either_is_composite` (`assertion_overlap.rs`) went with commit 1's relational
port; the dead helper is deleted so `-D warnings` stays clean.

**Measured** (against `69e9297`): +1 case (`stringLiteralTypesWithVariousOperators02`),
zero losses; lane MISSING TS2356 31 → 0, EXTRA TS2357 4 → 0, MISSING TS2469
21 → 14. Direct alternating timing (21 runs): domain-model 0.991, generic-imports
0.960 (median new/base).

## 5. The `+` / `+=` arm is ported and held: `operators-plus.diff`

**What the port is.** `checkBinaryLikeExpressionWorker`'s `+` arm
(`checker.go:12403`) in order: `checkNonNullType` with its reporter on both
operands when neither is `isTypeAssignableToKind(StringLike)` (not strict);
the result cascade (both `NumberLike` strict, both `BigIntLike` strict, either
`StringLike` strict, either `IsTypeAny`); with a result,
`checkForDisallowedESSymbolOperand` (TS2469); without one,
`reportOperatorError` with the close-enough closure (`OperatorRelation::
CloseEnough`) and no `checkAssignmentOperator`. `+=` joins the walk's numeric
dispatch. It replaces the flag-built `addition_operands_have_no_result`, the
type-parameter veto, `operand_is_nullish`, and `nullable_operand.rs`'s
`check_nullable_operand` (whose `+`-only TS18050 path is the reporter the
other arms already use). Each kind test is a `Ternary`; an `Unknown` step
stops the arm without a diagnostic, as §1.

**Measured** (against `15f1743`): diagnostics +9 cases WRONG→RIGHT
(`additionOperatorWithInvalidOperands`, `additionOperatorWithTypeParameter`,
`compoundAdditionAssignmentWithInvalidOperands`, `compoundAdditionAssignment
LHSCanBeAssigned`, `arithmeticOnInvalidTypes2`, `expr`,
`noUncheckedIndexedAccessCompoundAssignments`, `symbolType6`, `symbolType12`);
lane MISSING TS2365 75 → 25, TS2469 14 → 0, TS18050 27 → 17. **One loss:**
`conformance/genericRestParameters1` RIGHT → WRONG, an extra TS2365 at
`(125,39)`.

**The loss is a producer outside this lane.** The line is
`f30(42, x => "" + x, x => x + 1)` with
`declare function f30<T, U extends ((x: T) => any)[]>(x: T, ...args: U): U`.
Upstream infers `T = number` and types `x` as `number`. In TSR the call's
rest-tuple inference from context-sensitive arguments is unported — the type
dump already answers `[error, error]` for `c30` and `any` for each `x` — and
the walk then contextually types the arrow through
`contextual_type_for_argument_resolving`'s single-generic-candidate road
(`contextual.rs`, "the arrow ADOPTS the type parameters"), so `x` reads as
the uninstantiated `T`. `T + number` is a correct TS2365 for that type; the
type is wrong. Probes: `h<T>(x: T, f: (x: T) => any)` and
`k<T, U extends (x: T) => any>` give `number`; only the rest-parameter shapes
`U extends ((x: T) => any)[]` and `U extends [(x: T) => any]` give `T`. The
relational arm already shows the same (`g(42, x => x < 1)` is TS2365 on `T`);
no corpus case reaches it.

**Reopening condition.** When the calls/contextual owner types that arrow's
parameter as `number` (or as `any`, which silences TS2365 the way upstream's
`IsTypeAny` does), `git apply docs/parity/notes/operators-plus.diff` should
measure +10 with zero losses. Not shipped now because a loss is never
accepted (`box-protocol.md` §5).

## 6. The equality arm's literal and NaN rules (TS2839, TS2845)

`check_equality_operator` (`operator_operands.rs`) is the two rules of the
equality arm (`checker.go:12479`) that precede `reportOperatorErrorUnless`:
TS2839 when either operand, as written, is `isLiteralExpressionOfObject`
(object, array, regex, function or class literal; parentheses not skipped),
in JS only for `===`/`!==`; then `checkNaNEquality` (`checker.go:12827`),
TS2845 when an operand with parentheses skipped is an identifier `NaN`
resolving to the global `NaN` value symbol (`isGlobalNaN`, via
`resolve_name` against `globals()["NaN"]`, the pattern `calls.rs` uses for
`Promise`). Both report on the binary expression with `'false'` for
`==`/`===` and `'true'` otherwise. The comparability report (TS2367) that
follows them stays `crate::comparison_overlap`'s. The `Did you mean
'Number.isNaN(…)'?` related information is not modelled, as nowhere in this
port's reporter.

They live in this lane's file rather than `comparison_overlap.rs` (the flow
box's) because they are independent rules of the same arm, not part of its
comparability decision; they share no state with it. Upstream skips all three
under `CheckModeTypeOnly`; the diagnostics walk is never that mode.

**Measured** (against `15f1743`): diagnostics +5 cases WRONG→RIGHT
(`conditionalEqualityOnLiteralObjects`, `nanEquality`, `narrowByEquality`,
`functionImplementations`, `plainJSTypeErrors`), zero losses in either dump;
MISSING TS2839 28 → 0, TS2845 17 → 0.

