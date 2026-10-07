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
