# r5-relater8 — `object`'s apparent `{}`, variance reliability (`tsr-2zk.1124`)

Lane under epic `tsr-2zk`, successor to r5-relater7 ([`r5-relater7.md`](r5-relater7.md)).
Owns `crates/tsr-checker/src/relater.rs`, `index_access_reports.rs`,
`relation_cache.rs`, `variances.rs`, `identity.rs`, the tests for its items
(`crates/tsr-checker/tests/relater8_arms.rs`) and this file. Native anchors are
`vendor/typescript-go` @ `5b1047d`. Every expectation below was checked against
a native tsgo built from that submodule (`scripts/offline-cargo/build-tsgo.sh`).

## 0. Frozen base

**No frozen base; nothing landed as code.** The lane was told to wait for
batch AW (the r5-relater7 merge). AW had not reached
`claude/beautiful-shannon-ar5gh0` when the round was wrapped up (tip `77afe69`,
batch AU). Everything below was built on r5-relater7's branch tip `68bd970`
and is **WIP**: [`r5-relater8-wip-variance.diff`](r5-relater8-wip-variance.diff)
(items 1 and 2, against `68bd970`; it does not apply to AU because it builds on
r5-relater7's relation budget) and
[`r5-relater8-wip-conditional.diff`](r5-relater8-wip-conditional.diff) (item 3,
partial, on top of the first). The test file
`crates/tsr-checker/tests/relater8_arms.rs` (five tests, all checked against
native tsgo) is inside the first diff's scope but not pushed.

Measured so far, against `68bd970`'s own `diagverdictdump` (RIGHT 5460,
EMPTY_RIGHT 5595, WRONG 1131, EMPTY_WRONG 52): **+6 cases, 0 losses** →
RIGHT 5463, EMPTY_RIGHT 5598, WRONG 1128, EMPTY_WRONG 49:
`intersectionPropertyCheck`, `quickIntersectionCheckCorrectlyCachesErrors`
(WRONG → RIGHT); `lambdaParameterWithTupleArgsHasCorrectAssignability`,
`nongenericPartialInstantiationsRelatedInBothDirections`,
`unwitnessedTypeParameterVariance` (EMPTY_WRONG → EMPTY_RIGHT);
`requiredMappedTypeModifierTrumpsVariance` (WRONG → RIGHT).

**Left before it can land:** rebase onto AW, freeze both dumps there, run
`verdictdump` (not yet measured at all), slowcases, Ir on domain-model and
generic-imports, workspace tests and clippy; split into the item 1 and item 2
commits (item 1 is the `non_primitive_source_related_to` hunk alone).

## 1. Item 1: the non-primitive `object` meets an object target through `{}`

**Forcing constraint.** structuredTypeRelatedToWorker replaces a source by its
apparent type before the structural arm (relater.go:3762), and the apparent
type of `object` is the empty object type (getApparentType, checker.go). The
structural arm (relater.go:3864) then relates `{}` to the target's properties,
signatures and index infos. The port took only the definite negative (a
required target property `{}` cannot supply) and answered every other
`object -> T` pair `Unknown`. That hid the extra source-intersection check
(relater.go:3243, `isSourceIntersectionNeedingExtraCheck`): for `x: { a?:
string }` and `y: T & { a: boolean }` with `T extends object`, `T`'s
constituent relates through `object -> { a?: string }`, and only then does the
optional-property pass find `boolean -> string` (`intersectionPropertyCheck`
17:5).

**Ported** (`Relater::non_primitive_source_related_to`): an `object` source
against an object target with members relates `{}` through the three
conjuncts, stopping on the first False. `sourceIsPrimitive` is false for
`object`, so `{ [x: string]: any }` keeps its shortcut and
`{ [x: string]: number }` fails (`{}` is not an object literal type, so
`isObjectTypeWithInferableIndex` is false). A generic mapped target keeps its
own arm, and a qualified alias mint's flags are not evidence of an object.

**Stated gap.** Targets without a member table stay `Unknown`: a tuple (`[]`)
and a resolved non-generic mapped type (`Record<string, unknown>`). Both are
False natively (`length` is required; the string index is not inferable).

**Measured.** WIP: see §0 (diagnostics only, combined with item 2).

**Falsifier.** An `object -> T` pair where native's `{}` walk differs from the
port's `properties_related_to(emptyObject, T)`; the likeliest is a target
property whose type the port reads differently through `Object`'s members.

Test: `tests/relater8_arms.rs`
`the_non_primitive_object_meets_a_target_through_its_apparent_empty_type`.

