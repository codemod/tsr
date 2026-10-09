# r6-jsdoc — the JSDoc triage's held clusters (round 6)

Lane `r6-jsdoc`, epic `tsr-2zk` (with `tsr-2zk.1110`, `tsr-2zk.1100`).
Native source is `vendor/typescript-go` @ `5b1047d`. Continues
[r5-jsdoc5](r5-jsdoc5.md) §2/§3/§6 (the T1–T12 triage), [r5-js](r5-js.md)
and [ADR-0046](../../adr/0046-jsdoc-reparse-is-a-checker-query.md).

## 1. Base and method

Frozen base `b18aec06` (main `17265fac` plus bookkeeping), measured in this
container: types **549,853 RIGHT** of 556,303 aligned lines (843 GAP, 5,607
WRONG); diagnostics **5,530 RIGHT / 5,596 EMPTY_RIGHT** of 12,238 rows
(1,063 WRONG, 49 EMPTY_WRONG). Perf is Callgrind Ir of `tsr -p <project>
--singleThreaded --pretty false --noEmit`; the base binary reads
domain-model **1,091,518,529** and generic-imports **343,082,952**, and two
runs of that same binary differ by about 56k Ir (0.005%).

Every expectation was checked against a native `tsgo` built from the pinned
submodule (`scripts/offline-cargo/build-tsgo.sh`). Every measurement is
unfiltered against the base; `slowcases` runs on both dumps.

Lane ownership leaves most producers in other files. Each such change ships
as a measured diff here, against this lane's own commit on the branch; the
lane's commits are neutral alone (both dumps identical to the base), so a
diff's numbers are the whole effect.

## 2. T4: `@overload` tags are declarations (`tsr-2zk.16.163`)

**Forcing fact.** `overloadTag1`'s `export function overloaded(a,b)` under
two `@overload` blocks printed `(a: number, b: number) => number` — the
first overload's parameters read as the implementation's — and its `a`
printed `any`. Native's reparser turns each `@overload` of every comment on
a function, method or constructor declaration (outside object literals)
into a body-less declaration of the host's kind, name and modifiers
(`reparseUnhosted`, `parser/reparser.go:134`; `reparseJSDocSignature`,
`:142`), and `parseListIndex` (`parser/parser.go:619`) puts it **before** the
host. The binder makes it another declaration of the host's symbol;
`getSignaturesOfSymbol` (`checker.go:19806`) reads it as an overload and
skips the host as the implementation (`previous.Flags&NodeFlagsReparsed !=
0` stands in for adjacency); the host's own parameters read the comment's
top-level `@param`s.

**Port.** ADR-0046 keeps declaration-emitting arms as parser-built nodes.
Three halves:

