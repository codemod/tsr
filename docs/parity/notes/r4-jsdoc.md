# r4-jsdoc — JSDoc type hosting (round 4)

Lane brief: [round4.md](../round4.md), section `r4-jsdoc`. Issues
`tsr-2zk.16.98`, `.16.105`, `.16.106`, `.16.107`, `.16.38`. Native source is
`vendor/typescript-go` @ `5b1047d`.

## Ownership as worked

The owned files (`jsdoc_params.rs`, `jsdoc_annotations.rs`,
`jsdoc_modifiers.rs`, `jsdoc_full_signature.rs`, `js_case_data.rs`) hold the
replay of `reparseHosted` for function hosts, but none of the read sites the
five issues name. `js_case_data.rs` is generated Unicode case-mapping data
(`internal/stringutil/js_case_generated.go`) and has nothing to do with JSDoc.
On 2026-10-08 the integrator assigned three JSDoc-only helpers in
`symbols.rs` to this lane — `jsdoc_type_annotation`,
`jsdoc_parameter_annotation`, `jsdoc_cast_annotation` — editable inside their
bodies only. Every other site is delivered as a measured patch under
`docs/parity/notes/r4-jsdoc-*.diff`.

## 1. A parameter's own `@type` (`.16.98`, `jsdocTypeTagOnParameter1`)

