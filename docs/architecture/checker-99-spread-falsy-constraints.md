# Falsy generic constraints in object spread (tsr-8)

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.

## The missing native step

`tryMergeUnionOfObjectTypeAndEmptyObject` (`checker.go:13530`) first classifies
each union constituent with `isEmptyObjectTypeOrSpreadsIntoEmptyObject`
(`checker.go:13604`). A type parameter is not classified through its
constraint there. Thus, in `object | T` where `T extends undefined`, `object`
spreads into the empty object and `T` is the sole nominally nonempty
constituent.

Native then obtains the properties and index infos of that constituent.
Structured member resolution consults `T`'s base constraint, so `T extends
undefined` has no properties or indexes. The partial-object mint is therefore
`{}`. The port instead called `spread_properties(T)` directly; that semantic
boundary cannot enumerate a bare type parameter and returned the original
union, producing `T | {}`.

The fix preserves native's order. Classification still does not inspect a
type-parameter constraint. Only the later semantic-member read uses
`base_constraint_of_type`, and only when that constraint itself is empty or
spreads into empty. Truthy primitive constraints remain invalid spread
sources.

The two empty predicates remain separate, matching native. Actual
`isEmptyObjectType` owns union (`some`) and intersection (`every`) recursion.
`isEmptyObjectTypeOrSpreadsIntoEmptyObject` calls that predicate, then applies
its primitive mask only to the type at hand. In the all-empty branch, the fold
searches separately for an actual empty object and returns `emptyObjectType`
when there is none; a nullish or primitive constituent can never overwrite a
real empty object. Both helpers are private to the existing spread-union merge
seam.

## Evidence

The pinned `spreadObjectOrFalsy.types` baseline is the native control:

- `f4<T extends undefined>(a: object | T)` returns `{}`;
- its spread expression is `{}`;
- `g1` remains wrong for an independent intersection-normalization reason.

The focused checker regression copies the first expected signature and pairs
it with a truthy-string constraint refusal. Two more controls distinguish the
invalid `null | undefined` spread from `{} | null | undefined`, whose actual
empty-object constituent is preserved. Full `scorepair` against the exact
starting `cec7cef5` verdict moves two rows, both WRONG-to-RIGHT in
`conformance/spreadObjectOrFalsy`, with zero RIGHT losses, GAP-to-WRONG, or
changed already-WRONG rows: 457,641 to 457,643 matching assertions.

## Refused adjacent experiment: global empty-literal identity

Before this bounded unit, tsr-8.1 explored sharing every member-less,
alias-less type literal as native's `emptyTypeLiteralType`. It corrected the
11 `spreadUnion2` ordering rows, but this port also uses type identity in
conditional evaluation, narrowing and relation shortcuts that are not yet
compatible with that global identity. The full run lost 46 RIGHT rows and
turned seven more RIGHT rows into GAPs (53 adverse transitions), across
`indexingTypesWithNever`, `thisTypeInFunctions2`, type guards and related
families. The rejected patch is preserved at
`/tmp/tsr8/recovered-empty-type-literal.patch`; it is evidence, not an
integration candidate. A future tsr-8.1 implementation must first separate
the native identity from those port-specific shortcuts.

## Remaining boundary

The spread-empty predicate still recognizes concrete empty objects through
the port's captured anonymous-object seam. Full native `isEmptyObjectType`
also resolves other structured object forms. Widening that classification is
separate work: it can change generic/intersection reductions globally and
requires its own full transition measurement.