## 2. Item 2: Unmeasurable and Unreliable variance, and the structural fallback

**Forcing constraint.** getVariancesWorker (relater.go:1341) measures each
type parameter by relating marker instantiations, and records two flags beside
the direction: `VarianceFlagsUnmeasurable` and `VarianceFlagsUnreliable`
(types.go:307). They are set when the measuring comparison instantiates a
marker with `reportUnmeasurableMapper`/`reportUnreliableMapper`
(checker.go:1136): a mapped type's source constraint (`-?` reports
Unmeasurable, anything else Unreliable, relater.go:3978), a template-literal
source against a template target (relater.go:3582), a non-array rest
parameter (relater.go:1492), and an argument of an already-Unreliable
variance inside a measurement (relater.go:3935). The flags travel through
`recursiveTypeRelatedTo`'s scoped `reliabilityFlags` and every published
result (relater.go:3073, :3123-3138, :3162, :3173). typeArgumentsRelatedTo
(relater.go:3903) then relates an Unmeasurable argument only by identity, and
relateVariances lets a failed comparison under either flag fall back to the
structural arms (`VarianceFlagsAllowsStructuralFallback`). The port had none
of it: a measured variance's failure was final, and an unmeasured one was
assumed covariant with no fallback.

Three port gaps stood in front of the flags.

1. **The markers were objects.** Native's `markerSuperType`, `markerSubType`
   (constrained to `markerSuperType`) and `markerOtherType` are type
   parameters (checker.go:1034-1037). The port minted memberless `Named`
   objects, so `keyof Sub` was `never` and `Required<Sub>` resolved to `{}`:
   every mapped alias measured as Independent or not at all. They are now
   `TYPE_PARAMETER`-flagged, and the type-variable source arm gives them
   native's constraints (Sub's is Super; Super's and Other's are nil, so they
   explore `unknown`).
2. **Mapped alias bodies were not measured.** `inference_variances`' alias
   boundary admitted only function, constructor, type-literal and union
   bodies. A mapped body (`Required<T>`, `Partial<T>`) is measured now, its
   marker instantiation being the same alias image a written reference gets.
3. **An alias image has no member table of its own**, so a structural
   fallback on two alias references would walk nothing and answer Related
   (`VarianceShape<1 | 2> -> VarianceShape<1>`).

**Ported.**
- `relation_cache.rs`: a `Reliability` bit set; `RelationResults` carries the
  checker's current `reliability` (native `c.reliabilityFlags`) and, per
  measured symbol, the parameters' flags; each published result stores the
  flags its walk reported.
- `recursive_type_related_to` scopes the flags per pair and republishes them
  with each result; a cached hit re-reports its stored flags.
- `inference_variances` collects the flags per measured parameter.
- `report_variance_markers` stands in for the reporting mappers: the report
  fires when the instantiated type mentions a marker, which is exactly when
  native's mapper meets one. Markers exist only inside a measurement, so the
  walk is skipped outside one.
- The variance arm: an Unmeasurable argument relates by `is_type_identical_to`;
  an Unreliable one inside a measurement passes its report on; a failure under
  a flagged (or unmeasured) variance falls back to the structural arms.
- The fallback on alias references: a measured alias walks its images once
  their mapped members are resolved (`resolve_mapped_type_members`, as
  getPropertiesOfType resolves them) and both enumerate; an image of a
  homomorphic mapped type over a primitive keeps the argument answer, because
  instantiateMappedType (checker.go:22535) makes such a type the primitive
  itself; an unmeasured alias whose body is a written type reference relates
  its evaluated bodies; any other unmeasured alias keeps the variance answer.
- isDeeplyNestedType's `getMappedTargetWithSymbol` (relater.go:804): an
  instantiated homomorphic mapped type is tracked through its modifiers type,
  so `Id<{ x: Id<{ y: … }> }>`'s applications each keep their object
  literal's identity. Without it the newly reachable structural fallback cut
  `deeplyNestedMappedTypes`' six-deep pair as expanding (Maybe).

**Alternatives.**
- *Keep object markers and special-case mapped types over them.* Rejected:
  every construct that reads a type parameter (keyof, indexed access,
  constraints) would need its own marker arm, and native's answer comes from
  the ordinary type-parameter roads.
- *Fall back structurally on alias images directly.* Rejected: measured
  (`VarianceShape`, `Id`) and wrong (`Wat<T>`'s intersection of `typeof`
  images relates vacuously). The member-resolution and body routes above are
  the only ones whose answers matched native on every case tried.
