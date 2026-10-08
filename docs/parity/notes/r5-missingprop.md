# r5-missingprop: the sole-TS2741 and sole-TS2353 cases

Lane on epic `tsr-2zk`, vendor `5b1047d`. Target: the diagnostics cases whose
**only** wrong code is TS2741 ("Property 'x' is missing in type … but required
in type …") — 28 missing a report, 2 with an extra one — and then the 21 whose
only wrong code is TS2353 (excess property).

Frozen baseline: integration head `bd8ae26`. Diagnostics dump 5350 RIGHT,
1241 WRONG, 5581 EMPTY_RIGHT, 66 EMPTY_WRONG; types dump 544,047 RIGHT,
7,492 WRONG, 994 GAP.

Method: each case was run with the reporter instrumented at
`report_relation_failure` / `report_argument_failure` entry (the pair, the
three-valued relation, `assignability_pair_is_reportable`; not committed),
and every probe was compared with a native `tsgo` built from the pinned
submodule (`scripts/offline-cargo/build-tsgo.sh`). "Not reached" means no
report function was asked about the position at all.

## 1. Triage: sole TS2741 (30 cases)

| Cluster | Cases | Root cause | Where the fix lives |
|---|---|---|---|
| A. Heritage type arguments are never constraint-checked | `subtypingWithNumericIndexer2`/`3`/`4`, `subtypingWithStringIndexer2`/`3`/`4`, `genericTypeConstraints` (7) | `checkTypeReferenceNode` (`checker.go:2982`) runs for `ExpressionWithTypeArguments` too (class `extends`, `checker.go:4371`; interface heritage, `:5022`). `check_type_argument_constraints` returns unless the node is a `TypeReferenceNode`, so `extends A<Base>` is never checked. | `constraints.rs` (not owned) — §3a |
| B. Calls: the argument check never runs or is lost | `fixingTypeParametersRepeatedly2`, `typeParameterFixingWithContextSensitiveArguments2`/`3`, `overloadresolutionWithConstraintCheckingDeferred` (×3), `inferenceFromIncompleteSource`, `mappedTypeAsStringTemplate`, `narrowingGenericTypeFromInstanceof01`, `objectLiteralThisWidenedOnUse` (10) | Not reached, or reached only inside a callback body checked during overload resolution whose diagnostics are discarded (`overloadresolution…` 19,14 is asked `D → A`, NotRelated, and nothing survives). Inference fixing / inferred-type-argument constraint fallback (`getInferredType` replaces an inferred argument that fails its constraint with the constraint, so `new G(x)` relates `D → A`). | `calls.rs`, `inference.rs` (main's calls lane) |
| C. Type-only re-export aliases resolve to `error` | `chained`, `renamed`, `mergeSymbolReexportInterface` (3) | The target of `const d: D = {}` is `error` (`export type { A as B }` chains), so `assignability_pair_is_reportable` declines. | alias resolution (`symbols.rs`/`declared.rs`, not owned) |
| D. `using` without `strictNullChecks` | `usingDeclarations.14`, `usingDeclarationsWithIteratorObject` (2) | `check_using_declaration_initializer` declines when `Disposable \| null \| undefined` collapses to `Disposable`, saying the missing-property reporter is private to `assignreport.rs`. It is `pub(crate)` (`report_relation_failure` with a head). | `using_declaration.rs` (not owned) — §3b |
| E. JSX hyphenated attributes | `ignoredJsxAttributes`, `tsxUnionElementType3`, `tsxUnionElementType6` (3) | The element with a `data-*` attribute is declined by the JSX caller for want of the `isComparingJsxAttributes` relater flag (r5-report §2). | `jsx_component.rs` / `relater.rs` |
| F. Mapped relation | `assignmentCompatWithEnumIndexer` (`{}` vs `Record<E, any>` answers **Related**), `mappedTypeWithAsClauseAndLateBoundProperty` (**Unknown**) (2) | `Record<E, any>` over a numeric enum must have the required property `"0"`; the `as`-clause mapped type over `keyof number[]` is undecided. | `mapped.rs` (r5-mapped3), `relater.rs` (r5-relater5) |
| G. `yield*` assignability | `generatorTypeCheck20` (1) | `check_yield_expression_assignability` declined every `yield*`. | **`assignreport.rs` — fixed, §2.1** |
| H. Generator return annotation | `generatorTypeCheck7` (1) | `checkSignatureDeclaration`'s `checkGeneratorInstantiationAssignabilityToReturnType(returnType, flags, returnTypeNode)` (`checker.go:2763`) and TS2505 are not ported. | §2.2 |
| I. Destructuring-assignment rest | `nonIterableRestElement3` (1) | `[...c] = ["", 0]`: `checkArrayLiteralDestructuringElementAssignment`'s rest arm relates `(string \| number)[]` to `c`'s type; not reached. | destructuring assignment (not owned) |
| J. Expando members (extra TS2741) | `expandoFunctionExpressionsWithDynamicNames2` (×2), `expandoFunctionSymbolProperty` (3) | `foo[mySymbol] = true` / `bar[t] = true` after `const foo: Foo = () => {}` are expando assignments with late-bound element-access names; native binds them as members of the function, so the arrow's type has `[mySymbol]`. TSR's function type lacks them, so the relation is NotRelated. | binder / expando members (not owned) |

## 2. Fixed here

### 2.1 `yield*` (cluster G)

`checkYieldExpression` (`checker.go:10952`) relates
`getYieldedTypeOfYieldExpression` (`:11019`) to the annotation's yield type for
both forms: for `yield*` the yielded type is
`checkIteratedTypeOrElementType(IterationUseYieldStar, operandType, …)`.
`check_yield_expression_assignability` returned at the asterisk. It now asks
`yield_star_operand_types` (the existing iteration query also used by
return-type inference) for the yielded type and reports it at the operand
with the operand as the elaboration node, as native passes
`node.Expression()` for both. `anyType` comes back for a `never` or
non-iterable operand (whose TS2488 the iteration check already reports), so
nothing is double-reported. The plain operand's excess-property pre-check is
not run for `yield*`: the related type is the delegated element type, and
`elaborateError` on the literal reaches the members itself (native prints
TS2353 at `y` for `yield* [{x: 1, y: 2}]`; so does TSR).

Async generators stay declined for both forms (the yielded type is awaited
first), as before.

Probed against native tsgo on string, generator, non-iterable, `never`,
union-array, and union-annotation operands and an object-literal method:
identical output. The union-source chain child (`Property 'x' is missing in
type 'Baz'…` under `Type 'Baz | Foo' is not assignable…`) is not built by
this reporter at any site; only the head is pinned in the test.

Measured: `generatorTypeCheck20` WRONG → RIGHT; no diagnostics loss.
Both loss checks empty (types RIGHT 544,047 → 544,047). Perf, median child
CPU new/old: domain-model 1.011, generic-imports 1.007 at 41 samples (21
samples read 1.041/1.039 and were re-run per protocol; neither project has a
`yield*`). Coverage: checker_types 8232/9538, diagnostics 4518/5502.
