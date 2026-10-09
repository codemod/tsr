# shared-contract lane notes

## Source tuple variadic normalization (tsr-2zk.16.2)

`declared.rs` source tuple producer (`tuple_type_node_structural` rest branch)
now follows native `createNormalizedTupleTypeEx` (checker.go) on completed
element types: all elements resolve first; a variadic never/union operand maps
through the existing `normalize_variadic_tuple` (cross-product bound, branch
order); nested variadic and `any` operands take the same normalizer; an
optional slot before a required tail after concrete splicing becomes required
with `undefined`; the all-rest array path reuses the resolved TypeIds instead
of re-resolving the AST. No new cache; tuple owner/key unchanged.
Carried from `box/recover-alias` (c6fa4c5b..f5f9116c), declared.rs only.

## Optional tuple element optionality (tsr-2zk.16.79)

The source tuple producer applies native `getTypeFromOptionalTypeNode`
(`addOptionality(type, isProperty=true)`) under strictNullChecks before the
slot is stored, so `[T?]` carries `T | undefined` (or missing under
exactOptionalPropertyTypes) in its element TypeId, as tsgo prints
`[string, (number | undefined)?]`. No new cache; tuple key already includes
the ordered element type and optional mask. Tests that pinned the pre-port
print (`array_literals`, `tuple_element_inference`, `tuples`) now expect the
native print; `relater` exact-optional test fixes options before resolution,
as native options are fixed per checker. domain-model CPU samples are bimodal
(~0.39/0.43 s); 41-sample reruns 1.0104/0.9967, identical-binary control 0.9936.

### Fix-forward: missing in prints (tsr-2zk.16.527)

Two print sites still exposed `missingType` beside `undefined`. Native
`getUnionTypeWorker` builds the origin of an unaliased union from the reduced
`typeSet`, where `undefined` already displaced `missing`; the TSR origin entries
came from the unreduced operands, so `optional[1]` printed
`First | undefined | undefined`. The origin now drops `missing` when the reduced
set no longer holds it. Labeled optional tuple elements now print through
`removeMissingType` like unlabeled ones (`typeToTypeNodeHelper` tuple arm), so
exact mode prints `[b?: string]`; an explicit `b?: string | undefined` keeps
`undefined` (native control). No new cache or key.

## Declared alias bodies (tsr-2zk.16.2)

Carried from `box/recover-alias` (ca3ef430..2197be77, declared.rs only;
f82ccc9d TypeQuery migration omitted: needs `get_instantiation_expression_type`
from the unmerged instantiation-expression module). Ported pieces:
`getTypeFromTypeNodeWorker` parenthesis transparency (`skip_type_parentheses`)
for keyword, indexed, conditional, variadic-tuple and identity-mapped alias
bodies; `getDeclaredTypeOfTypeAlias` seeds `instantiations[(alias, own
parameter TypeIds)]` after successful resolution; `getTypeAliasInstantiation`
returns the declared body for literal/keyword/intrinsic bodies (no alias
image) and instantiates keyof/typeof bodies through `instantiate_type`;
parameter identity by binder symbol, not spelling; identity-mapped alias
reuse keyed by `(alias, ordered argument TypeIds)` in `instantiations`
instead of printed text; `unique` requires a `symbol` keyword operand before
publication; `createTypeNodesFromResolvedType` prints call before construct
signatures. No new cache: keys reuse the existing SymbolId-owned
`instantiations` table. `tsr-conformance/tests/original_callable_entry.rs`
pins the removed non-native TS2464 and needs `[]` (out of lane).

## New alias on generic alias instantiations (tsr-2zk.16.57)

`getTypeFromTypeAliasReference`'s `newAliasSymbol` arm: a generic alias
reference that is directly the body of a non-generic, non-local alias `A`
instantiates with `A` as its alias (`getTypeAliasInstantiation` →
`instantiateTypeWithAlias`). Ported where native hands the alias to the
created type: `getObjectTypeInstantiation` for type-literal, function and
non-homomorphic mapped bodies (followed through alias-reference bodies,
`Omit` → `Pick`), and `getUnionType`/`getIntersectionType` for union and
intersection bodies whose evaluated instantiation does not reduce to one type.
Homomorphic mapped bodies keep the target alias (`instantiateConstituent`
passes nil: `type P = Partial<User>` prints `Partial<User>`), and reducing
unions keep no alias (`U2<never>`, `Lit<string>`) — native controls. Image
cache: existing `deferred_alias_references[(A, canonical)]`; the image copies
the `type_reference_targets` pair, re-runs `capture_mapped_alias` and copies a
signature bake, so members, relations and inference read the same
instantiation; only `alias_of` differs. Published after every channel is
installed; the union check reuses the cached `evaluate_alias_body`.

