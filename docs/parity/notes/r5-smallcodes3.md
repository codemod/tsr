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
| TS17006 | `exponentiationOperatorInTemplateStringWithSyntaxError1`/`2`/`3`, `exponentiationOperatorSyntaxError1` (es2015) (4) | missing | `parseUnaryExpressionOrHigher` (`parser.go:4694`) | parser (main's): §4.1, lossless diff |
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
| TS2721–2723 (§3.3), without §3.4 | `interfaceClassMerging`, `nullableFunctionError`, `controlFlowOptionalChain`, `logicalAssignment5` ×4, `parserAmbiguityWithBinaryOperator4` | 17 | 3 (0 with §3.4) |

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

### 2.4 Re-measured on the integration head `26e3eab` (batch AS)

The branch merged the integration head after batches AO, AP, AQ and AS
landed. A new base was frozen there: diagnostics 11,080 right (5,484 RIGHT +
5,596 EMPTY_RIGHT), types 549,126 RIGHT. Every diff here still applies
cleanly to it.

The five code diffs stacked (TS2308, TS2523, the flow arms, TS2721–2723,
TS2702/TS2713), unfiltered:

- diagnostics: 11,080 → 11,096 right. The same 17 cases converted, and the
  rows matched per code are identical to §2.3.
- the only loss is `callbackTagNamespace` (TS2702, §3.2). Without the TS2702
  diff the stack is lossless.
- type lines: +10, the flow arms' (§3.4), zero losses.
- slowcases: only the KNOWN_SLOW cases, on both dumps.

## 3. Held diffs

### 3.1 Why these were held

Each reporter diff below is a faithful port that loses cases on its own. In
every loss the diff makes a correct report on an input that TSR builds
wrongly, in a file this lane does not own. Muting the report for those
shapes would be the §3a heuristic. TS2722's producer turned out to be two
unported `isMatchingReference` arms, which §3.4 ports, so that pair is now
lossless. TS2702's producer is still open.

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

### 3.3 TS2721–TS2723 — `r5-smallcodes3-cannot-invoke-nullish.diff`, on §3.4

`check_non_null_callee` (`calls.rs`) is the port of
`checkNonNullTypeWithReporter` with
`reportCannotInvokePossiblyNullOrUndefinedError` (`checker.go:9894`) as the
reporter. It already computed the facts and the non-nullable remainder, but
it did not report. Its own comment said why: the report trusts the callee's
narrowed type. The diff reports, choosing the message from the
`IsUndefined`/`IsNull` facts as upstream does.

**Alone it loses three cases** (all EMPTY_RIGHT → EMPTY_WRONG). Each is an
extra TS2722 on a callee that native narrows and TSR did not:

- `controlFlowSuperPropertyAccess`: `super.m && super.m()`;
- `importMetaNarrowing(module=es2020)` and `(module=esnext)`:
  `if (import.meta.foo) import.meta.foo()`.

Both come from `isMatchingReference` arms that were never ported. §3.4
ports them.

**Stacked on §3.4** (measured together against the frozen base):

- diagnostics: +8 cases (`interfaceClassMerging`, `nullableFunctionError`,
  `controlFlowOptionalChain`, `logicalAssignment5` ×4,
  `parserAmbiguityWithBinaryOperator4`). 17 rows matched (15 TS2722, one
  TS2721, one TS2723), no row of any other code changed, **zero losses**;
- type lines: §3.4's +10, zero losses;
- slowcases: only the KNOWN_SLOW cases.

The integrator lands §3.4 first.

Unit test: `tests/cannot_invoke_possibly_nullish.rs`. It covers TS2721,
TS2722 and TS2723, an `if (f) f()` that must stay silent, and
`super.m && super.m()`. The last assertion fails without §3.4.

### 3.4 `isMatchingReference`'s `super` and `MetaProperty` arms — `r5-smallcodes3-flow-super-meta-references.diff`

`isMatchingReference` (`flow.go:1597`) matches `super` against `super`
(`:1617`), and a `MetaProperty` against one with the same keyword and name
(`:1607`). `references_match` (`flow.rs`, main's) listed both as not ported
and answered `false`, so `super.m` and `import.meta.foo` never narrowed. The
diff adds the two arms beside the existing `this` arm.

Measured alone against the frozen base:

- diagnostics: unchanged;
- type lines: +10 RIGHT, zero losses:
  - `controlFlowSuperPropertyAccess` 0:5, 0:9, 0:10, 0:12;
  - `importMetaNarrowing` (es2020, esnext) 0:6, 0:7, 0:10;
- slowcases: only the KNOWN_SLOW cases;
- Ir: domain-model 1,117,759,558 → 1,117,032,341 (−0.065%, within the
  base's spread); generic-imports 342,931,033 → 342,932,389 (+0.0004%). CLI
  output is identical.

Native and TSR are both silent on the probe
(`super.m && super.m()`, `if (import.meta.foo) import.meta.foo()`, strict).

## 4. Open clusters

### 4.1 TS2391 and TS17006, after batches AP and AU

The brief held both until r5-smallcodes2's `error_span` missing-node diff
(batch AP) and r5-spans' span diffs (batch AU) were on the integration
branch. Neither was there when this lane started. Both landed during it, and
the numbers below are measured on a head that has them.

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

**The parser half, measured** (`r5-smallcodes3-parser-missing-brace-body.diff`,
held). `parse_block_with` (`statement.rs`) already ports `parseBlock`'s
missing-brace arm. Only `parse_function_block_or_semicolon`'s error path
bypassed it and returned no body. The diff routes that path through it.

Against the frozen base, unfiltered:

- diagnostics: +2 cases (`dottedModuleName`,
  `objectTypesWithOptionalProperties2`) and −2 (`commonMissingSemicolons`,
  `overloadConsecutiveness`);
- types: −31 lines.

Every loss is a checker reader that still treats "has a `Block`" as "has a
body", where upstream asks `NodeIsMissing(body)`:

| Reader (upstream) | Effect of the missing `Block` in TSR | Rows |
|---|---|---|
| `getReturnTypeOfSignature`: `NodeIsMissing(body) ? anyType : getReturnTypeFromBody` | `function f(x: number)=>2*x` prints `=> void`, native `=> any` | −31 type lines (`dottedModuleName`, `overloadConsecutiveness`, `reservedWords2`/`3`, …) |
| `checkFunctionOrMethodDeclaration`'s TS7010: `NodeIsMissing(body)` | TS7010 no longer reported | 10 TS7010 rows lost (`reservedWords3` ×5, `destructuringParameterDeclaration6` ×2, `parserErrantEqualsGreaterThanAfterFunction1`/`2`, `parserSkippedTokens16`) |
| `checkFunctionOrConstructorSymbol`: `bodyIsPresent := NodeIsPresent(body)` (`checker.go:3627`) | the missing body counts as an implementation | TS2393 ×6 and TS2389 ×2 extra, TS2391 ×4 lost (`overloadConsecutiveness`, `commonMissingSemicolons`) |

**Re-measured on `77afe69`** (batches AP and AU in):

- diagnostics: +2 (the same two cases) and −3. `parserEqualsGreaterThanAfterFunction1`
  joins the losses, because batch AP made its TS7010 position right and the
  missing `Block` now suppresses that TS7010.
- TS7010 rows lost: 13;
- types: −31 lines;
- causes: the same three readers.

The diff lands together with those three readers. They are spread over
`check.rs`, `signatures.rs` and the declared-type road, which are main's and
r5-printer3's. The parser half also removes 15 extra TS2391 rows, which is
what the cluster needs. TS7010's positions were moved by batch AP, so this
is re-measured on a head that has it.

**TS17006 / TS17007 — `r5-smallcodes3-exponentiation-unary-operand.diff`**
(lossless, measured after batch AU landed).

`parseUnaryExpressionOrHigher` (`parser.go:4660`) is the binary operand
parser. If the operand is not an update expression (`isUpdateExpression`:
not a prefix `+ - ~ ! delete typeof void await`, and not a `<` in a non-JSX
file), it is parsed by `parseSimpleUnaryExpression`. When `**` follows, the
parser reports:

- TS17007 for a type assertion;
- TS17006 otherwise, naming the operator.

The span runs from `SkipTrivia(pos)` to the operand's end (`:4694`). The tree
does not change: the binary loop takes the `**` next.

TSR's binary loop called `parse_unary_expression` directly, and no TS17006
site existed in the workspace. The diff adds
`parse_unary_expression_or_higher` (`tsr-parser/src/expression.rs`) as the
loop's operand entry. It has the same `isUpdateExpression` test, the report,
and upstream's same-position guard. Only that entry reports. A prefix
operator's own operand goes through the unchanged `parse_unary_expression`,
as upstream's goes through `parseSimpleUnaryExpression`, so `- -x ** 2`
reports once, at the outer `-`.

Probe, positions and texts identical in native and TSR:

| Source | Report |
|---|---|
| `-x ** 2` | TS17006 `'-'` |
| `typeof x ** 2` | TS17006 `'typeof'` |
| `- -x ** 2` | TS17006 `'-'` (once) |
| `++x ** 2` | none |
| `<number>x ** 2` | TS17007 |
| `delete x ** 2` | TS17006 `'delete'` |
| `1 + -x ** 2` | TS17006 at the `-` |

Measured alone, unfiltered, against a base frozen at `77afe69` (batch AU;
diagnostics 11,089 right, types 549,360 RIGHT):

- diagnostics: +5 cases (`exponentiationOperatorInTemplateStringWithSyntaxError1`/`2`/`3`
  and `exponentiationOperatorSyntaxError1`/`2`, all es2015). 169 TS17006 and
  5 TS17007 rows newly match. No extra row, no other code changed, zero
  losses;
- types: unchanged;
- slowcases: only the KNOWN_SLOW cases.

Ir: the new entry runs once per binary operand, so it is on the parser's hot
path. Two runs of each binary:

| Project | base | diff |
|---|---|---|
| domain-model | 1,113,345,654 / 1,112,640,369 | 1,113,293,362 / 1,113,283,779 |
| generic-imports | 343,104,889 / 343,079,736 | 343,102,964 / 343,080,846 |

The means differ by +0.03% (domain-model) and 0.000% (generic-imports). The
base's own two runs differ by 0.06%. CLI output is identical.

`tsr-parser` tests pass, including the new
`tests/exponentiation_unary_left_operand.rs` (six fixtures from the probe).
Clippy is clean on the parser.

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
| TS2721–2723 cannot invoke possibly-nullish | diff, lossless on the flow diff below | +8 |
| `isMatchingReference` `super`/`MetaProperty` arms | diff, lossless (`flow.rs`) | +10 type lines |
| TS2391 parser half (`parser-missing-brace-body.diff`) | held: needs the three `NodeIsMissing(body)` readers (§4.1) | +2 / −3, −31 type lines alone (on `77afe69`) |
| TS17006/TS17007 unary left of `**` | diff, lossless (parser) | +5 |
| TS2307, TS2883, TS1238, TS2403, TS2538 | routed, owners above | 19 |
| TS2540 | 2 on r5-smallcodes2's branch, 2 `mapped.rs` | 4 |
