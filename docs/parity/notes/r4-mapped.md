# r4-mapped — homomorphic mapped types (`tsr-2zk.925`, `tsr-2zk.16.121`)

Round-4 lane. Owns `crates/tsr-checker/src/mapped.rs`' instantiation and
member-resolution functions. Native reference: `vendor/typescript-go` @
`5b1047d`, `internal/checker/checker.go`. Every hypothesis below was confirmed
against a native `tsgo` built by `scripts/offline-cargo/build-tsgo.sh` before
code was written; the probes are quoted with the native output.

## §1 Cause 5 — a union argument is distributed before its constraint is read

**Native.** `instantiateMappedType` (checker.go:22535) instantiates the
homomorphic type variable first and, when it changed, returns
`mapTypeWithAlias(getReducedType(mappedTypeVariable), instantiateConstituent,
alias)`. The constraint (`keyof T`) of the *whole* union is never computed on
that path: each constituent becomes its own `{ [P in keyof A]: X }`.

**TSR before.** A generic mapped alias reference (`declared.rs`, the generic
alias mint) calls `capture_mapped_alias`, which evaluates the mapped type's
parts under the alias bindings — including `keyof (A | B)` — and then
`instantiate_mapped_alias_sequence`, which distributes a union argument. If
the eager `keyof` fails, `mapped_type_info` returns `None`, nothing is
captured, and `instantiate_mapped_alias_sequence` returned `None` before it
looked at the argument. The reference was left as a bare named type with an
empty member table.

`keyof` fails for an interface whose base has type arguments:
`declared.rs` `collect_keyof_property_names` walks `base_symbols_of`, which
refuses `extends Base<number>` (its reason — the base's *member types* need
instantiation — does not apply to key *names*). Measured probe:

```ts
interface Node { kind: number }
interface Base<T> extends Node { p: T }
interface OLE extends Base<number> {}
interface OLF extends Node { q: string }
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
declare const v: Mutable<OLE | OLF>;
export const d1: string = v.kind;       // native TS2322 number→string; TSR silent
export const d: Node = v;                // native OK; TSR TS2322
declare const k2: keyof (OLE | OLF);
export const a2: number = k2;            // native TS2322; TSR silent (errorType)
```