## Type-parameter declared types through alias chains (tsr-2zk.16.2)

`instantiateTypeWithAlias` over a type-parameter declared type answers the
mapper's image, so `type V<in out T> = Unconstrained<T>` over
`type Unconstrained<T> = T` declares `T` and `V<1>` is `1`.
`type_parameter_body_index` follows a reference body to an alias whose own
declared type is a parameter (bounded depth 8, exact arity, no defaults) and
answers the own parameter written at that position. Callers (declared type,
instantiation, annotation-reuse text) are unchanged; no new cache.
Interface-bodied controls (`VarianceShape<1>`) stay references.

## NoInfer collapse at the reference (tsr-2zk.16.2)

`getTypeAliasInstantiation`'s intrinsic `NoInfer` arm calls `getNoInferType`,
which wraps only an `isNoInferTargetType` base: `NoInfer<string>` is
`string`. The instantiation rebuild already collapsed; the written reference
now does too, in `create_type_reference_with_display` beside the
`NonNullable` arm. Object and type-parameter bases keep the wrapper (native
control `NoInfer<{ x: 1 }>` relation unchanged). No new cache.

## Enum declared unions sort by their alias name (tsr-2zk.47.5)

`CompareTypes`' `compareTypeNames` (`utilities.go:590`) reads
`getTypeNameSymbol`, which answers `t.alias.symbol` first. An enum's declared
union is created with `&TypeAlias{symbol}` (`getDeclaredTypeOfEnum`,
checker.go:23899), so two enum unions in a union origin (`insertType`,
checker.go:25724) order by enum name: `(E7 | E8 | E3 | E4)[]`. The port
excluded every enum-like type from the name comparison, so `E4 | E3` fell
to creation order. A union carrying a symbol (`TypeData::Union { symbol }`)
now names itself by that symbol; non-union enum members still answer no
name and order by declaration. No new cache.

## Indexed-access declared types through alias chains (tsr-2zk.16.56)

`getTypeAliasInstantiation` → `instantiateTypeWithAlias` over a declared
type that is an indexed access maps it through `getIndexedAccessTypeEx(..,
alias)` (checker.go:22176): a resolved access is the property type itself
(no alias), a deferred one carries the outer alias. The port's
indexed-access arm of `create_type_reference_with_display` admitted only a
written `O[K]` body, so `type W<O, K> = Idx<O, K>` minted a nominal `W<..>`
that related to nothing (`deferredLookupTypeResolution`'s
`ObjectHasKey<{ a: string }, 'a'>` is `"true"`). The arm now admits a body
that is a reference to a generic alias whose declared type is an indexed
access (`alias_body_is_indexed_access`, bounded chain depth 8 like
`alias_body_receives_new_alias`). Native controls: `W<{ a: 'x' }, 'a'>` is
`"x"`, `W<I, 'm'>` is `string`, `W<T, K>` / `W2<T, K>` inside a generic keep
their own alias heads. No new cache: the existing `instantiations` and
`alias_body_evaluations` keys.

## Generic alias declared as a reference that keeps its own alias (tsr-2zk.16.56)

`getDeclaredTypeOfTypeAlias` is `getTypeFromTypeNode(body)`. For a body that
references another generic alias, `getTypeFromTypeAliasReference` passes the
declaring alias as `newAliasSymbol` (with its own parameters), and
`instantiateTypeWithAlias` attaches it only where the instantiation creates an
aliasable type. It never does when the target's declared type is a type
parameter (the mapper's image), a literal/template/`keyof`/`typeof` type, or a
homomorphic mapped type over a non-union variable (`instantiateMappedType` →
`instantiateConstituent` with a nil alias), nor through a chain of such
aliases. Those declarations now publish the resolved body instead of the
`Name<Params>` mint: `type Gaps<T> = CleanedGaps<PartialGaps<T>>`,
`type T2<U> = T1<Same<U>>` (`Same<U>`), `type Merge3<T> = Identity<{..}>`.
Controls that must stay distinct (alias received): `Record`/`Pick`/
conditional/indexed targets keep `GenericStructure<K>`, `Omit<T, K>`,
`TestBit<A, B>`; a union answer keeps the mint (`mapTypeWithAlias`).
Predicate `alias_instantiation_keeps_declared_alias` (bounded chain, depth 8);
no new cache: `declared_types` stays the owner.
