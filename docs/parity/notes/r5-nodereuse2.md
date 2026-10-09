# r5-nodereuse2 — the pseudochecker's structural kinds (`tsr-2zk`)

Round-5 cloud lane, successor of r5-nodereuse
(`docs/parity/notes/r5-nodereuse.md`). It owns
`crates/tsr-checker/src/node_reuse.rs`, `crates/tsr-checker/src/pseudochecker.rs`,
`crates/tsr-checker/tests/node_reuse*.rs` and this note. Every call site that
lives in another lane's file ships as a measured diff (§5).

Native source is `vendor/typescript-go` @ `5b1047d`: `pseudochecker/lookup.go`
(the pseudo types), `checker/pseudotypenodebuilder.go`
(`pseudoTypeEquivalentToType`, `pseudoTypeToNode`), and the two consumers
`serializeReturnTypeForSignature` (`checker/nodebuilderimpl.go:2023`) and
`serializeTypeForDeclaration` (`:2181`).

Base, measured at the integrator's wrap-up call without waiting for batch AV:
integration tip `26e3eab` (after batch AS), merged with r5-nodereuse's branch
(`67c5fbb` on this branch) and its three call-site diffs applied. That base has
types 549,333 RIGHT / 853 GAP / 6,114 WRONG, and diagnostics 5,484 RIGHT + 5,596
EMPTY_RIGHT of 12,238 rows.

## 1. The targets, classified against native

