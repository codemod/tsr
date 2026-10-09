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

## 2. `tsr-2zk.1129`: the property slot, and the accessor arm it lacked

Batch AW refused r5-nodereuse's `r5-nodereuse-property-slot.diff`.
`circularAccessorAnnotations` 0:0 and 0:3 went RIGHT → WRONG:

```ts
declare const c1: { get foo(): typeof c1.foo; };
```

The port printed `c1 : { readonly foo: any; }`, where native prints
`{ readonly foo: typeof c1.foo; }`.

**Cause.** The diff replaces r5-printer2's `reused_property_annotation_text_at`
with `reused_property_type_text`. The old helper read a getter's annotation
directly. `reused_property_type_text` asked only
`pseudo_direct_declaration_node`, which answers nothing for an accessor, and
the structural arm then refused. Native's `serializeTypeForDeclaration`
(`nodebuilderimpl.go:2233`) has a separate accessor arm. It admits an
accessor declaration without the `ObjectFlagsRequiresWidening` gate, asks
`GetTypeOfAccessor` (`typeFromAccessor`, `lookup.go:146`), and passes
`isOptionalAnnotated = false`. `typeFromAccessor` tries the accessor's own
annotation first, then the pair's first accessor, then its second, then the
getter's return.

Circularity made the property `any`. `getTypeFromTypeNode(typeof c1.foo)` is
that same `any`, so the identity arm holds and the written query is reused.
The type's own print is `any`.

**Fix.** `reused_property_type_text` routes a get or set accessor
declaration to `reused_accessor_type_text`. A Direct answer goes through the
same equivalence and visitor as a property annotation. A structural getter
return goes through the structural arm. A keyword or literal leaf prints as
the type does (`r5-nodereuse.md` §3), so it declines. `pseudo_type_of_accessor`
already existed (r5-nodereuse2); only the slot did not ask it.

The setter-only case, `declare const c2: { set foo(value: typeof c2.foo); }`,
converts too (0:5, 0:9). tsgo prints `{ foo: typeof c2.foo; }`.

Falsifier:
`node_reuse_structural::an_accessor_property_reuses_its_written_annotation_under_circularity`.
With the arm removed, the dump prints `{ readonly foo: any; }` at 0:0 and 0:3
again, which is the gate's regression.

**The call-site hunks.** `reused_property_type_text` has no production caller
without them, so this commit alone changes no line. The hunks are r5-nodereuse's
diff, split by owner, and unchanged:

| diff | file (owner) | apply after |
|---|---|---|
| `r6-nodereuse-property-slot-printing.diff` | `printing.rs` (r6-printer) | `r5-nodereuse2-object-literal-slot.diff`, `r5-nodereuse2-predicate-at-site.diff` |
| `r6-nodereuse-property-slot-spreads.diff` | `spreads.rs` (r6-errorsplit) | the printing hunk |

`r5-nodereuse-property-slot.diff` is the two together.

### Measured

The base is the item-1 full stack (owned code plus both item-1 diffs). Both
dumps are unfiltered, and `slowcases` ran on both.

| tree | types | diagnostics | slowcases |
|---|---|---|---|
| + accessor arm (owned) alone | unchanged by construction (no caller without the hunks) | unchanged | n/a |
| + accessor arm + printing hunk | +2 RIGHT (`circularAccessorAnnotations` 0:5, 0:9), 0 lost | unchanged, 0 lost | clean |
| + spreads hunk | +19 RIGHT more, 0 lost | unchanged, 0 lost | clean |
| total over the item-1 stack | **+21 RIGHT, 0 lost** | unchanged, 0 lost | clean |
| total over the base `b18aec0` | **+47 RIGHT, 0 lost** (549,900 RIGHT / 5,560 WRONG) | unchanged, 0 lost | clean |

The +19 are r5-nodereuse's spread targets: `spreadObjectPermutations` (both
configurations) 4, `spreadObjectNoCircular1` 2, `spreadUnionPropOverride` 2,
`thislessFunctionsNotContextSensitive3` 2, `unionExcessPropsWithPartialMember`
2, `declarationEmitComputedPropertyName{Enum2,Symbol1,Symbol2}` 6, and
`intersectionIncludingPropFromGlobalAugmentation` 1.

The gate's own case, `circularAccessorAnnotations`, now has all 27 lines
RIGHT. 0:0 and 0:3 stay RIGHT, and 0:5 and 0:9 convert.

### Ir: what the spreads hunk costs, and the cuts

Callgrind Ir (`--singleThreaded --pretty false --noEmit`). Every row is
measured against the item-1 stack binary: domain-model 1,090,892,208, and
1,090,876,378 on a re-run (±0.002%); generic-imports 343,068,922.

