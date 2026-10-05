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
