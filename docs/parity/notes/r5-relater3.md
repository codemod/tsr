# r5-relater3 — relation arms that answer Unknown or the wrong verdict (`tsr-2zk.962`)

Lane under `tsr-2zk.962`. Owns relation arms in `crates/tsr-checker/src/relater.rs`
and `variances.rs`. Native anchors are `vendor/typescript-go` @ `5b1047d`.

Frozen base: `ac56208` (main). `diagverdictdump`: RIGHT 5070, EMPTY_RIGHT 5551,
WRONG 1521, EMPTY_WRONG 96. `verdictdump`: RIGHT 543119, WRONG 8301, GAP 1113.
Perf is a self-comparison against the base binary (median child CPU, new/old)
plus callgrind `Ir` of `--singleThreaded true` runs, which repeat exactly.

## 1. `tsr-2zk.929` — comparable optional-property handling

Ported as recorded in [`r4-relater2.md`](r4-relater2.md), both sites in
`Relater::properties_related_to_with_optionals`:

1. **Missing optional target member.** Native `propertiesRelatedTo`
   (relater.go:4232) sets `requireOptionalProperties` only for the subtype and
   strict-subtype relations. The arm admitted a missing optional member only
   under `Assignable`; it now admits it under every relation except the two
   subtype ones (this port has no identity relation). Comparable was the only
   relation that changed.
2. **Optional source member against a required target member.** Native
   `propertyRelatedTo` receives `skipOptional = relation == comparableRelation`
   (relater.go:4259) and tests it at :4318 for every source shape. The port
   skipped the check under Comparable only for an intersection source. The
   gate is now `relation != Comparable` alone.

Not changed: the weak-type check under Comparable is already native's
(`relater.go:2675`, unit sources only).

Measured against the frozen base: diagnostics +4 cases (`contextualTyping37`,
`optionalProperties01`, `optionalProperties02` EMPTY_WRONG→EMPTY_RIGHT;
`thislessFunctionsNotContextSensitive3` WRONG→RIGHT), types +6 lines, both loss
checks empty. Perf (21 samples): domain-model 0.904, generic-imports 0.994;
`Ir` 1,345,711,888 → 1,345,841,577 (+0.01%) and 399,687,303 → 399,684,492.
The realworld repro `assertion_to_weak_or_optional_target_is_comparable`
passes and loses its `#[ignore]`.

## 2. `tsr-2zk.927` — the alias-variance gate

Native `structuredTypeRelatedToWorker` (relater.go:3389-3392) probes
`getAliasVariances` only when `source.flags & (Object|Conditional) != 0`:
other aliased types are interned and relate structurally. This port mints
every generic alias instantiation as a named reference, so the merged
reference/alias arm decided `SearchResult<undefined>` to `SearchResult<string>`
(`SearchResult<T> = { value: T | undefined } | undefined`) by the default
covariance of an unmeasured alias: a false TS2322.

Ported in two owned places:

1. **Relater** (`non_object_alias_bodies` + the arm before `relateVariances`).
   For a TYPE_ALIAS pair, the evaluated source body (`evaluate_alias_body`,
   the `(symbol, arguments)` cache `compute_base_constraint` already reads)
   stands in for native's source flags. A body that is not
   `OBJECT|CONDITIONAL|INTERSECTION` relates body to body. An unevaluable
   body keeps the variance road.
2. **Variance measurement** (`inference_variances` /
   `create_variance_marker_type`). The candidate patch alone (step 1) cost a
   loss: `contravariantTypeAliasInference` started reporting TS2345. Its
   cause was a second gap, which step 1 only exposed. `inference_variances`
   declined every alias body but a function, constructor or type literal, so
   inference (`infer_from_type_arguments`) inferred `Func2<T> = ((x: T) =>
   void) | undefined` covariantly. The base binary answered `Func2<string>` to
   `Func2<"a">` *wrong* (an error native does not report). That wrong answer
   happened to agree with the wrong inference. Native's `createMarkerType`
   (relater.go:1420) is `getTypeAliasInstantiation` for any alias, so a union
   body is now measured over its instantiated body. Function, constructor and
   type-literal bodies are unchanged. Other bodies (references, mapped,
   indexed access and so on) stay unmeasured, which keeps the written
   reference-alias boundary that
   `invalid_variance_on_a_written_reference_alias_stays_unmeasured` pins.

**Stated divergence: intersection bodies keep the variance road.** Native relates
them structurally too. Including them converted `unionTypeInference`
(`DeepPromised<T>` is an intersection alias), but lost
`aliasInstantiationExpressionGenericIntersectionNoCrash2`'s TS2352: `Wat<T> =
ClassAlias<T> & FnAlias<T>` over `typeof Class<T>` / `typeof fn<T>`. Their
constituents have no members table, so the relater answers `Unknown`
(reasons row 3, `NoMembersTable`) where the variance road answered NotRelated.
A loss is never accepted. This divergence ends when instantiation-expression
types relate structurally. The falsifier is that same case: once its
intersection relation decides, dropping `INTERSECTION` from the gate must keep
it RIGHT and convert `unionTypeInference`.

Measured against the frozen base, with .929 included: the corpus counts are
the same as §1. The intersection-excluded arm converts no corpus case on its
own (`unionTypeInference` needed intersections). Both loss checks are empty.
The realworld repro `union_alias_relates_structurally_not_by_variance` passes
and loses its `#[ignore]`. A new variance test is
`a_union_alias_measures_its_members`. Perf (21 samples): domain-model 1.008,
generic-imports 0.985. `Ir` domain-model 1,345,715,425 → 1,353,496,633
(+0.58%), generic-imports 399,682,172 → 399,691,414 (+0.002%). The
domain-model cost is the union-alias measurements and body relations that
used to be skipped (an unmeasured alias answered covariant for free). It is
inside the CPU gate.
