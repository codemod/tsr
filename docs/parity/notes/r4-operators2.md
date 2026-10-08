# Lane notes: r4-operators2 (`tsr-2zk.941`)

Round-4 cloud lane continuing [r4-operators.md](r4-operators.md) and
[operators.md](operators.md). Native source is
`vendor/typescript-go/internal/checker/` @ `5b1047d`. The box first froze a
baseline at `59c76e7` and applied r4-operators' held `+` patches itself
(measured there: +24 type lines, zero losses; they also need the intrinsic
count in `tests/globals.rs` and `undefined_widening_modes.rs` raised from 26
to 29). Batch E then landed the same patches, so the lane was rebuilt on the
integration head `3f34e0d` and re-baselined there; numbers below are against
`3f34e0d` unless stated. Every hypothesis was reproduced against a native
`tsgo` built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

## 1. The operator arms run in JavaScript files

**Forcing constraint.** `check_operator_operands`,
`check_arithmetic_operand_types` and `check_unary_operator_operands`
(`operator_operands.rs`), `check_comparison_overlap`
(`comparison_overlap.rs`) and `check_delete_operand_symbol`
(`delete_operand.rs`) returned early on `in_js_file`. Upstream's
`checkBinaryLikeExpressionWorker` (`checker.go:12358`),
`checkPrefixUnaryExpression` (`:10855`) and `checkDeleteExpression`
(`:10804`) have no such test. Which diagnostics a JavaScript file keeps is
the program's decision: a plain JS file keeps only `plainJSErrors`
(`program.go:2137`), a `checkJs` file keeps everything, a `checkJs: false`
or `@ts-nocheck` file keeps nothing — all three already ported in
`tsr_compiler::program_diagnostics`. The gate therefore only ever removed
diagnostics from `checkJs` files, where native reports them:
`plainJSRedeclare2` (`var orbitol = 1 + false` → TS2365).

The one JS-specific rule that stays is upstream's own: TS2839 (object
literal compared by reference) in JS only for `===`/`!==`
(`checker.go:12482`).

**Alternative rejected.** Keeping the gate and special-casing `checkJs`
would re-derive in the checker what the program already filters, the §3a
"side pass" pattern.

**Measured** (against `3f34e0d`): diagnostics +1 case (`plainJSRedeclare2`
WRONG → RIGHT; 4372 → 4373), types unchanged (470755), zero losses in both
checks, and no other case's diagnostic list changed. Perf (median child CPU,
21 samples, new/base, measured on the `59c76e7` stack carrying this change):
domain-model 0.946, generic-imports 0.998.

**Falsifier.** A JS case whose native baseline lacks an operator
diagnostic that TSR now emits points at the JS operand *type* (JSDoc), not
at this gate.

## 2. TS2365 on `incorrectRecursiveMappedTypeConstraint`: not this lane's

The brief's hypothesis was that the patched `check_addition` never calls
`report_operator_error`. It does not need to: the `+`/`+=` diagnostics arm
already shipped as `check_addition_operator` (r4-operators §2) and calls
`report_operator_error` on a confident `NotRelated`. Reproduced with a
debug probe on `n += v[k]` (`T extends { [P in T]: number }`): the arm's
first test, `isTypeAssignableToKind(T[K], StringLike)`, answers
`Ternary::Unknown`, so the arm stops silently — correctly, since a guessed
`NotRelated` would manufacture diagnostics elsewhere.