**Port.** `instantiate_mapped_alias_sequence` now reads the homomorphic type
variable syntactically when capture declined
(`mapped_alias_homomorphic_parameter`, the same `keyof T` operand test
`mapped_type_info` uses — `getHomomorphicTypeVariable` reads only the
constraint declaration's operand) and runs the union arm
(`distribute_mapped_union`, shared with `instantiate_mapped_sequence`). Each
constituent then takes the ordinary alias road, so `Mutable<OLE>` is the
identity mint it already was. Nothing is added to any table; the union is the
same `union_with_origin_text` image the captured path already produced.

**Not ported (not owned).** The `keyof (OLE | OLF)` answer itself (the `k2`
probe) stays `errorType` until `collect_keyof_property_names` follows
type-argumented bases for names: the measured patch is
[`r4-mapped-keyof-generic-base.diff`](r4-mapped-keyof-generic-base.diff).

**Falsifier.** A union argument whose constituents need the whole union's
`keyof` (a non-homomorphic mapped type) would be wrong here; the fallback
only runs when the constraint is `keyof <alias type parameter>`, which is
exactly native's homomorphic test.

## §2 Cause 8 — concrete composite modifiers types enumerate their properties

**Native.** `isArrayOrTupleOrIntersection` (checker.go:23560) is true only
when *every* intersection constituent is an array or tuple, so
`Readonly<string[] & { __brand: any }>` is **not** distributed; it goes to
`instantiateAnonymousType`, and `resolveMappedTypeMembers` (checker.go:20894)
enumerates `getPropertiesOfType(modifiersType)` through
`forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType`. For an
intersection that is `getPropertiesOfUnionOrIntersectionType`
(checker.go:18861): every constituent's own properties, in order, each kept
when the combined property resolves (a union also drops partial
properties and stops after the first constituent without index
signatures). The repro's comment in `realworld_repros.rs` said native maps
the intersection's constituents; it does not, and the comment is corrected.

```ts
type P = string[] & { __brand: any };
declare const p: Readonly<P>;
export const n: string = p.length;   // native TS2322 number→string
export const f: string = p.push;     // native TS2322 (...items: string[]) => number
```

**TSR before.** `mapped_member_keys`' concrete branch called
`property_names_of(source)`, which reads one owner's member table and has none
for a union or intersection: the member table was published complete and
empty, so `p.length` was TS2339.

**Alternative rejected.** Routing composites through the certified
`get_property_names_of_type` (as the generic branch does) declines on
`string[]`: array methods mention `this`, and that enumeration refuses any
member type that does. The mapped key walk needs names only — each member's
value is the template under its key — so the certified type checks are not
this consumer's obligation. Measured: with that routing `p.length` became
*silent* (declined members), not `number`.

**Port.** `composite_modifiers_property_names` is
`getPropertiesOfUnionOrIntersectionType`'s loop: candidates from each
constituent's `property_names_of`, kept when `get_property_of_type(source,
name)` resolves, union early exit on a constituent with no index infos. It
publishes nothing; the caller's member image stays the only table.

**Known remaining difference.** Assignment to a mapped property
(`p.length = 3` under `Readonly`) is not TS2540 in TSR even for a single
written mapped type (`type M1 = { readonly [K in keyof O]: O[K] }; m1.a = 1`):
`readonly_target.rs` does not read the mapped image's readonly flag. Not
owned; reported, not ported.

## §3 `tsr-2zk.16.121` — template optionality (not committed)

Native (confirmed with tsgo): `getTemplateTypeFromMappedType` (checker.go:22697)
adds optionality for `?`, so a `?` map's index value is `number | undefined`
(`{ [P in keyof Foo]?: Foo[P] }["other"]`), and `createMappedTypeNodeFromType`
(nodebuilderimpl.go:1471) prints `removeMissingType(template)`, i.e.
`{ [P in keyof T]?: T[P] | undefined; }`.

The owned arms (`mapped_type_text` and the index arm of
`resolve_mapped_type_members_worker`) are ported in
[`r4-mapped-template-optionality.diff`](r4-mapped-template-optionality.diff).
Measured on top of the lane head: diagnostics dump unchanged; it converts
none of the six target cases, because their lines come from code this lane
does not own:

- the printed `{ [P in keyof T]?: T[P] | undefined; }` lines are written
  mapped type nodes printed by `declared.rs`' written-node renderer
  (`written_type_text`), not by `mapped_type_text`;
- `mappedTypeModifiers:100` (`Partial<Foo>["other"]`) goes through the
  identity mint `declared.rs` `instantiate_identity_mapped_alias`, whose
  index reads the source owner's index info without the `?` optionality.

The diff was held at wrap-up (types dump not finished); both declared.rs
arms are the next step for this issue.

## §4 Held cross-lane diff: `keyof` through a type-argumented base

[`r4-mapped-keyof-generic-base.diff`](r4-mapped-keyof-generic-base.diff)
(`declared.rs` `collect_keyof_property_names`: follow `extends Base<number>`
for key names via `base_symbols_of_ex(owner, false)`). Fixes the §1 `k2`
probe (`keyof (OLE | OLF)` is `"kind"`, native TS2322). Measured on top of
the lane head: diagnostics dump unchanged; the types dump did not finish
before wrap-up, so it is not yet loss-checked on types.

## Remaining differences seen (not owned)

- TS2540 on assignment through a mapped property: `readonly_target.rs` does
  not read the mapped member image's readonly flag (`Mutable<A | B>`'s
  `x.flags = 1` reports TS2540; native does not). jsTyping: 4 lines
  (binder.ts 2345/2348, nodeFactory.ts 2547/2548).
- Printing a type-literal argument's optional property (`b?: 2` vs native
  `b?: 2 | undefined`) inside `Readonly<…>`'s alias arguments.

## Ownership and work boundaries (checker port convention)

- **Native operations:** `instantiateMappedType` union arm;
  `getPropertiesOfUnionOrIntersectionType` as used by
  `resolveMappedTypeMembers`.
- **Identity/owner:** no new cache, side table or member image. The
  distributed union is keyed as before by the caller's `instantiations`
  entry `(alias symbol, arguments)`.
- **Publication:** unchanged. A declined composite enumeration cannot occur
  (the walk returns a possibly short list, as `property_names_of` already did
  for single owners); a composite whose combined property does not resolve
  loses that name, which is native's `combinedProp == nil` arm.
- **Work boundary:** the composite walk runs once per mapped member
  resolution of a concrete composite source (`anonymous_properties` publishes
  the result); the fallback distribution runs only when capture declined.