**Native.** `reparseHosted`'s `KindJSDocTypeTag` arm has a `KindParameter`
case (`parser/reparser.go:363`): a comment written on the parameter itself,
`function f(/** @type {string} */ message)`, sets the parameter's `Type` from
the first `@type` tag of its last comment. From then on every consumer —
`getTypeForVariableLikeDeclaration` for the symbol, `getSignatureFromDeclaration`
for the signature, the reparse of the function's own comment (`param.Type ==
nil` test of the `@param` arm, `noTypedParams` of the full-signature arm) —
sees a typed parameter.

**Port.** `Checker::jsdoc_parameter_hosted_type` (`jsdoc_params.rs`) answers
that `Type` from the parameter's `jsdoc_entries` entry. Two consumers read it:

- `jsdoc_parameter_annotation` (`symbols.rs`) asks it first, before the
  function hosts' `@param` tags, because the parameter is finished — and its
  comment reparsed — before the function's comment is.
- `jsdoc_reparsed_function`'s replay seeds its per-parameter `typed` state
  with it, so a later `@param` does not claim the slot and a function `@type`
  does not become the full signature (`noTypedParams` is false).

The signature line needs no third change: `getSignatureFromDeclaration`'s
port types an identifier parameter through the symbol.

**Measured.** `jsdocTypeTagOnParameter1` 7 WRONG -> RIGHT; no other type
line or diagnostic verdict moves.

**Convention record.** No cache, side table or traversal is added: one
`jsdoc_entries` lookup keyed by the parameter's `NodeId` (owner: the parser's
side table, immutable after parse), then one comment's tags. Nothing is
published.

**Falsifier.** A parameter with both its own `@type` and a matching `@param`
whose baseline shows the `@param` type would prove the order wrong.

## 2. Measured patches (not committed as code)

Native reproduction used a tsgo built from the pinned submodule
(`scripts/offline-cargo/build-tsgo.sh`). The three patches below were applied
together on top of §1 and measured against this box's frozen baseline
(`b23dd3d`, before §1): **+42 type lines over §1** (470017 RIGHT vs 469973),
diagnostics +5 cases RIGHT, −2 cases EMPTY_RIGHT → EMPTY_WRONG, 2 type lines
RIGHT → WRONG. Every one of the four losses is a coincidence the patch
unmasks, analysed in §2.4; none is a scope error of the patch. Perf (median
child CPU, new/old, 41 samples): domain-model 1.025, generic-imports 1.018.

They are delivered as patches because none of their sites is owned by this
lane (`tsr-binder`, `declared.rs`), and the third depends on the first.

### 2.1 `r4-jsdoc-scope-hop.diff` — JSDoc host hop in `resolveName` (`.16.107`)

**Forcing fact.** In a JS **module** a top-level `@typedef` never resolves
from a `@type`, a `@param`, or any other JSDoc type reference. Reproduced:
`export class P {} /** @typedef {string} A */ /** @type {A} */ var x = 1;`
— native tsgo reports TS2322 (`number` to `string`); TSR reports nothing and
types `x` as the unresolved `A`. In a *script* file the same reference works
only because the walk dead-ends at the JSDoc comment (no parent edge) and then
falls through to `globals`, where a script's locals were merged.

**Native.** The reparser parents every JSDoc type node under its host
(`parser/reparser.go` `reparseTags`, `finishReparsedNode`), so
`binder/nameresolver.go`'s walk climbs the host's scopes.

**Patch.** The binder records `JSDoc -> host` while it binds the comments
(`bind_jsdoc_declarations`), publishes it on `BindResult` (`jsdoc_host`), and
`resolve_name_excluding_with_export_alias` advances through
`scope_parent`, which crosses from a parentless `JSDoc` node to its host. The
comment's own locals (typedef `@template` scope) are still consulted first,
because the comment is the node the walk is on when it dead-ends.

**Convention record.** New side table `jsdoc_hosts: NodeId(JSDoc) ->
NodeId(host)`; owner: the binder, filled once per file during
`bind_jsdoc_declarations`, merged in `publish_file` exactly as
`computed_names`; immutable after publication; no receiver/alias context; the
work is one hash lookup per resolution walk that runs off a comment (no
traversal added). The checker already keeps an identical map
(`Checker::jsdoc_hosts`, `checker.rs:1296`); the integrator may prefer to
make that one read this.

**Converts** (type lines): assertionTypePredicates2 2, callbackOnConstructor
5, importTag24 4, inferThis 6, typeTagOnFunctionReferencesGeneric 3,
varRequireFromJavascript 2, varRequireFromTypescript 2,
expandoFunctionContextualTypesJs 2; diagnostics RIGHT:
checkJsdocTypeTagOnExportAssignment1/4/6, checkJsdocSatisfiesTag9.

**Not fixed by it.** `typedefScope1` still declares every typedef at file
scope (`binder.rs` `declare_jsdoc_symbol(root, …)`); native binds the
reparsed alias in the host's block. Its `notOK : B` line and TS2304 need the
typedef declared in the host's container — next step for `.16.107`. A JSDoc
reference to an undeclared name in a module still reports no TS2304.

### 2.2 `r4-jsdoc-class-template.diff` — class-host `@template` (`.16.106`)

**Native.** `reparseHosted`'s `KindJSDocTemplateTag` arm
(`parser/reparser.go:459-470`) gives an unparameterised `ClassDeclaration`
or `ClassExpression` the `gatherTypeParameters(jsDoc, false)` of its last
comment (nothing when that comment declares a typedef/callback).

**Patch.** `local_type_parameters_of` answers
`jsdoc_class_template_parameters(class)` for a JS class with no written type
parameters. **Known limitation:** the function returns a borrowed slice, and
two parameter-carrying `@template` tags would need a concatenation it has no
owner for, so that comment keeps the old (empty) answer. `tsr-2zk.901`
(merged class+interface parameters, first declaration only) is the same
function and the same ownership question; one owned-allocation change would
serve both.

**Converts:** jsdocClassMissingTypeArguments 3, jsdocTemplateTag6 11,
extendsTag1 1, extendsTag5 1, jsdocTemplateTag7 1, overloadTag3 1,
jsFileMethodOverloads 1; and it removes two overloadTag3 losses the hop
alone causes (`Foo<number>` resolving to a class with no parameters).

### 2.3 `r4-jsdoc-property-type.diff` — `@type` on a class property (`.16.98`)

**Native.** `reparseHosted`'s `KindPropertyDeclaration` case
(`parser/reparser.go:356`): an unannotated property, `#private` included,
takes the first `@type` of its last comment as its type node.

