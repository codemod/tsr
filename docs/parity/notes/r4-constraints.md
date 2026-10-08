# r4-constraints — lane notes

Round-4 lane `tsr-2zk.942` (parent `tsr-2zk.13`): the constraint of an indexed
access. Native reference: `vendor/typescript-go` @ `5b1047d`,
`internal/checker/checker.go` unless noted. Owned: `constraints.rs` (not
`check_type_argument_constraints`) and functions this lane adds.

## 0. The problem

`r4-subtype.md` §7 left two strict-subtype pairs `T[keyof T]` → `null`
(`compiler/indexedAccessConstraints`, `getField`'s `x ? x[k] : null`)
undecided, so `removeSubtypes` declined the union `T[keyof T] | null` and the
four lines printed a gap.

Native relates the pair in `structuredTypeRelatedToWorker`'s source
type-variable arm (`relater.go:3665`): `getConstraintOfType(source)`, or
`unknown` when that is nil. For `T extends object`:

1. `getConstraintFromIndexedAccess(T[keyof T])` (`:17227`): the index's
   `getSimplifiedTypeOrConstraint` is `keyof T`'s base constraint
   `string | number | symbol`, so the constraint is the deferred
   `T[string | number | symbol]`.
2. For that access the index constraint is itself, so the object's
   constraint `object` is indexed: `getIndexedAccessTypeOrUndefined(object,
   string | number | symbol)` with no access node. `object`'s apparent type
   is `emptyObjectType`; `string` has no property name and no index info
   applies, so `getPropertyTypeForIndexType` returns nil and the union loop
   stops (`:26975`). The constraint is nil.
3. `unknown` → `null` is false, so `T[keyof T]` is not a strict subtype of
   `null` and the union keeps both members.

TSR's relater asked `base_constraint_of_type`, which answers `None` both for
native's nil (it is nil here too: `computeBaseConstraint`'s indexed-access arm
indexes `object` and misses) and for every step this port has not got. The
relater could not tell them apart and answered `Unknown`.

## 1. Ported (committed, `constraints.rs`)

- **`resolved_base_constraint`** — `getResolvedBaseConstraint` (`:27447`)
  with native's two sentinels kept apart (`noConstraintType`,
  `circularConstraintType`). `base_constraint_of_type` folds both to `None`
  exactly as `getBaseConstraintOfType` (`:27436`) does, so its callers see no
  change. **Publication change to an existing cache** (checker port
  convention): `base_constraint_cache` (Checker-owned, whole-check lifetime,
  key = type + alias bindings, unchanged) now records a circular result as
  `Some(error)` instead of `None`. No completed base constraint is `error`
  (`next_base_constraint` maps an error operand to no constraint), and the
  cache is read only by `resolved_base_constraint`. A push that fails on
  re-entry answers `Circular` without publishing, as before.
- **`has_non_circular_base_constraint`** — `:17066`.
- **`constraint_of_type`** — `getConstraintOfType` (`:17047`), three-valued:
  `Constraint(t)`, `Nil` (native's decided nil) or `Undecided` (a step is not
  ported here; no caller may act on it). Arms:
  - type parameter: `getConstraintOfTypeParameter` (`:17059`). A declared
    parameter with no constraint node is `Nil`; one whose written constraint
    `type_parameter_constraint` could not read is `Undecided` (the relater's
    rule for the same parameter);
  - indexed access: `getConstraintOfIndexedAccess` (`:17220`) and
    `getConstraintFromIndexedAccess` (`:17227`), in native's order — mapped
    substitution, then the index's constraint, then the object's;
  - conditional: `Undecided`. `getConstraintOfConditionalType` needs
    `getConstraintOfDistributiveConditionalType`, not ported here;
  - otherwise `getBaseConstraintOfType`: a type with a base-constraint shape
    answers its base constraint, or `Undecided` when that is `None` (the port
    cannot tell nil from unported there); any other non-instantiable type is
    `Nil`. Types whose flags this port does not trust (unresolved mints,
    alias reference images, variadic tuples) are `Undecided`.
- **`simplified_type_or_constraint`** — `getSimplifiedTypeOrConstraint`
  (`:28033`). `getSimplifiedType` is the identity except on an indexed access
  or conditional. This port does not simplify, so wherever
  `getSimplifiedIndexedAccessTypeWorker` could rewrite (union/intersection
  operands, an indexed-access or conditional operand, a tuple or mapped
  object) or the type is conditional, the answer is `Undecided`.
