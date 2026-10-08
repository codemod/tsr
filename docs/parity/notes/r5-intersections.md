# r5-intersections — getIntersectionType fidelity (`tsr-2zk.971`)

Lane under `tsr-2zk.971`. Owns `crates/tsr-checker/src/intersections.rs`.
Native anchors are `vendor/typescript-go` @ `5b1047d`.

Frozen base: `935972e` (integration branch). `diagverdictdump`: RIGHT 5075,
EMPTY_RIGHT 5554, WRONG 1516, EMPTY_WRONG 93. `verdictdump`: RIGHT 543125,
WRONG 8295, GAP 1113. Perf is a self-comparison against the base binary
(median child CPU, new/old) plus callgrind `Ir` of `--singleThreaded true`
runs, which repeat exactly.

## 1. `removeConstrainedTypeVariables` and the identity of `{}`

Witness: `unknownControlFlow` `fx4`, `value: T & ({} | null)` with
`T extends {} | null`. Native prints `value : T`; the port kept
`T & ({} | null)`, and the `value === 42` comparison then overlapped.

Native path. `getIntersectionTypeEx` distributes over the union operand
(`checker.go:26203`): `T & {}` and `T & null`. Each is the two-constituent
type-variable case (`:26130`); neither reduces, and both are marked
`ObjectFlagsIsConstrainedTypeVariable`. `getUnionType` then sees
`IncludesConstrainedTypeVariable` and runs `removeConstrainedTypeVariables`
(`:25881`): the primitives `{}` and `null` cover every constituent of T's
base constraint `{} | null` (`containsType`, identity), so both intersections
collapse to `T`.

The port already had every one of these pieces (`reduce_constrained_intersection`,
the `constrained_type_variables` side table, `unions.rs`'s
`remove_constrained_type_variables`). The miss is identity: native resolves
every member-less, unaliased type literal to the one shared
`emptyTypeLiteralType` (`checker.go:22939`), while this port gives each
written `{}` its own `TypeId` and answers "is this `emptyTypeLiteralType`"
with `is_unaliased_empty_type_literal`. The annotation's `{}` and the
constraint's `{}` are two ids, so `containsType` found nothing.

Fix, in the owned file: when the side table records a constraining
intersection whose primitive side is an unaliased empty type literal, it
records the variable's base constraint's own `{}`
(`constraint_identity_of_empty_type_literal`). That is the identity native's
membership test sees. Rejected alternatives:

- **Unify `{}` at its producer** (`get_type_from_type_literal`, `declared.rs`).
  The faithful fix, and the one that would also dedupe `G<{}> & G<{}>`
  (`mixinAccessModifiers`). Not in this lane's files; shipped as a measured
  diff (§4).
- **Compare by `is_unaliased_empty_type_literal` inside
  `remove_constrained_type_variables`.** Same effect, but `unions.rs` is
  read-only for this lane, and the side-table canonicalization keeps the
  equality test native's.

Falsifier: once `{}` is one identity, `constraint_identity_of_empty_type_literal`
returns its input on every call and can be deleted.

Side table (`constrained_type_variables`, Checker-owned, whole-check lifetime):
key is the intersection `TypeId` minted by `create_intersection`; value is
`(variable, primitive)`; published once, when the intersection is first
created with the constrained-variable mark. Only the recorded primitive
changes here. The base-constraint read happens only for an `{}` operand of a
constrained pair; `remove_constrained_type_variables` reads the same constraint
later, and `base_constraint_cache` holds it.

Measured against the frozen base: types +4 lines (`unknownControlFlow`
`0:419`, `0:421`, `0:423`, `0:424`), and the case's missing TS2367 at line 341
is now reported (the case stays WRONG on unrelated TS2322/TS2345/TS2536). Both
loss checks empty. Perf (21 samples, median child CPU new/old): domain-model
0.996, generic-imports 1.013. `Ir`: 1,345,819,958 → 1,345,815,899 and
399,682,984 → 399,678,193; CLI output identical.

## 2. Measured diffs outside this lane's files

