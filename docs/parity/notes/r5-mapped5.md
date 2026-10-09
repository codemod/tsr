# r5-mapped5 — mapped laziness and truncation, mapped pieces for the relater (`tsr-2zk.1033`, `.1089`, `.1093`)

Round-5 cloud lane, successor to r5-mapped4 (`r5-mapped4.md`). Owns
`crates/tsr-checker/src/mapped.rs`, `intersections.rs`, `node_reuse.rs`,
`printing::prints_as_a_single_token`, new intrinsics in `intrinsics.rs`, and
ADR-0050. `declared.rs` (r5-declared3), `relater.rs` (r5-relater7),
`indexed.rs` (r5-errorsplit5) and main's files ship as measured diffs.
Native source is `vendor/typescript-go` @ `5b1047d`.

Baseline frozen at `e20cdd4` (batch AH, which carries r5-mapped4's head and
both of its diffs): types 548,751 RIGHT / 6,640 WRONG / 900 GAP; diagnostics
5,431 RIGHT + 5,590 EMPTY_RIGHT of 12,238 rows. Coverage: checker_types
8,358 of 9,538 (configured 1,670 of 1,928); diagnostics 4,589 of 5,502
(configured 842 of 1,089).

Setup as r5-mapped3 §6: the offline bootstrap ran with a stdlib-only
`tomlkit` stand-in kept in the session scratchpad.

## 1. Mapped property types on first read; truncated member print (committed, ADR-0050)

**Forcing constraint.** r5-mapped4's declared route was held on
`hugeDeclarationOutputGetsTruncatedWithError`:
`{ [K in manyprops]: { [K2 in manyprops]: `${K}.${K2}` } }`, 676 × 676
members. Native instantiates a mapped property's type on the first
`getTypeOfSymbol` (`getTypeOfMappedSymbol`, `checker.go:20984`). Its `.types`
print stops at the node builder's NoTruncation limit (1,000,000 approximate
length, `checkTruncationLength`, `nodebuilderimpl.go:140`). The port
instantiated and printed every member when the node was evaluated.

**Ported** (ADR-0050 has the alternatives):

- `Slot::Mapped` (`objects.rs` `property_slot`): the slot holds the owner,
  index, key and the strip-optional flag. `get_type_of_mapped_symbol`
  instantiates the template on the first read and publishes a resolved slot
  into the owner's member table.
- `mapped_object_text`: the member print with `createTypeNodesFromResolvedType`'s
  truncation and the node builder's length accounting. A nested resolved
  mapped object is printed in the same budget.
- One mapped type per node and context: `MappedTypeInfo::node_key` records the
  `TypeLiteralKey` of the capture. The first capture is published in
  `type_literal_types`, and a second capture under the same key (an alias
  instance's `capture_mapped_alias` and the node's own evaluation) reads the
  first's member table, so no slot is instantiated twice.

**`objects.rs` is not in this lane's list.** The change adds one variant and
its two dispatch arms to the private `property_slot` module, next to the
existing `Accessor` variant, which has the same on-demand contract. No other
box claims `objects.rs` in round 5, and the route cannot be unblocked without
it. I flag it here for the integrator.

