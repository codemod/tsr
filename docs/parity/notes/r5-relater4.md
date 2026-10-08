# r5-relater4 — relation arms behind the TS2322/TS2345 census (`tsr-2zk.1008`)

Lane under `tsr-2zk.1008`, continuing r5-relater3 ([`r5-relater3.md`](r5-relater3.md)).
Owns relation arms in `crates/tsr-checker/src/relater.rs` and `variances.rs`.
The buckets come from the r5-triage2322 census ([`r5-triage2322.md`](r5-triage2322.md) §3):
X4 (`tsr-2zk.978`), X3 (`.976`), X6 (`.977`) and M4 (`.983`). Native anchors are
`vendor/typescript-go` @ `5b1047d`.

Frozen base: `2919d8c` (integration head). `diagverdictdump`: RIGHT 5189,
EMPTY_RIGHT 5557, WRONG 1402, EMPTY_WRONG 90. `verdictdump`: RIGHT 543275,
WRONG 8152, GAP 1106. Perf is a self-comparison against the base binary (median
child CPU, new/old, `scripts/whole_project_perf.py`) plus callgrind `Ir` of
`--singleThreaded --pretty false` runs, which repeat exactly.

## 1. `tsr-2zk.978` — typeRelatedToDiscriminatedType

Native `structuredTypeRelatedToWorker` ends (relater.go:3889) with one more
chance for an object or intersection source against a union target that no
single constituent accepted: `typeRelatedToDiscriminatedType` (relater.go:3989).
`{ type: 'a' | 'b' }` is assignable to `{ type: 'a' } | { type: 'b' }` because
every combination of the source's discriminant types (`'a'`, `'b'`) matches a
constituent, and each matched constituent accepts the remaining members. The
port had no such arm, so its union-target arm answered NotRelated: a confident
false TS2322/TS2345.

Ported as `Relater::type_related_to_discriminated_type`. It is called from the
union-target arm of `structured_type_related_to_worker` when the some-type walk
answered NotRelated, never Unknown. The order is native's: no arm between
unionOrIntersectionRelatedTo and the end of the worker can fire for an object
source against a union target. The structural arm needs an object target, and
alias variance needs the same alias on both sides. The target is filtered to
`Object|Intersection|Substitution` constituents (`extractTypesOfKind`), and the
arm runs only when two or more remain.

The steps are native's:
1. Find the discriminant properties (`findDiscriminantProperties`).
2. Apply the 25-combination limit before any allocation. Over the limit, or
   with a `never` discriminant, the answer is False.
3. Match each combination with a single-type `propertyRelatedTo`.
   `skipOptional` is set under strictNullChecks or comparability.
4. Relate each matched constituent on its remaining properties
   (`excludedProperties`), call and construct signatures, and indexes.

Supporting refactors:
- **`properties_related_to_excluding`** is the existing property walk, with
  native's `excludedProperties`.
- **`property_privacy_related`** is the privacy switch of `propertyRelatedTo`,
  lifted out of the walk unchanged so the discriminant check can share it.

Judgment calls and stated divergences:

- **`isDiscriminantProperty` is computed, not read from a synthetic union
  symbol.** This port builds no `createUnionOrIntersectionProperty` symbol with
  `CheckFlags`. `is_discriminant_property_of` reproduces the flags. It reads
  apparent constituents and skips error and `never` constituents. The member
  types must be non-uniform and include a literal or pattern literal, and their
  union must not be generic. A private/protected member without one shared
  declaration yields no property. `flow.rs` and `assignreport.rs` hold the same
  computation as private methods in files this lane does not own. A shared
  `pub(crate)` helper is left to the integrator.
- **Property identity.** Native skips a discriminant whose source and target
  symbols are the same (`sourceProperty == targetProperty`). This port's
  property symbols are uninstantiated declarations, so
  `IteratorReturnResult<void>` and `IteratorReturnResult<undefined>` share the
  `value` symbol. The skip therefore also requires equal member types. Without
  that, the configured `iterableTReturnTNext(strictbuiltiniteratorreturn=true)`
  lost its TS2416, the measured loss that forced this.
- **Member existence is the member read** (`get_type_of_property_of_type`), as
  in `propertiesRelatedTo`'s port. An instantiated member may have no symbol.
- **Tuple matches are related whole.** Native's tuple arm of
  `propertiesRelatedTo` excludes discriminant *positions*, and
  `tuples_related_to` takes no exclusions. A matched constituent with a tuple on
  either side is therefore related with the full relation. That relation also
  re-checks the discriminants at their full types, so a success is native's
  answer and a failure becomes `Unknown`. An earlier draft answered `Unknown`
  for any tuple, which lost `concatTuples`'s two `.types` lines. In that case
  `concat`'s `T | ConcatArray<T>` target has a tuple constituent, which native
  decides through the `length` discriminant.
