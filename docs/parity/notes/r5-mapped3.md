# r5-mapped3 — mapped types and `keyof` (`tsr-2zk.1033`)

Round-5 cloud lane. Owns `crates/tsr-checker/src/mapped.rs`; the `keyof`
implementation (`getIndexType`) lives in `declared.rs`
(`resolved_keyof_type`, the `TypeOperatorNode` arms of
`get_type_from_type_node`), which r5-typeparams2 owns, so every `keyof`
change here ships as a measured diff. Native source is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/checker.go` unless
noted; every hypothesis was checked against a native `tsgo` built by
`scripts/offline-cargo/build-tsgo.sh`.

Baseline frozen at `02a7110` (this branch's start, `integrate: merge
origin/main`): types 543,912 RIGHT / 1,017 GAP / 7,604 WRONG of 552,533
aligned lines; diagnostics 5,328 RIGHT + 5,581 EMPTY_RIGHT of 12,238 rows.

## 1. `any` key domains and template optionality (committed)

Four arms of `resolveMappedTypeMembers` and its helpers, all in
`mapped.rs`:

- **`any` key constraint.** `getConstraintFromTypeParameter`
  (checker.go:17085) answers `stringNumberSymbolType` for a mapped type
  parameter whose declared constraint is `any` (not `errorType`), so
  `{ [P in any]: Item }` has string, number and symbol index signatures.
  `mapped_type_info` applies it to the computed constraint.
- **`any` modifiers type.** `forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType`
  (checker.go:22731) contributes `string` for an `any` modifiers type in
  place of its index infos, so `Id<any>` (`{ [K in keyof T]: T[K] }`) has a
  string index whose value is the template under `string`. `mapped_member_keys`
  had no such arm and published an empty table (`z[id] : any` in
  `mappedTypeWithAny`, native `Data`).
- **Index key normalization.** The index arm maps `any`/`string` to
  `string` and `number`/enum types to `number`, as native's switch does
  (checker.go:20963); other valid keys keep their identity.
- **Template optionality** (`tsr-2zk.16.121`, r4-mapped §3's held diff):
  `getTemplateTypeFromMappedType` (checker.go:22697) adds optionality for
  `?`, which reaches the index-signature value and the printed template
  (`createMappedTypeNodeFromType`, nodebuilderimpl.go:1471, prints
  `removeMissingType(template)`).
- **Non-generic mapped types print their members.**
  `createTypeNodeFromObjectType` (nodebuilderimpl.go:2690) only takes the
  mapped arm when `isGenericMappedType` (checker.go:24908); otherwise it
  prints the resolved members. `create_semantic_mapped_type` now does the
  same through `resolved_mapped_object`, the materialization the
  instantiation worker already used (factored out, not duplicated).

**Measured** against the frozen baseline, both dumps unfiltered: types
+30 RIGHT (543,942), diagnostics +2 cases
(`deleteExpressionMustBeOptional(strict=false|true)`), **zero losses** on
both. Converted type lines: `deleteExpressionMustBeOptional` ×16 (four
configurations), `correlatedUnions` ×5, `mappedTypeContextualTypesApplied`
×4, `mappedTypeWithAny` ×3, `indexSignatures1`,
`reverseMappedTypeRecursiveInference`.

Full parity run (`coverage`): checker_types 8,218 → 8,219 of 9,538
(configured 1,636 → 1,640 of 1,928); diagnostics 4,498 → 4,499 of 5,502
(configured 829 → 831 of 1,089).

Perf (median child CPU, 21 samples, new/old): domain-model 1.011,
generic-imports 0.987, `diagnostics_match: true`. Callgrind Ir
(`--singleThreaded --pretty false`): domain-model 1,258,697,400 →
1,259,332,314 (+0.05%), generic-imports 399,712,451 → 399,720,832
(+0.002%).

**Why so few of the six `.16.121` cases convert.** Their printed
`{ [P in keyof T]?: T[P] | undefined; }` lines are written mapped nodes
that `declared.rs`' written-text arm prints (§3); `mapped_type_text` only
runs when that renderer declines. `mappedTypeModifiers:100`
(`Partial<Foo>["other"]`) goes through `declared.rs`
`instantiate_identity_mapped_alias`.

**Falsifier.** A mapped type whose constraint is an *error* type that
TSR spells as `any` would now gain three index signatures where native
has the error's empty table; the arm compares against
`intrinsics.error`, which is the producer's own error identity.

## 2. Enum literal keys (held: needs a flow.rs change)

`isTypeUsableAsPropertyName` admits enum literals, so
`{ [K in E]: string | null }` with `enum E { A, B }` has properties `0`
and `1` (native, confirmed: `m2[0] : N.X`, `m3.a : E.A | F` when two
enums share a value). `mapped_key_property_name` reads only string and
number literals, so enum keys fell into the index arm and vanished.

Adding the enum arm alone measured types +40, diagnostics +2, but **two
losses**: `typeGuardNarrowsIndexedAccessOfKnownProperty11`/`12`
EMPTY_RIGHT → EMPTY_WRONG (an extra TS2531 at `m[E.A].toString()`). The
property now exists, so `m[E.A]` has type `string | null`, and the
narrowing by `m[E.A] !== null` does not apply because flow does not
recognize `m[E.A]` as a reference. Native's
`tryGetNameFromEntityNameExpression` (flow.go:1770) names an enum-member
argument by its initializer's literal, or, without an initializer, by
the member's own name (`"A"`, not the value `0`). TSR's
`accessed_property_name_at` (`flow.rs`, main's) answers `None` for an
enum-literal key type.

Held as [`r5-mapped3-enum-keys.diff`](r5-mapped3-enum-keys.diff): the
`mapped.rs` arm plus the `flow.rs` port. It lands once main's flow lane
takes the flow half; the mapped half must not land alone. The pair's
unfiltered measurement is not yet recorded here.

## 3. Mapped nodes printed from their parts (held: `declared.rs`)

`MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT` (`.16.8`, `.16.91`, `.16.121`):
`declared.rs`' `MappedTypeNode | ConditionalTypeNode` arm mints the
written text, and `create_semantic_mapped_type` is only its fallback. The
faithful order is the reverse: a mapped node is a `MappedType` printed by
`createMappedTypeNodeFromType` from its typed parts, or by its members
when not generic (§1). [`r5-mapped3-declared-route.diff`](r5-mapped3-declared-route.diff)
tries `create_semantic_mapped_type` first and keeps the written text as
the fallback. It also carries `.16.108`'s `keyof any`: the concrete
`keyof` arm routes `any`/`never`/`unknown` operands to
`resolved_keyof_type`, which already has getIndexTypeEx's arms
(checker.go:26701), instead of `keys_of` (which declines, so `keyof any`
printed `any`).

Filtered measurement on top of §1 (mappedTypeAsClauses, mappedTypeModifiers,
mappedTypeWithAny and the `.16.108` cases): `bigintIndex` ×5,
`mappedTypeModifiers:130/132/137`, `mappedTypeWithAny:3–6`,
`keyofAndIndexedAccessErrors:7`, `intersectionWithUnionConstraint:29`,
`genericCallInferenceInConditionalTypes1:31`, `mappedTypeAsClauses`
×3 convert. Losses seen, each outside this lane's files:

- `mappedTypeAsClauses:101/102`: `P & keyof NameMap` prints
  `P & (keyof NameMap)`. The key union carries the `keyof NameMap`
  origin, and `intersections.rs` parenthesizes every non-aliased union
  constituent. Native's origin is a `TypeOperator` node, which needs no
  parentheses in an intersection. Owner: `intersections.rs` /
  `printing::prints_as_a_single_token`.
- `mappedTypeContextualTypesApplied:2/6/21/35`: a signature's type
  parameter constraint `{ [P in string]: TakeString; }` now prints its
  members. Native reuses the written constraint node there
  (`tryReuseExistingTypeNode`). Owner: `signatures.rs`/`node_reuse.rs`.

Not landable until both are taken. The diff has not been measured
unfiltered yet.

## 4. `tsr-2zk.948` — the repro already passes

`mapped_intersection_keeps_members_after_object_literal_relation`
(`realworld_repros.rs`, `#[ignore]`) passes on the baseline head, with and
without this lane's changes: `w.u`/`v.u` are `boolean` and no TS2339 is
reported in either order. Something since round 4 fixed it. The
integrator can drop the `#[ignore]` (that file is not owned here) and
re-check `importFixes.ts` on the real-world run before closing `.948`.

