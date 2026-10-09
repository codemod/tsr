# r5-jsdoc5 — the JSDoc-only remainder, by native producer (round 5)

Lane `r5-jsdoc5`, epic `tsr-2zk` (with `tsr-2zk.1100`). Native source is
`vendor/typescript-go` @ `5b1047d`. Continues [r5-jsdoc4](r5-jsdoc4.md)
(its §6 table), [r5-jsdoc3](r5-jsdoc3.md), [r5-jsdoc2](r5-jsdoc2.md), the
r5-js triage (`docs/parity/notes/r5-js.md` on
`claude/beautiful-shannon-ar5gh0-r5-js`, §2 row S8: 53 cases + 2 by
correction) and [ADR-0046](../../adr/0046-jsdoc-reparse-is-a-checker-query.md).

Frozen baseline (integration head `28648eb`, batch AL): types **548,914
RIGHT** of 556,291 aligned lines (879 GAP, 6,498 WRONG); diagnostics
**5,440 RIGHT / 5,595 EMPTY_RIGHT** of 12,238 rows (1,151 WRONG, 52
EMPTY_WRONG).

## 1. Method

r5-js counted its S8 row at `e20cdd4` and did not commit the case list. It
is regenerated here at `28648eb`:

- **Types.** 94 cases have every non-RIGHT `.types` line in a `.js`/`.jsx`
  file (r5-typetriage's `file_names` over the reference baselines). The
  r5-js rows that are not JSDoc (S1 expandos, S2 CommonJS, S5–S7, S9) are
  dropped by reading; the rest are below, each assigned to the producer of
  its first wrong line after a probe (`probefile`) of the shape.
- **Diagnostics.** 88 WRONG/EMPTY_WRONG rows have every missing and extra
  diagnostic in a JS file; the JSDoc ones are listed with their codes, plus
  r5-smallcodes2's four routed cases.

A case is listed once, under the producer that blocks it first. "diff"
means the producer is in a file this lane does not own (the box brief's
MUST-NOT-TOUCH list); the change ships as a measured diff.

## 2. Types: the JSDoc-only cases by producer

| # | native producer | TSR site (owner) | cases | witnesses |
|---|---|---|---:|---|
| T1 | `getEffectiveTypeAnnotationNode` reading the reparsed `param.Type` and the `makeNewCast` wrapper on `return` (`reparser.go:378`, `getContextualType`'s `KindAsExpression` arm) | `symbols.rs` `jsdoc_parameter_annotation` keeps its own host walk (no `return` host; name-text match, not `findMatchingParameter`) — diff; the cast's contextual type — `jsdoc_annotations.rs` (owned) | 2–3 | `jsdocSignatureOnReturnedFunction`, `unicodeEscapesInJSDoc` |
| T2 | `getSignatureOfFullSignatureType` in `getSignaturesOfSymbol` / `getTypeParametersFromDeclaration` (`checker.go:19827`, `:19913`): a function's `@type` signature, type parameters included, *is* its signature | `signatures.rs` `get_signatures_of_symbol_for_type` (r5-printer2) — diff; `jsdoc_full_signature.rs` (owned) | 5–7 | `typeTagWithGenericSignature`, `typeTagOnFunctionReferencesGeneric`, `typeTagNoErasure`, `checkJsdocTypeTag7`, `typeFromJSInitializer3`, `jsdocTemplateTag7`/`8` |
| T3 | `gatherTypeParameters`' `@template {C} T` constraint on the reparsed parameter (`reparser.go:293`) | `signatures.rs` `type_parameter_of` reads `node.constraint` only (`members.rs` already asks `jsdoc_template_constraint`) — diff | 4 | `jsdocTemplateTag3`, `contravariantOnlyInferenceFromAnnotatedFunctionJs`, `jsdocTemplateTag6`, `jsdocTemplateTagDefault` |
| T4 | `reparseJSDocSignature`: each `@overload` is a reparsed declaration in `symbol.Declarations` | binder + `signatures.rs` (`.16.163`) | 5 | `overloadTag1`/`2`/`3`, `jsFileMethodOverloads`, `jsFileMethodOverloads3` |
| T5 | `@callback` → `JSTypeAliasDeclaration` of a function type; `@template` parameters of the callback | `declared.rs` / contextual signature of the alias (unread further) | 2 | `callbackTag2`, `callbackTagNamespace` |
| T6 | JS value references and import types in JSDoc (`getTypeFromJSDocValueReference`, `getTypeFromImportTypeNode` with qualifiers/arguments, `typeof import(…)`) | `declared.rs` (r5-declared3) | 13 | `checkJsdocTypeTagOnExportAssignment1`/`2`/`5`/`6`, `jsDeclarationsTypeReassignmentFromDeclaration`, `jsDeclarationsImportNamespacedType`, `jsDeclarationsUniqueSymbolUsage`, `jsDeclarationEmitDoesNotRenameImport`, `jsdocImportType`, `commonJSImportClassTypeReference`, `commonJSImportExportedClassExpression`, `varRequireFromTypescript`, `jsDeclarationsParameterTagReusesInputNodeInEmit2` |
| T7 | `getIntendedTypeFromJSDocTypeReference`'s `Object.<K, V>` arm → `Record<K, V>` (`checker.go:23058`) | `declared.rs` `get_intended_type_from_jsdoc_type_reference` (declines: "no alias instantiation by type list") | 1 | `checkJsdocSatisfiesTag2` |
| T8 | `withJSDoc(eof, …)`: a comment before end of file hosts typedefs on the EOF token (`parser.go:438`) | parser `parse_source_file` (main, `.17.1`) — diff | 2–3 | `importTypeResolutionJSDocEOF`, `jsdocResolveNameFailureInTypedef`, `jsdocTypeDefAtStartOfFile` |
| T9 | the node builder reusing the written JSDoc node (`(number \| null) \| null`, `Array<any>`, `n?: number \| undefined`) | `node_reuse.rs` / printer (r5-nodereuse, r5-printer2) | 5 | `jsdocPostfixEqualsAddsOptionality`, `jsdocParseBackquotedParamName`, `jsdocTypeGenericInstantiationAttempt`, `checkJsdocParamOnVariableDeclaredFunctionExpression`, `jsdocFunctionTypeFalsePositive` |
| T10 | literal and tuple preservation under a JSDoc contextual type (`@satisfies`, `@type`, `@template` default) | `contextual.rs` / literal freshness (main) | 6 | `checkJsdocSatisfiesTag8`, `checkJsdocSatisfiesTag15`, `jsdocBracelessTypeTag1`, `jsDeclarationsTypedefFunction`, `arrowExpressionBodyJSDoc`, `arrowExpressionJs` |
| T11 | `@return {this is T}` / `asserts` predicates on JS methods and functions | `signatures.rs` predicate of a reparsed return (r5-printer2) | 2 | `returnTagTypeGuard`, `assertionsAndNonReturningFunctions` |
| T12 | not yet read to a producer | — | 9 | `jsDeclarationsFunctionJSDoc` (`@param {null}` under `strict: false`), `jsDeclarationsDefault`, `jsDeclarationsReexportAliases`, `jsDeclarationsExportSpecifierNonlocal`, `jsDeclarationsWithDefaultAsNamespaceLikeMerge`, `instantiateTemplateTagTypeParameterOnVariableStatement`, `optionalBindingParameters3`/`4`, `jsdocTemplateTagNameResolution` |

Ranges are where a case's later lines may belong to a second producer; §4
records the measured number for each port.

## 3. Diagnostics: the JSDoc-only rows by producer

| code(s) | cases | producer | site |
|---|---|---|---|
| TS2394, TS7012, TS2769, TS2554 | `overloadTag1`/`2`, `jsFileMethodOverloads3` | overload signatures | T4 |
| TS2304 | `jsdocResolveNameFailureInTypedef`, `typedefScope1` | EOF host (T8); `check_type_reference_name`'s JS decline (r5-jsdoc4 §4.1, refused at +1/−6) | parser; `declared.rs` value arms first |
| TS2314 | `typedefMultipleTypeParameters` | the arity rule ignores a typedef's `@template` count (`.1100`) | `type_argument_arity.rs` — diff |
| TS2344 | `unmetTypeConstraintInJSDocImportCall` | import type with type arguments answers `error` (`.1100`) | `declared.rs` |
| TS18046, TS2339 | `jsdocCatchClauseWithTypeAnnotation` | `unknown` receiver (`.1100`); TS twin fails identically | `nullable_operand.rs`/`members.rs` (main) |
| TS2300 | `importTag4`, `jsDeclarationsDefaultsErr`, `typedefCrossModule5` | `@import`/`@typedef` declarations conflicting in the binder | binder (main) |
| TS2352 | `checkJsTypeDefNoUnusedLocalMarked`, `jsDeclarationsDefault` | the deferred JSDoc cast check | `expressions.rs` (hub) |
| TS2322/TS2345 | `arrowExpressionBodyJSDoc`, `jsdocBracelessTypeTag1`, `checkJsdocSatisfiesTag8`/`15`, `importTag24`, `jsdocPostfixEqualsAddsOptionality`, `contextuallyTypedParametersOptionalInJSDoc`, `jsdocTemplateTagNameResolution`, `typeTagNoErasure`, `jsdocCallbackAndType` | follow the type rows (T2, T10, T6) | — |
| TS1273/TS1274/TS1277, TS2706/TS2744 | `jsdocTemplateTag7`/`8`, `jsdocTemplateTagDefault` | `@template` modifiers and defaults not walked | `jsdoc_checks.rs` (owned) |
| TS6196 | `unusedTypeParameters_templateTag` | unused `@template` parameter | `unused.rs` |
| TS2339/TS2741 | `jsdocTemplateTag3` | follows T3 | — |
| TS2463 | `optionalBindingParameters3` | binding pattern optional via JSDoc | T12 |
| TS7009 | `constructorTagOnObjectLiteralMethod` | `@constructor` on an object-literal method | unread |
| TS2686, TS4023, TS2823/TS2857 | `jsdocReferenceGlobalTypeInCommonJs`, `jsDeclarationsTypeReassignmentFromDeclaration2`, `importTag15` | UMD global in JSDoc; declaration emit; import attributes on `@import` | unread |
| TS2454 vs TS18048 | `jsdocImportType` | T6 | `declared.rs` |

## 4. Ports

Perf is Callgrind Ir of `tsr -p <project> --singleThreaded --pretty false
--noEmit`; the frozen base binary reads domain-model **1,156,991,440** and
generic-imports **342,951,514**. Every measurement is unfiltered against the
frozen base, with `slowcases` run on both dumps.

### 4.1 T1, half one: the contextual type of a reparsed `@type` cast (committed)

**Forcing fact.** `function f() { /** @type {(a: number) => number} */
return function (a) { … } }` typed `a` as `any`. Native's reparser turns
that `@type` into `return <fn> as T` (`reparseHosted`, `KindReturnStatement`
and `KindParenthesizedExpression`, `parser/reparser.go:378`), so the
function's contextual type is `getContextualType`'s `KindAsExpression` arm:
the asserted type, or none for `@type {const}`. The function never gets a
`FullSignature` there: `getFunctionLikeHost` sees the cast, not a function.

**Port.** `jsdoc_cast_contextual_type` (`jsdoc_annotations.rs`): an operand
of a `return` or parenthesized expression whose last comment's first typed
`@type`/`@satisfies` tag is a `@type` answers that type. It runs inside the
existing `jsdoc_satisfies_contextual_type` hook at the head of
`get_contextual_type` — the two are the two `makeNewCast` wrappers, and the
innermost one (the first tag) is the operand's parent — so no file outside
this lane changes. TypeScript files pay one parent probe and one
`jsdoc_entries` probe, the same as the satisfies arm before.

**Judgment call.** The cast arm reads the *last* comment's *first* typed
tag (native's hosting); the typing road `jsdoc_cast_annotation`
(`symbols.rs`) still reads the first typed `@type` of *any* comment. They
disagree only for a paren or `return` with two comments that both type it,
which no corpus case has.

**Measured.** Types **548,914 → 548,928 RIGHT (+14 lines)**, no case
converts alone (`jsdocSignatureOnReturnedFunction` also needs §4.2's diff);
diagnostics unchanged; zero losses on both dumps; no non-RIGHT line changed
text; `slowcases` clean. Ir domain-model 1,156,990,783 (−0.0001%),
generic-imports 342,956,087 (+0.001%).
`crates/tsr-checker/tests/jsdoc_reparsed_casts.rs`: four tests (return
cast, paren cast, `const`, untyped comment).

**Falsifier.** A case with `/** @type {A} */` and `/** @type {B} */` as
two comments on one `return`, whose function wants `A`'s context.

### 4.2 T1, half two: the `@param` read goes through the replay (diff)

**Forcing fact.** With §4.1, `/** @param {number} a */ return (a) => …`
still typed `a` as `any`. `symbols.rs`' `jsdoc_parameter_annotation` — the
road `get_type_for_variable_like_declaration` and `contextual.rs` take for
an unannotated JS parameter — keeps its own host walk from before ADR-0046:
it climbs variable, property, expression-statement and paren hosts but not
`return` (`getFunctionLikeHost`'s `KindReturnStatement` arm), matches a
`@param` by name text in *any* comment rather than by
`findMatchingParameter` over the last one, and skips binding-pattern
parameters. ADR-0046 says a consumer of a reparsed fact calls the query.

**Diff** [`r5-jsdoc5-param-replay.diff`](r5-jsdoc5-param-replay.diff):

- `symbols.rs` `jsdoc_parameter_annotation`: after the parameter's own
  `@type` and the existing `@overload` decline, the slot
  `jsdoc_reparsed_function` gives the parameter's position — its reparsed
  `param.Type` and the matched tag's brackets.
- `destructure.rs` `get_type_for_binding_element_parent`: the effective
  type node of a `Parameter` holder is its written annotation, else
  `jsdoc_reparsed_parameter_type` (`tryGetTypeFromEffectiveTypeNode`,
  `checker.go:16694`). Without this hunk the first hunk lost
  `optionalBindingParameters3`'s `a : string`: the positional replay now
  types `@param {Foo} [options]` onto `function f({ a = "a" })`, and the
  parent road, which reads written annotations only, met the bracket's
  `Foo | undefined` through the symbol road instead of native's
  `includeOptionality: false` parent `Foo`.

**Measured** (on §4.1's `00527ff`, unfiltered): types **548,928 →
548,945 RIGHT (+17 lines)**, cases **+2**
(`jsdocSignatureOnReturnedFunction`,
`declarationEmitClassSetAccessorParamNameInJs3`); diagnostics unchanged;
zero losses on both dumps; `slowcases` clean. Two non-RIGHT lines change
text, both toward native (`optionalBindingParameters3`/`4`'s signatures now
read `Foo`/`{ cause?: string; }` and only lack the printer's dropped
`| undefined`). Ir domain-model 1,156,346,178 (−0.06%), generic-imports
342,958,210 (+0.001%).

**Falsifier.** A JS case where a `@param` in a *non-last* comment of the
host types a parameter in native: the replay reads the last comment only,
as `reparseTags`' `isLast` does.

### 4.3 T2: a function's `@type` signature is its signature (diff)

**Forcing fact.** `/** @type {<T>(param?: T) => T | undefined} */ function
typed(param)` printed `(param: any) => any`. Native's `getSignaturesOfSymbol`
(`checker.go:19827`) takes `getSignatureOfFullSignatureType(decl)` before
`getSignatureFromDeclaration`, and that is `getSingleCallSignature` of the
tag's type (`:20072`): the tag's own signature, **type parameters
included** (`getTypeParametersFromDeclaration`, `:19913`, reads the same).
TSR built the declaration's signature and patched each parameter from the
tag through the *contextual* `single_call_signature` (`contextual.rs`),
which refuses a generic signature, so every generic `@type` fell to `any`.

**Diff** [`r5-jsdoc5-full-signature.diff`](r5-jsdoc5-full-signature.diff),
three files that must land together:

- `jsdoc_full_signature.rs` (owned): `jsdoc_full_signature(function)` —
  `getSignatureOfFullSignatureType` over a faithful
  `getSingleSignature(t, Call, allowMembers=false)` (`:19353`: one call
  signature, no construct signature, no property or index). The
  parameter and return readers go through it.
- `signatures.rs` `get_signatures_of_symbol_for_type`: the full signature
  first, as native's loop.
- `members.rs` `property_names_of_type`: the anonymous arm answers a
  function or constructor type node's literal symbol (it declares only
  `__call`/`__new`). Without it the `getSingleSignature` property test
  meets "unknown" for an **alias-named** function type (`@type {Foo}` with
  `@typedef {(a: string) => void} Foo`, whose `signature` print flag is
  cleared by the alias name) and declines: alone, the first two hunks
  measured **+26 / −7 lines** (`checkJsdocTypeTag7` ×3 and
  `returnTagTypeGuard` ×4 — an `@callback Cb` predicate — RIGHT→WRONG),
  which the old contextual reader had answered.

**Not split.** The owned hunk alone is wrong: the parameter reader would
read the tag's `T` while the function's signature keeps no `T`
(`f : (l: T) => T`). So the owned file ships inside the diff.

**Measured** (all three hunks, on `00527ff`, unfiltered): types **548,928
→ 548,945 RIGHT (+17 lines)**, cases **+2** (`typeTagWithGenericSignature`,
`checkJsdocTypeTag7`); diagnostics unchanged; zero losses on both dumps; no
non-RIGHT line changed text; `slowcases` clean. Ir domain-model
1,156,961,873 (−0.003%), generic-imports 342,949,676 (−0.002%).
`crates/tsr-checker/tests/jsdoc_full_signature_generic.rs` (in the diff):
two tests, one fails without it.

**Left.** `typeTagOnFunctionReferencesGeneric`'s arrow and
`typeTagNoErasure` want the contextual generic signature's type parameters
on the arrow itself (`<T>(j: T) => T`, contextual instantiation —
`contextual.rs`, main); `jsdocTemplateTag7`/`8` want `@template private T`
and `<in T>` kept, which `type_parameter_of` declines whole for any
non-`const` modifier (`signatures.rs`); `typeFromJSInitializer3` wants the
tag's non-widening `undefined`/`null` return at the call.

**Falsifier.** A JS function whose `@type` names a type with both a call
signature and a property (a callable interface): native refuses the full
signature (`allowMembers=false`) and types the parameters from context;
this port does the same — a case wanting the tag's parameters there would
show the property test is wrong.

### 4.4 T3: a `@template {C} T` constraint on a function's signature (diff)

**Forcing fact.** `@template {{ a: number, b: string }} T,U` on a JS
function printed `<T, U, …>`. `gatherTypeParameters` (`reparser.go:293`)
gives the tag's *first* parameter the constraint; `members.rs`'
`type_parameter_constraint` already asks `jsdoc_template_constraint` for it,
but `signatures.rs`' `type_parameter_of` — which builds the printed and
inferred `TypeParameter` — read `node.constraint` only.

**Diff** [`r5-jsdoc5-template-constraint.diff`](r5-jsdoc5-template-constraint.diff):
`type_parameter_of` reads `node.constraint`, else
`jsdoc_template_constraint(node)` (one parent-kind test for an
unconstrained TypeScript parameter). Test
`jsdoc_template_constraint.rs` (in the diff) fails without it.

### 4.5 `.1100`: the arity rule counts a typedef's `@template` (diff)

`declared_type_parameter_arity` (`type_argument_arity.rs`) returned `None`
for a `JSDocTypedefTag`/`JSDocCallbackTag` declaration, silencing TS2314.
The reparsed alias's parameters are `local_type_parameters_of(symbol)` —
the list `declared.rs` already gathers (`gatherTypeParameters(jsDoc,
true)`). **Diff** [`r5-jsdoc5-typedef-arity.diff`](r5-jsdoc5-typedef-arity.diff),
with `jsdoc_typedef_arity.rs` (one of two tests fails without it).

### 4.6 §4.4 + §4.5 measured

Stacked (disjoint files, disjoint rows), on `affe6c2`, unfiltered: types
**548,928 → 548,936 RIGHT (+8 lines)**, cases **+1** (`jsdocTemplateTag3`);
diagnostics **5,440 → 5,441 RIGHT** (`typedefMultipleTypeParameters`),
EMPTY_RIGHT unchanged; zero losses on both dumps; no non-RIGHT line changed
text; `slowcases` clean. Ir domain-model 1,157,001,705 (+0.001%),
generic-imports 342,948,011 (−0.002%). The constraint hunk owns every type
line (the arity hunk only reports), and the arity hunk the diagnostics row.

## 5. Totals and landing

| step | on | types RIGHT | cases | diagnostics RIGHT | losses |
|---|---|---:|---|---:|---|
| base | `28648eb` | 548,914 | — | 5,440 | — |
| §4.1 cast context (commit `00527ff`) | base | 548,928 (+14) | 0 | 5,440 | none |
| §4.2 `r5-jsdoc5-param-replay.diff` | `00527ff` | +17 | +2 | — | none |
| §4.3 `r5-jsdoc5-full-signature.diff` | `00527ff` | +17 | +2 | — | none |
| §4.4–4.5 `template-constraint` + `typedef-arity` | `affe6c2` | +8 | +1 | +1 | none |

Sum of the separate measurements: types **+56 lines, +5 cases**,
diagnostics **+1 case**. The four diffs touch disjoint hunks (the two
`signatures.rs` hunks are in different functions) and apply together on
`affe6c2` in any order; `cargo fmt --check` is clean with all four, and
`cargo test --workspace --release` passes with all four applied (exit 0).
Every diff carries its test, each failing without its diff.

## 6. Remaining, and what each waits on

| cluster | cases | waits on | site (owner) |
|---|---:|---|---|
| T6 JS value references / import types | 13 | `getTypeFromJSDocValueReference`, qualified and type-argument import types; then narrowing `check_type_reference_name`'s JS decline (r5-jsdoc4 §4.1) | `declared.rs` (r5-declared3) |
| T4 `@overload` | 5 | reparsed overload declarations in the symbol's declarations | binder + `signatures.rs` (`.16.163`) |
| T10 literal/tuple preservation under JSDoc context | 6 | contextual literal freshness | `contextual.rs` (main) |
| T9 printer reuse of JSDoc nodes | 5 | node reuse of `?`/`=`/nullable JSDoc forms; `?: Foo` without the bracket's `undefined` on a pattern parameter | `node_reuse.rs`, `printing.rs` |
| T2 left | 4 | contextual generic signature instantiation of an arrow (`typeTagNoErasure`, `typeTagOnFunctionReferencesGeneric`); `@template` variance modifiers declining `type_parameter_of`; non-widening tag return at a call | `contextual.rs`, `signatures.rs` |
| T8 EOF comment | 2–3 | `parse_source_file` attaching `_end_docs` to the EOF token, the binder binding it, the walk reaching it | parser (main, `.17.1`) |
| T5 `@callback` parameters | 2 | contextual signature of a callback alias for an arrow initializer | unread past the producer |
| T7 `Object.<K, V>` | 1 | `getTypeAliasInstantiation(Record, [K, V])` | `declared.rs` |
| T11 predicates via `@return` | 2 | `this is T` / `asserts` from a reparsed return on a JS method | `signatures.rs` |
| TS1273/TS1274/TS1277, TS2706/TS2744 | 3 | `checkGrammarModifiers`' type-parameter arms and `checkTypeParameters`' default rules are unported **for TypeScript too**; the faithful home is `check_type_parameter_list` (`check.rs`), which the walk would then call for `@template` lists | `check.rs` (hub) |
| TS2300 on `@import`/`@typedef` conflicts | 3 | the binder's duplicate-declaration report for reparsed aliases | binder (main) |
| TS18046/TS2339 on `unknown` (`.1100`) | 1 (+TS twin) | `check_non_null_type_reporting` declines `unknown` (r5-jsdoc4 §3.2) | `nullable_operand.rs`/`members.rs` (main) |