- **Undecided combinations.** A constituent whose discriminant relation is
  `Unknown` cannot be counted as a match or a miss. A combination with no
  definite match and an undecided constituent makes the arm `Unknown`, never
  False.

Measured against the frozen base: diagnostics +4 cases with both loss checks
empty. Three cases go EMPTY_WRONG → EMPTY_RIGHT: `unionRelationshipCheckPasses`,
`discriminableUnionWithIntersectedMembers` and
`relatedViaDiscriminatedTypeNoError2`. `assignmentCompatWithDiscriminatedUnion`
goes WRONG → RIGHT. Its union-of-tuples row also converts, through the whole
tuple relation. Types: RIGHT unchanged, GAP unchanged.

The unit test `discriminated_target_tests::a_union_discriminant_covers_a_discriminated_target`
pins three answers: a covering union discriminant is Related; an uncovered
discriminant value is NotRelated; a covered discriminant with an incompatible
remaining member is NotRelated.

Perf, **corrected**: the numbers first recorded here (domain-model 0.996,
generic-imports 0.997, `Ir` +0.035%) were measured against a stale
`target/release/tsr`. `cargo build --examples` does not rebuild the `tsr`
binary, so that run compared base with base. Re-measured on the binary built
from this commit (41 samples): domain-model 1.009, generic-imports 0.979. `Ir`
domain-model 1,236,738,041 → 1,241,015,670 (+0.35%); generic-imports
399,471,827 → 399,463,940. The cost is the discriminant search: every failed
object→union relation enumerates the source's properties against each
constituent. Native caches `isDiscriminantProperty` on the synthetic union
property, and this port has no such symbol to cache on. Identical binaries
differ by up to ±0.04% `Ir` between runs here. Full
parity run: checker_types 8,184/9,538, diagnostics 4,456/5,502. Workspace
tests pass except
`member_completeness::tests::parameter_properties_need_certified_optionality_for_a_complete_table`,
which fails identically on the base.

## 2. `tsr-2zk.976` — the conditional relation arms

The census's X3 bucket. The port had no conditional arm. A deferred
conditional pair therefore fell either to the end of the worker (`Unknown`, the
missed TS2322s) or to a road that does not apply to it (the extras):

- An inline conditional is minted with OBJECT flags (`declared.rs` §906). The
  "object against a decidable primitive" rule then rejected `T extends B ?
  number : string` against `string | number` (`inlineConditionalHasSimilarAssignability`).
- An alias conditional's branch-literal member image was walked structurally.

Ported in `relater.rs`, after native's alias-variance probe and before the
structural arm, as native orders them:

1. **The gate** (`is_related_to_with_flags`) routes a deferred conditional on
   either side to the worker. It does this after only the any/unknown/never arms
   of isSimpleTypeRelatedTo, which are the only ones native has for it. The
   gate first applies getSimplifiedConditionalType (`simplified_conditional`,
   checker.go:28006) to both sides, as getNormalizedType does.
   `Exclude<T, never>` and `Extract<T, T>` reduce to `T`. Without that step,
   `conditionalTypesSimplifyWhenTrivial` lost to a new false TS2322.
2. **Conditional target** (`conditional_target_related_to`, relater.go:3540).
   The arm applies when the root has no `infer` positions, is not distribution
   dependent, and is not the source's root. It relates the source to both
   branches unless `skipTrue`/`skipFalse` say otherwise.
3. **Conditional source** (`conditional_source_related_to`, relater.go:3721).
   It tries the conditional↔conditional arm, then the default constraint, then
   (for a non-conditional target) the distributive constraint. A conditional
   source is never related structurally. The union- and intersection-target
   arms fall through to it on failure (relater.go:3380 lets an instantiable
   source past unionOrIntersectionRelatedTo).
4. **Non-variable source against a conditional target.** Once the target arm
   has not related it, the answer is final. Type-variable sources took their
   constraint arms earlier.

**Stated divergence: arm order for a type-variable source.** The port's source
type-variable arms come before the conditional-target arm. A `T` source
therefore reaches a conditional target through its constraint, not through
the target arm first.

Every step that needs a piece this port lacks answers `Unknown`, not False.
Each is recorded with the evidence that forced it:

- **`skipTrue`/`skipFalse` without permissive/restrictive instantiations.**
  They are computed only where they reduce to flags:
  - a naked type-parameter check against a parameter-free extends type;
  - a naked type-parameter extends type;
  - `T extends T`.

  Every other pair is `Unknown`.
