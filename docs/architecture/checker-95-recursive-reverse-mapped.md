# Recursive reverse mapped inference

Pinned tsgo: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Issue: bd tsr-6.9.

## The forcing constraint

Three corpus families failed the same way. `Spec<T> = { [P in keyof T]: Func<T[P]> |
Spec<T[P]> }` reversed `{ nested: { mul: fn } }` to `{ nested: unknown }`;
`Deep<T> = { [K in keyof T]: Deep<T[K]> }` reversed `interface A { a: A }` to
`{ a: unknown }`; and `{ [P in keyof T]?: T[P] }` reversed `a?: number` to
`a: number | undefined`. Native answers are `{ nested: { mul: string } }`,
`{ a: { a: any } }` and `a: number`.

Four upstream mechanisms were missing.

1. **Self references had no mapped metadata.** `capture_mapped_alias` guards an
   alias against re-entry, so `Spec<T[P]>` created while `Spec<T>` is captured was
   never captured. Upstream reads `getConstraintTypeFromMappedType` and
   `getTemplateTypeFromMappedType` lazily (`checker.go:22697`). The guard now
   records such references in `deferred_mapped_aliases`; `ensure_mapped_type_info`
   captures them on first inference use, after the outer capture has finished.
2. **The template lacked optionality.** `getTemplateTypeFromMappedType` adds
   `undefined` for a `?` mapped type (`addOptionalityEx`, checker.go:22701), so
   reverse inference from `number | undefined` to `T[P] | undefined` matches the
   `undefined` away. `mapped_template_type` applies that only to the inference
   targets (`inferReverseMappedTypeWorker` and the non-homomorphic template
   fallback); forward member substitution keeps the written template because the
   property's optional flag already carries the `undefined`.
3. **Reverse recursion had no expanding guard.** `inferReverseMappedType`
   (`inference.go:1066`) keeps reverse source/target stacks and stops when both
   are deeply nested. That needs mapped types to share a recursion identity:
   `getRecursionIdentity` tracks an object by its symbol, which every
   instantiation of one mapped type node shares, so `relation_recursion_identity`
   now answers the mapped declaration for any captured mapped type. The old
   unknown-sentinel member cache is replaced by those stacks.
4. **Reverse mapped objects were eager.** Upstream's `createReverseMappedType`
   returns an object whose members `resolveReverseMappedTypeMembers` produces when
   read, and property links apply `replaceIndexedAccess` (`inference.go:1133`) so a
   reversal through `T[K]` is cached as the reversal through `T`. That laziness is
   what keeps `Deep<XMLHttpRequest>` finite: native resolves only the members a
   program reads. The first eager attempt here exhausted memory on
   `mappedTypeRecursiveInference` (the whole DOM graph, with nested printed text).

## The design, and why it is not fully lazy

This port prints at creation: a type's text is fixed when it is minted
(`TypeData::Named`), so a fully lazy reverse object would print wrong whenever a
consumer printed it before reading a member. The node builder supplies the escape:
`shouldUsePlaceholderForProperty` (`nodebuilderimpl.go:2302`) prints `any` for a
reverse mapped property nested under a property whose source type is not
anonymous (interfaces, classes, references). Such a nested reverse object needs
only its member names to be printed in its parent.

So `reverse_homomorphic_mapped_type` mints every reverse object with an
`any`-placeholder text and records it in `pending_reverse_mapped`. It completes the
object immediately unless it is being created for a property whose source type is
non-anonymous (`reverse_property_anonymous`). A pending object completes when a
consumer reads it: `resolve_mapped_type_members` (the existing lazy member hook
used by property lookup, index signatures, spreads and name enumeration), a
property-type read in `get_type_of_property_with_this_argument`, or a cache hit
outside a placeholder context. Completion recomputes the member plan, applies
`replaceIndexedAccess` by instantiating the target with `K → 0` and `T → [T]`,
resolves member types and replaces the reserved identity's text through
`TypeStore::complete_object`. A member whose source type is non-anonymous prints
its reverse type's recorded placeholder text, so a later completion of that nested
object does not change its parent's text.

Cycles fall out of the cache: completing `R1 = reverse(A)` for `A { a: A }` reads
the member reversal `(A, Deep<T>, keyof T)`, which is cached as `R1` itself, so
`out.a.a.a` prints `{ a: { a: any; }; }` as native does.

The reverse filter for intersection constraints (`getLimitedConstraint`) also
compared `keyof U` by identity. `resolved_keyof_type` mints a fresh deferred index
type per call while `getIndexType` caches one per operand, so the filter now treats
two deferred keyof types with the same operand as the reverse constraint. Upstream's
emptiness test reads the source's properties before filtering; the plan follows it.

## Alternatives considered

- **Eager recursion with a depth cap.** Rejected: any cap is a fitted number, and
  the eager DOM walk exhausted memory before a cap mattered.
- **Fully lazy reverse objects with lazy text.** The faithful shape, but printing
  takes `&Type` with no store access; making text lazy is a printer redesign. If
  the printer becomes able to render from structure (ADR-0044's reach work), the
  placeholder bookkeeping here should be deleted in favour of native laziness.
- **Interning deferred `keyof`.** Would fix the filter identity everywhere, but
  changes identity corpus-wide; the narrower operand comparison is local to the
  rule that needed it.

## Accepted consequences and known limits

- A pending object printed before any member read prints placeholders. Reads
  through paths that bypass `resolve_mapped_type_members` (direct
  `anonymous_properties` access) see no members until something completes it.
- Rule (2) is applied only where the reverse type is the member type itself; a
  nested reverse type inside a union or array keeps its completed text. Rules (1)
  and (3) of `shouldUsePlaceholderForProperty` are not ported.
- Primitive sources still decline: native reverses `string` through its apparent
  `String` members (`inferFromTypes` takes the apparent source), giving
  `label: { toString: any; ... }`; this port answers `unknown`.
- Native's `ExpandingFlags` on index-signature reversals print `{ [x: string]:
  any; }` for `interface B { [s: string]: B }`; this port still prints a nested
  index signature there (already-wrong rows, unchanged).

## How we would know this is wrong

A RIGHT row printing `any` where a nested reverse member should be expanded, or a
reverse object whose properties read as its source's declared types, would mean a
consumer bypassed completion. The regression test
`crates/tsr-conformance/tests/reverse_mapped.rs`
(`recursive_reverse_mappings_resolve_members_on_demand`) pins the recursive,
optional, cyclic-interface and alias-function controls against pinned tsgo
declaration output.

## Measurement

See STATUS.md §7 for the landing row; the numbers there are measured at the
rebased commit that was pushed.
