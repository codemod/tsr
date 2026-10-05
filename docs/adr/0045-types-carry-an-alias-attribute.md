# ADR-0045: Types carry an alias attribute, set only by alias-accepting constructors

**Status:** Proposed — **designed, not built.** This records the shared
contract the type-refs parity box (`tsr-2zk.13`) was asked to design for the
TYPE-ALIAS port set (`tsr-2zk.16.2`: TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION,
TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, DEFERRED-TYPE-REFERENCE-ALIAS,
INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, CONDITIONAL-TYPE-ROOT-ALIAS,
IMPORTED-ALIAS-REFERENCE-WRITTEN-ARGS; 69 cases blocked, 40 finished when all
arms land, `docs/parity/types-triage.md` §3, §4, §19, §90, §91). No code in the
tree implements it yet; the first box round spent its time on the QUALIFIED
set (`docs/parity/notes/type-refs.md` §1), which is a prerequisite for the
qualified half of this one. It builds on
[ADR-0003](0003-tree-plus-side-tables.md) and
[ADR-0044](0044-a-name-is-rendered-from-its-symbol-at-the-site.md).

## The forcing constraint

Upstream's `Type` has an `alias *TypeAlias` field (`symbol` + `typeArguments`).
It is set **only** by the constructors that receive an alias —
`getAliasForTypeNode` hands one to object literals, unions, intersections,
mapped, conditional and indexed-access types and to deferred type references —
and it is **replaced or dropped** by `instantiateTypeWithAlias`
(`checker.go:22104`). The node builder's alias arm (`nodebuilderimpl.go:3362`)
prints `Alias<Args>` from it whenever the alias symbol is accessible, and
prints structure otherwise.

This port has no such field. `TypeData` bakes a printed `text` at creation
(`crates/tsr-checker/src/types.rs`); a generic alias's declared type is an
opaque `Named("Name<Params>")` mint
(`get_declared_type_of_type_alias`, `declared.rs`), and a reference is an
opaque `(symbol, args)` pair in `Checker::instantiations` printed as written.
Six measured clusters are the same missing operation seen from six sides:

| cluster | upstream answer | this port |
|---|---|---|
| declared body (§3) | `type Id<T> = T; type X = Id<string>` → `string` | `Id<string>` |
| new alias (§4) | `type Bar = Omit<Foo, "c">` → `Bar` | `Omit<Foo, "c">` |
| deferred reference (§19) | `type S = Box<string>` prints `S` | eager `Box<string>` |
| mapped drop (§91) | homomorphic over a non-union drops the outer alias | keeps it |
| conditional root (§90) | the conditional's root carries the alias | no alias |
| imported written args | `import { A }`; `A<number>` prints local args | target spelling |

Union types already carry half of this (`TypeData::Union { symbol }`, the §53
origin machinery in `unions.rs`), which is why union-bodied aliases mostly
print right today and every other body shape does not.

## The decision (proposed)

1. **Representation.** A side table, not a `TypeData` field:
   `alias_of: FxHashMap<TypeId, TypeAlias { symbol: SymbolId, arguments:
   Vec<TypeId> }>` owned by the `Checker` (one per checker, same lifetime as
   the `TypeStore`). Key identity: the `TypeId` of the type the constructor
   built. Upstream interns alias-bearing types *separately* (an object literal
   with alias `A` is a different type from the same literal without one), so
   every alias-accepting constructor's intern key must include the alias —
   `(kind key, alias symbol, alias arguments)` — or two aliases of one body
   would share a `TypeId` and one table entry.
2. **Writers.** Only the ported alias-accepting constructors write it:
   `get_union_type`/`get_intersection_type` with an alias (after the singleton
   early-return, `getUnionTypeEx :25632`, `getIntersectionTypeEx :26127`),
   type-literal, mapped, conditional and indexed-access construction from a
   type node whose `getAliasSymbolForTypeNode` answers, and deferred type
   references (`isDeferredTypeReferenceNode :23236`).