**Measured** against the frozen base, both dumps unfiltered:
- types +1 RIGHT (`mappedTypeIndexedAccess:13`: the union constituents are
  now created in read order, which is native's order);
- diagnostics identical;
- zero losses; slowcases clean on both dumps.

Every other type line is byte-identical (key, verdict, want, got).
- Coverage: checker_types 8,358 → 8,359.
- Callgrind Ir (`--singleThreaded --pretty false`): domain-model
  1,156,103,804 → 1,154,370,415 (−0.15%); generic-imports 342,935,869 →
  342,937,447 (+0.0005%).
- Median child CPU new/old (21 samples): domain-model 1.010, generic-imports
  0.997; `diagnostics_match: true`.

The case converts only with the route (§2). Alone, this commit is a
laziness change with no print effect.

### Ownership and work boundaries (checker port convention)

- **Native operations:** `getTypeOfMappedSymbol` (lazy `valueSymbolLinks.resolvedType`);
  `createTypeNodesFromResolvedType` / `checkTruncationLength` /
  `noTruncationMaximumTruncationLength`; `getTypeFromMappedTypeNode`'s
  `typeNodeLinks.resolvedType`.
- **Key identity and owner:** a mapped symbol's type is keyed by
  `(owner TypeId, member index)`. It is owned by that owner's
  `anonymous_properties` entry, which `resolve_mapped_type_members` publishes.
  A copied property still points at its owner. The node's type is keyed by
  `TypeLiteralKey` in `type_literal_types`, as r5-mapped4 §2 set up.
- **Publication states:** an unread slot is `Slot::Mapped`. A slot whose
  read is in progress is `Resolved(errorType)`, which re-entrant readers see.
  A read slot is `Resolved(t)`. The member table itself is published as
  before: empty during resolution, then complete.
- **Receiver/alias context:** the template and parameter are read from the
  owner's `MappedTypeInfo` at read time. An instantiated owner has its own
  info and its own clone parameter (r5-mapped4 §5), so its slots instantiate
  its own template. `node_key` is cleared on instantiation, because the key
  names a written node's context.
- **Expensive work boundary:** template instantiation, once per read slot. On
  the huge case the print reads about 106 of the outer type's 676 slots, as
  native's builder does, instead of all 676 × 676.

## 2. The declared route diff, refreshed (diff; held on Ir only)

[`r5-mapped5-declared-route.diff`](r5-mapped5-declared-route.diff) is
r5-mapped4's [`r5-mapped4-declared-route.diff`](r5-mapped4-declared-route.diff)
regenerated against `declared.rs` at `e20cdd4`. Its content is unchanged:
- the semantic mapped arm (`create_semantic_mapped_type`) runs before the
  written-text mint;
- `keyof any|never|unknown` goes to `resolved_keyof_type` (`.16.108`).

**Measured** on top of §1, both dumps unfiltered, against the frozen base
and against §1's dumps:
- types **+86 RIGHT** vs the base (+85 on §1);
- diagnostics **+1 case** (`bigintIndex`);
- zero verdict losses, and no base-RIGHT key missing;
- **slowcases clean on both dumps.** `hugeDeclarationOutputGetsTruncatedWithError`
  is now 295 ms / 112 MiB, where it was 2,060 ms / 376 MiB before §1. Its
  8 lines are all RIGHT, including the three 1.3 MB truncated prints.

Converted lines by case:
- `verbatim-declarations-parameters` ×7;
- `paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized` ×7;
- `assignmentGenericLookupTypeNarrowing` ×6;
- `deeplyNestedMappedTypes` ×5, `bigintIndex` ×5;
- `mappedTypeWithAny` ×4, `hugeDeclarationOutputGetsTruncatedWithError` ×4,
  `declarationEmitMappedTypePropertyFromNumericStringKey` ×4;
- `mappedTypeModifiers`, `mappedTypeAsClauses`,
  `dependentDestructuredVariablesFromNestedPatterns`, and
  `typeGuardNarrowsIndexedAccessOfKnownProperty11`/`12` ×3 each;
- 18 more cases at 1–2 lines.

**Perf:**
- Median child CPU new/old against §1's binary: domain-model 1.020 at 21
  samples and 1.024 at 41; generic-imports 1.045 at 21 and 1.001 at 41.
  `diagnostics_match: true`.
- Callgrind Ir against §1: domain-model 1,154,370,415 → 1,159,903,424
  (**+0.48%**); generic-imports 342,937,447 → 342,914,452 (−0.007%).

**Why the Ir is not printing.** The brief's hypothesis was that the
non-generic arm's eager member print made domain-model slower. It does not:
- With §1 in place, an experiment that printed every member's type as a
  placeholder (no type reads) left domain-model at +0.45%. The print's own
  reads cost 0.3 M Ir.
- The callgrind diff (§1 binary vs route) is spread out. 8.5 M is in
  `resolved_mapped_object` for 40 `KeysOfType<ModelNNNLine, number>` bodies
  (`{ [K in keyof T]-?: T[K] extends V ? K : never }[keyof T]`), each
  instantiating its conditional per key. The rest is
  `get_type_from_type_node`, `evaluate_conditional_*` and the relater,
  +0.5–3 M each.

Native also reads every member of that body to answer `[keyof T]`
(getIndexedAccessType over a non-generic mapped type reads each key's
`getTypeOfMappedSymbol`). So the extra work is the route doing the
evaluation the written-text mint skipped, not a print. I have not traced how
the base answered those 40 references without it; the CLI diagnostics are
identical.

Two attempts at reducing it, both kept in §1 because they are correct on
their own:
- sharing one member table per node context (domain-model −0.06%);
- publishing an alias instance's capture as the node's type, so the body
  re-evaluated by `binding_type_alias_body` during narrowing answers the
  instance (removes 40 of the 81 member-printed bodies).

**What would decide it.** Either the integrator accepts +0.48% domain-model
Ir for +86 lines and +1 case at CPU 1.02, or the route lands after the
`KeysOfType` evaluation path is profiled to the native operation it stands
for. The second is `declared.rs`' alias body evaluation (r5-declared3).

## 3. `tsr-2zk.1089` (b): getIndexTypeForMappedType over a generic key domain (committed + diff)

**Forcing constraint.** r5-relater6 §3 cause 2: `keyof Mapped5<K>` (`{ [P in
K as P extends `_${string}` ? P : never]: P }`) has the base constraint
`string | number | symbol` in the port. Native's is the filtered keys. Native
reaches them through `computeBaseConstraint`'s Index arm (`checker.go:27523`):
a generic mapped type with a name type and no `keyof` constraint
declaration answers `getIndexTypeForMappedType`. Over a generic constraint,
that maps each constituent of the constraint through the name type
(`forEachType(constraintType, addMemberForKeyType)`, `:26892`). `getIndexType`
itself defers this `keyof` (`shouldDeferIndexType`), and the port's
`resolved_keyof_type` already does.

**Ported.** `index_type_for_generic_mapped_type` (`mapped.rs`):
- the constituents of the constraint, mapped through the name type;
- a concrete string name also contributes `number`;
- a homomorphic mapping answers `None`, where native goes to
  getIndexTypeForGenericType, the deferred `keyof` the caller already holds.

Its caller is `compute_base_constraint`'s deferred-`keyof` arm in
`constraints.rs` (not this lane's):
[`r5-mapped5-generic-mapped-keys-constraint.diff`](r5-mapped5-generic-mapped-keys-constraint.diff).
That diff also removes the function's `cfg_attr(not(test), allow(dead_code))`.

**Measured** with the diff applied, on top of §1, both dumps unfiltered:
byte-identical to §1 (key, verdict, want, got). Zero losses; slowcases clean;
Ir domain-model 1,154,370,415 → 1,154,386,735 and generic-imports
342,937,447 → 342,942,434 (both +0.001%). Nothing converts yet. The relater
still declines these pairs (the `mapped_conditionals` gate, r5-relater6 §3),
and lifting it is r5-relater7's step.

The unit test reads a conditional key by its operands. The key's print for
`Mapped5<K>` is `P extends …` where native prints `K extends …`: the
conditional's written-text mint under bindings is `declared.rs`' (`.16.71`).
The operands are right (`K`), and they are what relating reads.

## 4. `tsr-2zk.1089`: a computed symbol property is keyed by its name type (committed + diff)

**Forcing constraint.** r5-relater6 §4's held decline:
`contextuallyTypedSymbolNamedProperties` types `ap` as `"[A]"` once
`typeof A → T['type']` relates. `getContextualTypeForObjectLiteralElement`
passes the property symbol's `nameType` (`symbolLinks.nameType`,
`checker.go:29928`) to `getIndexedMappedTypeSubstitutedTypeOfContextualType`
(`:30607`). That is `typeof A` for `[A]`, and native falls back to
`getStringLiteralType(name)` only when there is none. The port passed the
display name and always minted a string literal.

**Ported.** `generic_mapped_contextual_property_type_of_key(id, key)` takes
the property name type. `generic_mapped_contextual_property_type(id, name)`
is the no-name-type caller, and the two existing callers in `symbols.rs`
keep it. The object-literal caller in `contextual.rs` (main's) passes the
regular type of a late-bound computed name's expression:
[`r5-mapped5-contextual-name-type.diff`](r5-mapped5-contextual-name-type.diff).

**Measured** with the diff applied, on top of §3, both dumps unfiltered:
byte-identical to §1. Zero losses; slowcases clean. Ir domain-model
1,154,370,415 → 1,154,429,396 (+0.005%), generic-imports 342,937,447 →
342,974,956 (+0.011%). Nothing converts until r5-relater7 lifts the
write-constraint decline that §4 of r5-relater6 held for this case.

## 5. `tsr-2zk.1089`: a concrete mapped alias instance keeps its mapped identity (committed)

**Forcing constraint.** r5-relater6 §4's forced declines: `Partial<Foo1>[K]`
and `Partial<Config>[T]` are `isMappedTypeGenericIndexedAccess`. Their
constraint is the substitution `E[P := X]` (getConstraintFromIndexedAccess,
`checker.go:17227`). The port's concrete instance (`instantiate_identity_mapped_alias`,
`declared.rs`) is minted with the source's members and no mapped info, so
`mapped_indexed_access_constraint` could not substitute.
`mapped_substitution_out_of_reach` declined (`relater.rs`).

**Ported.** Native's instance is a MappedType
(getTypeAliasInstantiation → instantiateMappedType). `ensure_mapped_type_info`
now captures a mapped alias instance's parts on first ask. That covers a
reference target `(alias, arguments)`, and the image of an argument-less
alias (`Funcs`). It goes through `capture_mapped_alias`, which already
declines unless the alias body is a mapped node. The members still come from
the source; only the mapped info is added, and only for the 13 callers that
ask for it (relater, inference, mapped). **No `declared.rs` change was
needed.** The brief expected one, but the capture belongs on the read side
the relater already calls.

**Measured** against §1 (with §3 and §4's diffs measured separately above),
both dumps unfiltered:
- types **+3 RIGHT**: `correlatedUnions:0:469/470/475`. Two of them were GAP
  (`error`). The `Config[T]` lines now type through the substitution;
- diagnostics unchanged;
- zero losses; slowcases clean;
- Ir: domain-model 1,154,370,415 → 1,154,901,490 (+0.046%), generic-imports
  342,937,447 → 342,971,668 (+0.010%);
- median CPU (21 samples): 0.975 and 1.012.

The relater's `mapped_substitution_out_of_reach` decline should now never
fire for these instances, since the object has mapped info. r5-relater7 can
retire it, and the intersection-source variant
(`NonNullable<Partial<Config>[T]> → Config[T]`), after re-measuring.

## 6. `reducibleIndexedAccessTypes`: isGenericReducibleType (committed + three diffs)

**Forcing constraint.** r5-mapped4 §6 found the root cause.
`(Payload & { dataType: K })["data"]` under
`{ [K in Type]?: (data: …) => void }` must stay deferred, so that the
instance's `K := Type.A` reduces the union to `PayloadA & { dataType: Type.A }`
and the read is `string`. Native defers through `shouldDeferIndexedAccessType`'s
third disjunct, `isGenericReducibleType` (`checker.go:27370`, `:24932`). The
port projected the whole value union at once.

**Ported, in this lane's files:**
- `Intrinsics::unique_literal`: `uniqueLiteralType` (`checker.go:1015`), a
  distinct `never`. It is created last, so every earlier intrinsic keeps its
  slot (`tests/globals.rs` slot list, 31 types).
- `is_generic_reducible_type` and `is_reducible_intersection`
  (`intersections.rs`): instantiate with every type parameter mapped to
  `uniqueLiteralType`, then ask `get_reduced_type`. The port's instantiation
  takes an explicit mapper, so the parameters are collected first. They are
  the constituents and property types (and their union constituents) that
  are type parameters, which are the positions `isDiscriminantWithNeverType`
  reads. Native caches the instantiation on the intersection; the port
  recomputes it, since only a type-level access with an otherwise concrete
  union object asks.

**Diffs, one per owner, measured together:**
- [`r5-mapped5-reducible-indexed-access.diff`](r5-mapped5-reducible-indexed-access.diff)
  (`indexed.rs`, r5-errorsplit5), with two hunks:
  - `resolved_indexed_access_type` reduces its object first (`getReducedType`,
    `checker.go:26939`);
  - it defers a generic reducible union object. It also removes
    `intersections.rs`' `allow(dead_code)`.
  - The expression road needs no arm: native applies the reducible disjunct
    only without an access node or for an `IndexedAccessTypeNode`
    (`:27374`).
- [`r5-mapped5-unique-literal-flow.diff`](r5-mapped5-unique-literal-flow.diff)
  (`flow.rs`, main's): `intersection_has_never_discriminant` does not count a
  `uniqueLiteralType` property as HasNeverType
  (createUnionOrIntersectionProperty, `:21621`).
- [`r5-mapped5-reducible-keyof.diff`](r5-mapped5-reducible-keyof.diff)
  (`declared.rs`, r5-declared3), with two hunks:
  - `resolved_keyof_type` reduces its operand first (`getIndexTypeEx`,
    `:26685`);
  - it defers a generic reducible union (shouldDeferIndexType, `:26838`).
  - **Divergence kept:** native's keyof-target relation arm (relater.go:3514)
    passes `IndexFlagsNoReducibleCheck`. The port's `resolved_keyof_type` has
    no index flags.

**Measured,** all three diffs on top of §5, both dumps unfiltered:
- types **+14 RIGHT**:
  - `reducibleIndexedAccessTypes:0:19/22/23/27/28/33` (all six);
  - `iterableWithNeverAsUnionMember(target=esnext)` ×4;
  - `intersectionReduction` and `intersectionReductionStrict` `:55` and
    `:58`.
- diagnostics unchanged;
- zero losses against §5 and against the frozen base; slowcases clean;
- Ir against §5: domain-model 1,154,901,490 → 1,155,222,864 (+0.028%),
  generic-imports 342,971,668 → 342,954,503 (−0.005%).

One WRONG line changes text: `mappedTypeNotMistakenlyHomomorphic:0:26`
(`keyof Gen2<ABC.A>`, native `"a" | "v"`) printed `"v"` and now prints the
deferred `keyof ({ v: ABC.A; } & ({ … } | { … }))`. `keyof Gen<T>` is now
deferred, as native's is, because `Gen<T>` is a generic reducible union. Its
instance does not re-resolve, because the port keeps
`{ v: A } & (X | Y)` as an intersection of a union. Native distributes it
into a union of intersections when it creates the type, and that union then
reduces. That is `intersections.rs`' distribution rule, not this port's
deferral. It is a follow-up for this lane.

The commit alone (no diffs) is byte-identical to §5 on both dumps.
`crates/tsr-conformance/tests/undefined_widening_modes.rs` asserts the
construction-time type count, and its 30 becomes 31. That one number is the
only change in a harness file, flagged for the integrator. The unit coverage
is the corpus case. The reducible predicate needs the
`flow.rs` diff to answer true, so a unit test here would only exercise the
false arm.
