# r6-nodereuse — the structural pseudo types re-landed, and their scope (`tsr-2zk`)

Round-6 cloud lane, successor of r5-nodereuse (`r5-nodereuse.md`) and
r5-nodereuse2 (`r5-nodereuse2.md`). It owns
`crates/tsr-checker/src/node_reuse.rs`, `crates/tsr-checker/src/pseudochecker.rs`,
its tests and this note. Call sites in other lanes' files ship as measured
diffs, listed with each item.

Native source is `vendor/typescript-go` @ `5b1047d`. Every expectation below
was checked against a native `tsgo` built from that pin by
`scripts/offline-cargo/build-tsgo.sh` (`tsgo --declaration`).

Base frozen at `b18aec0` (main `17265fa` plus bookkeeping): types 549,853
RIGHT / 843 GAP / 5,607 WRONG; diagnostics 5,530 RIGHT + 5,596 EMPTY_RIGHT of
12,238 rows.

## 1. `tsr-2zk.1118`: the structural pseudo types, and the scope the gate caught

Batch AZ backed out r5-nodereuse2's merge (`45f2350`) and its
predicate-at-site diff (`0484e27`): with them,
`shadowed_names::constraints_and_defaults_keep_outer_parameter_names` failed.

```ts
function constraints<T>(value: T) {
    type Outer = T;
    return function inner<T extends Outer = Outer>(local: T): Outer | T { ... };
}
```

The port printed `<T extends Outer = Outer>(local: T) => ...` at
`constraints`, where `Outer` is out of scope. Native prints
`<T_1 extends T = T>(local: T_1) => T | T_1`.

**Cause.** `structural_pseudo_text` seeded the visitor's root
(`ReuseContext::root`) with the printed declaration. That is right for a
Direct node, which replaces the root with itself. The structural arms,
though, emit two kinds of node with no Direct wrapper: an entered
signature's type parameters, and an object literal's member names. For
those the root stayed the printed function. `declared_inside_reused_node`
then counted every name declared in that function's body as declared inside
the reused node. It accepted `Outer` without tracking it at the site.

Native gives each of those nodes its own boundary: `reuseNode(tp)` for every
type parameter (`pseudotypenodebuilder.go:183`, `:244`) and `reuseName(e.Name)`
for every member name (`nodecopy.go:24`). Its visitor's
`trackExistingEntityName` (`nodecopy.go:317`) resolves `Outer` at the
enclosing declaration. `resolveName` hides a function's local types from its
signature, so the lookup fails, and the type-node recovery (`nodecopy.go:866`)
prints `getTypeFromTypeNode(Outer)`, which is the outer `T`.

**Fix, part 1: one root per reused node.**
`pseudo_type_parameters_text` swaps the root to each type parameter
declaration, and the object-literal arm swaps it to each member name.
Falsifier: `node_reuse_scope::a_body_local_alias_in_an_entered_constraint_is_tracked_at_the_site`.
tsgo prints `noShadow<T>(value: T): <U extends T = T>(local: U) => T | U`.
Without the swap the port prints `<U extends Outer = Outer>`.

**Fix, part 2: a renamed entered type parameter declines.** With part 1
alone, the failing test printed `<T extends T = T>(local: T_1)`. Native's
`enterNewScope` names every entered type parameter through
`typeParameterToName`. That renames the inner `T` to `T_1`, because an
enclosing render holds `T`. The visitor emits a declaration's written name,
and it does not model a nested allocation inside a reused node. That is the
same limitation `renamed_annotation_in_scope` already documents for written
annotations that declare type parameters. So an entered list in which any
parameter would be renamed declines, and the slot is serialized from its
type, which renames. A parameter counts as renamed when its written name is:

- held by an enclosing render's parameter (`render_type_parameter_scope`);
- allocated by this render to another type, or allocated to this parameter
  under a different name;
- the name of a different type parameter that it resolves to at the site.

The first version also refused every name this render had allocated. The
render had already allocated `U` to the very parameter being entered, so
every generic returned function declined. Falsifier:
`node_reuse_scope::a_shadowed_entered_type_parameter_declines_to_the_type`.

**Not taken: modelling `typeParameterToName` inside the visitor.** It would
let the structural arm print `<T_1 extends T = T>` itself. The visitor has
no allocation scope per entered signature. Every name inside the reused node
(`local: T`, `Outer | T`) would then have to be renamed consistently, and the
written-annotation path has the same gap. The declined slot prints the same
text through the type serializer. This decision would flip if a case printed
differently between the two, for example a renamed signature whose other
parts carry written spellings the serializer loses. That case would need the
allocation modelled.

The structural code itself is r5-nodereuse2's `e8bb88d`, unchanged except
for the two parts above. Its design is in `r5-nodereuse2.md` §2. The two
call-site diffs it measured are re-landed unchanged with it, and they apply
on `b18aec0` as they are:

- `r5-nodereuse2-object-literal-slot.diff` (`printing.rs`, r6-printer);
- `r5-nodereuse2-predicate-at-site.diff` (`signatures.rs`, r6-printer).

### Measured

Against the base above, both dumps unfiltered, compared with `cut -f1,2`;
`slowcases` on both dumps.

| tree | types | diagnostics | slowcases |
|---|---|---|---|
| owned code only (this commit) | +2 RIGHT (`declarationEmitScopeConsistency3` 0:3, 0:12), 0 lost | unchanged, 0 lost | clean |
| + object-literal-slot + predicate-at-site diffs | **+26 RIGHT, 0 lost** (549,879 RIGHT / 5,581 WRONG) | unchanged, 0 lost | clean |

The +26 are exactly r5-nodereuse2's converted lines:
`noUsedBeforeDefinedErrorInTypeContext` 5, `complexRecursiveCollections` 3,
`circularObjectLiteralAccessors` 3, `conditionalTypes2` 3,
`declarationEmitScopeConsistency3` 2, `unionAndIntersectionInference1` 2,
`intlNumberFormatES2023` 2, `formatToPartsFractionalSecond` 2,
`literalTypeWidening`, `quickinfoTypeAtReturnPositionsInaccurate`,
`intersectionsOfLargeUnions`, `intersectionsOfLargeUnions2`.

With both diffs applied, `cargo test --workspace --release` passes (3,491
tests, `shadowed_names` included). Clippy reports nothing in the files this
lane touched.

**Perf, full stack against the base binary.** Callgrind Ir
(`--singleThreaded --pretty false --noEmit`):

- domain-model: 1,090,905,040 → 1,090,892,208 (−0.00%);
- generic-imports: 343,083,909 → 343,068,922 (−0.00%).

Median child CPU, new/old, 21 samples: domain-model 0.967, generic-imports
0.949. `diagnostics_match: true` on both.