| tree | domain-model | generic-imports |
|---|---|---|
| item-2 stack as first written | 1,094,985,260 (+0.38%) | +0.01% |
| + site-free top-level shortcut, `T[]` without `format!` | 1,092,666,238 (+0.16%) | +0.00% |
| + type-literal members without `format!` (this commit) | **1,092,220,478 (+0.12%)** | **+0.00%** (343,086,437) |

Median child CPU against the item-1 stack: the first row (21 samples) gave
domain-model 0.995 and generic-imports 1.016. The final row gave 1.035 and
1.031 at 21 samples, then 1.048 and 0.989 at 41, all with
`diagnostics_match: true`. Two controls show these numbers are harness
noise, not cost:

- the identical binary against itself, same harness, 41 samples: 1.028
  (slot bias);
- an interleaved A/B (alternating order, user+sys per run, 41 pairs): final
  stack 1.027 on domain-model and 0.997 on generic-imports, against an
  identical-binary control of 1.023.

On this container, then, the CPU harness cannot resolve a difference below
about 3%, and the deterministic Ir above is the measure.

**Where it goes.** `spreads.rs` bakes a spread member's text when the spread
type is created. Its own object print reads the slot at once
(`spreads.rs:552`), so the reuse runs once per annotated spread member,
whether or not anything prints it. On domain-model that is 1,248 calls of the
site-free visitor: 452 array, 316 type-reference, 320 keyword and 160
type-literal annotations. Native runs `serializeTypeForDeclaration` only when
it prints. Of the first +4.09M Ir, 3.5M was `reused_property_type_text`. Most
of that was `track_existing_leftmost_identifier`: two `resolve_name` walks per
entity name, at about 890 Ir each, plus string reallocation.

**Cuts kept.**

- **Site-free top-level shortcut.** A symbol with a declaration directly in
  the top-level scope of the annotation's file is what the name resolves to
  from that top level, since the lookup starts with the file's locals. Its
  `is_global_name` and `resolves_from_file_top_level` walks are skipped
  (`declared_at_top_level_of_file_of`). That covers an import or a top-level
  declaration. `resolve_name` calls fell from 3,044 to 1,675 on
  domain-model. Falsifier: a name whose declaration's container is the file
  but which the file's top level resolves to a different symbol. No binder
  path makes one; a merged symbol is compared through `merged_symbol` in
  any case.
- **`T[]` and type-literal members built in place**, without `format!`.

