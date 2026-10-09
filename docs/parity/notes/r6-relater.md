# r6-relater — variance reliability, held lifts, TS2321 (`tsr-2zk.1124`, `.1065`)

Lane under epic `tsr-2zk`, round 6, successor to r5-relater8
([branch `claude/beautiful-shannon-ar5gh0-r5-relater8`], its notes
`r5-relater8.md` and diffs `r5-relater8-wip-variance.diff`,
`r5-relater8-wip-conditional.diff` live on that branch only). Owns
`relater.rs`, `relation_cache.rs`, `variances.rs`, `identity.rs`,
`index_access_reports.rs`, `assignreport.rs`, the tests
`crates/tsr-checker/tests/relater8_arms.rs` and this file. Native anchors are
`vendor/typescript-go` @ `5b1047d`; every test expectation was checked
against a native tsgo built from that submodule
(`scripts/offline-cargo/build-tsgo.sh`).

## 0. Frozen base

`b18aec06` (`claude/beautiful-shannon-ar5gh0`: main `17265fac` plus round-6
bookkeeping). r5-relater7's tip `68bd970`, which r5-relater8's WIP was built
on, is an ancestor; the variance diff applied to it with offsets only.

- `diagverdictdump`: RIGHT 5530, EMPTY_RIGHT 5596, WRONG 1063, EMPTY_WRONG 49.
- `verdictdump`: RIGHT 549853, WRONG 5607, GAP 843.
- Callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,079,039; domain-model 1,091,485,686.

Every `Ir` below uses that command, and each run's complete CLI output is
compared with the base's (`cmp`). Loss checks are `box-protocol.md` §5's, on
`cut -f1,2`, unfiltered; slowcases runs on both dumps.

## 1. `object` meets an object target through its apparent `{}` (`.1124` a)

r5-relater8 §1, split out of its variance diff (the
`non_primitive_source_related_to` hunks alone).

**Forcing constraint.** structuredTypeRelatedToWorker replaces a source by
its apparent type before the structural arm (relater.go:3762); the apparent
type of `object` is the empty object type (getApparentType). The structural
arm (relater.go:3864) then relates `{}` to the target's properties,
signatures and index infos. The port took only the definite negative (a
required target property `{}` cannot supply) and answered every other
`object -> T` pair `Unknown`, which hid isSourceIntersectionNeedingExtraCheck
(relater.go:3243): for `x: { a?: string }`, `y: T & { a: boolean }`, `T
extends object`, the optional-property pass finds `boolean -> string`
only after `object -> { a?: string }` relates (`intersectionPropertyCheck`
10:3).

**Ported** (`Relater::non_primitive_source_related_to`): the three conjuncts
over `{}`, stopping on the first False. `sourceIsPrimitive` is false for
`object`, so `{ [x: string]: any }` keeps its shortcut, and `{ [x: string]:
number }` fails (`{}` is not an object literal, so its index is not
inferable). A generic mapped target keeps its own arm; a qualified alias
mint's flags are not evidence of an object.

**Alternative.** Leaving every non-negative pair `Unknown` (the old rule):
correct but silent wherever the answer is consumed as a decision, as above.

**Stated gap.** Targets without a member table stay `Unknown`: a tuple, and a
resolved non-generic mapped type (`Record<string, unknown>`). Both are False
natively.

**Measured** against §0, both loss checks empty, slowcases clean:
- diagnostics: `intersectionPropertyCheck` WRONG → RIGHT;
- types unchanged;
- `Ir`: generic-imports 343,079,039 → 343,082,196 (+0.001%); domain-model
  1,091,485,686 → 1,090,903,686 (−0.05%). CLI output identical.

**Falsifier.** An `object -> T` pair where native's `{}` walk differs from
`properties_related_to(emptyObject, T)`; likeliest a target property the
port reads differently through `Object`'s members.

Test: `tests/relater8_arms.rs`
`the_non_primitive_object_meets_a_target_through_its_apparent_empty_type`.

## 2. Unmeasurable and Unreliable variance, and the structural fallback (`.1124` b)

r5-relater8 §2 (the rest of its variance diff), rebased onto §0 and made
zero-loss. r5-relater8 measured only `diagverdictdump`; on §0 its diff as
written lost 10 type lines (`deeplyNestedMappedTypes` 3,
`indexingTypesWithNever` 6, `genericIndexedAccessVarianceComparisonResultCorrect`
1), then 2 diagnostics cases once the first fixes below exposed them. Each
was traced to a native step and ported; nothing was suppressed.