| case (lines) | native rule | where native decides | port |
|---|---|---|---|
| `declarationEmitScopeConsistency3` 0:3, 0:12 | **Single call signature.** `f = (v: "inner") => () => null! as typeof v` returns an arrow, so `typeFromSingleReturnExpression` answers `PseudoTypeSingleCallSignature` whose return is `Direct(typeof v)`. `pseudoTypeToNode` enters the inner signature's scope inside the outer signature's, which binds `v`, so the node survives at a site (`f`'s declaration) where `v` is the OUTER `"outer"` parameter. | `pseudotypenodebuilder.go:546` (equivalence), `:192` (print) | §2, owned |
| `circularObjectLiteralAccessors` 0:0, 0:6, 0:12 | **Object literal, accessor pair.** `getAccessorMember` (`lookup.go:363`) keeps both accessors when the getter and the setter are both annotated; `addPropertyToElementList` → `serializeTypeForDeclaration` prints `b: { get foo(): string; set foo(value: string); }`. 0:1 (the literal expression itself) keeps `b: { foo: string; }`: its fresh type has `ObjectFlagsRequiresWidening`, which refuses reuse (`nodebuilderimpl.go:2232`). | `pseudotypenodebuilder.go:422` (equivalence), `:208` (print) | §2 owned, §5 diff `r5-nodereuse2-object-literal-slot.diff` |
| `complexRecursiveCollections` 1:24, 1:598 | **Written predicate at the site** (r5-nodereuse's R2, `pseudoReturnTypeMatchesPredicate`). The corpus case reaches `signature_to_string_at`, which handed every predicate signature to the site-free printer. There `Collection.Keyed` resolves only inside `declare module Immutable`, so the site-free visitor marks it scope-local and refuses; the predicate prints sorted. A reduced repro at top level reuses correctly, which is why r5-nodereuse's repro did not reproduce it. | `nodebuilderimpl.go:2045` | §4, diff `r5-nodereuse2-predicate-at-site.diff` |

Native `tsgo --declaration` (built from the pinned submodule) confirms both
structural rows on `export const f = (v: "inner") => () => null! as typeof v;`
and the case's object: it emits `export declare const f: (v: "inner") => () =>
typeof v;` and `b: { get foo(): string; set foo(value: string); };`.

## 2. The structural pseudo types

`pseudochecker.rs` now builds the whole `PseudoType` tree (`lookup.go`) — Direct,
the keyword and literal leaves, `MaybeConstLocation`, the `| undefined` union
`addUndefinedIfDefinitelyRequired` makes, and the three structural kinds — and
`node_reuse.rs` ports `pseudoTypeEquivalentToType` and `pseudoTypeToNode` over
it (`Checker::structural_pseudo_text`).

**Asked only after the Direct answer, and only with a site.** The two slots
(`reused_return_text`, `reused_property_type_text`) keep r5-nodereuse's
Direct path untouched; the tree is built only where that answer is none, the
tree is structural, and the printer has a site. Native asks either serializer
only with `b.ctx.enclosingDeclaration != nil`. The port's site-free printers
are the ones that bake a type's text when the type is created, on the
checking path (`r5-nodereuse.md` §7), so a structural walk there would cost
every unannotated function returning a function, whether or not it is ever
printed. Ir stayed flat (§6).

**One walk for equivalence and print.** Native checks the whole tree, then
prints it. Both recurse over the same tree against the same checker types,
and nothing is emitted until the whole walk succeeds, so the port does both in
`pseudo_node_text`: a `None` anywhere drops the partial text. This keeps the
checker type of each part at hand, which the print of an `Inferred` part needs.

**Scope.** `ReuseContext::pseudo_owner` held the printed signature, and a
parameter was in scope only when its function WAS that signature
(`r5-nodereuse.md` §3, R5). `pseudoTypeToNode`'s structural arms call
`enterNewScope` for every signature they enter: the single call signature,
and an object literal's methods and accessors. The rule is now "the
parameter's function lies between the printed declaration and the reused
node". Below a function, the pseudochecker reaches a node only through
`typeFromFunctionLikeExpression` or an object literal's method or accessor,
so every such function is a scope the walk entered. For a Direct node of the
printed signature itself nothing lies between, so the old rule is the special
case.

**`reuseName`** (`nodecopy.go:24`): a reused member name is re-classified by
`classifyPropertyName`. Without it the first measurement lost 7 RIGHT lines
in `jsDeclarationsPackageJson`, whose JSON keys printed `"cli"` where native
prints `cli`.

**`ObjectFlagsRequiresWidening`** has no flag in this port. It is read as
"an object literal that widening has not replaced (`is_object_literal_type`),
an array-literal image, a widening nullable, or a union or intersection
holding one". Falsifier: a property whose type carries `ContainsWideningType`
through a member this reading misses (an object type that is not a literal
but propagates the flag). Its structural pseudo would then be reused where
native serializes the type.

**Not ported, with the reason:**

- **`Inferred` is equivalent only through error charity.** Native's
  `pseudoTypeToType(Inferred)` is `getWidenedType(getRegularTypeOfExpression(node))`,
  or the return of `getSignatureFromDeclaration` for a signature return.
  Asking the checker for an expression's type at print time can mint types
  (and with them union order) that the check never made. A structure with an
  `Inferred` part (`() => (y: number) => y`) therefore declines and is
  serialized from its type. That prints the same text whenever the
  structure's other parts print as their types do. Falsifier: a returned
  function with a written parameter alias and an identifier body, which
  native prints with the alias.
- **A literal leaf reads `node_types` only.** `getRegularTypeOfExpression` is
  a cached read in native too. An expression the check never typed declines
  rather than minting a literal type at print time.
- **The contextual-type consultation of `MaybeConstLocation`**
  (`pseudotypenodebuilder.go:94`). It applies to a node the pseudochecker sees
  in a const context and the checker does not. It declines.
- **A method with no single call signature on the target.** Native skips
  validating it and prints it from syntax alone. That print has no checker
  type for an `Inferred` part, so it declines.
- **Accessor declarations in the property slot** (`GetTypeOfAccessor`, a
  Direct answer for an annotated accessor property). Out of this lane's
  structural scope, and it would change the Direct path's population; not
  measured.

## 3. Ir (item 2)

The structural arm runs only for site-aware prints, so the checking path does
no new work. Callgrind Ir (`--singleThreaded --pretty false --noEmit`):

| tree | domain-model | generic-imports |
|---|---|---|
| base (above) | 1,114,748,056 | 342,946,276 |
| + this lane, both diffs (final stack) | 1,114,558,207 (−0.02%) | 342,948,199 (+0.00%) |

An earlier preview measurement (base at batch AP plus r5-nodereuse) of the
owned code plus the object-literal-slot diff gave −0.01% and −0.00%.
r5-nodereuse's base binary varied by 0.08% between two runs, so both results
are within noise.

## 4. The predicate signature at the site (item 3)

`signature_to_string_at` (`signatures.rs`, r5-printer3) returned the
site-free print for every predicate signature ("a predicate's print is
site-independent"). That held before r5-nodereuse: the predicate was printed
from its type. Now the written predicate node is reused, and a reused node's
names are tracked at the print site (`trackExistingEntityName`). The
site-free tracker stands for the top level of the annotation's file. A
predicate naming `Collection.Keyed` inside `declare module Immutable` is
scope-local there, so it is refused and the sorted type prints.

`r5-nodereuse2-predicate-at-site.diff` removes the early return. The
predicate signature then takes the site-aware printer, and its return slot
falls back to `type_predicate_to_string` when the written node is not reused,
which is `serializeReturnTypeForSignature`'s order. The parameters of a
predicate signature are now printed at the site as well. The old comment
said "the twin's counterfactual excluded them"; the measurement (§6) shows no
loss.

## 5. Measured diffs (call sites in files this lane does not own)

Each applies on top of this lane's committed `node_reuse.rs`.

| diff | file (owner) | adds |
|---|---|---|
| `r5-nodereuse2-object-literal-slot.diff` | `printing.rs` (r5-printer3) | `certified_object_literal_text_at` asks `reused_property_type_text` for each property-assignment slot before serializing it: `addPropertyToElementList` → `serializeTypeForDeclaration` |
| `r5-nodereuse2-predicate-at-site.diff` | `signatures.rs` (r5-printer3) | §4 |

## 6. Measurements

All against the base of the header, both dumps unfiltered, compared with
`cut -f1,2`; `slowcases` on both dumps.

| tree | types | diagnostics | slowcases |
|---|---|---|---|
| final stack (owned + both diffs) | **+26 RIGHT, 0 lost** (549,359 RIGHT / 6,088 WRONG) | unchanged, 0 lost | clean |

Converted lines, by root cause. These were measured stepwise on the preview base (AP + r5-nodereuse) and
add up to the final stack's 26:

- owned code and `r5-nodereuse2-object-literal-slot.diff`, measured together
  (10): `declarationEmitScopeConsistency3` 0:3, 0:12 (the structural return
  slot, owned code alone, verified filtered), `circularObjectLiteralAccessors`
  0:0, 0:6, 0:12 (needs the diff), and `noUsedBeforeDefinedErrorInTypeContext`
  0:2, 0:7, 0:8, 0:14, 0:20. Whether these five need the diff was not split
  out before the wrap-up;
- `+ r5-nodereuse2-predicate-at-site.diff`: `complexRecursiveCollections` 1:24,
  1:598, 1:814, `formatToPartsFractionalSecond` (2), `intersectionsOfLargeUnions`,
  `intersectionsOfLargeUnions2`, `quickinfoTypeAtReturnPositionsInaccurate`,
  `conditionalTypes2` (3), `intlNumberFormatES2023` (2), `literalTypeWidening`,
  `unionAndIntersectionInference1` (2) (16).

The first preview measurement, before `reuseName` (§2), lost 7 lines in
`jsDeclarationsPackageJson`. With it, no line is lost.

`cargo test --release -p tsr-checker` passes, including the five new
`tests/node_reuse_structural.rs` tests. `clippy -D warnings` reports nothing
in the code this lane touched. Stable clippy flags pre-existing code in
`enum_initializer.rs`, `members.rs`, `index_signatures.rs`, `signatures.rs:4596`,
`templates.rs` and `unique_symbols.rs`.

**Not measured:** the wall/CPU harness (`whole_project_perf.py`). The round
was wrapped up before it ran. Ir is the deterministic stand-in.

## 7. Remaining

- **The `Inferred` equivalence** (§2): it needs a print-time expression type
  that mints nothing.
- **The accessor property slot** (`GetTypeOfAccessor`, a Direct answer).
- **`builtinIteratorReturn`, `divergentAccessors1`,
  `declarationEmitPartialNodeReuseTypeReferences`, and
  `mappedTypeTupleConstraintAssignability`** are unchanged from
  `r5-nodereuse.md` §5. They belong to other lanes' printers.
