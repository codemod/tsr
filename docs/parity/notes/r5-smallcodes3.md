# r5-smallcodes3: thirteen small sole-code clusters

Lane on epic `tsr-2zk`, item `tsr-2zk.1117`, vendor `5b1047d`. Method as in
`r5-smallcodes.md` and `r5-smallcodes2.md`: list the diagnostics cases that
are WRONG on exactly one code, classify each against a native `tsgo` built
from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`), then port
the root causes.

Frozen base: integration head `918a4d0` (batch AN snapshots). Diagnostics
dump 12,238 rows: 5,470 RIGHT, 5,595 EMPTY_RIGHT, 1,121 WRONG, 52
EMPTY_WRONG. Types dump 556,291 lines: 549,034 RIGHT, 876 GAP, 6,381 WRONG.

Setup: PyPI answers 403 here (r5-operators3 §4). A stdlib-only stand-in for
`tomlkit`'s `parse`, `inline_table` and `dumps`, kept in the session
scratchpad and not committed, built the vendored tree.

The cases were listed with a throwaway script over `diagverdictdump`: WRONG
and EMPTY_WRONG rows whose differing `(file, line, column, code)` entries all
carry one code.

## 1. Triage

Every brief cluster, re-listed on the frozen base. Two of them shrank before
this lane started: batch AN landed r5-smallcodes' TS2880 parser diff (all
four cases RIGHT now) and its TS2538 index-image diff (six of seven).

| Code | Cases on the base | Shape | Native site | Where the fix lives |
|---|---|---|---|---|
| TS2722 | `interfaceClassMerging`, `controlFlowOptionalChain`, `logicalAssignment5` ×4, `parserAmbiguityWithBinaryOperator4` (7) | missing | `reportCannotInvokePossiblyNullOrUndefinedError` (`checker.go:9894`) through `checkNonNullTypeWithReporter` (`:8511`) | `calls.rs` (main's): §3.3, held diff |
| TS2391 | `dottedModuleName`, `destructuringParameterDeclaration6`, `objectTypesWithOptionalProperties2`, `parserErrantEqualsGreaterThanAfterFunction1`/`2`, `parserSkippedTokens16` (6) | extra | `parseFunctionBlock` builds a missing-brace `Block` (`parser.go:1205`), so `Body() != nil` and `checkFunctionOrConstructorSymbol`'s last test (`checker.go:3681`) does not report | parser (main's): §4.1, waits for batch AP |
| TS2307 | `importInsideModule`, `noCrashOnParameterNamedRequire`, `privacyGloImportParseErrors`, `tslibInJs`, `emitModuleCommonJS` ×2 (6) | missing | alias-target resolution at the use; the JS reparser's `require` import | `symbols.rs`, binder/reparser (main's); unchanged from `r5-smallcodes.md` §3.7 |
| TS2883 | `declarationEmitCommonJsModuleReferencedType`, `…ObjectAssignedDefaultExport`, `…ReexportedSymlinkReference3`, `declarationEmitUsingTypeAlias1` (4) | missing | module specifiers into nested `node_modules` | `tsr-2zk.999`/`.1098`; unchanged |
| TS1238 | `constructableDecoratorOnClass01`, `decoratorCallGeneric`, `decoratorOnClass8` (es2015), `esDecorators-arguments` (4) | missing | `checkDecorator` → `resolveDecorator` | `calls.rs` (main's): §4.2 |
| TS2540 | `omitTypeHelperModifiers01`, `readonlyAssignmentInSubclassOfClassExpression`, `readonlyMembers` (es2015), `globalThisReadonlyProperties` (4) | missing | `isAssignmentToReadonlyEntity` | two are fixed on r5-smallcodes2's branch (its §2.4), not yet integrated; two are `mapped.rs` (r5-mapped6) |
| TS2403 | `typeOfEnumAndVarRedeclarations`, `FunctionAndModuleWithSameNameAndCommonRoot`, `objectLiteralContextualTyping`, `parserCastVersusArrowFunction1` (4) | missing | `isTypeIdenticalTo`; inference of `T` with no candidate | relater identity (r5-relater7), `inference.rs` (main's); `r5-smallcodes2.md` §4 |
| TS17006 | `exponentiationOperatorInTemplateStringWithSyntaxError1`/`2`/`3`, `exponentiationOperatorSyntaxError1` (es2015) (4) | missing | `parseUnaryExpressionOrHigher` (`parser.go:4694`) | parser (main's): §4.1, waits for batch AU |
| TS2702 | `decoratorMetadataWithImportDeclarationNameCollision7` (es2015), `invalidUseOfTypeAsNamespace`, `strictModeReservedWordInClassDeclaration` (3) | missing | `checkAndReportErrorForUsingTypeAsNamespace` (`checker.go:1608`) | `check.rs` (main's): §3.2, held diff |
| TS2308 | `doubleUnderscoreExportStarConflict`, `exportNamespace8`, `exportStar` (es2015) (3) | missing | `getExportsOfModuleWorker` (`checker.go:16148`) | new module plus a `check.rs` hook: §2.1, lossless diff |
| TS2523 | `expressionsForbiddenInParameterInitializers`, `parser.asyncGenerators.functionDeclarations`/`functionExpressions.es2018` (alwaysstrict) (3) | missing | `checkGrammarYieldExpression` (`grammarchecks.go:1783`) | `check.rs` (main's): §2.2, lossless diff |
| TS2538 | `identifierStartAfterNumericLiteral` (1) | missing | `getPropertyTypeForIndexType` (`checker.go:27206`) on `3e[null]` | `index_access_reports.rs` (r5-relater7); not probed, see §4.3 |
| TS2880 | none | — | — | landed in batch AN |

Every file these clusters need is outside this lane's ownership (`check.rs`,
`calls.rs` and the parser are main's), so every port below ships as a
measured diff in this directory. Nothing in this lane's commits changes code.

## 2. Lossless diffs

### 2.1 TS2308 — `r5-smallcodes3-export-star-conflicts.diff`

`getExportsOfModuleWorker` (`checker.go:16148`) builds a module's export
table. Its `visit` closure merges every `export *`'s re-exported table with
`extendExportSymbols` (`:16235`). That function records, per name, the first
star's specifier text and every later star that brought a different resolved
symbol. The worker then reports TS2308 on each later star, unless the module
exports the name itself. `checkExternalModuleExports` forces the table for
every checked module (`:5672`), so a module's collisions are reported when
its own file is checked.

This port answers export lookups one name at a time (`get_export_from_star`,
`symbols.rs`, whose doc comment lists the collision table as not ported). The
diff adds `export_star_conflicts.rs`. It builds the whole table once per
checked module file that has an `export *`, and the table is dropped when the
call returns. It is called from `check_source_file` after the walk.

Kept from upstream:

- the recursion and its visited list;
- the `default` skip;
- `export=` skipped in the report;
- `resolveSymbol` on both sides of the comparison;
- the specifier as written, quotes included (`'./t1'` vs `"./t1"`, both
  probed).

A nested module's table is also visited **without**
`resolveExternalModuleSymbol`, as upstream's `visit(resolvedModule, …)` is.

Checker port convention:

- **Native operation:** `getExportsOfModule` → `getExportsOfModuleWorker`.
- **Key identity:** the binder's module `SymbolId`.
- **Cache:** none. The map is local to the call and nothing is published.
- **Expensive work:** the module-name resolution of each star, which every
  star reader already does.

Two deviations:

- **Reporting level.** Only the checked module's own level reports.
  Upstream's recursion also reports the nested levels and its diagnostic
  collection deduplicates the repeats. The two differ only for a module that
  is visited but never checked.
- **No table without a star.** A module with no `export *` has no collision
  table, so it returns before building one. Upstream builds and caches the
  table for its other readers.

Probe (native and TSR identical):

```text
t4.ts(2,1): error TS2308: Module './t1' has already exported a member named 'x'. …
```

Unit test: `tests/export_star_conflicts.rs`. Its four fixtures cover two
names that collide, a name the module exports itself, the same symbol reached
twice, and `default`.

### 2.2 TS2523 — `r5-smallcodes3-yield-parameter-initializer.diff`

`checkGrammarYieldExpression`'s second arm (`grammarchecks.go:1783`) is an
ungated `c.error(node, …)`, the twin of TS2524's arm in
`checkGrammarAwaitOrAwaitUsing`. TSR already had TS2524
(`check_await_in_parameter_initializer`, `check.rs`) and its climb
(`is_in_parameter_initializer_before_containing_function`). The diff adds
`check_yield_in_parameter_initializer` beside it.

Upstream only reaches the arm for a node its parser built as a
`YieldExpression`, and that depends on where the `yield` is written:

- **Inside a generator's parameter list.** `parseParametersWorker` sets the
  yield context, so every `yield` is a yield expression.
- **Anywhere else.** `isYieldExpression` takes `yield` as an expression only
  before an identifier, keyword or literal on the same line. A bare `yield`
  is a name.

TSR's parser builds a `YieldExpression` in both places. So outside a
generator's parameters the rule asks for the operand shape that
`check_yield_grammar` (TS1163) already bounds itself to.

Probe, every row identical in native and TSR:

| Source | Report |
|---|---|
| `function* g(a = yield 1, { b = yield }: any = 0) {}` | TS2523 ×2 |
| `function f(c = yield 2) {}` | TS1163 + TS2523 |
| `function h(yield = 1, d = yield) {}` | no TS2523 |
| `async function* k(e = yield) {}` | TS2523 |
| `const o = { *m(z = yield 3) {} };` | TS2523 |

Unit test: `tests/yield_in_parameter_initializer.rs`.

### 2.3 Measured

All four diffs (§2 and §3) were measured together, unfiltered, against the
frozen base. Each diff emits only its own codes. A changed row is therefore
attributed to its diff by code, and no row of any other code changed.

| Diff | Cases converted | Rows matched | Losses |
|---|---|---|---|
| TS2308 | `doubleUnderscoreExportStarConflict`, `exportNamespace8`, `exportStar(target=es2015)` | 5 | 0 |
| TS2523 | `expressionsForbiddenInParameterInitializers`, `parser.asyncGenerators.functionDeclarations.es2018(alwaysstrict=true)`, `…functionExpressions.es2018(alwaysstrict=true)` | 8 | 0 |
| TS2702/TS2713 (§3.2) | `decoratorMetadataWithImportDeclarationNameCollision7(target=es2015)`, `invalidUseOfTypeAsNamespace`, `errorForUsingPropertyOfTypeAsType02` | 8 | 1 |
| TS2721–2723 (§3.3) | `interfaceClassMerging`, `nullableFunctionError`, `controlFlowOptionalChain`, `logicalAssignment5` ×4, `parserAmbiguityWithBinaryOperator4` | 17 | 3 |

Together:

- **Diagnostics:** RIGHT + EMPTY_RIGHT went from 11,065 to 11,078 (+17, −4).
- **Types dump:** verdict columns unchanged (549,034 RIGHT).
- **slowcases:** only the KNOWN_SLOW cases, on both dumps.

The early return in §2.1 was added after this run. It cannot change output,
because a module without a star has no collision table.

Tests: the four new test files pass with the diffs applied, and
`cargo test --workspace --release` passes (§5). Clippy flags nothing in the
diffs; stable 1.97 flags pre-existing code in `unique_symbols.rs`,
`templates.rs`, `index_signatures.rs`, `enum_initializer.rs`, `signatures.rs`
and `members.rs`. `cargo fmt --check` is clean.

Ir (callgrind, `--singleThreaded --pretty false --noEmit`), each diff alone
against the base binary:

| Binary | domain-model | generic-imports |
|---|---|---|
| base `918a4d0` | 1,116,822,678 | 342,954,998 |
| TS2308 (with the early return) | 1,117,902,599 (+0.097%) | 342,962,670 (+0.002%) |
| TS2523 | 1,117,023,442 (+0.018%) | 342,957,022 (+0.001%) |
| TS2702/TS2713 | 1,116,760,407 (−0.006%) | 342,943,012 (−0.003%) |
| TS2721–2723 | 1,117,025,698 (+0.018%) | 342,956,749 (+0.001%) |

Ir is **not** deterministic on this box. A second pair of runs read the base
at 1,117,787,775 (+0.086% against its first run) and TS2308 at 1,117,198,762,
which is −0.05% against that base. Neither bench project has an `export *`,
so TS2308's work there is two table probes per module file. In that profile
`check_export_star_conflicts` is inlined and nothing in it is visible. Every
row above is within the base's own run-to-run spread. CLI output is
byte-identical to the base on both projects.

## 3. Held diffs

### 3.1 Why these are held

Each diff below is a faithful port, and each loses cases. In every loss the
diff makes a correct report on an input that TSR builds wrongly, in a file
this lane does not own. Muting the report for those shapes would be the
§3a heuristic, so the diffs wait on the producers.

### 3.2 TS2702 / TS2713 — `r5-smallcodes3-type-as-namespace.diff`

`onFailedToResolveSymbol` asks `checkAndReportErrorForUsingTypeAsNamespace`
(`checker.go:1608`) when a qualified name's left side fails its namespace
lookup. That function runs `resolveSymbol(resolveName(left, Type &^
Namespace))`. If the result's declared type has a property named by the
right side, the qualified name gets TS2713. Otherwise the left side gets
TS2702.

`check_qualified_type_name_at` (`check.rs`) had declined that arm ("resolves
under another meaning → return", §302). The diff adds three things:

- the arm itself, `check_using_type_as_namespace`;
- `getPropertyOfType`'s `getReducedApparentType`, so `T.abc` with
  `T extends { abc: number }` is TS2713;
- `getSymbol`'s alias test. An alias found by the namespace lookup is
  admitted only when `getSymbolFlags(alias)` has the namespace meaning, so a
  default import of a class used as `db.db` reaches the arm. That is
  `decoratorMetadataWithImportDeclarationNameCollision7`.

`errorForUsingPropertyOfTypeAsType03` gains all four of its TS2702/TS2713
rows and stays WRONG on an unrelated TS2749. `…01` gets nothing. Its last
line, `import lol = Test5.Foo.`, is a parse error, and
`check_qualified_type_name` returns for any file with parse errors. Upstream's
report is an ordinary `c.error` and is not gated that way. That gate is older
than this lane and is left as it is.

**The loss: `callbackTagNamespace`** (EMPTY_RIGHT → EMPTY_WRONG, a TS2702 at
`@type {NS.Nested.Inner}`).

- **Native.** `parseCallbackTag` reads the name with
  `parseJSDocTypeNameWithNamespace` (`parser/jsdoc.go:992`). The reparser
  wraps the type alias in namespace declarations (`wrapInJSDocNamespace`,
  `reparser.go:100`), so `NS` is a namespace and the reference resolves.
- **TSR.** The parser (`parse_callback_tag`, `tsr-parser/src/jsdoc.rs`)
  reads only the first identifier, and the binder declares a type alias
  named `NS`.

The report is right for TSR's tree. The tree is wrong.

- **Owner:** the parser (main's) and the JSDoc binder arm (r5-jsdoc5).
- **Falsifier:** once dotted `@callback`/`@typedef` names bind as
  namespaces, the diff measures +3 with no loss. If `callbackTagNamespace`
  still reports then, the diff is wrong.

Unit test: `tests/type_used_as_namespace.rs`. Probe: every TS2702/TS2713 row
of `invalidUseOfTypeAsNamespace` and of a reduced
`errorForUsingPropertyOfTypeAsType01`/`02` matches native.

### 3.3 TS2721–TS2723 — `r5-smallcodes3-cannot-invoke-nullish.diff`

`check_non_null_callee` (`calls.rs`) is the port of
`checkNonNullTypeWithReporter` with
`reportCannotInvokePossiblyNullOrUndefinedError` (`checker.go:9894`) as the
reporter. It already computed the facts and the non-nullable remainder, but
it did not report. Its own comment said why: the report trusts the callee's
narrowed type. The diff reports, choosing the message from the
`IsUndefined`/`IsNull` facts as upstream does.

**Losses** (all EMPTY_RIGHT → EMPTY_WRONG). Each is an extra TS2722 on a
callee that native narrows and TSR does not:

- `controlFlowSuperPropertyAccess`: `super.m && super.m()`. TSR does not
  narrow a `super` property access as a reference.
- `importMetaNarrowing(module=es2020)` and `(module=esnext)`:
  `if (import.meta.foo) import.meta.foo()`. TSR does not narrow an
  `import.meta` property access either.

Both are `isMatchingReference` arms in the flow lane (`flow.rs`, main's).

- **Falsifier:** once those two references narrow, the diff measures +8 with
  no loss.

Unit test: `tests/cannot_invoke_possibly_nullish.rs`. It covers TS2721,
TS2722 and TS2723, and an `if (f) f()` that must stay silent.

## 4. Open clusters

### 4.1 TS2391 and TS17006 wait for batches AP and AU

The brief holds both until r5-smallcodes2's `error_span` missing-node diff
(batch AP) and r5-spans' span diffs (batch AU) are on the integration branch.
Neither was on it when this lane started.

**TS2391.** For `function f() => 4;` native's `parseFunctionBlockOrSemicolon`
falls through to `parseFunctionBlock` → `parseBlock(…)` (`parser.go:1205`).
With the `{` missing, that builds a zero-width `Block` with a missing
statement list. The two checks then read it differently:

- `checkFunctionOrConstructorSymbol`'s last test asks `Body() == nil`
  (`checker.go:3681`). The body exists, so there is no TS2391.
- The `NodeIsPresent(body)` test above it (`:3627`) says the body is absent,
  so the declaration counts as an overload.

TSR's `parse_function_block_or_semicolon` (`tsr-parser/src/declaration.rs`)
returns no body at all. The faithful fix has two halves:

- the parser builds the empty block;
- every checker reader of "has a body" must tell `Body() == nil` apart from
  `NodeIsMissing(body)`. TS7010, for one, is reported on a missing body.

That reaches TS7010, which batch AP's span diff also moves
(`parserErrantEqualsGreaterThanAfterFunction1`/`2` are in both), so it
waits.

**TS17006.** `parseUnaryExpressionOrHigher` (`parser.go:4694`) reports when
a simple unary expression is followed by `**`. The report spans
`SkipTrivia(pos)` to the operand's end. TSR's `parse_unary_expression`
(`tsr-parser/src/expression.rs`) has no such report, and no TS17006 site
exists anywhere in the workspace. It is a parser diff in the same function
batch AU's parser diff touches, so it waits.

### 4.2 TS1238: `resolveDecorator` is a calls.rs feature

`decorators.rs` already ports `getDecoratorCallSignature` for contextual
typing. TS1238 itself comes out of `resolveCall` with decorator arguments:

- **No call signatures** (`constructableDecoratorOnClass01`): the
  `invocationErrorDetails` chain.
- **Arity** (`decoratorOnClass8`, `esDecorators-arguments`): "The runtime
  will invoke the decorator with N arguments…", from
  `getDecoratorArgumentCount` and `getLegacyDecoratorArgumentCount`.
- **Assignability after inference** (`decoratorCallGeneric`).

A check-site TS1238 outside `chooseOverload` is the "side pass re-deriving a
decision" the integrator rejects (box protocol §3a). Routed to main's calls
lane, unchanged from `r5-smallcodes.md` §3.4.

### 4.3 TS2538 on `3e[null]`

Native's CLI reports only syntactic diagnostics for a file that has any, so
`identifierStartAfterNumericLiteral` cannot be probed through it. The
baseline's TS2538 (`Type 'null' cannot be used as an index type`) is
`getPropertyTypeForIndexType`'s `:27206` arm, in `index_access_reports.rs`
(r5-relater7). Not probed further.

## 5. Summary

| Item | Status | Cases |
|---|---|---|
| TS2308 export-star collisions | diff, lossless | +3 |
| TS2523 `yield` in a parameter initializer | diff, lossless | +3 |
| TS2702/TS2713 type used as namespace | diff, held (`callbackTagNamespace`: JSDoc dotted names) | +3 / −1 |
| TS2721–2723 cannot invoke possibly-nullish | diff, held (`super.m`, `import.meta.foo` narrowing) | +8 / −3 |
| TS2391, TS17006 | wait for batches AP/AU | 10 |
| TS2307, TS2883, TS1238, TS2403, TS2538 | routed, owners above | 19 |
| TS2540 | 2 on r5-smallcodes2's branch, 2 `mapped.rs` | 4 |
