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

## 6. Namespace object types — `typeof N` relates over its exports

r4-heritage §5 recorded `clodulesDerivedClasses` (TS2417) as "typeof namespace
vs typeof namespace answers Unknown". The relater's `has_members` admitted a
`TypeData::Anonymous` only when it carried signatures. A namespace's value type
(`typeof N`, native `createObjectType(ObjectFlagsAnonymous, symbol)` with the
exports as members) has none, so every relation with it as a side fell to
row 3 (`NoMembersTable`). That covered even `const w: { f(): number } = B`.

`is_namespace_object_symbol` admits an anonymous type whose symbol is a
VALUE_MODULE not merged with a function, class or enum, since those keep
their own arms. The structural arm then relates properties, signatures and
indexes as native does. Property names and types come from the existing
`get_property_names_of_type` / `get_type_of_property_of_type` readers, and
`object_type_has_inferable_index` already counts VALUE_MODULE.

Two tests outside this lane's files pinned the old `Unknown`. Both are moved to
native's answer:

- `tsr-conformance/tests/module_augmentation_absence.rs`: the augmented module
  object is Function-related, which is what its header says native does
  (renamed `merged_augmentation_proves_function_relation`).
- `tsr-checker/tests/symbol_chain.rs`
  `module_copy_calls_require_a_certified_empty_global_function`: a namespace
  value is now an untyped call (`isUntypedFunctionCall`, checker.go:9936) for
  three more global `Function` shapes. Two have a `[key: string]: any` index,
  own or inherited, which a namespace object satisfies through its inferable
  index. The third is empty through a generic base. The two rows whose
  `Function` inherits a call/construct signature stay errors, because §5
  made inherited signatures target requirements.

Measured against the frozen base with §1–§5: +14 diagnostics cases over §5,
all WRONG → RIGHT. They are `aliasAssignments`, `esModuleInteropDefaultImports`,
`moduleNodeDefaultImports` ×4, `typeofAmbientExternalModules`,
`typeofExternalModules`, `typeofInternalModules`,
`everyTypeWithAnnotationAndInvalidInitializer`,
`forStatementsMultipleInvalidDecl`, `importCallExpressionCheckReturntype1`,
`invalidMultipleVariableDeclarations`,
`typesOnlyExternalModuleStillHasInstance` and `valuesMergingAcrossModules`.
Types are unchanged and both loss checks are empty. Perf at 41 samples:
domain-model 1.029, generic-imports 0.977. The first 21-sample run read
generic-imports 1.035 and domain-model 1.029. `Ir` over §5's binary is
+0.0006% / −0.003%, so the CPU spread is run noise.

`clodulesDerivedClasses` itself is still WRONG. Its relation now decides:
`typeof Path` to `typeof Shape` answers NotRelated through `Utils`. But the
TS2417 is not emitted. r4-heritage §4 records the heritage check's
merged-source decline for a class merged with a namespace, which is not in
this lane. The printer also says `typeof Utils` where native says
`typeof Path.Utils`.

## 7. Refused: `UNIQUE_ES_SYMBOL` in `FLAG_DECIDABLE` (`uniqueSymbolJs2`)

Native `isSimpleTypeRelatedTo`'s only unique-symbol arm is `ESSymbolLike` →
`ESSymbol` (relater.go:236), and this port has it. Two unique symbols relate
only by identity. On the relation side, then, adding the flag is faithful. It
converted `uniqueSymbolJs2` (TS2367), `symbolType2` and `uniqueSymbolsErrors`.

**It was refused at −3 RIGHT cases**: `uniqueSymbols`,
`uniqueSymbolsDeclarations` and `computedPropertiesNarrowed` went RIGHT →
WRONG, with extra TS2322s such as `const constTypeAndCall: unique symbol =
Symbol()`. The cause is identity, not the relation. Native keys a
declaration's unique symbol on its symbol (`getESSymbolLikeTypeForNode`,
checker.go:22982, `links.uniqueESSymbolType`), so the annotation and the
`Symbol()` initializer are one type. This port mints the written
`unique symbol` once per type node (`declared.rs`, `unique_symbol_nodes`), and
the `Symbol()` call mints a fresh one on every evaluation (`calls.rs`, the
`is_symbol_or_symbol_for_call` arm). The flag stays out until both producers
key on the declaration symbol. They belong to r5-typeparams2 and main. The
falsifier is that re-adding the flag after that change shows zero losses.

