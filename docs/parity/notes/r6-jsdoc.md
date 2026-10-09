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