3. **Instantiation.** `instantiate_type_with_alias(type, mapper, alias)` ports
   `:22104`: a type-parameter body returns the mapped argument untouched; an
   object/mapped/conditional instantiation takes `newAlias` (from
   `getTypeFromTypeAliasReference :23580`'s outer-alias rule) or the
   instantiated old alias; homomorphic mapped over a non-union and resolved
   conditional branches take none. The instantiation cache key becomes
   `(symbol, args, alias)` (`getTypeAliasInstantiation :23641`).
4. **Declared type.** `get_declared_type_of_type_alias` stops minting
   `Name<Params>`: it resolves the body with the alias passed down, records the
   alias's own type parameters, and lets the constructors decide whether the
   alias lands. Its keyword, bare-type-parameter, rest-tuple and
   mapped-sequence arms become consequences of rule 2, not special cases.
5. **Printing.** `type_to_string_at` consults `alias_of` first and renders
   `Alias<Args>` from the symbol at the site (ADR-0044's name-from-symbol
   render), falling back to the baked text. `type_to_string` without a site
   renders the alias's bare name. `IsTypeSymbolAccessible` gating
   (TYPE-ALIAS-ACCESSIBILITY-GATE, §25) belongs to the site render.

### Checker port convention record

- **Native operation:** `getDeclaredTypeOfTypeAlias :23837`,
  `getTypeFromTypeAliasReference :23580`, `getTypeAliasInstantiation :23641`,
  `instantiateTypeWithAlias :22104`, node-builder alias arm `:3362` @ `5b1047d`.
- **Identity and owner:** key `TypeId` (Checker-private store); value alias
  symbol (program identity space, ADR-0034) + ordered argument `TypeId`s.
  Intern keys of alias-accepting constructors include the alias.
- **Publication:** written once, at construction, by the constructor that
  interns the type; never mutated. An absent entry means "no alias", which is a
  completed answer — there is no provisional state, because the alias is
  known before the type exists.
- **Consumer context:** printing only. Relations, members and inference must
  ignore the table (upstream's relater ignores `alias` except for the
  alias-variance fast path, `isTypeReferenceWithGenericArguments`, which stays
  unported until measured).
- **Work boundary:** one hash insert per alias-bearing construction and one
  lookup per print. The expensive work — body instantiation — already exists
  in the evaluators this replaces; the cache-key widening is the measured risk
  (every alias instantiation keys on one more field).

## The alternatives, taken seriously

- **A field on `TypeData` / `Type`.** Upstream's shape. Rejected for now
  because `Type` is copied through `store.get(id)` in hot loops and every
  constructor builds a `TypeData` literal; a side table touches only the
  constructors that can attach an alias. It would win if profiling showed the
  extra lookup on the print path mattered — printing is not on the
  relation-hot path, so that is unlikely.
- **Keep baking the alias into `text` (today's mints).** Rejected: the text is
  fixed at creation, so an instantiation cannot replace or drop the alias, and
  the opaque mint has no structure for consumers (§3's `choices<{}>` must
  reduce to `{ shoes: boolean; food: boolean; }`). Every cluster above is a
  symptom of exactly this.
- **Per-cluster patches** (more arms in `get_instantiated_type_reference`).
  The current tree has six such arms; each fixed one shape and the triage
  still finds 69 blocked cases. Rejected as the strategy, not as a stepping
  stone.

## Consequences accepted

- The intern-key change touches unions, intersections, type literals, mapped,
  conditional and indexed-access construction at once; it cannot land in
  slices without double-interning risk, so it needs a whole-corpus run and the
  perf self-ratio on both benches before merge.
- The huge RIGHT surface that today depends on the mints (`Box<T>`, `Fn<T>`
  print their alias in tsgo too) must be preserved by rule 2, measured as
  zero R→W, not assumed.
- Qualified alias names (`N.T`) still need NB-SYMBOL-CHAIN for the printer to
  qualify from the symbol; until then `qualified_type_reference` keeps its
  written-text mint for alias-carrying bodies (`notes/type-refs.md` §1.2).

## How we would know this is wrong

- If, with the table built, any of §3/§4/§19's "finished alone" cases still
  fails on an alias-print line, the writer set in rule 2 is incomplete.
- If the cache-key widening moves either bench's self-ratio above 1.03 at 21
  samples, the side table must be revisited (field, or alias-free fast key).
- If a relation result changes because of the table, rule "printing only" was
  violated.