Each was measured alone on top of §1 against the frozen base (both unfiltered
dumps), and all four together. Ir is `--singleThreaded true` on the two bench
projects (base 1,345,819,958 / 399,682,984); every variant's CLI output is
byte-identical to the base's. "Net" excludes §1's four lines.

| Diff | Files | Types | Diag | Losses | Ir dm / gi |
|---|---|---|---|---|---|
| [`printer-reduced-type`](r5-intersections-printer-reduced-type.diff) | `checker.rs` (hook), `intersections.rs` (`get_reduced_type`) | +94 net | 0 | none | +0.004% / +0.003% |
| [`inference-intersection-origin`](r5-intersections-inference-intersection-origin.diff) | `inference.rs` | +13 net | 0 | none | +0.007% / +0.001% |
| [`base-constraint-with-string-mapping`](r5-intersections-base-constraint-with-string-mapping.diff) | `intersections.rs`, `string_mapping.rs` | +4 net RIGHT, 18 GAP resolved | 0 | none | +0.16% / +0.001% |
| [`declared-empty-type-literal`](r5-intersections-declared-empty-type-literal.diff) | `declared.rs`, `checker.rs` (field) | +3 net | 0 | none | −0.01% / −0.0005% |

### 2.1 `getReducedType` at the printer (`nodebuilderimpl.go:3228`)

Native's node builder calls `getReducedType` on every type it prints unless
`NoTypeReduction` is set (`typeToStringEx` never sets it by default). A union
that contains intersections is rebuilt from its reduced constituents
(`getReducedUnionType`, `checker.go:21843`); an intersection with a
never-reduced property (`isNeverReducedProperty`, `:21856`: a conflicting
discriminant or a conflicting private) is `never`. The port had the
never-discriminant test (`flow.rs` `intersection_has_never_discriminant`,
used by members/calls/relater) but never applied it when printing, so
`abc & (b | c)` printed its origin instead of `b | c`, and
`type Circle = Shape & { kind: "circle" }` kept the rectangle branch.

The function is a pure port and belongs beside `getIntersectionType`; the hook
is one line at the top of `type_to_string_at_worker` (`checker.rs`, hub,
not lane-specific), so both ship together. Converted: `intersectionReduction`
21, `intersectionReductionStrict` 15, `typeVariableConstraintIntersections` 10,
`mixinAccessModifiers` 9, `iterableWithNeverAsUnionMember(target=esnext)` 9,
`discriminatedUnionTypes2` 8, `intersectionWithConflictingPrivates` 3,
`stringLiteralTypesAsTags01`–`03` 6, `neverTypeErrors1`/`2` 4,
`neverIntersectionNotCallable` 2, `identityRelationNeverTypes` 2, and one line
each in `typeGuardsWithInstanceOf`, `objectSpread(target=es2015)`,
`unionWithIndexSignature`, `genericRestTypes`,
`distributiveConditionalTypeNeverIntersection1`.

This is not §290's refused approximation (`declared.rs`): that one guessed
never-ness from literal disjointness at intersection construction. This one
reduces only where native reduces (printing) and uses the discriminant test
the port already shares with relater and members. Not ported: the reduced-union
cache (`resolvedReducedType`); Ir shows no need.

### 2.2 A union whose origin is an intersection instantiates the origin

`instantiateTypeWorker` (`checker.go`) maps a union with an intersection origin
by instantiating the origin's constituents and calling `getIntersectionType`,
so `T_1 & ({} | undefined)` re-forms as `T & ({} | undefined)` (with its
origin) under `T_1 := T`. The port mapped the distributed constituents and
lost the origin. Converted: `unknownControlFlow` 13 more lines
(`ensureNotNull(a) : T & ({} | undefined)` and its kin).

### 2.3 Read `getBaseConstraintOfType` in the two-constituent reduction

