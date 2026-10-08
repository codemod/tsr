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

## 3. `tsr-2zk.917` — weak-type check under an intersection target: already ported

`is_related_to_with_flags` computes `check_excess = !self.intersection_target`
and gates both the excess-property arm and `fails_common_property_check` on it
(native `isPerformingCommonPropertyChecks`, relater.go:2676). Main landed this
in `9b381a8`. `{ a: number }` to `{ key?: string } & { a: number; b?: string }`
reports nothing, as native does. The JSX witnesses (`checkJsxChildrenProperty2`,
`tsxIntrinsicAttributeErrors`) are still WRONG for the other two r4-jsx causes:
`jsx_attributes_inference_type` mints a `Named` with no members table
(`jsx_intrinsic.rs`), and the TS2741 constituent descent is missing
(`assignreport.rs`). Neither is in this lane.

## 4. Protected target property — `isValidOverrideOf`

`propertyRelatedTo`'s second privacy case (relater.go:4285) relates a
protected target property only when `isValidOverrideOf(sourceProp,
targetProp)` holds (checker.go:11928): the source property's declaring class
has the target's declaring class as a base (`isPropertyInClassDerivedFrom`,
`hasBaseType`). The arm answered `Unknown`. It is now ported as
`Relater::is_valid_override_of`, built from two parts:

- `declaring_class` stands in for `getDeclaringClass` (`prop.Parent` is a
  class). It reads the value declaration's parent node: a class element, or a
  constructor parameter property. Native reads the symbol parent. The node
  read gives the same class for both shapes and needs no new binder fact.
- `class_has_base` stands in for `hasBaseType` between class/interface
  symbols, over `get_base_types`. `base_types.rs` has a private `has_base_type`
  over types, but that file is not this lane's. The relater copy is ten lines.
  The integrator may fold it into a `pub(crate)` `has_base_type`.

Declines kept: an intersection source stays `Unknown`, because native ORs over
the synthetic property's constituents (`forEachProperty`) and this loop ANDs.
A target whose declaring class is not found also stays `Unknown`.

Measured against the frozen base, with §1–§2 included: +6 diagnostics cases.
All six go WRONG → RIGHT: `implementingAnInterfaceExtendingClassWithProtecteds`,
`interfaceExtendingClassWithProtecteds`, `interfaceExtendingClassWithProtecteds2`,
`derivedClassOverridesProtectedMembers`, `derivedClassOverridesProtectedMembers2`
and `derivedClassTransitivity4`. Types are unchanged and both loss checks are
empty. Perf (21 samples): domain-model 0.990, generic-imports 0.993. `Ir`
against base is the same as §2's (+0.59% / +0.001%), so this arm adds nothing
measurable.

## 5. Inherited call/construct signatures as target requirements

The relater's structural arm (relater.go:3864) ran its signature conjunct only
when the target *declared* call or construct signatures itself
(`declares_call_or_construct`, which reads the owner's own interface or
type-literal members). Native `signaturesRelatedTo` (relater.go:4441) reads
resolved signatures, and those include every base type's
(`resolveDeclaredMembers`). So `{ own: number }` was assignable to `interface F
extends P {}` with a callable `P`. Both the base and this branch accepted
`const b: F = o`, where native reports TS2322.

`call_or_construct_bearing` extends the test over `get_base_types` for a class
or interface owner. `signatures_of_type_kind` already resolves the inherited
signatures, which is how calls through `F` already worked. It is used at the
structural arm and at `related_signatures`' entry. `signature_bearing`'s
inferable-index use (`object_type_has_inferable_index`) is unchanged; native
`isObjectTypeWithInferableIndex` reads the symbol's own declarations there.

The corpus does not move (no diagnostics or types change, and both loss
checks are empty). It is the prerequisite for §6: without it, a namespace value
would satisfy a global `Function` that inherits a call signature, and
`symbol_chain::module_copy_calls_require_a_certified_empty_global_function`
pins that it must not. Test:
`relater::inherited_call_and_construct_signatures_are_target_requirements`.
Perf (21 samples): domain-model 1.011, generic-imports 1.021. `Ir` over §4's
binary is +0.04% / +0.004%: one base walk per structural pair whose target
declares no signature of its own.
