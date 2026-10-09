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

## 4. Lift: the constrained-object write constraint (r5-relater6 §4) — held

**What it is.** `indexed_access_write_constraint` answers `Undecided` for a
property key when index signatures are excluded (the object had a
constraint), where native's getIndexedAccessTypeOrUndefined(…, Writing |
NoIndexSignatures) returns the property's type.

**Measured** (the decline removed, on §3's commit):
- diagnostics: no verdict moves;
- types: **+23** (`contextuallyTypedSymbolNamedProperties` 13 lines, the
  case that refused the lift in round 5 and no longer regresses;
  `contextualTypeFunctionObjectPropertyIntersection` 10 lines);
- **2 lost**: `contextualTypeFunctionObjectPropertyIntersection:0:107`,
  `:108` (RIGHT → WRONG): `bar: (ev) => {}` against `MachineConfig2`'s
  `{ [K in TEvent["type"] as K extends Uppercase<string> ? K : never]?:
  Action<…> }`. Native types `ev` as `any` (TS7006; TS2353 on `bar`), the
  port now as `{ type: "bar" }`.

**Cause (outside the lane).** The relation the lift decides is native's:
`"bar" -> TEvent["type"]` relates through the write constraint `string`
(relater.go:3443-3488), and tsgo agrees. Before the lift it was `Unknown`,
which made `mapped.rs`'s `generic_mapped_contextual_property_type_of_key`
decline, and `any` came out by accident. Now the port substitutes the
template for `bar`. Natively, getTypeOfPropertyOfContextualTypeEx
(checker.go:30565) takes the substitution only for a mapped type whose
getMappedTypeNameTypeKind (:26842) is not Remapping. The port decides that
kind from the conditional's branches (`K | never`, so Filtering). Native
decides it with `isTypeAssignableTo(nameType, K)` on the whole conditional,
and the resulting `any` shows native answers Remapping here. The owner is
`mapped.rs` (r6-mapped): its name-type kind should relate the conditional
itself, not its branch union.

Held as [`r6-relater-write-constraint.diff`](r6-relater-write-constraint.diff).
Re-measure once the kind is native's.

## 5. Lift: the mapped substitution an indexed-access source cannot reach (r5-relater6 §4)

Three declines answered `Unknown` where native reaches
isMappedTypeGenericIndexedAccess's substitution `E[P := X]`
(getConstraintFromIndexedAccess, checker.go:17227) on a resolved mapped
alias instance (`Partial<Foo1>`, `Funcs`), which this port cannot build
because the instance has no mapped identity:
1. the type-variable arm, for a source whose object is such an instance
   (`mapped_substitution_out_of_reach`);
2. the intersection-source effective constraint, when a constituent is one;
3. a failed union/intersection-target walk for an
   `is_mapped_type_generic_indexed_access` source.

**Measured** on §3's commit:
- all three lifted: `correlatedUnions` EMPTY_RIGHT → EMPTY_WRONG (TS2322 at
  181:5, `const func: Func<K> = funcs[key]`, and 299:3, `return o[k]` with
  `o: Partial<Foo1>`), no gains;
- (3) alone: the same loss;
- (1) and (2) together: **no verdict or line moves** in either dump.

**Landed: (1) and (2)**, which narrows two declines to native's road (the
type-variable arm asks `constraint_of_type`, and the intersection takes its
effective constraint). `mapped_substitution_out_of_reach` and
`is_resolved_mapped_alias_instance` are gone.