`getIntersectionTypeEx` (`checker.go:26130`) reduces `T & P` against T's
**base constraint**, for any type variable (`TypeParameter | IndexedAccess`),
excluding a generic string-like P (`isGenericStringLikeType`). The port walked
the written `extends` clause of a `TypeParameterDeclaration` and answered the
**error type** for a parameter with no recorded symbol, which is every
mapped-type key: `[K in keyof D as \`use${Capitalize<K & string>}Query\`]`
made `createApi`'s whole signature `error`. The diff reads
`base_constraint_of_type`, widens the variable test to indexed access, and
adds the generic-string-like exclusion; the error decline for a template or
string-mapping constraint part stays (narrowed, not removed).

Alone it **loses** three diagnostics cases
(`declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1`–`3`,
EMPTY_RIGHT → EMPTY_WRONG): with the gap gone, `Capitalize<K & string>`
instantiates to `test`, not `Test`, and `useTestQuery` is TS2339. The cause is
`getStringMappingType`'s missing `isGenericIndexType(t)` arm
(`checker.go:29233`, `string_mapping.rs`): the generic intersection `K & string`
fell through to "return t", dropping the mapping. That arm alone is neutral
(0 changes); with it the pair is lossless: +4 net RIGHT
(`circularReferenceInReturnType2` 3, `mappedTypeAsClauses` 1,
`jsDeclarationsInterfaces(target=es2015)` 1, minus overlap), and 18 GAP lines
of those three cases now compute (3 RIGHT, 15 WRONG on the generic-alias
cluster of §3). Because §2.3 is lossless only with the `string_mapping.rs`
arm, it is not committed on this branch; the integrator applies the two
together.

### 2.4 One `emptyTypeLiteralType`

Native's `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
(`checker.go:22938`) resolves every member-less literal no alias names to the
shared `emptyTypeLiteralType`. The diff caches the first such reserved type in
a new `Checker` field and returns it for every later one. Converted:
`mixinAccessModifiers` 3 (`ProtectedGeneric<{}> & ProtectedGeneric<{}>` now
dedupes). It makes §1's canonicalization a no-op (§1's falsifier).

The +0.16% on domain-model is the `base_constraint_of_type` read that now
runs for every two-constituent type-variable/primitive pair (the syntactic walk
skipped most of them by answering early); it is cached per type and below the
CPU gate (§2.5).

### 2.5 All four together

Applied together on §1: types +122 RIGHT over the frozen base (543,125 →
543,247; WRONG 8,295 → 8,191; GAP 1,113 → 1,095), diagnostics unchanged
(RIGHT 5,075, EMPTY_RIGHT 5,554); both loss checks empty;
`cargo test --workspace --release` passes; fmt and clippy clean in the touched
files. Ir 1,347,567,622 (+0.13%) / 399,687,171 (+0.001%). Median child CPU
(21 samples, new/old): domain-model 0.995, generic-imports 0.993;
`diagnostics_match: true`. The four diffs apply in any order to `cd9cc81`.

## 3. Remaining clusters (not converted)

- **Generic alias whose body reduces to one constituent keeps the alias.**
  `type Id<T> = { [K in keyof T]: T[K] } & {}`: native's
  `getIntersectionTypeEx` returns `typeSet[0]` before attaching the alias
  (`checker.go:26127`), so `Id<X>` prints as the mapped type. The port's
  intersection already returns the singleton, but generic alias references
  are named in `declared.rs`'s alias machinery regardless. Non-generic
  `type K = {a:1} & {}` already matches. 15 WRONG lines in the
  `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1`–`3` cases
  once §2.3 lands; owner `declared.rs`.
- **TS2590 from `checkCrossProductUnion`.** The port returns the error type at
  the 100,000 limit without reporting; native reports at `c.currentNode`,
  which the port does not track. One case
  (`normalizedIntersectionTooComplex`), which also needs UnionToIntersection
  inference and the TS7006 before it can convert.
- **`in`-keyword narrowing of a generic** (`inKeywordTypeguard` `T` vs
  `T & Record<"length", unknown>`, 4 lines per variant): `narrowTypeByInKeyword`
  in `flow.rs`, not intersection construction.
