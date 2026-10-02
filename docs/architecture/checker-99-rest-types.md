# Object rest keys, `Omit` member reads and `?` optionality (tsr-8)

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Unit: `bd tsr-8` (spread distribution, ordering and semantic members).

## Relationship to tsr-6.25

This unit was developed in parallel with tsr-6.25
([rest index infos](checker-99-rest-index-infos.md)), and the two converged
on the same port of the non-generic `getRestType` tail (`checker.go:17792`):
rest properties copied through `spread_properties`, nullable filtering, union
distribution, the unspreadable-parent guard (`checker.go:17724`) and native
`ast.IsAssignmentTarget` array-literal tuples with
checkExpressionForMutableLocation elements (`checker.go:8026`). tsr-6.25
landed first; on rebase its version was kept and this unit keeps only the
three pieces it did not contain. Independently reaching the same code is
itself evidence for that shape: both drafts were derived from the native
source, not from each other.

## The forcing observation

At `9044e902`, after the shared rest port, `compiler/destructuringUnspreadableIntoRest`
still held rows that no copy of the rest members could fix:

1. The generic `Omit<T, keys>` mint pushed `private`/`protected` names into the
   omit list. Native collects `unspreadableToRestKeys` through
   `getLiteralTypeFromProperty(prop, …, includeNonPublic=false)`, which is
   `never` for a non-public member, so those names never reach the union. The
   baseline wants `Omit<this, "getter" | "method" | "setter">`.
2. Property reads on `Omit<T, K>` read `T`'s member directly. Native resolves
   `Pick<T, Exclude<keyof T, K>>`: `keyof T` excludes non-public members, so
   `rest1.privateProp` is an error (printed `any`); and for a generic `T` the
   resolved member is the template `T[P]` with `P` fixed, a deferred indexed
   access (`rest1.publicProp : this["publicProp"]`).

Fixing (1) alone measured 8 RIGHT→WRONG: the private names had been making
`rest1.privateProp` fail by accident, through the omit list. (2) is what made
(1) landable — the read now fails for the native reason.

3. `property_is_optional` treated any postfix token as optional, so
   `remainder!: string` copied into a rest as `remainder?: string`
   (`objectRest`). The binder sets `SymbolFlagsOptional` only for `?`; the
   helper now tests the token kind. It is shared, so every reader of
   member optionality moved with it; the full run below is its measurement.

## What changed

- `object_rest_type`'s generic branch (`destructure.rs`) skips non-public
  members when collecting unspreadable keys; `is_non_public_member`
  (`spreads.rs`) is the shared `getDeclarationModifierFlagsFromSymbol &
  (Private|Protected)` reader, also used by `spread_properties`.
- The `Omit<T, K>` branch of property lookup (`members.rs`) refuses a
  non-public member of the apparent type and, for a type-parameter `T`,
  returns `resolved_indexed_access_type(T, "name")`.
- `property_is_optional` (`members.rs`) requires `QuestionToken`.

## Measurement

Iteration against `7332f284` (my own rest port, before rebasing onto
tsr-6.25) took the private-key fix from 10 RIGHT losses to 0 by adding the
`Omit` read, and the `?` fix converted `objectRest:116`. After rebasing onto
tsr-6.25's version, the full run against a separate worktree at `7184f54a`
(457,321 RIGHT) reads 457,356: +35 (32 W→R in
`destructuringUnspreadableIntoRest`, one each in `objectRest` and
`mappedTypeConstraints`, one G→R in `mappedTypeConstraints`), zero RIGHT
losses, zero GAP→WRONG and no changed WRONG rows.

## Rejected and remaining

- Passing the rest declaration's symbol as the minted type's owner was tried
  and abandoned: this port's `Named { members }` owner is a member table, not
  upstream's naming symbol.
- The generic test is still "bare type parameter". Native
  `isGenericObjectType(source) || isGenericIndexType(omitKeyType)` also covers
  generic mapped/indexed sources and computed keys.
- `Omit<T, K>` reads on a non-type-parameter generic `T` (a generic mapped
  type, an indexed access) still read the member directly.
- `for-of49` prints `[string, ...[boolean]]` where native createTupleType
  flattens a spread tuple to `[string, boolean]`; that is the display-only
  rest branch of `check_array_literal`.

## How this would be shown wrong

A read on `Omit<T, K>` whose `T` is a constrained type parameter with an
index signature, or whose key is numeric, answering a deferred `T["k"]` where
native resolves (or the reverse), would show up as RIGHT→WRONG in
`genericObjectRest`, `objectRestNegative` or a mapped-type case.