**Held: (3)**, the union-walk decline. Lifting it needs the substitution for
a resolved instance: either `mapped.rs` keeps the mapped identity of a
concrete instance (`ensure_mapped_type_info` captures `Funcs`, an
argument-less alias image, and `Partial<Foo1>`, a reference, today; what is
missing is the union-walk fallthrough's `{ [P in K]: E }[constraint of X]`
step, relater.go:3681, for which the port has no constraint road), or the
port carries the fallthrough for an indexed-access source
(r5-relater5 §3's stated divergence, which waits on `flow.rs`).

**Measured, (1)+(2)**, both loss checks (against §0) empty, slowcases clean:
`Ir` generic-imports 343,085,849 → 343,086,425 (+0.0002%); domain-model
1,091,396,745 → 1,092,030,115 (+0.06%). CLI output identical.

**Falsifier.** A pair whose type-variable or intersection constraint the
port builds weaker than native's substitution, now decided False where the
decline kept `Unknown`.

## 6. Conditional source arms (`.1124` c)

### 6.1 r5-relater8's conditional WIP

Its diff applied cleanly to §5's commit:
- isDistributionDependent (relater.go:4993) over the root's declaration
  (symbol-based isTypeParameterPossiblyReferenced on the branches), in
  place of the decline for an instantiated distributive root
  (`Extract<T[K1], string>`);
- `restrictive_assignable` extended to indexed accesses built only from type
  parameters (getRestrictiveInstantiation leaves them unconstrained).

Measured alone: **no verdict or line moves** in either dump. It lands with
6.2, which uses the second piece.

### 6.2 The inferred true type of a bare check reference

**Forcing constraint.** `keyofAndIndexedAccessErrors` 115 (`tj = tk`, `T[K]
-> T[J]` with `K extends Extract<keyof T, string>`, `J extends K`) fails
natively on `K -> J`, that is `Extract<keyof T, string> -> J`. The
conditional source arm (relater.go:3721) skips the branch comparison
(Extract's root is distribution dependent) and relates the default
constraint, getDefaultConstraintOfConditionalType: the union of
getInferredTrueTypeFromConditionalType (checker.go:24555) and the false
type. Extract's true branch is written `T`, which getConditionalFlowTypeOfType
turns into a substitution type (`T` constrained by `U`). Its instantiation
(instantiateTypeWorker's substitution arm) is the check type when the
extends type is a top type or the check type is restrictively assignable to
it; otherwise `extends & check` for a check type that is not a type
variable. Here that is `string & keyof T`, and `string & keyof T -> J` is
False. The port used the unflowed `keyof T`, and because native's true type
is narrower it declined (`Unknown`).

**Ported.**
- `inferred_true_type_of_check_reference`: for a root (alias or mapped
  template) without `infer` whose true branch is written as the bare check
  parameter, the instantiated flow type as above. A type-variable check
  against a generic extends type stays a substitution type natively, which
  the port does not represent, so it gives `None`.
- The default-constraint step relates `inferred | false` (with
  getDefaultConstraintOfConditionalType's `any` rules) where it used to
  decline. An inferred type equal to the written one means the first
  comparison already was native's.
- `restrictive_assignable`: `keyof` of an unconstrained type (a type
  parameter or a parameter-only access) has keyofConstraintType `string |
  number | symbol` as its constraint, so it relates to a parameter-free type
  exactly when that union does.

**Alternatives.** A general substitution type in the port: the faithful
long-term road, but it is a new type kind across `declared.rs`,
`instantiation` and every reader. The bare-reference case is exactly
Extract's shape and needs none of it.

**Measured** against §5's commit, both loss checks (against §0) empty,
slowcases clean:
- diagnostics: no verdict moves. `keyofAndIndexedAccessErrors` 104:9,
  106:9 and 115:5 now report as native does. The case stays WRONG on
  TS2537/TS2538 (`indexed.rs`), TS2345 65:33 and 68:24, TS2536 74-75 (the
  held reporter), and 123-124 (r5-relater6 §3's mapped-parameter identity);
- types: **+23** (`isomorphicMappedTypeInference` 13, `keyofAndIndexedAccessErrors`
  3, `typeGuardsTypeParameters` 4, `keyofAndForIn` 2,
  `extractInferenceImprovement` 1);
- `Ir`: generic-imports 343,086,425 → 343,073,010 (−0.004%); domain-model
  1,092,030,115 → 1,092,026,042 (−0.0004%). CLI output identical.

**Falsifier.** A bare-reference true branch whose native instantiation is a
substitution type the port reads as `extends & check` (a check type the
port does not flag as a type variable but native does).

Test: `tests/relater8_arms.rs` `an_extract_source_relates_through_its_inferred_true_type`.

### 6.3 Not converted, with causes

- **`deepComparisons` 5** (`T[K1][K2] -> Extract<T[K1][K2], string>`, an
  error natively): the type-variable arm walks the constraint chain down to
  `T[string | number | symbol][string | number | symbol]`, whose
  `constraint_of_type` is undecided. Native's getConstraintOfType there is
  nil (an unconstrained `T`), so it relates `{}`, False. Needs
  `constraints.rs` (MAIN): that constraint should be nil.
- **`flatArrayNoExcessiveStackDepth` 20**: `FlatArray`'s indexed-access
  body is not measured (r5-relater8 §3). Not reached this round.
- **`conditionalTypeAssignabilityWhenDeferred` 41, 65**: `conditional_root`
  cannot read an inline conditional's node; `ConditionalInferenceNode::declaration`
  is private in `declared.rs` (r6-declared) and needs to be `pub(crate)`.

## 7. TS2321: the stack-depth overflow (`tsr-2zk.1065`)

**Forcing constraint.** recursiveTypeRelatedTo (relater.go:3103) overflows
the check when `len(sourceStack) == 100 || len(targetStack) == 100`, after
the cache, the budget and the maybe-key tests. checkTypeRelatedToEx (:371)
then records the top pair as `Failed | StackDepthOverflow` (`relationCount
> 0` tells it from the complexity overflow) and reports TS2321 "Excessive
stack depth comparing types '{0}' and '{1}'." in place of the relation's
error. The port kept per-side stacks but refused on its raw `MAX_DEPTH`
(every recursion), an unpublished `Unknown`, so TS2321 was never reported.
r5-relater7 §9 declined to turn that cap into TS2321, because it counts
recursions native's stacks never see.

**Ported.**
- `recursive_type_related_to`: the per-side test, in native's place,
  before the raw cap (which stays as the port's guard for recursion that
  pushes neither stack);
- `CachedRelation::StackDepthOverflow`, published for the top pair when
  the overflow was not the budget's; a nested read is False, a top-level
  read `Unknown` (as for the complexity overflow);
- `assignability_overflow` (was `assignability_overflowed`) answers which
  overflow; `assignreport.rs`'s reporting site issues TS2321 or TS2859
  from it.

**The identity gap this exposed.** With the per-side test,
`deepComparisons`' `f2` and `f3` (`Foo<U> <- Bar<U>` with `type Bar<T> = {
x: Bar<T[]> }`) reported TS2321; tsgo reports nothing. Native's
instantiations of an alias written as a type literal are anonymous types
whose symbol is the literal's, shared by every instantiation, so
getRecursionIdentity tracks them as one and isDeeplyNestedType cuts the
walk as expanding (Maybe) at the third level. The port's
`relation_recursion_identity` gave each alias image its own TypeId. It now
tracks such an image by the literal's node (`type_alias_literal_node`), as it
already did for mapped types.

**Measured** against §6's commit, both loss checks (against §0) empty,
slowcases clean:
- no verdict or type line moves. No corpus case expects or emits TS2321
  (`diagverdictdump` has no 2321 on either side), so this lands as a
  faithful no-op for the corpus;
- `Ir`: generic-imports 343,073,010 → 343,075,857 (+0.0008%); domain-model
  1,092,026,042 → 1,091,570,495 (−0.04%). CLI output identical.

**Falsifier.** A check whose port stacks reach 100 where native's do not:
the port runs `recursive_type_related_to` for pairs native answers without
it (identity and literal pairs it caches), and pushes per-side entries under
the same flags, so its stacks can only be as deep or deeper. A TS2321
report anywhere in a future dump that tsgo does not make is that case.

Test: `tests/relater8_arms.rs`
`a_literal_alias_expanding_through_its_arguments_is_cut_not_overflowed`
(no positive TS2321 test: no small program found that tsgo reports TS2321
for).

## 8. TS2322 remainder: re-triage, and the reporter acting on a Related pair

### 8.1 Re-triage (§7's commit)

On §0 there are 94 single-code TS2322 cases (WRONG/EMPTY_WRONG whose only
differing code is 2322); on §7's commit, 90. Joined with r5-ts2322's
`map.tsv`, 30 rows name the relater or `assignreport.rs`:

| Row | Cases | Status this round |
|---|---|---|
| R-apparent | `assignFromBooleanInterface2`, `assignFromNumberInterface2`, `invalidBooleanAssignments` | **converted, §8.2** |
| R-variance | `genericIndexedAccessVarianceComparisonResultCorrect` | its type line converted (§2); the extra TS2322 at 29 stays |
| R-variance | `inferFromNestedSameShapeTuple`, `invariantGenericErrorElaboration` | not reached |
| R-conditional | `deepComparisons` | §6.3: `constraints.rs` (MAIN) |
| R-conditional | `conditionalTypeAssignabilityWhenDeferred` | §6.3: `declared.rs` visibility (r6-declared) |
| R-conditional | `flatArrayNoExcessiveStackDepth` | §6.3: unmeasured indexed-access body |
| R-conditional | `conditionalTypesExcessProperties` | not reached |
| R-identity | `exactOptionalPropertyTypesIdentical` | the generic signature pair is `Unknown` because `canonical_signature` declines for `<T>() => T extends … ? 0 : 1` (`signatures.rs`, r6-printer) |
| R-signatures | `identicalTypesNoDifferByCheckOrder` | §8.3 |
| R-signatures | `assignmentCompatWithGenericCallSignatures4` | `inference.rs` (MAIN; r5-relater8 §4) |
| R-index-primitive | `assignmentCompat1` | held `r5-relater7-primitive-index.diff` (waits on `contextual.rs`, `tsr-2zk.1121`) |
| R-enum | `enumAssignmentCompat3`, `enumLiteralAssignableToEnumInsideUnion` | `declared.rs` (r5-relater7 §1) |
| R-mapped | `mappedTypeAsClauseRelationships` | per-instance mapped parameters (`mapped.rs`, §3) |
| R-mapped | `mappedTypeInferenceFromApparentType`, `unionTypeInference` | not reached |
| R-this | `thisTypeInFunctions` (6 extras) | not reached |
| R-circular-constraint | `typeParameterHasSelfAsConstraint` | the check site declines first (r5-relater7 §1) |
| R-indexed | `undefinedAssignableToGenericMappedIntersection` | not reached |
| S-write-type | `divergentAccessorsTypes8`, `noUncheckedIndexedAccess`, `symbolProperty46`, `symbolProperty47` | the write type of an element access (`members.rs`/`expressions.rs`, MAIN; r5-relater6 §4) |
| S-generator-return, E-duplicate-member, R-gate, S-destructuring | `generatorReturnContextualType`, `lastPropertyInLiteralWins`, `logicalOrOperatorWithTypeParameters`, `objectRestNegative` | not reached |

### 8.2 The reporter reported pairs the relation relates

**Forcing constraint.** checkTypeRelatedToEx (relater.go:369) reports only
when `isRelatedToEx` fails. The missing-property report is an elaboration of
that failure (propertiesRelatedTo's unmatched arm, relater.go:4233).
`report_relation_failure` (`assignreport.rs`) has two stand-ins for an
undecided relation, and both fired on a pair the relation *relates*:
- `missing_required_property` ran before the relation was asked;
- `object_against_primitive` (an object against a primitive, or a primitive
  against a target with a required property) let the report through for
  any non-NotRelated answer, Related included.

In `assignFromBooleanInterface2` 18 (`b = x`, `x` narrowed to `true`,
`NotBoolean { doStuff(): string }`, `Boolean` augmented with `doStuff`)
the relation is Related through `true`'s apparent `Boolean`, and the
second stand-in reported TS2322.

**Ported.** The relation (and its overflow report) is computed first. The
missing-property report and the `object_against_primitive` road are taken
only when the relation is not Related. Both stay the port's stand-ins for an
`Unknown` relation.

**Measured** against §7's commit, both loss checks (against §0) empty,
slowcases clean:
- diagnostics: `assignFromBooleanInterface2`, `assignFromNumberInterface2`,
  `invalidBooleanAssignments` WRONG → RIGHT (RIGHT 5536, EMPTY_RIGHT 5599,
  WRONG 1057, EMPTY_WRONG 46);
- types unchanged;
- `Ir`: generic-imports 343,075,857 → 343,076,453 (+0.0002%); domain-model
  1,091,570,495 → 1,089,884,049 (−0.15%). CLI output identical.

**Falsifier.** A pair whose port relation is a wrong Related, previously
masked by a stand-in report: a lost TS2322 in a future dump with the
relation Related.

Test: `tests/relater8_arms.rs` `a_related_primitive_source_is_not_reported`.

### 8.3 `identicalTypesNoDifferByCheckOrder`: not the relater

Natively `FunctionComponent1<SomePropsX> -> FunctionComponent1<SomeProps>`
fails on the contravariant `SomeProps -> Required<Pick<SomeProps, "x">>`
(`x` is optional in the source). The port's variance is right, but it
relates that pair at once: the image `Required<Pick<SomeProps, "x">>` takes
its member table from the inner `Pick` image (the same members symbol as
`Required<PX>`), so the `-?` is lost and `x` stays optional. `Required<{ x?:
string }>` (a literal argument) is right. Owner: member resolution of a
nested identity mapped alias (`declared.rs`'s
`instantiate_identity_mapped_alias` / `mapped.rs`).

### 8.4 `thisTypeInFunctions`: not the relater

Its six extras (`d1.polymorphic = d2.polymorphic` and the like) compare
`(this: Derived1) => number` with `(this: Derived2) => number`, bivariantly
on `this`. Natively `Derived1 -> Derived2` relates: resolveTypeReferenceMembers
(checker.go:19095) pads a class's own type arguments with the type itself,
so its members (inherited ones through getTypeWithThisArgument) read
`Derived1.polymorphic` as `(this: Derived1) => number`, and the nested pair
is a maybe-key. The port's member table keeps the declaration's unbound
`this` (`(this: this) => number`), and `Base1.this -> Base2.this` fails.
Owner: class instance member resolution (`members.rs`, MAIN).

## 9. Round summary

Commits, in order, each zero-loss on both dumps against §0, slowcases clean,
CLI output identical on both bench projects:

| § | Commit | Diagnostics | Types |
|---|---|---|---|
| 1 | `3e65366` object's apparent `{}` | +1 | 0 |
| 2 | `e17d326` variance reliability and fallback | +5 | +1 |
| 3 | `baf5b35` `mapped_conditionals` lift | 0 | +5 |
| 5 | `ee54555` two mapped-substitution lifts | 0 | 0 |
| 6 | `f7b00e9` conditional source arms | 0 | +23 |
| 7 | `e9dc417` TS2321 | 0 | 0 |
| 8 | `65eb0e9` reporter never reports a related pair | +3 | 0 |

Totals against §0: diagnostics RIGHT 5530 → 5536, EMPTY_RIGHT 5596 →
5599, WRONG 1063 → 1057, EMPTY_WRONG 49 → 46 (+9 cases); types RIGHT
549,853 → 549,882 (+29; first written as 549,883 / +30 and corrected
against the final dump). `coverage`: checker_types 89.03%, diagnostics
84.37% (round 5 closed at 89.00% and 84.26%). `Ir` over the round:
generic-imports 343,079,039 → 343,076,453 (−0.0008%), domain-model
1,091,485,686 → 1,089,884,049 (−0.15%).

Held diff: [`r6-relater-write-constraint.diff`](r6-relater-write-constraint.diff)
(§4: +23 type lines, 2 lost, waiting on `mapped.rs`'s name-type kind).

Needed outside the lane (function, file, reason, cases):
- `constraint_of_type` (`constraints.rs`, MAIN): nil for
  `T[string | number | symbol][string | number | symbol]` with an
  unconstrained `T` (`deepComparisons` 5);
- `ConditionalInferenceNode::declaration` (`declared.rs`, r6-declared):
  `pub(crate)`, so `conditional_root` reads an inline conditional's node
  (`conditionalTypeAssignabilityWhenDeferred` 41, 65);
- getMappedTypeNameTypeKind (`mapped.rs`, r6-mapped): relate the whole
  `as`-clause conditional to the iteration parameter, not the union of its
  branches (unblocks §4's diff);
- `-?` and nested identity mapped aliases (`mapped.rs`/`declared.rs`,
  r6-mapped/r6-declared): `x` of `RequiredDeep<{ a?: 1 }>` is `1` natively;
  `Required<Pick<…>>` keeps `Pick`'s optionality
  (`identicalTypesNoDifferByCheckOrder`);
- instantiateMappedType's primitive arm for non-identity homomorphic
  aliases (`declared.rs`, r6-declared): `RequiredDeep<undefined>` should be
  `undefined` (the relater normalizes it, §2(2), but printing does not);
- resolveTypeReferenceMembers' `this` padding for a class instance type
  (`members.rs`, MAIN): `thisTypeInFunctions` 6 extras;
- the mapped substitution for a resolved mapped alias instance and the
  union-walk fallthrough (§5(3)): `correlatedUnions` 181, 299;
- `canonical_signature` for `<T>() => T extends … ? 0 : 1`
  (`signatures.rs`, r6-printer): `exactOptionalPropertyTypesIdentical`.