The `Unknown` comes from the relater's source-indexed-access arm
(`relater.rs`, the "source-variable branch also explores an indexed
access's constraint" block): `base_constraint_of_type(T[K])` is `None`
because `T`'s constraint is circular (`constraints.rs` reports TS2313 and
caches `None`), and `None` maps to `Unknown`. Upstream
(`relater.go:3665`) asks `getConstraintOfType(source)` and relates
`unknownType` when it is `nil`, so `T[K]` is related to neither `string`
nor `number`, the arm reports TS2365 and answers `any`. TSR's `None`
conflates "circular / no constraint" with "could not compute", so the fix
belongs to the relater and constraint owners (distinguishing a resolved
circular constraint — upstream's `circularConstraintType` — from a gap),
not to an operator file.

## 3. Cluster-2 triage: where each leftover operator code comes from

Measured on the baseline diagnostics dump (WRONG cases only), each case
reproduced against native tsgo.

| Code | Cases | Root cause | Owner |
|---|---|---|---|
| TS18050 ×6 | `inOperatorWithInvalidOperands` (4), `widenedTypes` (2) | `check_in_expression` (`assignreport.rs`) calls the non-reporting `check_non_null_type`; upstream's `checkInExpression` (`checker.go:13095`, `:13098`) calls `checkNonNullType` with its reporter | assignreport (hunk in §4) |
| TS18050 ×1 | `omittedExpressionForOfLoop` | `checkRightHandSideOfForOf`'s `checkNonNullExpression` (`checker.go:17680`) not reporting | iteration |
| TS2367 | `compareTypeParameterConstrainedByLiteralToLiteral` | `"x"` comparable-to `T extends "a"\|"b"` is `Unknown`; native falls through the type-parameter-target arm (`relater.go:3435`) to false | relater |
| TS2367 | `uniqueSymbolJs2` | two distinct `unique symbol` types relate `Unknown` both ways (native: false; `isSimpleTypeRelatedTo` only relates a unique symbol to `symbol`); also printed `unique symbol` for `typeof x` | relater, printer |
| TS2367 | `unknownControlFlow` (2) | narrowing of `unknown` (6 other mismatches in the case) | flow |
| TS2365 | `incorrectRecursiveMappedTypeConstraint` | §2 | relater / constraints |
| TS2365 | `newExpressionWithCast`, `importCallExpressionWithTypeArgument`, `parseUnaryExpressionNoTypeAssertionInJsx3` | parser: TSR's tree differs (TS1109/TS1326/TS17004/TS1003 missing too) | parser |
| TS2365 | `plainJSRedeclare2` | §1, fixed | this lane |
| TS2532 | `narrowingOfQualifiedNames` (2), `destructuringAssignabilityCheck` (4) | narrowing of qualified names; binding-pattern parameter/initializer `checkNonNullType` | flow, destructure |
| TS18048 extra | `for-of58` | iterated type of `X[] & Y[]` carries `undefined` | iteration |
| TS18048 extra | `dependentDestructuredVariables`, `jsdocImportType` | narrowing; JSDoc import type | flow, jsdoc |
| TS18048 missing | `mappedTypeIndexedAccessConstraint`, `typeofThis` | property-access receiver nullability (`M1[K]` constraint, `typeof this.no`) | property access |
| TS18047 extra | `narrowingWithNonNullExpression` | `m! && m[0]` narrowing through a non-null expression | flow |
| TS2362/2363 extra | `mappedTypeProperties` (34 other mismatches) | mapped-type member resolution | mapped types |
| TS2363 missing | `YieldStarExpression1_es6` | TSR reports nothing in the file (TS1212/TS2304 missing too) | parser / grammar |

## 4. Cross-lane hunk: `in` operands report through `checkNonNullType`

`r4-operators2-in-operand.diff` (`assignreport.rs` `check_in_expression`):
both operand types go through `check_non_null_type_reporting`
(`nullable_operand.rs`, this lane's reporter) instead of the silent
`check_non_null_type`. Reproduced: `null in {}`, `x in undefined` give
TS18050 in both tsgo and patched TSR.

## 5. Measured cross-lane diffs (not committed)

Measured on the `59c76e7` stack (held `+` patches + §1), against that
stack's own dumps; not re-measured on `3f34e0d` (wrap-up). All three apply
cleanly to `3f34e0d`.

| Diff | File | Diagnostics | Types | Losses |
|---|---|---|---|---|
| `r4-operators2-comparable-type-parameter-target.diff` | `relater.rs` | +7 cases: compareTypeParameterConstrainedByLiteralToLiteral, genericTypeAssertions4, genericTypeAssertions5, genericWithNoConstraintComparableWithCurlyCurly, subtypingWithCallSignatures2, subtypingWithCallSignatures3, thisTypeInClasses; `fuzzy` and `unknownControlFlow` each lose one mismatch | +1 line | **none** |
| `r4-operators2-in-operand.diff` + `r4-operators2-indexed-access-unknown.diff` (measured together) | `assignreport.rs`, `relater.rs` | +3 cases: inOperatorWithInvalidOperands (in-operand), incorrectRecursiveMappedTypeConstraint, indexedAccessConstraints (indexed-access) | +4 lines | **1**: indexedAccessTypeConstraints EMPTY_RIGHT → EMPTY_WRONG |

**Comparable target arm.** The relater's "concrete source cannot inhabit a
target type parameter" arm excluded `Comparable`. Upstream's
`structuredTypeRelatedToWorker` type-parameter-target case
(`relater.go:3423`-`3441`) has no arm that relates a non-mapped object or
primitive source under comparability either; only `isRelatedToEx`'s reverse
simple check (`relater.go:2607`) can, and it relates every `unknown`
source, so the hunk excludes `unknown`. Owner: r4-relater2 (relation arms).

**Indexed-access `None` constraint.** The broad hunk relates `unknown` when
`base_constraint_of_type(T[K])` is `None`, as upstream relates `unknownType`
for a nil `getConstraintOfType` (`relater.go:3668`). Its loss is
`indexedAccessTypeConstraints` `x = y` with `y: T['content']`: TSR's `None`
there means "not computed", while upstream's `getConstraintFromIndexedAccess`
(`checker.go:17227`) finds `C` through the object constraint. The faithful
fix ports `getConstraintOfIndexedAccess`/`getConstraintFromIndexedAccess`
into that arm (index constraint first, then object constraint, then
`unknown`); held for r4-relater2 / r4-constraints. The `in` hunk alone is
expected lossless (it only adds reporting through this lane's reporter) but
was not measured separately.

**TS18046 / TS2571 for an unknown operand** (`r4-operators2-unknown-operand-ts18046.diff`,
`nullable_operand.rs`, this lane's file). `checkNonNullTypeWithReporter`
(`checker.go:7413`) reports TS18046 for an entity name under 100 characters,
TS2571 otherwise; reproduced identical to tsgo on `u++`, `u + 1`, `-u`,
`u < 2`. **Not measured** (the run was cut by wrap-up); r4-operators
measured an equivalent wiring at one loss (`nonInferrableTypePropagation2`,
EMPTY_RIGHT → EMPTY_WRONG). Re-measure on the current head before landing.

## 6. Not pursued

`unique symbol` vs `unique symbol` (`uniqueSymbolJs2`): `UNIQUE_ES_SYMBOL` is
deliberately outside the relater's `FLAG_DECIDABLE` set; adding it is a
relater-wide decision (r4-relater2). TS18046 at property-access receivers
(`privateNameAndAny`, `es2016IntlAPIs`, `useUnknownInCatchVariables01` line
6) belongs to the property-access lane.