## 8. Real-world check (jsTyping / typingsInstallerCore)

Scratch `tsconfig.scratch.json` per r4-realworld, `--pretty false`, base binary
against this branch's head:

| | base total | head total | `SearchResult<undefined>` | `TracingNode`/`EmitNode` TS2352 |
|---|---|---|---|---|
| jsTyping | 478 | 453 | 12 → 0 | 5 → 0 |
| typingsInstallerCore | 484 | 459 | 12 → 0 | 5 → 0 |

No diagnostic was added. The 8 other removals per project are TS2352
comparable false positives that §1 also fixes (`checker.ts` 15401/23925/46772,
`parser.ts` 10718/10770, `resolutionCache.ts` 1713/1714, `utilities.ts`
2486). TypeScript compiles its own sources cleanly.

## 9. Totals

Against the frozen base (`ac56208`), at the head of this lane:
`diagverdictdump` RIGHT 5070 → 5092, EMPTY_RIGHT 5551 → 5554, WRONG 1521 →
1499, EMPTY_WRONG 96 → 93. `verdictdump` RIGHT 543119 → 543125. Both loss
checks are empty. The coverage run gives `checker_types` 8176 → 8183 and
`diagnostics` 4394 → 4414 (before is the checked-in snapshot at `ac56208`).

## 10. `tsr-2zk.978` — `typeRelatedToDiscriminatedType`: held as a patch

Assigned after the round started (r5-triage2322 buckets X4/B12/D/B8: 16 lines,
5 cases, all false positives). The port is in
[`r5-relater3-discriminated-type.diff`](r5-relater3-discriminated-type.diff).
It contains `type_related_to_discriminated_type` (relater.go:3989),
`discriminant_property_related_to` (propertyRelatedTo with the source read as
one discriminant type), `is_discriminant_of` (isDiscriminantProperty over the
target's object-only constituents), `properties_related_to_excluding`
(propertiesRelatedTo's `excludedProperties`), and the call from the union-target
arm (relater.go:3889-3897). It also carries a single-combination early return
(see the last bullet).

**Measured on probes:** all the triage repros relate as native does. That covers
`{ type: 'a' | 'b' }` → `{ type: 'a' } | { type: 'b' }`, `{ a: 0 | 2, b: 4 }`
→ the three-way `T`, the tuple union `[b, 1]`, `{ foo?: number | undefined }`
→ `{ foo?: undefined } | { foo: number }`, and the conditional-type
`{ x: 'x' | 'y', y }` extends `Y`. The two negatives are kept:
`{ type: 'a' | 'c' }` → `Action`, and a discriminant match whose other
property fails.

**Not landed. Neither corpus dump finished.**
`varianceProblingAndZeroOrderIndexSignatureRelationsAlign` and `…Align2` take
about 0 s on the base binary. With the arm they run out of memory in about 18 s
(a 3 GB cap; the unbounded dump reached 6.9 GB). The trace is a recursive
expansion. In order, it goes `properties_related_to_excluding` →
`related_signatures` → `instantiate_signature_in_context` →
`create_type_reference_with_display`, over `Either<L, (a: A) => B>`
(Left/Right classes discriminated by `_tag`, whose `map`/`ap` return `Either`
of a growing argument). Native has the same arm. It survives because
`recursiveTypeRelatedTo`'s `isDeeplyNestedType` cuts expanding generic
recursion by type identity. This port's relation stack does not cut that
expansion, and each discriminated decomposition multiplies the work at every
level. Returning early for a single combination (every discriminant one
type) did not bound it.

**Prerequisite:** a faithful `isDeeplyNestedType` recursion identity for
expanding generic instantiations in `recursive_type_related_to`. That is the
relation stack, not an arm, so it is not this lane's to change. A falsifier for
the patch once the cut exists: both `varianceProbing…` cases finish at base
speed, and the dumps show zero losses.

Corpus sweep tool, for whoever picks this up: a per-file `tsr --noEmit
--strict` run with `ulimit -v 3000000; timeout 20`. Of the files it flags,
`recursiveConditionalCrash3` aborts and `relationComplexityError` /
`templateLiteralTypes1` take 30–55 s on the **base** binary too.