**Patch.** `jsdoc_type_annotation` (an r4-jsdoc function) answers that for a
`PropertyDeclaration`. **Converts:** jsdocPrivateName1 1,
typeFromPrivatePropertyAssignmentJs 9, lateBoundAssignmentCandidateJS1 8,
jsDeclarationsInheritedTypes 3 (+ its diagnostics case).

**Why it is not committed.** Alone it loses
`typedefOnSemicolonClassElement:0:1`: the property's `@type {A}` names a
typedef in a JS module, which only §2.1 makes resolvable; the line was RIGHT
because the property was typed from its initializer instead of its tag.
Apply after §2.1.

### 2.4 The losses the set unmasks

| Line / case | Native | With patches | Unported piece it exposes |
|---|---|---|---|
| jsdocImportType:0:8 | `D` | `any` | `/** @type {D} */` naming a `require` value: `getTypeFromJSDocValueReference` (`checker.go`). Before, `D` did not resolve and node reuse printed the name. |
| importTag24:1:18 | `() => Foo` | `() => string` | return-annotation node reuse of `@returns {Foo}` in the signature printer (`node_reuse.rs`); before, `Foo` was unresolved and printed by name. |
| checkJsdocTypeTagOnExportAssignment8 (diag) | none | TS2322 | `getContextualType`'s `KindExportAssignment` arm (`checker.go:29398`, `tryGetTypeFromTypeNode(parent)`) for a hosted `@type`: `b: 'b'` widens to `string`. `contextual.rs`. |
| expandoFunctionContextualTypesJs (diag) | none | 4× TS2322 | contextual type of an expando assignment `F.p = {...}` from the declared type's property; literals widen. `contextual.rs`. |

Each needs its native piece ported before the patch set can merge loss-free.

## 3. `.16.105` — `export default` with `@type`

Reproduced: typing the ExportAssignment arm of `get_type_of_symbol` from the
tag (`symbols.rs`, not owned) converts checkJsdocTypeTagOnExportAssignment2
only; the other six print `Foo` where native prints `import("./a").Foo`.
Cause: `Checker::symbol_chain` (`checker.rs`) declines the `import("./a").`
qualifier whenever the reference's file imports the module under any binding
(`imported_here`), but native qualifies when the *name* is not accessible
(`b.js` imports only the default). That is the symbol-chain printer lane
(`tsr-2zk.39`). `checkJsdocTypeTagOnExportAssignment8` additionally needs the
contextual arm in §2.4.

## 4. `.16.38` — setter `@param`

`accessor_annotation` (`symbols.rs`, not owned) reads only the written type
of the setter's first parameter. Native `getEffectiveSetAccessorTypeAnnotationNode`
(`checker.go:20118`) reads `GetSetAccessorValueParameter(node).Type()` — the
first non-`this` parameter, whose type in JS is the reparsed `@param` (or its
own `@type`, §1). The faithful change is to fall back to
`jsdoc_parameter_annotation(value_parameter)`. Not measured in this round.

## 5. Catch-clause `@type` (`.16.98`, jsdocCatchClauseWithTypeAnnotation)

`catch (/** @type {unknown} */ err)` — native's `KindVariableDeclaration`
case hosts the tag on the catch variable. TSR's early return in
`get_widened_type_for_variable_like_declaration` (`symbols.rs`) answers
`any`/`unknown` from `useUnknownInCatchVariables` whenever the *written*
annotation is absent, before any JSDoc read, and `check.rs`'s TS1196 check
reads only the written type. Both sites are outside the lane; the reader half
(`jsdoc_type_annotation` for a declaration whose parent is a `CatchClause`)
is one more arm in the lane's function once those consumers ask it.