**Not cut: the remaining +0.12%.** It is about one `resolve_name` per entity
name of an annotated spread member, the lookup native's
`trackExistingEntityName` also makes, but only at print time. Removing it
needs one of two things. Either `spreads.rs` stops baking the member's text
at creation (`PrintedSlot::on_demand`, then printed through
`property_printed_type`; r6-errorsplit's and r6-printer's files), or the
checker keeps a node → resolved-symbol table, as native's
`symbolNodeLinks.resolvedSymbol` does (`checker.rs`, main's). Neither is this
lane's file. The CPU harness does not separate the cost from noise.

## 3. r5-nodereuse2 §7's remainder

### 3a. `Inferred` equivalence

The port's pseudo tree folded native's `PseudoTypeKindInferred` and
`PseudoTypeKindNoResult` into one bare `Inferred`. Its equivalence then held
only through error charity. So a structure with an Inferred part, such as
`() => (y: A) => y`, was serialized from its type, and the written `A` was
lost.

`PseudoType::Inferred` now carries native's fields (`InferredSource`): the
node, `IsSignatureReturn`, and whether native records error nodes.
`Inferred(None)` is NoResult. Every producer in `pseudochecker.rs` is mapped
to native's choice:

- `typeFromSingleReturnExpression`'s fallbacks are `Inferred(fn, true)`:
  an async generator, a missing body, no single candidate, or a contextually
  typed non-assertion.
- A non-value signature is NoResult.
- `typeFromExpression`'s default arm is `Inferred(expr)`. A class expression,
  and an object or array literal that `canGetTypeFrom…` refuses, are Inferred
  with errors.
- The declarations' "fallback to NoResult if Inferred without error nodes"
  is `PseudoType::or_no_result`.
- A parameter initializer's Inferred, and a getter return's Inferred, have
  their error moved up to the parameter or accessor (`lookup.go:669`, `:159`).

`pseudo_type_to_type` maps an Inferred as `pseudoTypeToType` does
(`pseudotypenodebuilder.go:697`):

- A signature return is the return type of the declaration's signature.
- An expression is `getWidenedType(getRegularTypeOfExpression(node))`, read
  from the expression cache only (`node_types`), as the literal leaf already
  is. The check typed the node, so nothing is checked at print time.
- `getWidenedType` is the identity unless the type requires widening. A
  widening nullable widens to `any`. Any other such type answers its image in
  `getWidenedType`'s root cache (`widened_object_types`), which is the type
  the check widened the return or declaration to. If no image is cached, it
  answers none instead of minting one.

The print of an Inferred part is unchanged: the slot's type at the site,
which the equivalence just proved equal to the declaration's type. That is
what native's `serializeTypeForDeclaration` and
`serializeReturnTypeForSignature` print for it.

tsgo confirms the cases, and
`node_reuse_structural::an_inferred_member_of_a_structural_pseudo_type_is_its_checked_type`
and `::an_inferred_fresh_literal_reads_the_widening_cache` pin them:

- `() => (y: number) => y` prints `(y: number) => number`;
- `() => (y: A) => y` prints `(y: A) => number`;
- `() => (y: A) => ({ a: 1, ...p })` prints `(y: A) => { b: number; a: number; }`.

**Falsifier for the cache read:** a fresh literal whose widened image the
check never cached, printed where native reuses it. The port then serializes
the structure from its type, which differs only if a sibling part carries a
written spelling.

### 3b. The `MaybeConst` contextual consultation

`pseudotypenodebuilder.go:94` covers a node the pseudochecker sees in a
const context and the checker does not. It asks the contextual type, and
prints the const form when that form `isLiteralOfContextualType` of the
instantiated contextual type. The port declined that print. It now asks
`get_contextual_type`, `instantiate_contextual_type_without_signature`
(`ContextFlagsNone`) and `is_literal_of_contextual_type`. Equivalence still
reads only the checker's const context, which is native's
`pseudoTypeToType`, so the consultation changes only the print.

### 3c. A method whose target has no single call signature

Native skips that method's validation (`continue`, `:478`) and prints it from
syntax. Each Inferred part then serializes its declaration's own type: the
parameter's `serializeTypeForDeclaration` and the method's
`serializeReturnTypeForSignature`. The port declined. It now uses the method
declaration's own signature (`get_signature_from_declaration`) as the target.
That is the type each such print reads. A Direct part is equivalent to it by
construction. A part that is not, an Inferred one whose widening is
uncached, declines the literal rather than printing what native prints.

### Measured (3a–3c together)

The base is the item-2 stack: owned code plus all four call-site diffs. Both
dumps are unfiltered; `slowcases` ran on both.

- types: 0 gained, 0 lost; diagnostics: unchanged, 0 lost; slowcases: clean.
- Only three lines change their printed text, and all three are WRONG
  before and after. They are `declarationEmitTypeParameterNameShadowedInternally`
  0:3, 0:4 and 0:10:
  - native: `<T_1>(y: T_1) => readonly [T, T_1]`;
  - before: `<T>(y: T) => readonly [T, T]`;
  - now: `<T>(y: T) => readonly [T_1, T]`.

  The `[x, y] as const` tuple of Inferred parts is now reused, as native
  reuses it. The rest of the error is the signature printer's: it does not
  rename the inner `T` at a site where the outer `T` is visible, and the
  Inferred parts are named against the render scope that printer pushed. That
  is `signature_to_string_at`'s site renaming (`signatures.rs`, r6-printer),
  and it was wrong at the base too.
- `cargo test --workspace --release` passes (3,493).
- Ir: domain-model 1,092,326,464 against the item-2 stack's 1,092,220,478,
  which measured 1,093,053,314 on a re-run (±0.08%); generic-imports
  343,064,755 (−0.01%). Within noise.

No corpus line exercises 3b or 3c alone. They are ported because §7 named
them and native's arms are plain. Each now asks what native asks where the
port used to decline.

## 4. Remaining in the lane, with causes

- **`declarationEmitTypeParameterNameShadowedInternally` (3 lines):** the
  site renaming of a printed signature's own type parameters, in
  `signatures.rs` (§3, r6-printer).
- **The spreads hunk's +0.12% Ir (§2):** it needs on-demand spread member
  text (`spreads.rs`/`objects.rs`) or a node → resolved-symbol table
  (`checker.rs`).
- **The structural visitor's own `typeParameterToName` allocation (§1):**
  declining is exact today. It needs modelling only if a renamed entered
  signature prints differently from the type's serialization.
- **Unchanged from `r5-nodereuse.md` §5, all in other lanes' printers:**
  `builtinIteratorReturn` (4), `divergentAccessors1` (4),
  `declarationEmitPartialNodeReuseTypeReferences` (6) and
  `mappedTypeTupleConstraintAssignability` (2).