- **getConditionalFlowTypeOfType.** Native's true branch of `Extract<T,
  Function>` is `T & Function`. This port's raw branch is `T`. A failed default
  constraint whose true branch mentions the check type (or, for a unary tuple
  check, any type parameter) is no proof. The flow substitution is a
  `declared.rs` producer.
- **The distributive constraint** is read only where `declared.rs` captured it
  for an alias reference, as a `(c, c)` pair in
  `conditional_constraint_branches`. Native computes it with `forConstraint`,
  which adds the true branch when some constituent of the extends type is
  assignable to the check's constraint. `Foo<T extends string>` over `T
  extends "abc" | 42 ? true : false` is `boolean`, not `false`.
  `for_constraint_extra` adds that branch. A failure without it is already a
  failure, because the extra only adds a constituent.
- **An `infer` root as a target inside a conditional-alias evaluation frame.**
  Native answers False. Here the answer is `Unknown`. The evaluator's infer
  road (`declared.rs` `evaluate_conditional_inference`, the `relate_ternary`
  at :8345) relates the raw check and extends types and takes False as
  definite. Native relates their permissive instantiations there. Answering
  False collapsed `ramdaToolsNoInfinite`'s `Tail<Tail<T>>`, so `Head<T>`
  printed `never`. Outside a frame, native's False stands. Narrowing `a2` by
  `isA` against `ReturnType<T[M]>` needs it
  (`genericConditionalConstrainedToUnknownNotAssignableToConcreteObject`).
- **An OBJECT-flagged conditional mint** answers `Unknown` on either side.
  Consumers test its genericity by flags. `declared.rs`'s conditional
  evaluator defers only an instantiable check, and so does
  `indexed_access_index_is_generic`. A decided answer is acted on as if the
  type were concrete: `Distributive<[T] extends [never] ? X : never>`
  evaluated to `X` (`conditionalTypeAssignabilityWhenDeferred`). It waits for
  the mint to carry CONDITIONAL.
- **A mapped template's conditional** (`mapped_conditionals`, an `as`
  clause) answers `Unknown`. Deciding `P extends \`_${string}\` ? P : never`
  → `P` makes `mapped_indexed_access_constraint` treat the mapped type as
  filtering, which is native's reading. The base constraint this port then
  computes for `keyof Mapped5<K>` is the whole key domain, where native's is
  the filtered keys. Narrowing then substituted it (`mappedTypeConstraints2`).
- **A conditional whose check type is not generic** is not a deferred type.
  Native resolves every such conditional, so one reaching the relater is a
  conditional this port failed to evaluate
  (`StepSelection<QuickPickStep<QuickPickItem>>` in
  `generatorYieldContextualType`). The conditional arms do not apply to it.
  It keeps the relater's other roads, as before
  (`is_deferred_conditional`).

Repro (`@strict`; each line's verdict now matches native):

```ts
interface A { foo(): void } interface B { bar(): void }
function test1<T extends A>(y: T extends B ? number : string) { const n: string | number = y; } // ok (was TS2322)
type IsArray<T> = T extends unknown[] ? true : false;
function f2<T extends unknown[]>(x: IsArray<T>) { let f: false = x; }      // TS2322 (was silent)
type Foo<T> = T extends "abc" | 42 ? true : false;
function f20<T extends string>(x: Foo<T>) { let t: false = x; }            // TS2322 (was silent)
function f21<T extends string>(x: Foo<T>) { let t: boolean = x; }          // ok
function f<T>(e: Extract<T, Function>) { const g: Function = e; }          // ok (was TS2322)
type Foo2<T> = T extends true ? string : "a";
function test<T>(x: Foo2<T>, s: string) { x = s; }                         // TS2322 (was silent)
function g<T>(x: T) { let v1: Extract<T, string> = x; }                    // TS2322 (was silent)
```

`test1` and `f` are declines (`Unknown`), not decisions: an OBJECT-flagged
mint and a flow-substituted true branch respectively. Neither reports, as in
native.

Measured against the frozen base, with §1 included. Diagnostics: no further
case flips beyond §1's four. The census's CS/CT/CC cases also differ on other
lines. `inlineConditionalHasSimilarAssignability` stays EMPTY_WRONG: its
`any[] extends T ? any[] : never` has a non-generic check type, so
`conditional_inference_operands` gives no operands, and native's OK needs the
flow substitution `any[] & T`. Types: +17 lines WRONG/GAP → RIGHT, all
narrowing results that now decide: `conditionalTypes2` ×11, `keyofAndForIn` ×4,
`extractInferenceImprovement`, and
`genericConditionalConstrainedToUnknownNotAssignableToConcreteObject`. Both
loss checks are empty. Perf (41 samples, against the base, final binary):
domain-model 1.019, generic-imports 1.025. An earlier 41-sample run on a
loaded box read 1.059 / 0.955, and the base binary's own median moved by 8%
between the two runs. `Ir` over §1's binary is +0.036% after `is_conditional`
was gated on flags. The first draft read three hash maps per relation entry,
+0.13%.
