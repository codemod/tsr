# Lane notes: r4-operators (`tsr-2zk.906`)

Round-4 cloud lane continuing [operators.md](operators.md) (the round-1/2
lane's record, which stays as written). Native source is
`vendor/typescript-go/internal/checker/checker.go` @ `5b1047d`. Baselines
are this box's frozen dumps at `b23dd3d`.

## 1. `!x` reads `getTypeFacts` (`tsr-2zk.16.85`)

**Forcing constraint.** `negated_truthiness_type` (`expressions.rs`) was a
hand-built table: unit literals, nullable, unions of decidable constituents,
primitives → `boolean`, objects/symbols → `false`, and `error` for
everything else — type parameters, indexed accesses, intersections,
`never`, and an `error` operand. Upstream's arm (`checker.go:10887`) is

```go
facts := c.getTypeFacts(operandType, TypeFactsTruthy|TypeFactsFalsy)
switch {
case facts == TypeFactsTruthy: return c.falseType
case facts == TypeFactsFalsy:  return c.trueType
default:                       return c.booleanType
}
```

and `getTypeFacts` (`:30974`) resolves instantiable types through their base
constraint and folds unions/intersections. The flow lane already ports it as
`Checker::get_type_facts` (`flow.rs`), so the arm now asks that function
instead of re-deriving a subset of it. No new state; nothing is cached.

**Consequences accepted.**

- An `error` operand answers `boolean`. Upstream's `errorType` is an
  intrinsic `any`, whose `AnyFacts` carry both bits; the old `error` answer
  was a gap, not a decision. Measured: no line went GAP → WRONG.
- Loose mode is now native for objects: `ObjectFacts` carry `Falsy` without
  `strictNullChecks`, so `!obj` is `boolean` there (it was `false`). The
  unit test `ordinary_and_runtime_this_false_branches_preserve_negation_limitation`
  (`flow_object_facts_tests.rs`) pinned the old `false` with a comment saying
  native returns `boolean`; its assertion now states the native answer per
  mode. That test file is not this lane's; the one-line edit is the
  consequence of this port and is reported to the integrator.
- Where `get_type_facts` itself is imprecise, `!` inherits it. Its
  undecidable default is "every bit", which prints `boolean`.

`checkTruthinessOfType` (TS1345 on a `void` operand) is not part of this
change.

**Measured** (against `b23dd3d`): types +24 lines (20 WRONG → RIGHT,
4 GAP → RIGHT), +5 whole cases in the verdict dump; coverage
`checker_types` 8075 → 8079; diagnostics unchanged (4232); zero losses in
either dump. Perf (median child CPU, 21 samples, new/base): domain-model
0.997, generic-imports 1.017.

**Falsifier.** A corpus line where upstream prints `true`/`false` for `!x`
and TSR prints `boolean` points at `get_type_facts`, not at this arm.

## 2. The `+` / `+=` diagnostics arm ships (`tsr-2zk.906`)

[operators.md](operators.md) §5 held `operators-plus.diff` (the port of the
`+`/`+=` arm of `checkBinaryLikeExpressionWorker`, `checker.go:12414`) on one
loss, `genericRestParameters1`, whose producer was the rest-tuple contextual
typing of `f30(42, x => "" + x, …)`. Re-applied unchanged on `1e6d66b`, the
patch measures **+10 diagnostics cases, zero losses in either dump**: that
loss no longer occurs, so §5's reopening condition holds and the patch ships as written.
The design (Ternary kind tests, `Unknown` stops the arm without a
diagnostic) is §5's and is not repeated here.

**Cases converted** (WRONG → RIGHT): `arithmeticOnInvalidTypes2`, `expr`,
`noImplicitSymbolToString`, `noUncheckedIndexedAccessCompoundAssignments`,
`numberVsBigIntOperations`, `additionOperatorWithInvalidOperands`,
`additionOperatorWithTypeParameter`, `compoundAdditionAssignmentLHSCanBeAssigned`,
`compoundAdditionAssignmentWithInvalidOperands`, `symbolType6`.

**Measured:** coverage `diagnostics` 4232 → 4242, `checker_types` unchanged
(8079). Perf (median child CPU, 21 samples, new/base): domain-model 0.982,
generic-imports 1.018. The `check.rs` hunk is the walk's numeric dispatch
(`is_numeric_binary_operator` gains `+=`; the arm combines the two operand
checks), operator-specific code in a hub file.

## 3. The `+` arm's type cascade (`tsr-2zk.16.123`): ported, held on one loss

**What the port is** (`r4-operators-plus-type.diff`). `binary.rs`
`check_addition` becomes `checker.go:12414` in order: `checkNonNullType` on
both operands unless either `isTypeAssignableToKind(StringLike)`; then both
`isTypeAssignableToKindEx(NumberLike, strict)` → `number`, both
`BigIntLike` → `bigint`, either `StringLike` → `string`, either `IsTypeAny`
→ `errorType`/`any`, otherwise `any` (upstream's own "the result is of type
Any" after TS2365). The kind tests are
`operator_operands.rs` `is_type_assignable_to_kind` — flag test **or** full
assignability — replacing a flag test with one-level union descent and a
type-parameter-constraint read that declined `T[K]` and branded
intersections. Each test is a `Ternary`; `Unknown` returns `errorType` (a
gap) rather than a guessed answer, as §1 of [operators.md](operators.md).
The type half of `checkNonNullType` moves into `nullable_operand.rs`
`non_null_operand_type` (the reporting wrapper now calls it), so the
non-strict tail (`checker.go:7429`) is the same for type and diagnostic.

§257's old refusal of the `any` tail (`checker-notes` measured 57 GAP →
WRONG from incomplete literal arithmetic) no longer holds: with the full
kind test, no line went GAP → WRONG.

**Measured** (on `60d399b`, both patches applied): types +24 lines
(22 GAP → RIGHT, 2 WRONG → RIGHT), +8 whole cases
(`typeParameterExtendsPrimitive`, `additionOperatorWithConstrainedTypeParameter`,
`operatorsAndIntersectionTypes`, `plainJSRedeclare3`, `spellingUncheckedJS`,
`tsNoCheckForTypescript`, `tsNoCheckForTypescriptComments1`,
`tsNoCheckForTypescriptComments2`); diagnostics unchanged; perf CPU
new/base domain-model 0.996, generic-imports 0.965. **One loss**, below.

**Loss 1 — fixed by a hunk outside this lane** (`r4-operators-zero-literals.diff`,
`intrinsics.rs`). Without it, `numberVsBigIntOperations` lines 538–541 print
`1n | 0n` for upstream's `0n | 1n`. Upstream creates `emptyStringType`,
`zeroType` and `zeroBigIntType` at checker construction (`checker.go:1049`),
so `0n` precedes every program literal in type-id (union) order; TSR interns
`0n` lazily. The new relation `1n` → `number` interns the regular `1n` at
line 6, before anything interns `0n`. The baseline's RIGHT was ordering luck.
The hunk interns the three zero literals in `Intrinsics::new`, in upstream's
order; measured alone with the type port it removes those 4 losses and
changes nothing else.

**Loss 2 — not fixable here; holds the cluster.**
`incorrectRecursiveMappedTypeConstraint` `n += v[k]` with
`T extends { [P in T]: number }`: upstream detects the circular constraint
(TS2313, `getConstraintOfTypeParameter` → `circularConstraintType`), so
`T[K]` is not number-like, the arm reports TS2365 and answers `any`. TSR
reports no TS2313 and resolves the constraint, so the relation says `T[K]`
is assignable to `number` and the arm answers `number`. The type is right
for the constraint TSR builds; the constraint is wrong. Owner: the
declarations / type-parameter constraint code (`declared.rs`), not this
lane. **Reopening condition:** when circular type-parameter constraints
resolve to upstream's `circularConstraintType`, `git apply` both diffs and
expect +24 lines / +8 cases with zero losses.

## 4. Re-measured and still blocked (no code)

**`||` / `??` generic gate (`tsr-2zk.16.25`).** Re-measured on `35e96b7`: deleting
the type-parameter/`unknown` gate in `check_logical_or_coalescing`
(`binary.rs`) still overflows the stack in
`conformance/discriminatedUnionJsxElement`, the cycle
[operators.md](operators.md) §9 describes (JSX discriminant value type →
`check_expression` → narrowable type → contextual type → same attribute). The
diagnostics dump is unaffected (+1 RIGHT, 0 losses), but the type dump
aborts. `nullishCoalescingOperator2` / `_es2020` lines 35–36 (`a7 ??
'whatever'`, want `{}`) also stay gaps: the subtype reducer cannot decide
`"whatever"` against `{}`. Owner of the fix: the JSX lane's
`jsx_discriminant_value_type` reading `getContextFreeTypeOfExpression`
(`checker.go:7542`). Unchanged reopening condition.

**TS18046 / TS2571 (`checkNonNullTypeWithReporter`, `checker.go:7414`).**
Re-measured on `35e96b7`: reporting in `check_non_null_type_reporting`
(name under 100 characters → TS18046, otherwise TS2571) now loses only
`compiler/nonInferrableTypePropagation2` (EMPTY_RIGHT → EMPTY_WRONG). The
earlier `mapGroupBy` and `neverInference` losses are gone. It converts no
whole case, so it is not shipped. Owner of the remaining loss: inference
publishing the speculative `unknown` parameter type (operators.md §2).
