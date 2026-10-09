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
