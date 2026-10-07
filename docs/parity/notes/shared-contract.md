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
Under exactOptionalPropertyTypes the slot carries missing: labeled optional
elements now print through native `removeMissingType` like unlabeled ones,
and the unaliased union origin drops missing once undefined displaced it in
the reduced set (`getUnionTypeWorker` builds origin from typeSet), so
`optional[1]` prints `First | undefined`, not `First | undefined | undefined`.