**Forcing constraint.** getVariancesWorker (relater.go:1341) records
`VarianceFlagsUnmeasurable`/`Unreliable` (types.go:307) when a measuring
comparison instantiates a marker with `reportUnmeasurableMapper` or
`reportUnreliableMapper` (checker.go:1136): a mapped type's source
constraint (`-?` Unmeasurable, else Unreliable, relater.go:3978), a template
source against a template target (:3582), a non-array rest parameter
(:1492), an Unreliable argument inside a measurement (:3935). The flags
travel through `recursiveTypeRelatedTo`'s scoped `reliabilityFlags` and every
published result (:3073, :3123-3138, :3162, :3173). typeArgumentsRelatedTo
(:3903) relates an Unmeasurable argument only by identity, and
relateVariances falls back to the structural arms on a failure under either
flag. The port had none of it.

**Ported (from r5-relater8, unchanged):** the `Reliability` bits in
`relation_cache.rs` (per result, the checker's current flags, per measured
parameter); their scoping and republication in `recursive_type_related_to`;
`report_variance_markers` standing in for the reporting mappers; the markers
as type parameters (`markerSubType` constrained to `markerSuperType`,
checker.go:1034-1037); mapped alias bodies measured; the identity arm and
structural fallback in the variance arm; `getMappedTargetWithSymbol`
(relater.go:804) in isDeeplyNestedType. See r5-relater8 §2 (on its branch)
for the alternatives it rejected.

**Added here, each from a measured loss:**

1. *An undecided identity is not a False.* Under an Unmeasurable variance the
   port's `is_type_identical_to` can answer `Unknown` where native's
   identity is decided (`Required<{ a?: { c: 1 } }> -> Required<{ a?: { c:
   2 } }>`). That `Unknown` blocked the fallback, so conditional types over
   such pairs stayed deferred. Now such an argument enters the structural
   fallback; a path that keeps the variance answer keeps it `Unknown`.
