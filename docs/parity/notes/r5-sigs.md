# Lane notes: r5-sigs (`tsr-2zk.1021`; `.1016`, `.1015`)

Round-5 cloud lane. Owns `signatures.rs` (signature construction, return-type
inference, signature printing) and `node_reuse.rs`. Native source is
`vendor/typescript-go` @ `5b1047d` (`internal/checker/` unless noted).
Frozen baseline: this box's dumps at `7521044` (main `f1ba6b6` merged) —
types 543,559 RIGHT / 7,946 WRONG / 1,028 GAP of 552,533 aligned lines;
diagnostics 12,238 rows.

## §1 Written annotations under a print-only type-parameter rename (`tsr-2zk.1016`)

**The cases.** `complexRecursiveCollections:1:910/916/924` print the
`zipWith` overload set of `Immutable.Collection.Indexed`, whose third overload
returns the written `Collection.Indexed<Z>`. Native prints
`Collection.Indexed<Z_1>` where the shadow rename gives `Z` the name `Z_1`.
With r5-typeparams2's merged type-parameter list, the print-only rename clone
(`rename_type_parameters_for_site`, `inference.rs`) stops declining for these
overloads. It instantiates `Collection.Indexed<Z>` (a qualified generic mint,
`declared.rs` §42 v2) to the fresh `Z_1`. `instantiate_type_worker`'s
reference arm then rebuilds it through `create_type_reference_with_display`,
which prints the bare declared name: `Indexed<Z_1>`. That is three R→W lines
in r5-typeparams2 §5.

**What native does.** Native never instantiates the signature to rename it.
`signatureToSignatureDeclarationHelper` (`nodebuilderimpl.go:1792`) prints the
ORIGINAL signature inside `enterNewScope` (`nodebuilderscopes.go:59`), and the
renaming is `typeParameterToName`'s allocation (`nodebuilderimpl.go:1404`,
`ctx.typeParameterNames`). So every slot's reuse decision
(`serializeTypeForDeclaration`, `serializeReturnTypeForSignature`) is asked of
the original's types, where the written annotation is equivalent by
construction. The reused node's type-parameter identifiers are spelled by
`attachSymbolToLeftmostIdentifier` (`nodecopy.go:292-302`), which calls
`typeParameterToName` and so gets `Z_1`. The `Collection.Indexed` qualifier
is the written node's.

**The port.** This port keeps its print-only clone (the decision that the
§102/§107 rename is an instantiation is not reopened here). It adds the two
pieces native has:

