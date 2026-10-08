# r5-iteration: iteration-types lane notes (round 5)

Lane `tsr-2zk.963` (iteration types) and `tsr-2zk.957` (`yield*` awaited
diagnostics). Pinned tsgo `5b1047d`; baseline frozen at `ac56208`
(diagnostics RIGHT 5070 / EMPTY_RIGHT 5551 / WRONG 1520+96 of 12238 rows).

## 1. The for-of operand reports through `checkNonNullExpression`

`checkRightHandSideOfForOf` (`checker.go:17678`) hands the iterated type
`checkNonNullExpression(statement.Expression())`, the *reporting*
`checkNonNullType` (`checker.go:7409`). `check_for_of_iteration` called the
silent `check_non_null_type` (`members.rs`), so `for (x of undefined)` lost
its TS18050 (`omittedExpressionForOfLoop`, r4-operators2 §3). It now calls
`check_non_null_type_reporting` (`nullable_operand.rs`, r4-operators'
reporter, called rather than copied): the same reporter that picks TS18050
for a `null`/`undefined` keyword operand and TS18048/TS2532 for a nullable
name or expression, and the same `errorType` answers for `unknown` and for
an all-nullable operand, after which the iteration check returns at
`IsTypeAny(errorType)` as upstream does.

**Not ported:** TS18046/TS2571 for an `unknown` operand. The shared
reporter declines it (the reason is in its own doc comment); the
iteration check still sees `errorType` and reports nothing, which is
native's behaviour apart from the missing TS18046.

**Measured** (against `ac56208`): diagnostics +1 (`omittedExpressionForOfLoop`
WRONG → RIGHT), types unchanged, zero losses in both checks.

**Falsifier.** A for-of case whose baseline lacks a TS18048/TS2532 that TSR
now reports on the operand: that points at the operand's *type* (narrowing,
or a declared type carrying `undefined` it should not), not at this call.
