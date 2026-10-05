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
