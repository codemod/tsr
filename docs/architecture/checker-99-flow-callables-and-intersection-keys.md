# Aliased callables in narrowing and intersection keys in deferred reads

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 5871239: 457,356/478,855 matching assertions (95.51%).
Unit: bd tsr-6.34 (adjusted-fact consumers and generic indexed reads).

## 1. `L & Function` was a relater gate reading a print flag

`narrowingByTypeofInSwitch` and `nonNullReferenceMatching` answered
`L & Function` / `X & Function` / `ElementRef & Function` where native records
`L`, `X` and `ElementRef` after `typeof x === "function"`. `narrowTypeByTypeFacts`
(internal/checker/flow.go) is already transliterated arm for arm
(`narrow_type_by_type_facts`): strict-subtype of the implied `Function` keeps the
constituent; otherwise the intersection arm fires. So the defect sat in
`isTypeRelatedTo(L, Function, strictSubtype)`, as checker-notes-deferred.md §844 predicted.

A direct probe of the relater showed `L -> Function` answering **Unknown**
without ever reaching `properties_related_to`. The reason is the relater's
structured-type gate, `has_members`, which admitted an anonymous object only when
`TypeData::Anonymous { signature: true }`. That field is the printed node kind
(a bare `FunctionTypeNode`, used by the union formatter to parenthesise). An
aliased function type `type L = (x: number) => string` prints as `L` and so
records `signature: false`, although it carries the same call signatures. The
gate therefore treated every aliased or otherwise non-bare-printed callable as
an object with no member table. Upstream's gate is the type's flags
(`StructuredOrInstantiable`), never its printed spelling.

`has_members` now also admits an `Anonymous` type whose registered signature list
is non-empty. Property lookup on a callable already falls back to the global
`Function` members (`getPropertyOfType`'s resolved-call-signature fallback,
ported in members.rs), so `L -> Function` relates through `apply`, `call`,
`bind`, `toString`, `length`, ... and the strict-subtype arm keeps `L`. For a
type parameter `X extends L`, the source constraint road reaches the same
comparison.

Alternatives considered:

- Special-casing `Function` as a target in `narrow_type_by_type_facts`. Rejected:
  it would hide the gate defect from every other relation consumer, and the
  measured transitions show other consumers benefit (`strictBindCallApply1`
  overload sets, `multiSignatureTypeInference`).
- Setting `signature: true` on aliased callables. Rejected: the flag is a print
  decision; flipping it would parenthesise alias names in unions.

How this would be shown wrong: a RIGHT row lost where an aliased callable now
relates structurally to an object target with required members; none occurred
in the full run below.

## 2. `NonNullable<T>[K]` — the generic key belongs to an intersection operand

`typeVariableTypeGuards:87` (`obj[key]` with `obj: NonNullable<T>` after a
truthiness guard and `key: K extends keyof T`) and `controlFlowGenericTypes`
answered `error`/`any` where native defers to `NonNullable<T>[K]`.
`deferred_indexed_access` (indexed.rs) admits a generic index only when its
`keyof` operand is this object — a deliberate narrowing of native's
`isGenericObjectType || isGenericIndexType` gate that checker-notes-callres.md §786 measured (deferring
`x[k]` for `K extends keyof U`, `U extends T` lost 6 RIGHT rows).

`getIndexType` over an intersection is the union of its constituents' key sets,
so a key of one constituent is a key of the whole intersection. The gate now
also accepts the keyed operand when it is a constituent of an intersection
object. The narrowed receiver is the semantic intersection `T & {}` carrying the
`NonNullable<T>` alias print (checker-99-adjusted-facts.md), so its constituent
list contains `T`. The `U extends T` refusal is unchanged: `U` is not a
constituent of `T & {}` (pinned by a second assertion in the new test).

Without a lib `NonNullable` alias the receiver is native's fallback `T & {}`,
and the deferred read prints `(T & {})[K]`; the regression test uses that
lib-less spelling.

## 3. Measurement

Full scorepair against a baseline measured at origin/main in a separate
worktree (own target directory) at 5871239, candidate 44279dc: **+35 assertions (32 WRONG-to-RIGHT, 3
GAP-to-RIGHT), zero RIGHT losses, zero GAP-to-WRONG, zero changed already-WRONG
rows** (a full verdict TSV diff, not only the class matrix). Rows:
nonNullReferenceMatching 18, narrowingByTypeofInSwitch 8,
controlFlowGenericTypes 3, multiSignatureTypeInference 2, strictBindCallApply1 2,
typeVariableTypeGuards 1, unknownControlFlow 1.

Two regressions: `typeof_function_keeps_an_aliased_callable_without_an_intersection`
(narrowing.rs) and `a_key_of_an_intersection_constituent_defers_over_the_intersection`
(deferred_indexed_access.rs). Each fails with its production change reverted
(`L & Function`, `error`).

## 4. What remains in tsr-6.34

- narrowingByTypeofInSwitch still has union-order (`R | L` vs `L | R`),
  `object | Function` default-clause and `[X | Y]` tuple-union ordering rows;
  these are union ordering / switch-default facts, not the callable relation.
- Non-strict fact aggregates and the fully general subtype reduction remain
  as described in checker-99-adjusted-facts.md.
