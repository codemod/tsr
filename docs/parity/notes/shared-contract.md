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