- *Treat an unmeasured variance as final, as before.* Rejected:
  `unwitnessedTypeParameterVariance` (`A<T> = B<T>`, measured Independent
  natively) needs the fallback through the referenced interface.

**Stated divergences.**
- The fallback on an unmeasured alias body that is not a written type
  reference keeps the variance answer (`Wat<T>`'s intersection body:
  the `typeof` query images it relates are vacuous here).
- `evaluate_alias_body` answers a mapped body uninstantiated (its `T` is not
  the declared parameter's TypeId), so the mapped fallback walks the image's
  resolved members instead of the body.

**Convention record** (the new cache state): native operations `relation.set`
with `propagatingVarianceFlags` (relater.go:3162, :3173), `reliabilityFlags`
(checker.go:736) and `VarianceFlags` (types.go:300); key identity the port's
`RelationKey` per relation, owned by `Checker::relation_results` for the
checker's lifetime, plus `SymbolId` (merged) for the per-parameter flags;
published with each Succeeded/Failed result and once per measured symbol
(only when some flag is set); read by `recursive_type_related_to` (re-report)
and the variance arm; no receiver context (a marker is checker-global); the
expensive work bounded is the marker-mention walk, run only while a
measurement is in progress.

**Measured.** WIP: see §0. A first version lost six cases
(`varianceReferences`, `aliasInstantiationExpressionGenericIntersectionNoCrash1`/`2`,
`deeplyNestedMappedTypes`, `recursiveIntersectionTypes`,
`conditionalTypeVarianceBigArrayConstraintsPerformance`, then
`varianceProblingAndZeroOrderIndexSignatureRelationsAlign`); each was traced
and recovered by the alias-fallback rules and `getMappedTargetWithSymbol`
above. `varianceProbling…` and `relationComplexityError` timings not yet
re-measured.

**Falsifier.** A variance native measures with a flag that the port's
marker-mention test misses (a construct whose instantiation reaches a marker
the port's `mentions_type_parameter` walk does not follow), or a fallback on
a mapped image whose resolved members differ from native's.

Tests: `tests/relater8_arms.rs` `an_unreliable_variance_falls_back_to_the_structure`,
`an_unmeasurable_variance_relates_only_identical_arguments`,
`an_unwitnessed_parameter_of_a_reference_alias_is_independent`,
`nested_homomorphic_applications_keep_their_own_recursion_identity`.

## 3. Item 3 (conditional arms): WIP, partial

In [`r5-relater8-wip-conditional.diff`](r5-relater8-wip-conditional.diff), unmeasured as a whole:
- `isDistributionDependent` ported over the root `ConditionalTypeNode`
  (symbol-based `isTypeParameterPossiblyReferenced` on the branches), replacing
  the decline for an instantiated distributive root (`Extract<T[K1], string>`);
- `restrictive_assignable` extended to indexed accesses built only from type
  parameters (their restrictive instantiation has no constraint).
`deepComparisons` line 4 converts on the CLI; line 5 needs a constraint for
`T[string|number|symbol][…]` from `constraints.rs`.

Blocked outside owned files:
- `conditionalTypeAssignabilityWhenDeferred` 41, 65: `conditional_root` cannot
  read an inline conditional's node because `ConditionalInferenceNode::declaration`
  is private (`declared.rs`, r5-declared4): make it `pub(crate)`.
- `flatArrayNoExcessiveStackDepth`: `FlatArray`'s indexed-access body is not
  measured, so the unmeasured covariant guess (`any -> D`) succeeds where native's
  Unmeasurable variance falls back.
- `keyofAndIndexedAccessErrors` 114: not reached.

## 4. Item 4: not started as code; causes found

- `exactOptionalPropertyTypesIdentical`: `canonical_signature` answers `None`
  for `<T>() => T extends … ? 0 : 1` (`signatures.rs`, r5-printer3).
- `assignmentCompatWithGenericCallSignatures4`: the relater steps match native;
  the inferred source parameter from `instantiate_signature_in_context` does not
  (native falls back to the instantiated constraint `I2<T>`; `inference.rs`, main).
- `identicalTypesNoDifferByCheckOrder`: `SomeProps ↔ SomePropsCloneX` both relate
  where native fails one direction; not traced further.

## 5. Item 5 (TS2321): not started

The port already keeps per-side `source_stack`/`target_stack`; the refusal tests
the combined `depth`. The relater half is a swap to native's `len == 100` test
plus a `StackDepthOverflow` result; the report is `assignreport.rs`'s, as with
r5-relater7's TS2859 diff. Unmeasured.

## 6. Integrator's item 0 (r5-relater7's three lifts): not started

Received during the round; no time remained.