1. `Checker::carry_written_annotations_through_rename` (`node_reuse.rs`)
   re-attaches each written annotation the original signature could reuse
   (the original slot holds exactly the annotation's type). The annotation is
   keyed to the clone's image, so every printer's identity gate
   (`WrittenAnnotation::is_equivalent_to`) holds unchanged, and it is marked
   `renamed`. Slots the original could not reuse keep the clone's
   serialization.
2. The existing-node visitor's `TypeReferenceNode` arm names a type-parameter
   identifier through the render's allocations
   (`render_type_parameter_names.allocations`, this port's
   `ctx.typeParameterNames`) when one exists, with or without a print site.
   A `renamed` annotation printed when no allocation is live (outside the
   clone's render) is refused, and the clone's image is serialized instead.

**Where the call lives.** `rename_type_parameters_for_site` is in
`inference.rs` (main's). The commit calls the carry from the one owned
caller, `signature_to_string_at`. `r5-sigs-rename-reuse.diff` adds the call
inside `rename_type_parameters_for_site` itself, so the composite overload
printer (`checker.rs`), the type-literal member printer (`printing.rs`) and
the callable-object printer (`callable_expandos.rs`) get it too. The
composite printer is the road for the three target lines. The carry is
idempotent (a slot that already has an annotation is skipped), so both calls
can stand.

**Port-convention boundary.** No cache or side table. The carried
annotation is the same `Copy` pair plus one flag, carried on the clone,
which is print-only and dropped after the render. The allocations it reads
are the existing per-render name table, truncated by the same printers that
push it. Work: one type comparison per annotated slot per rename, and only
for signatures that were actually renamed.

**Alternatives.**
- *Preserve the qualified spelling through instantiation* (rebuild the
  §42 v2 mint at the image arguments in `instantiate_type_worker`). This is
  rejected because it is not native's mechanism: native prints the written
  node, not a re-rendered reference. It would also respell every
  instantiation of a qualified generic, not only print-only renames. That
  is NB-SYMBOL-CHAIN territory (`tsr-2zk.39`), where routing namespace-rooted
  generics measured 54 R→W (`declared.rs` §42 v2 comment).
- *Print the original signature with renamed names* (drop the clone). That
  would be the most faithful option, but it rewrites §102/§107's measured
  renaming machinery across four printers. It wins if the clone's image
  serialization is ever shown to differ from native for an unannotated slot.

**How we would know this is wrong.** A renamed overload whose written
annotation native does NOT reuse (an annotation `pseudoTypeEquivalentToType`
refuses against the original slot) printing the written spelling here. The
carry only fires when the original slot is the annotation's own type, so
that would mean the original slot's type differs from native's.

**The inner-declaration decline.** The first full-stack run lost one line,
`jsxGenericComponentWithSpreadingResultOfGenericFunction:0:0`. The reused
return annotation `<T>(obj: T) => Omit<T, K>` declares its own `T`. Native's
visitor gives that declaration its own `enterNewScope`
(`nodecopy.go:700-845`), and `typeParameterToName` renames it to `T_1`
against the outer overload's `T`. This port's visitor emits inner
type-parameter declarations as written (a limitation for every reuse, which
does not show where nothing is renamed). A `renamed` annotation whose node
declares type parameters (function, constructor or member signature lists,
mapped-type keys, `infer`) is therefore refused, and the clone's
serialization, which already renames them, prints it. Porting the inner
`enterNewScope` allocation would remove this decline.

**Measured** (unfiltered, against the frozen baseline above; both loss
checks of `box-protocol.md` §5 printed nothing in every row):

| Set | type lines RIGHT | type losses | diag cases | diag losses |
|---|---|---|---|---|
| this commit alone (`signature_to_string_at` call) | ±0 (dumps byte-identical) | 0 | ±0 | 0 |
| this commit + `r5-sigs-rename-reuse.diff` + r5-typeparams2 identity-substitution + merged-parameters + alias-call-sites | **+170** (543,559 → 543,729) | **0** | **+3** | **0** |
| same stack before the inner-declaration decline | +170 / −1 | 1 | +3 | 0 |

The stack's diagnostics gains are r5-typeparams2's three
(privacyCheckExportAssignmentOnExportedGenericInterface1,
nonPrimitiveInGeneric, nonPrimitiveStrictNull). The three
complexRecursiveCollections lines (910/916/924), which were r5-typeparams2's
residual losses, stay RIGHT. Applying the stack needed one textual merge in
`perf_links.rs` (r5-perf4's `visiting_scratch` field next to the new
`local_type_parameters` field; keep both).

Perf (this commit's `tsr` against the baseline binary,
`whole_project_perf.py`, median child CPU, 41 samples): domain-model
**0.989**, generic-imports **0.981**, `diagnostics_match: true`. Callgrind
Ir (`--singleThreaded --pretty false`): domain-model-large 5,203,726,464 →
5,206,274,194 (+0.049%), generic-imports 399,493,630 → 399,500,918
(+0.002%).

## §2 Async generator next type from an IIFE's contextual type (`tsr-2zk.1015`) — blocked outside this lane

The 42 WRONG lines in `types.asyncGenerators.es2018.1/.2` are all one shape:
the inner generator of `yield* (async function * () { yield 1; })()` under a
contextual `AsyncIterableIterator<number>` (or `AsyncIterable<number>`).
Native prints `AsyncGenerator<number, void, any>`; this port prints
`unknown` for `next`. (The plain contextually typed generators,
`assignability1`/`2`, are already RIGHT: the `[]` arm of the next slot
answers `any` through `getContextualIterationType`.)

**Native road.** The inner function has no yields that contribute a next
type (`yield 1;` is a statement, and `getContextualType` of a yield whose
parent is an ExpressionStatement is nil). So `getReturnTypeFromBody` takes
`getContextualIterationType(Next, fn)` (`checker.go:20242`). The function
has no contextual signature, and `getContextualReturnType`'s IIFE arm
(`checker.go:29690`) answers the call's contextual type. That is the
`yield*` operand's (`getContextualTypeForYieldOperand`, `checker.go:29719`):
`Generator<number, never, any> | AsyncGenerator<number, never, any>`. On that
union `getIterationTypesOfIterable(…, AsyncGeneratorReturnType)` has no
async iterable for the `Generator` constituent, so the union answers
`noIterationTypes`. `getIterationTypesOfGeneratorFunctionReturnType` then
reads the union as an iterator, and `next`'s parameter gives `any`.

**This port.** Probed: TSR computes the same contextual return type
(`AsyncGenerator<number, never, any> | Generator<number, never, any>`), so
the IIFE arm and the yield-star operand are ported. The iteration query
answers `Err` (undecided): `[Symbol.asyncIterator]` on
`Generator<number, never, any>` is not *decidably absent*, because
`Generator` inherits through `IteratorObject<T, TReturn, TNext>`, a
type-argument base (`get_iteration_types_of_iterable_slow_worker`,
`iteration.rs`). That is the inherited-member lookup gap r5-iteration
recorded (`tsr-2zk.1013`, `members.rs`). `signatures.rs` then falls back to
the contextual signature's third type argument, and with no contextual
signature (an IIFE) it answers `unknown`.

Nothing in `signatures.rs` can decide that absence without guessing, so no
change is made here. Once `.1013` makes the absence decidable, the existing
`[]` arm should answer `any` with no change in this file (the union then
falls through to the iterator read, whose `next` members `Generator` and
`AsyncGenerator` declare directly). Cases: 42 lines.
