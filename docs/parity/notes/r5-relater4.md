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

Perf: the first 21-sample run read domain-model 1.040 and generic-imports
0.978, but it overlapped a test compile. The 41-sample re-run on an idle box
read domain-model 0.996 and generic-imports 0.997. `Ir` domain-model
1,236,310,046 → 1,236,744,958 (+0.035%), generic-imports 399,471,422 →
399,469,479. The arm runs only after a union target has already failed. Full
parity run: checker_types 8,184/9,538, diagnostics 4,456/5,502. Workspace
tests pass except
`member_completeness::tests::parameter_properties_need_certified_optionality_for_a_complete_table`,
which fails identically on the base.