## 5. Remaining roots (not owned or blocked)

- **`.16.71`/`.16.100` conditional nodes**: `evaluate_conditional_node`
  and the written conditional mint are `declared.rs`; same order problem
  as §3 for `ConditionalType`.
- **`.16.99` KEYOF-RESOLVED-OPERAND-INDEX-TYPE**: the `keyof` arms in
  `declared.rs` resolve only some operand shapes; native is always
  `getIndexType(getTypeFromTypeNode(operand))`. Same owner.
- **`.16.253` KEYOF-UNIQUE-SYMBOL-KEYS**: needs unique-symbol identity
  (`tsr-2zk.1005`; r5-relater3 refused decidability on the same per-node
  mint).
- **`.16.153` CONTEXTUAL-ARG-GENERIC-MAPPED**: `contextual.rs` (main's).

## 6. Environment notes

- PyPI is blocked; the offline bootstrap ran with the stdlib `tomlkit`
  stand-in r5-operators3 §4 describes, kept outside the repository.
- `varianceProblingAndZeroOrderIndexSignatureRelationsAlign` peaks at
  ~13.6 GB RSS **on the baseline** (measured with `getrusage`, filtered
  `verdictdump`). A full dump run concurrently with a `cargo build` is
  OOM-killed in the 15 GB container; run dumps alone.

## Ownership and work boundaries (checker port convention)

- **Native operations:** `resolveMappedTypeMembers` key and index arms;
  `forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType`'s `any` arm;
  `getConstraintFromTypeParameter`'s mapped `any` arm;
  `getTemplateTypeFromMappedType`; `createTypeNodeFromObjectType`'s
  `isGenericMappedType` split.
- **Identity/owner:** no new cache or side table. A non-generic mapped
  node now mints the same resolved object the instantiation worker mints
  (`mapped_types`, `anonymous_properties`, `object_literal_index_infos`
  keyed by the new `TypeId`), once per node evaluation.
- **Publication:** unchanged. When member resolution declines, the
  mapped text type is returned, as before.
- **Work boundary:** the non-generic arm resolves members eagerly at node
  evaluation where native resolves them at first read; it is reached only
  when the written renderer declines, and the Ir delta above (+0.05%)
  bounds its cost on the benches.
