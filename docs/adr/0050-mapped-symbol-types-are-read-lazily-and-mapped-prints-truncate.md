# ADR-0050: Mapped property types are instantiated on first read, and a mapped type's member print truncates as the node builder does

- **Status:** Accepted. Built in `crates/tsr-checker/src/mapped.rs`
  (`get_type_of_mapped_symbol`, `mapped_object_text`, `TruncationBudget`) and
  `crates/tsr-checker/src/objects.rs` (`property_slot::Slot::Mapped`).
- **Date:** 2026-10-09
- **Issue:** `bd tsr-2zk.1033` (lane r5-mapped5). Measurements:
  [`docs/parity/notes/r5-mapped5.md`](../parity/notes/r5-mapped5.md) §1.
- **Pinned upstream:** `vendor/typescript-go` @ `5b1047d`.

## The forcing constraint

r5-mapped3's declared route (a written mapped node becomes a `MappedType`
built from its typed parts; a non-generic one prints its members) is held on
one case. `hugeDeclarationOutputGetsTruncatedWithError` writes
`{ [K in manyprops]: { [K2 in manyprops]: `${K}.${K2}` } }` with 676 keys on
each level: 457,000 members. The port resolved every member's type when the
node was evaluated, and printed them all: 172 ms / 45 MiB at the base,
2,060 ms / 376 MiB with the route (r5-mapped4.md §4), a `SLOWER` case.

Native does two things the port did not:

1. **A mapped property's type is read lazily.** `resolveMappedTypeMembers`
   (`checker.go:20894`) creates one symbol per key with a `keyType`, and
   `getTypeOfMappedSymbol` (`checker.go:20984`) instantiates the template on
   the first `getTypeOfSymbol`, publishing it on `valueSymbolLinks`. So
   resolving the outer type's 676 members instantiates none of the 676 inner
   mapped types.
2. **The printer stops.** `.types` baselines print with
   `TypeFormatFlagsNoTruncation` (`type_symbol_baseline.go:394`), which the
   node builder still bounds at `noTruncationMaximumTruncationLength`
   (1,000,000, `nodebuilderimpl.go:114`). `createTypeNodesFromResolvedType`
   (`:2627`) checks `checkTruncationLength` before each property; once the
   approximate length passes the limit it writes the last property and stops,
   and any object printed after that is a `NotEmittedTypeElement` (`{  }` once
   comments are dropped). Native reads only the ~106 inner types it prints.

This port computes a type's text when the type is minted (`TypeData::Named`,
`types.rs`), so the member print happens at mint, not at print time. That
is the constraint every option below works around.

## The decision

- **`Slot::Mapped`**: a mapped type's property slot records its owner, index,
  key and whether the modifier strips optionality. `property_type` asks
  `get_type_of_mapped_symbol`, which instantiates the template once and
  publishes a `Slot::Resolved` into the owner's member table. A read that
  re-enters while the instantiation runs answers `errorType`, as native's
  failed `pushTypeResolution` does. The printed slot is on-demand, so the
  print reads the same published type.
- **`mapped_object_text`** prints a resolved mapped type's members with the
  node builder's budget (`TruncationBudget`): `len(name) + 1` per property
  (`:2522`), a string literal's value plus 2 (`:3288`), `+ 2` per type literal
  (`:2745`), `+ 9` for `readonly`; it applies the `i + 2 < len − 1` rule and
  the sticky `truncating` flag, and prints a property whose type is itself a
  resolved mapped object by recursing with the same budget, as the builder
  does.
- **One mapped type per node and context.** `getTypeFromMappedTypeNode`
  answers one type per node (`typeNodeLinks.resolvedType`). The port can
  capture the same node in the same alias-evaluation context twice: an alias
  instance (`capture_mapped_alias`) and the node's own evaluation
  (`create_semantic_mapped_type`). The first capture is published in
  `type_literal_types` under the node's `TypeLiteralKey`, and a later
  capture with the same key reads the first's member table. Without this,
  the slots of two images of one type were instantiated twice.

## Alternatives

1. **Lazy text: print a type when it is first printed.** This is native's
   model and would also defer member *resolution*. Every `type_to_string` reads
   the baked text through `&self` (577 call sites across the checker and the
   conformance crate), and printing a mapped type needs `&mut` to instantiate
   its property types. Making the text lazy means making every printer
   `&mut`, a cross-cutting change outside this lane. This option wins if that
   refactor is done for another reason. Then `mapped_object_text` becomes the
   print-time renderer unchanged.
2. **Keep members eager and make the printer cheaper.** Rejected. The cost
   is the 457,000 template instantiations, not the string building. The text
   needs every property type, and so did the eager member table.
3. **Truncate without lazy slots.** Rejected. The member table would still
   instantiate all 676 inner types when the outer type is resolved, and each
   inner type's mint resolves and prints its own 676 members.
4. **Use the written text for very large mapped types.** Rejected. It is a
   size heuristic, and it prints the wrong text (the base's 60 characters
   were WRONG).

## Consequences

- The case converts: all 8 lines RIGHT, including the three 1.3 MB lines that
  end `…; vi: "fr.vi"; zz: "fr.zz"; }; zz: {  }; }`. Cost: 335 ms / 113 MiB.
- **Approximate lengths for other kinds are the printed length.** A property
  type that is not a string literal or a resolved mapped object counts its
  text length, not native's per-node estimate (for example, `string` counts 6
  in both, but a type reference counts its symbol name only in native). The
  two differ only when a print passes 1,000,000.
- **A type's text is still fixed at mint**, so a mapped type nested in
  another composite prints its own text, truncated from length 0. Native
  shares one budget across the whole print. This also differs only past
  1,000,000.
- **Circularity.** The re-entrant read gets `errorType`, as in native. The
  outer read keeps its instantiation. Native also turns the outer read into
  `errorType` and reports TS2615. The eager table this replaces had no
  circularity at all, and no measured case reaches it.
- `peek_property_type` answers `None` for a mapped slot not yet read. A
  `&self` walk follows no edge for it, as for an unread accessor.

## How we would know we were wrong

- A case whose print passes 1,000,000 and whose truncation point differs from
  native. That means the length approximation for some kind matters, and that
  kind's native estimate should be ported.
- A property read that yields a different type before and after a member
  table's other slots are read. That means a template instantiation depends
  on another instantiation's side effect, which the eager loop used to order.
- A `TS2615` in native output for a mapped type the port answers without
  error.