- **`indexed_access_type_or_undefined`** — `getIndexedAccessTypeOrUndefined`
  (`:26935`) with no access node, over `resolved_indexed_access_type`. A
  resolved access is the constraint. A miss is `Nil` only where
  **`indexed_access_miss_is_decided`** proves native's nil: a `object`/`{}`
  intrinsic object (apparent `emptyObjectType`, no members, no index infos)
  and a `string`/`number`/`symbol` key, or a union key with one such
  constituent (the union loop returns nil at the first miss). Literal keys are
  excluded: `getPropertyOfType` adds the global `Object` members
  (`toString`, …) for them. Every other miss is `Undecided`, because
  `resolved_indexed_access_type` returns `None` for unported lookups too.
- **Owned consumer.** `effective_constraint_of_intersection` is
  `getEffectiveConstraintOfIntersection` (`relater.go:2282`), whose loop is
  `getConstraintOfType`. It now asks `constraint_of_type` and keeps its
  previous type-parameter/base-constraint step only for `Undecided`.
  Measured neutral on the corpus (no verdict changed).

**Rejected alternative.** Answering `base_constraint_of_type(T[keyof T])` as
`Some(unknown)`, or making it return `getConstraintFromIndexedAccess`, would
convert the pair without touching `relater.rs`, but the base constraint is a
different native query with 33 callers outside this file; both answers would
be wrong for them. It would win only if every caller wanted
`getConstraintOfType`, which `computeBaseConstraint`'s own indexed-access arm
shows they do not.

**Known limit.** `constraint_of_type` does not run `getSimplifiedType`, so a
chain that native continues through a distribution (`Slices[string | number |
symbol]` → `Slices[string] | …`) ends `Undecided`. Measured on
`compiler/quickinfoTypeAtReturnPositionsInaccurate`: relating
`Slices[SliceId][SliceKey]` through the full `getConstraintOfType` chain ends
there, while the base constraint `NumClass<number> | StrClass<string>`
decides. That is why the relater hunk below consults `constraint_of_type`
only when there is no base constraint.

## 2. Cross-lane hunk: `r4-constraints-relater-indexed-access.diff`

`relater.rs` is not owned. The source indexed-access arm
(`is_related_to_with_flags`, "the source-variable branch also explores an
indexed access's constraint") keeps its base-constraint step; when there is
no base constraint it now relates `constraint_of_type(source)`, or `unknown`
for `Nil` (`relater.go:3668`), and stays `Unknown` for `Undecided`. Only pairs
that answered `Unknown` before can change.

Measured against the baseline frozen at `59c76e7`, unfiltered, with the hunk
applied on top of the committed `constraints.rs`:

- types: 470211 → 470215 RIGHT (+4: `compiler/indexedAccessConstraints`
  lines 19, 22, 23, 28), 0 lost;
- diagnostics: +2 cases (`compiler/indexedAccessConstraints`,
  `compiler/incorrectRecursiveMappedTypeConstraint`), 0 lost.

The first form of the hunk related `constraint_of_type` even when a base
constraint existed (native's exact arm). It lost three RIGHT lines in
`compiler/quickinfoTypeAtReturnPositionsInaccurate` (68, 69, 71): the chain
ended `Undecided` (the §1 limit) where the base constraint had decided, so the
predicate candidate `Extract<Slices[SliceId][SliceKey], NumClass<any>>` stayed
deferred and flow narrowing of `NumClass<number> | StrClass<string>` by a
deferred conditional went wrong. Rejected for that loss; it becomes viable
once `getSimplifiedType` is ported for indexed accesses.

## 3. Measured (committed `constraints.rs` alone)

Baseline frozen at integration head `59c76e7`:

- types and diagnostics: no verdict changed (470211 RIGHT both sides).
- callgrind, `generate_perf_project.py --modules 100`: 3,740,512,691 →
  3,738,901,169 Ir (−0.04%).
- median child CPU at 41 samples, new/old: domain-model 1.027,
  generic-imports 1.018; diagnostics match.

## 4. Remaining

- `NonNullable<Partial<Config>[T]>` → `Config[T]` (`correlatedUnions`, 1
  pair): the indexed-access *target* arm, main's (`r4-subtype.md` §7).
- `getSimplifiedType` for indexed accesses (the §1 limit), and the
  conditional arm of `getConstraintOfType`
  (`getConstraintOfDistributiveConditionalType`), both outside this file's
  current scope.