- **Parser** (`parse_tag`'s `overload` arm, diff): `parseOverloadTag`
  (`parser/jsdoc.go:1157`) — the run of `@param`/`@this` tags and one
  `@return` folds into the tag through `parse_jsdoc_signature`, as for
  `@callback`, stored as the reparsed function type (real
  `ParameterDeclaration`s with the reparsed `?`, `...` and names). Unlike a
  callback's, a missing `@return` leaves the return type unset: the reparse
  is a body-less declaration, whose return is `any`.
- **Binder** (`bind_jsdoc_declarations`, diff): binds the signature, and
  inserts the tag into the host symbol's declarations ahead of the host,
  only for the hosts `reparseUnhosted` accepts.
- **Checker** (`jsdoc_overloads.rs`, owned, committed): the parts a function
  type cannot carry — the host (`jsdoc_overload_host`), the tags before a
  host (`jsdoc_overload_tags`), the implementation rule against a reparsed
  previous (`jsdoc_overload_precedes_implementation`), the error location
  (the tag name, `finishReparsedNode(signature, tag.TagName())`) and the
  TS7012 arm of `checkFunctionOrMethodDeclaration` (`checker.go:3446`,
  `reportImplicitAny`'s reparsed arm at `:18332`). The diff adds
  `jsdoc_overload_signature`: `getSignatureFromDeclaration` over the tag's
  function type, with `gatherTypeParameters(jsDoc, false)`
  (`reparser.go:293`) for a function or method; a constructor's are the
  class's, set by the constructor list.

The hooks (diff [`r6-jsdoc-overload.diff`](r6-jsdoc-overload.diff)):

- `signatures.rs`: `get_signatures_of_symbol_for_type` takes an overload
  tag's signature; `resolve_class_construct_signatures` lists each
  constructor's tags before it (native's `__constructor` symbol holds
  them); `is_overload_implementation` asks the owned rule first;
  `type_parameter_of` becomes `pub(crate)`.
- `check.rs`: `checkFunctionOrConstructorSymbol` sees the tags as body-less
  function declarations beside their host (shared parent, host's modifiers,
  no "implementation expected" for a reparsed previous); TS2394 relates the
  tag's signature and reports at the tag name; `constructor_siblings_of`
  includes them; and the JS gate of `checkFunctionOrMethodDeclaration`
  (`checker.go:3430`): a JS file never checks the local symbol and checks the
  symbol only when it has a parent (an export or a member).
- `symbols.rs` `jsdoc_parameter_annotation`: the decline on any comment
  carrying `@overload` goes. It was written when the implementation's
  parameters read the first overload's `@param`; the replay reads the last
  comment's top-level tags, which no longer include an overload's run.

**Judgment calls.** The declaration in the symbol is the **tag** node and the
signature's declaration is the tag's **function type**: no node can be the
reparsed `FunctionDeclaration` itself (ADR-0046 fact 2), and
`reorder_candidates` must see a function-like declaration (a tag would read
as parentless and reverse the list). The committed owned functions carry
`#[expect(dead_code)]` naming the diff, the handshake `signatures.rs:2832`
describes: applying the diff deletes them, and forgetting to fails
`-D warnings`.

**Measured** (the diff on this lane's commit, unfiltered): types **549,853
→ 549,890 RIGHT (+37 lines)**, cases **+4** (`overloadTag1`,
`overloadTag2`, `jsFileMethodOverloads`, `jsFileMethodOverloads3`);
diagnostics **5,530 → 5,531 RIGHT** (`jsFileMethodOverloads3`: TS7012 ×2);
`overloadTag1`/`2` gain their TS2394 rows' codes but stay WRONG (below).
Zero losses on both dumps; no other row changed text; `slowcases` clean.
Ir domain-model 1,092,357,885 (+0.08%), generic-imports 343,074,123
(−0.003%). `crates/tsr-checker/tests/jsdoc_overloads.rs` (in the diff):
three tests, each failing without it; each expectation checked against
native `tsgo`.

**Left in T4.**

- `overloadTag1` TS2769 ×2 and `overloadTag2` TS2554: `calls.rs`'
  `check_resolve_call_arity` answers `Undecided` for every call in a JS
  file, so no argument or arity error is reported there. Narrowing it is
  `chooseOverload`'s JS arms (`isUntypedSignatureInJSFile`, `arguments`
  use), main's file.
- `overloadTag2` TS7006 on the constructor's undocumented `b`:
  `implicit_any.rs` `check_implicit_any_parameters` declines every JS file
  (its own comment: written before the `@param` replay existed).
- `overloadTag3`'s two `new Foo()` lines (`Foo<any>` for `Foo<number>`):
  not the overload — a JS `new` of a `@template` class with no arguments
  does not infer from the assignment's contextual type, overload or not.

**Falsifier.** A JS method with `@overload` whose host's own `@template`
differs from the overload comment's: native gives each overload its own
comment's type parameters; a case wanting the host's would show
`gatherTypeParameters` read from the wrong comment.

## 3. T11: `@return` predicates (`tsr-2zk.1110`)

**Forcing fact.** `returnTagTypeGuard`'s `chunk.isInit(chunk) ? chunk.c :
chunk.d` read `chunk.c` on `Entry | Group` (`error`), and
`assertionsAndNonReturningFunctions`' `assert2(typeof x === "string");
x.length` left `x : any`. `reparseHosted`'s `KindJSDocReturnTag` arm
(`parser/reparser.go:514`) clones `tag.TypeExpression().Type()` — the
braces' content — into the function's `Type`, so `{value is T}` and
`{asserts x}` are the return type node and `getTypePredicateOfSignature`
(`relater.go:2029`) builds the predicate from it. `signatures.rs`' older
JSDoc road kept the `JSDocTypeExpression` wrapper, so its
`TypePredicateNode` match never fired and the predicate was dropped.

The `@type {AssertFunc}` arm of the same case is
`isDeclarationWithExplicitTypeAnnotation` (`flow.go:2197`): in a JS file a
variable's `node.Type()` is its reparsed `@type`, so `getTypeOfDottedName`
types `assert` and its `asserts` signature narrows. `flow.rs`'
`get_explicit_type_of_symbol` read written annotations only.

**Diff** [`r6-jsdoc-return-predicates.diff`](r6-jsdoc-return-predicates.diff):
`signatures.rs` unwraps the tag's `{…}` (`get_signature_from_declaration`'s
`@returns` read); `flow.rs` asks ADR-0046's hosted-type queries
(`jsdoc_type_annotation`, `jsdoc_self_hosted_type`,
`jsdoc_reparsed_parameter_type`) when nothing is written.

**Measured** (alone, on this lane's `6460af0`): types **549,853 → 549,870
RIGHT (+17 lines)**, no case converts alone; diagnostics unchanged; zero
losses; no other row changed text; `slowcases` clean. Ir domain-model
1,091,827,831 (+0.03%), generic-imports 343,066,152 (−0.005%).
`jsdoc_return_predicates.rs` (in the diff): two tests, each failing without
it, each checked against `tsgo`.

**Left.** `returnTagTypeGuard`'s `(val: boolean | number) => void` prints the
union in id order where native reuses the written `@param` node (T9, node
reuse). `assertionsAndNonReturningFunctions`' `f2 : (b: boolean) => 0 | 1`
is `error`: `signatures.rs`' return inference declines two or more distinct
return types in any JS file, a decline written for `@overload` before §2
existed; removing it on top of the overload diff is this lane's next step.

## 4. T5: generic `@callback` and function-type `@typedef` instantiations

**Forcing fact.** `/** @type {Id<string>} */ var one_twenty = s => "120"`
under `@template T @callback Id` typed `s : any`, and a call through any
generic JS function-type alias (`@typedef {(t: T) => T} F`, `F<string>`)
answered `error`; the non-generic forms and the TypeScript twin worked.
Native's reparse makes both `JSTypeAliasDeclaration`s, instantiated by
`getTypeAliasInstantiation` like a written alias. `declared.rs`' §947.2 arm,
which gives a generic alias whose body is a function or constructor type its
signatures, read only a written `TypeAliasDeclaration`.

A second, smaller producer showed once the first was fixed:
`/** @template V @callback One … */ /** @type {One<string>} */ var b = t =>
t` printed `<V>(t: string) => string`. Both comments are hosted on the
statement, and `signatures.rs`' older JSDoc road (and the module host's
`jsdoc_template_parameters`, `tsr-compiler`) skipped a comment's
`@template` for a typedef but not for a callback; `gatherTypeParameters`
(`parser/reparser.go:293`) returns none for either.

**Diff** [`r6-jsdoc-generic-callback.diff`](r6-jsdoc-generic-callback.diff):
the §947.2 arm reads `type_alias_body` and `local_type_parameters_of` (both
already JSDoc-aware, and equal to the written alias's fields for a
TypeScript alias); both template gatherers skip a callback comment too.

**Measured** (alone, on `6460af0`): types **549,853 → 549,864 RIGHT (+11
lines)**, cases **+1** (`typeTagNoErasure`); diagnostics verdicts unchanged;
zero losses; `slowcases` clean. The callback-comment hunk alone measures
zero (it needs the first to give the alias a signature) and adds nothing on
top of it in the corpus; it ships because the probe above, checked against
`tsgo`, shows it. Ir domain-model 1,090,795,391 (−0.07%), generic-imports
343,063,700 (−0.006%). `jsdoc_generic_callbacks.rs` (in the diff): three
tests, each failing without it.

Two diagnostics rows change text, both still WRONG:

- `jsdocCallbackAndType` gains its expected TS2322 (6,5); its TS2554 waits on
  `calls.rs`' JS decline (§2).
- `typeTagNoErasure` now reports a TS2322 native does not: with the arrow
  typed `<T1 extends number>(dibbity: T1) => T1` (its `.types` lines are now
  all RIGHT), the relater refuses it against `Test<number>`. The same gap
  shows in pure TypeScript — `declare const g: <T1 extends number>(d: T1) =>
  T1; const t: Test<number> = g` with `type Test<T> = <T1 extends T>(data:
  T1) => T1` reports TS2322 where `tsgo` reports nothing; the TypeScript twin
  of the case hides it only because its arrow gaps to `error`. Routed to the
  relater lane (generic signature against an instantiated generic alias's
  signature).

**Left.** `callbackTag2`'s four `Final<…>` lines: the `@callback Final`
comment ends the file, so it waits on T8's EOF host (this lane's item 5).
`callbackTagNamespace` waits on dotted `@callback` names (item 6).

## 5. T4, continued: a JS function's return union (after §2)

**Forcing fact.** With §2 applied, `assertionsAndNonReturningFunctions`' `f2`
(`switch (b) { case true: return 1; case false: return 0; }`) still printed
`error`, as does any JS function with two distinct return types.
`signatures.rs` declines `getReturnTypeFromBody`'s `UnionReductionSubtype`
aggregate (`checker.go:20191`; and its yield, generator-return and async
twins) in every JS file. The decline's own comment gives the reason: "JSDoc
`@overload` signatures this port does not model — 10 of §11.1's 26 wrong
lines". §2 models them, so the decline's reason is gone; native has no JS arm
here.

**Diff** [`r6-jsdoc-js-return-union.diff`](r6-jsdoc-js-return-union.diff),
**applied after** `r6-jsdoc-overload.diff`: the four `in_js_file` declines
go.

**Measured** (on §2's diff state, unfiltered): types **549,890 → 549,893
RIGHT (+3 lines)**, cases **+2** (`unreachableJavascriptChecked`,
`unreachableJavascriptUnchecked`; the third line is `f2`); diagnostics
unchanged; zero losses against both §2's state and the frozen base; no other
row changed text; `slowcases` clean. Ir domain-model 1,092,343,897 (−0.001%
against §2's state), generic-imports 343,055,524 (−0.006%).
`js_return_union.rs` (in the diff): one test, failing without it, checked
against `tsgo`.

**Falsifier.** A JS function with two return types that native prints as one
of them: the decline would have been hiding a reduction difference, not an
overload.

## 6. `checkTypeParameters` and the type-parameter modifier arms

**Forcing fact.** TS1273/TS1274/TS1277 (`@template private T`, `@template
in T` on a function, `@template const T` on a typedef) and TS2706/TS2744
(a required parameter after a defaulted one; a default naming a later
parameter) were missing in `jsdocTemplateTag7`/`8`/`jsdocTemplateTagDefault`
— and, unported for TypeScript too, in `varianceAnnotations`,
`typeParameterConstModifiers`, `genericDefaults`, `genericDefaultsErrors`,
`subclassThisTypeAssignable01` and
`typeArgumentDefaultUsesConstraintOnCircularDefault`. Their native homes:

- `checkTypeParameters` (`checker.go:7002`) with
  `checkTypeParametersNotReferenced` (`:7022`): this port's
  `check_type_parameter_list` (`check.rs`) carried only its duplicate scan.
- `checkGrammarModifiers` (`grammarchecks.go:295` TS1273, `:303-312`
  TS1277, `:526-543` TS1274/TS1030/TS1029): this port's loop is
  `check_modifier_order`, which had no type-parameter arm. The `in`/`out` arm
  also fires off a type parameter (`class C { in a = 0 }`).

In a JS file the reparser hands `@template` lists to declarations
(`gatherTypeParameters`, `parser/reparser.go:293`): every comment declaring
a typedef or callback gives its alias the comment's whole list
(`reparseUnhosted`); the last comment otherwise gives it to the
`getFunctionLikeHost` function (unless that has a written list or the
comment's `@type` became its full signature first) or to a class
(`reparseHosted`, `:453`). `checkGrammarModifiers` then reads that
declaration as the parameter's `Parent`.

**Port.** Owned (`jsdoc_checks.rs`, committed, neutral alone — both dumps
identical to the base): `jsdoc_template_lists(node)` answers those lists
with their declaration's kind, the check walk visits each list's parameters
(so `checkTypeParameter`'s node checks reach them, constraints and defaults
included), and `jsdoc_template_owner_kind(parameter)` answers the reparsed
parent. Diff [`r6-jsdoc-template-grammar.diff`](r6-jsdoc-template-grammar.diff)
(`check.rs`): the two default rules in `check_type_parameter_list` (now
`pub(crate)`); `check_type_parameter_modifier`, called from
`check_modifier_order` for a type parameter or an `in`/`out` token, reading
the parent through `jsdoc_template_owner_kind`; and `check_type_parameter_list`
on each JS list beside the written one. TS2744 compares the reference's type
with each later parameter's declared type — one type per symbol here, as
native compares `t.symbol`.

**Measured** (owned + diff, on `208c3ec`, unfiltered): diagnostics **5,530 →
5,534 RIGHT (+4 cases)**: `jsdocTemplateTag7`, `jsdocTemplateTag8`,
`typeParameterConstModifiers`,
`typeArgumentDefaultUsesConstraintOnCircularDefault`. Five more WRONG rows
move only toward the baseline (expected diagnostics gained, none lost, none
unexpected): `genericDefaults` +11, `varianceAnnotations` +8,
`jsdocTemplateTagDefault` +4, `genericDefaultsErrors` +1,
`subclassThisTypeAssignable01` +1. Types unchanged; zero losses on both
dumps; `slowcases` clean. Ir domain-model 1,092,529,002 (+0.09%),
generic-imports 343,079,120 (−0.001%). `type_parameter_grammar.rs` (in the
diff): two tests (written lists and `@template` lists), both failing without
it; every expectation checked against `tsgo`.

**Left.** `jsdocTemplateTagDefault`'s TS2322 at (9,20) (`/** @type {A} */
const aDefault2 = [0]` with `A`'s `T` defaulting to `string`): the alias's
default under a JS reference without arguments. `varianceAnnotations`'
TS2636/TS2637 (variance checks) and one TS2322; `genericDefaultsErrors`' and
`subclassThisTypeAssignable01`'s TS2344 — other producers.

## 7. TS2300 on JSDoc declarations: three producers

**Forcing facts.** `importTag4` (two `@import { Foo }` in one file) wants
TS2300 on both `Foo`s; `typedefCrossModule5` (script `mod1.js` with
`@typedef {number} Foo` and `class Bar {}`, script `mod2.js` with
`class Foo {}` and `const Bar = 3`) wants TS2300 on both `Foo`s and TS2451
on both `Bar`s; `jsDeclarationsDefaultsErr` wants TS2300 on `export default
class C` and a `@typedef … default`. Read one at a time, three producers:

1. **The binder's JSDoc declarations skip `declareSymbol`'s exclusion
   test.** `declare_jsdoc_symbol` merges a same-named tag into the first
   symbol and never reports. Native binds the reparsed `JSImportDeclaration`
   through `declareModuleMember`'s alias arm (`binder/binder.go:380`) and the
   `JSTypeAliasDeclaration` through `bindBlockScopedDeclaration` (`:1238`),
   where a module-level alias is implicitly exported
   (`IsImplicitlyExportedJSDocDeclaration`, `ast/utilities.go:4184`).
2. **The cross-file merge reporter skips every JS declaration.**
   `reportMergeSymbolError` (`checker.go:14215`) skips a side only when its
   first declaration is in a *plain* JS file (`ast.IsPlainJSFile`: no
   `@ts-check` directive and `checkJs` unset); `merge_conflicts.rs` read
   JS-ness alone because the checker could not see `checkJs`.
3. **Exporting a typedef by name then reported TS2484** once (1) exported it
   implicitly (`importingExportingTypes`' `export { JSDocType }`):
   `checkAliasSymbol`'s JS arm (`checker.go:6750`) — TS18042 for a type
   imported in JS, TS18043 for one exported, then `return` before the
   conflict test — was unported.

**Diffs** (each measured unfiltered against the base, zero losses on both
dumps, `slowcases` clean; types unchanged by all three):

- [`r6-jsdoc-js-alias-types.diff`](r6-jsdoc-js-alias-types.diff)
  (`symbols.rs` `check_alias_symbol` and `report_js_type_alias`;
  `check.rs` `declaration_is_type_only` made `pub(crate)` as
  `IsTypeOnlyImportOrExportDeclaration`): diagnostics **+2 cases**
  (`elidedJSImport1`, `importingExportingTypes`), and
  `jsDeclarationsInterfaces` (+4) and `jsxCheckJsxNoTypeArgumentsAllowed`
  (+1) move only toward the baseline. `tsr-compiler/tests/js_alias_types.rs`.
- [`r6-jsdoc-jsdoc-declarations.diff`](r6-jsdoc-jsdoc-declarations.diff),
  **applied after** the alias diff (`binder.rs` `declare_jsdoc_alias`,
  `declare_jsdoc_import`): **+1 case** (`importTag4`), nothing else moved
  on top of the alias diff; alone it adds a false TS2484 to
  `importingExportingTypes`, which is why it rides on it.
  `tsr-binder/tests/jsdoc_declarations.rs`. A module's typedefs are now in
  its exports, so `@import { Foo } from "./mod1"` of a typedef resolves
  (probe, checked against `tsgo`); no corpus row reads it yet.
- [`r6-jsdoc-plain-js-merge.diff`](r6-jsdoc-plain-js-merge.diff)
  (`ModuleHost::is_plain_js_file` in `resolution.rs`, the program's answer
  through `program_diagnostics::is_plain_js_file`, and the per-side test
  plus the typedef/callback name as error node in `merge_conflicts.rs`):
  **+1 case** (`typedefCrossModule5`).
  `tsr-compiler/tests/plain_js_merge_conflicts.rs`.

All three together: diagnostics **5,530 → 5,534 RIGHT**. Ir (all three)
domain-model 1,092,121,372 (+0.06%), generic-imports 343,083,326 (+0.0001%).
Every test fails without its diff; every expectation checked against `tsgo`.

**Left.** `jsDeclarationsDefaultsErr`: its `@typedef … default` comment ends
the file, so it waits on T8's EOF host (item 5); with that, the binder diff
above reports it (the unit test shows the shape with a following statement).

## 8. T8: comments before end of file document the EOF token

**Forcing fact.** `jsdocTypeDefAtStartOfFile`'s `/** @type {Third} */ var c`
typed `c : Third` (unresolved) because `@typedef {number} Third` follows
the last statement. `parseSourceFileWorker` (`parser/parser.go:438`) runs
`withJSDoc(eof, endJSDoc)`: the trailing comments document the end-of-file
token, and `reparseTags` reparses their typedefs, callbacks and imports into
the file's statements like any host's. `parse_source_file` parsed them for
their diagnostics and dropped them; its comment held them back because "a
bound end-of-file `@typedef` meets the checker's unfinished typedef alias
bodies" (`js.md`), which no longer reproduces.

**Diff** [`r6-jsdoc-eof-host.diff`](r6-jsdoc-eof-host.diff) (parser): attach
them to the EOF token. **Applied after** §7's two diffs: an EOF typedef in a
module is then implicitly exported, as native's is; without them
`reuseTypeAnnotationImportTypeInGlobalThisTypeArgument` trades its false
TS2305 for a false TS2459 ("declared locally, but not exported").

**Measured** (on §7's alias + declarations diffs, unfiltered): diagnostics
**+2 cases** (`jsDeclarationsDefaultsErr` — §7's binder diff now sees its
EOF `@typedef … default`; `recursiveResolveDeclaredMembers`, EMPTY_WRONG →
EMPTY_RIGHT), and `reuseTypeAnnotationImportTypeInGlobalThisTypeArgument`
loses its false TS2305; types **+3 lines**, **+1 case**
(`jsdocTypeDefAtStartOfFile`), and `jsDeclarationsUniqueSymbolUsage` moves
toward (its remainder is the written import's quote style, T9). Zero losses
against both that state and the frozen base; `slowcases` clean. Ir
domain-model 1,092,146,397 (+0.002% on that state), generic-imports
343,050,102 (−0.01%). `jsdoc_eof_host.rs` (in the diff): one test, failing
without it.

**Left.** `importTypeResolutionJSDocEOF` and
`jsdocResolveNameFailureInTypedef` now resolve their EOF aliases; their
remaining lines print the alias name where native prints its target
(`import("./interfaces").Bar`, `CantResolveThis`), and the second's TS2304
waits on `check_type_reference_name`'s JS decline (r5-jsdoc4 §4.1).
`callbackTag2`'s `Final` lines take this diff and §4's together.

## 9. T2 leftovers: two producers, neither JSDoc-specific

r5-jsdoc5 §6 left four T2 cases. `typeTagNoErasure` converted with §4;
`typeTagOnFunctionReferencesGeneric` is already RIGHT at this base. The
other two:

- **`jsdocTemplateTag7`'s `f : <T>(x: T) => T` was `any`**: `@template
  private T`. `type_parameter_of` (`signatures.rs`) declined the whole
  signature for any modifier but `const`. Native's
  `getTypeParameterModifiers` (`relater.go:1433`) keeps only `in`, `out` and
  `const`; `private` is TS1273's grammar error (§6) and nothing more. Diff
  [`r6-jsdoc-type-parameter-error-modifiers.diff`](r6-jsdoc-type-parameter-error-modifiers.diff):
  any other modifier is ignored; `in`/`out` still decline, because the
  printer has no slot for them (`jsdocTemplateTag8`'s `<in T>(x: T) =>
  void`, printer lane). Measured alone: types **+1 line, +1 case**
  (`jsdocTemplateTag7`), zero losses, `slowcases` clean, Ir
  1,092,117,518 / 343,065,785 (noise). Test
  `type_parameter_error_modifiers.rs`.
- **`typeFromJSInitializer3`'s `const a = f1()` was `any`** where `f1`
  returns a declared `undefined` under `strictNullChecks: false`; the
  TypeScript twin fails the same way, and so does `declare const u:
  undefined; const c = u` (`tsgo` prints `undefined` for all three).
  `symbols.rs`' non-strict widening widened the plain `null`/`undefined` as
  well as `createWideningType`'s twins; `getWidenedType` (`checker.go:16090`)
  widens only types carrying `ObjectFlagsContainsWideningType`, which
  `Intrinsics::is_widening_nullable` already answers. Diff
  [`r6-jsdoc-nonstrict-declared-nullable.diff`](r6-jsdoc-nonstrict-declared-nullable.diff).
  Measured alone: types **+6 lines, +2 cases** (`typeFromJSInitializer3`,
  and T12's `jsDeclarationsFunctionJSDoc` — its `@param {null} b` under
  `strict: false`), zero losses, `slowcases` clean, Ir 1,092,127,431 /
  343,065,099 (noise). Test `nonstrict_declared_nullable.rs`, checked
  against `tsgo`'s declaration output.