2. *instantiateMappedType's unmapped constituents* (checker.go:22551,
   `instantiateConstituent`): a homomorphic mapped alias applied to an
   argument that is not any/unknown, instantiable, an object or an
   intersection *is* that argument. `mapped.rs` distributes
   `RequiredDeep<1 | undefined>` into `1 | RequiredDeep<undefined>`, minting
   the second as a memberless alias image (only identity mapped aliases get
   the primitive arm, `declared.rs`), which `1` relates to through `Number`.
   `non_object_alias_image_body` (the gate's alias-image normalization) now
   answers such an image as its argument
   (`homomorphic_alias_unmapped_argument`, reading the written `keyof T` as
   getHomomorphicTypeVariable does). Base answered `RequiredDeep<1 |
   undefined> extends RequiredDeep<2 | undefined>` `true`; native and now
   the port: `false`. This retired r5-relater8's `homomorphic_over_primitive`
   fallback rule, which the normalization subsumes.
3. *A reference-bodied alias is measured when native measures it*
   (createMarkerType, relater.go:1420, is getTypeAliasInstantiation).
   `type T<X> = Pick<X, 'x'>` instantiates `Pick`'s mapped type with the
   alias `T`, so native measures `X` covariant with no flag (the `K`
   argument `'x'` mentions no marker) and `T<A> -> T<B>` fails finally
   (`TA_extends_TB : false`). The port left `T` unmeasured, and the
   unmeasured fallback related `Pick<A, 'x'> -> Pick<B, 'x'>` structurally
   (`true`). `variances.rs` now measures an alias whose body is a reference
   to a class, an interface, or an alias it can measure
   (`measurable_alias_body`); a reference chain ending at any other alias
   (`PropertiesReduce -> PropertiesReducer -> Evaluate`, a conditional) stays
   unmeasured, because its marker instantiation is the evaluator's and the
   first version measuring it lost `deeplyNestedMappedTypes`' 73:5 and 81:5.
4. *An alias written as a class or interface reference carries no alias.*
   getTypeAliasInstantiation attaches the alias only to a type it creates;
   `type VarianceShape<in out V> = Shape<V>` instantiates to the interned
   `Shape<V>`, so native relates the references by `Shape`'s variances and
   the written `in out` never applies (`varianceReferences` 48, 63: no
   error). The variance arm relates such an alias's bodies directly,
   following alias chains (`alias_interned_reference`). Measuring them in
   (3) without this lost `varianceReferences`.

**Corrected test.** `variances::tests::invalid_variance_on_a_written_reference_alias_stays_unmeasured`
pinned `type Op<in out T> = Box<T>` (`Box` an object literal) as unmeasured.
Native honours that `in out`: `Op<1>`/`Op<1 | 2>` report TS2322 both ways
and no TS2637, because `Op<X>` is `Box`'s literal re-aliased. The test is
now `written_variance_on_a_reference_to_an_object_alias_is_declared`
(`Invariant`); base reported only one of the two errors.

**Stated divergences.**
- The fallback on an unmeasured alias body that is not a written type
  reference keeps the variance answer (r5-relater8: `Wat<T>`'s
  intersection of `typeof` images relates vacuously here).
- `x.a` of `RequiredDeep<{ a?: 1 }>` is `RequiredDeep<1 | undefined>` in
  the port and `1` natively (the `-?` removal and distribution happen in
  `mapped.rs`). (2) makes the relation over that type native's, but the
  printed type differs. Owner: r6-mapped.

**Convention record** (the new cache state): native operations `relation.set`
with `propagatingVarianceFlags` (relater.go:3162, :3173),
`reliabilityFlags` (checker.go:736) and `VarianceFlags` (types.go:300); key
identity the port's `RelationKey` per relation, owned by
`Checker::relation_results` for the checker's lifetime, plus the merged
`SymbolId` for the per-parameter flags; published with each Succeeded/Failed
result, and once per measured symbol when some flag is set; read by
`recursive_type_related_to` (re-report) and the variance arm; no receiver
context (a marker is checker-global); the bounded expensive work is the
marker-mention walk, run only while a measurement is in progress.

**Measured** against §1's commit (and §0), both loss checks empty, slowcases
clean:
- diagnostics: `requiredMappedTypeModifierTrumpsVariance`,
  `quickIntersectionCheckCorrectlyCachesErrors` WRONG → RIGHT;
  `lambdaParameterWithTupleArgsHasCorrectAssignability`,
  `nongenericPartialInstantiationsRelatedInBothDirections`,
  `unwitnessedTypeParameterVariance` EMPTY_WRONG → EMPTY_RIGHT
  (RIGHT 5533, EMPTY_RIGHT 5599, WRONG 1060, EMPTY_WRONG 46);
- types: `genericIndexedAccessVarianceComparisonResultCorrect:0:17` WRONG →
  RIGHT (RIGHT 549854);
- `Ir`: generic-imports 343,082,196 → 343,085,705 (+0.001%); domain-model
  1,090,903,686 → 1,091,417,136 (+0.05%). CLI output identical.

**Falsifier.** A variance native measures with a flag that the port's
marker-mention walk misses; a reference-bodied alias native re-aliases that
`measurable_alias_body` rejects (or the reverse); a homomorphic mapped
alias argument whose unmapped test differs from instantiateConstituent's.

Tests: `tests/relater8_arms.rs` `an_unreliable_variance_falls_back_to_the_structure`,
`an_unmeasurable_variance_relates_only_identical_arguments`,
`an_unwitnessed_parameter_of_a_reference_alias_is_independent`,
`nested_homomorphic_applications_keep_their_own_recursion_identity`,
`a_homomorphic_alias_over_a_primitive_is_that_primitive`,
`an_alias_written_as_an_interface_reference_keeps_no_alias_variance`;
`variances::tests::written_variance_on_a_reference_to_an_object_alias_is_declared`.

## 3. Lift: the `mapped_conditionals` decline (r5-relater6 §3)

**What it was.** The gate answered `Unknown` for every pair with a mapped
template's conditional on either side (an `as` clause such as `T[P] extends
Function ? P : never`). r5-relater6 §3 refused the lift on three extras
(`mappedTypeAsClauseRelationships` 11:9, `mappedTypeConstraints2` 32:7 and
50:7), caused by two `mapped.rs` gaps: the shared (not per-instance) mapped
iteration parameter, and `keyof` of a generic `as`-clause mapped type being
the whole key domain instead of getIndexTypeForMappedType's filtered keys.

**Re-measured on §2's commit** with the two `mapped_conditionals` tests
removed from the gate (the object-flagged-conditional decline stays):
- diagnostics: no verdict moves; none of the three extras returns, so the
  `mapped.rs` gaps that produced them have closed since round 5;
- types: `mappedTypeConstraints2:0:90`, `:91`, `:92`, `:95`, `:98` WRONG →
  RIGHT (`boundsForKey : NumericBoundsOf<T>[keyof NumericBoundsOf<T>]` and
  its `min`/`max` reads);
- both loss checks (against §0) empty, slowcases clean;
- `Ir`: generic-imports 343,085,705 → 343,085,849 (+0.00004%); domain-model
  1,091,417,136 → 1,091,396,745 (−0.002%). CLI output identical.

r5-relater6's target diagnostic lines (`mappedTypeAsClauseRelationships` 12
and 22; `mappedTypeConstraints2` 10, 16, 59, 90) do not convert: they wait
on per-instance mapped iteration parameters (r5-relater6 §3, cause 1), which
is `mapped.rs`'s.

**Falsifier.** An `as`-clause mapped pair whose decided answer feeds a
consumer that reads it as concrete where native defers (the reason the
object-flagged decline stays).
