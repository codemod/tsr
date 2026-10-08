# r5-align — the types producer's walker and case loading

Parity box on epic `tsr-2zk`. Owns `crates/tsr-conformance/src` (the types
producer, its walker, case loading, `case_guard`),
`crates/tsr-conformance/examples/*`, and this file. No checker, binder,
parser or scanner file is touched. Frozen base: `5ef1036` (integration head
`22a5e1a` with r5-harness's unmerged branch `ed39ee1` merged in, because this
lane's items build on its `case_guard` and `slowcases`).

## 1. `tsr-2zk.1071`: which cases do not align, and why

### The population

`r5-typetriage` §1 found 81 plain `checker_types` cases with lines the dump
cannot show: 52 fail on nothing else, 29 also have WRONG/GAP lines. They were
re-derived at the base with that note's method:

```bash
B=/tmp/box/base
TSR_VERDICT_EXPR=1 cargo run -q --release -p tsr-conformance --example verdictdump > $B/typesx.tsv
cargo run -q --release -p tsr-conformance --example casequery -- checker_types --list > $B/ctlist.tsv
cut -f1 $B/typesx.tsv | sed 's/:[0-9]*:[0-9]*$//' | sort | uniq -c | awk '{print $2"\t"$1}' > $B/dumpcounts.tsv
awk -F'\t' '$2=="FAIL"{split($3,a,"/"); print $1"\t"a[2]}' $B/ctlist.tsv | sort > $B/failtotals.tsv
join -t$'\t' -a1 -e0 -o 0,1.2,2.2 $B/failtotals.tsv <(sort $B/dumpcounts.tsv) | awk -F'\t' '$2!=$3' > $B/unaligned.tsv
```

81 cases, 745 baseline lines with no aligned row. `examples/unaligned.rs` (new)
prints, per section, the first position where the walker's expression and
the baseline's stop agreeing, with the node kind and its parent:

```bash
TSR_FILTER=$(cut -f1 $B/unaligned.tsv | paste -sd,) TSR_CONTEXT=1 \
  cargo run -q --release -p tsr-conformance --example unaligned
```

Alignment is the prefix test `walker_agreement` uses (upstream's line starts
with `{our expression} : `), so it needs no split of a line whose expression
contains `" : "`.

### The causes

| Cause | Kind | Cases | Fixed here |
|---|---|---:|---:|
| JSON units rendered empty | file-set | 27 | 27 |
| section matched to the wrong unit | file-set | 2 | 2 |
| echoed code line read as an assertion | expression text (reader) | 2 | 2 |
| `@types: *` root under `/.src` | file-set (harness cwd) | 1 | 0 |
| parser or scanner divergence | parser | 49 | 0 (§3) |

No case misaligned on a walker *visit rule* (`isExpressionNode`, the
type-assertion or reparsed-node rules, JSDoc) or on rendering trivia,
comments, conflict markers or newlines in the expression text. Every text
difference traced to a node the parser built differently; §3 groups them.

## 2. The three harness causes

### 2.1 JSON units (27 cases)

`render_case` rendered a section empty when its unit was `.json`. Native's
`iterateBaseline` (`type_symbol_baseline.go:208`) walks every test file
through `program.GetSourceFile`. A JSON file the program loaded (an import
with `resolveJsonModule`, or a `require` in JS) is a source file whose single
statement wraps the object literal, and that literal and its members are
expressions, so they get lines: `>{ "a": true, "b": "hello" } : { a: boolean; b: string; }`.
A JSON unit the program did not load still renders empty, as before:
`program.source_file` answers `None`.

The filter had no recorded reason. It predates the program-based producer,
when each unit was parsed alone as TypeScript. Measured: all 27 JSON cases
align once it is gone, and 25 of them flip to PASS.

### 2.2 Section-to-unit matching (2 plain cases, plus 24 configured entries)

A section header is `removeTestPathPrefixes(unitName)` (`:241`). The producer
picked the first unit for which `same_unit` held, and `same_unit` also
accepts a suffix match at a separator. In
`compiler/esModuleInteropImportTSLibHasImport` the units are `types.d.ts`,
`utils/username.ts`, `utils/index.ts`, `hello.ts`, `index.ts`, so section
`index.ts` was rendered from `utils/index.ts`. Same in
`compiler/moduleDeclarationExportStarShadowingGlobalIsNameable`
(`model/index.ts` before `index.ts`) and in the `nodeModules*` configured
entries. `types_producer::unit_for_section` now takes the unit whose printed
name (`full_oracle::printed_path`) equals the header and keeps `same_unit` as
the fallback. `binder_suite` already prefers an exact name the same way.

### 2.3 Echoed code lines (2 cases)

The baseline interleaves the file's own lines with the `>` rows.
A source line can itself start with `>` and contain `" : "`.
`compiler/deferredConditionalTypes2` continues a type alias across lines:

```text
export type IsEqual<A, B> = (<G>() => G extends A ? 1 : 2) extends <
>IsEqual : IsEqual<A, B>

  G,
>() => G extends B ? 1 : 2          <- source line 3, echoed
```

`types_baseline::parse` counted that echo as an assertion, so every line after
it in the section was off by one, and no walker could ever produce it.
`compiler/intersectionConstraintReduction` has `> = ReturnTypeKeys extends …`.
§294 of the reader already rejects echoes without the separator. These have
one.

`types_baseline::parse_with_sources` reads each section in step with its
unit's code lines, split as `codeLinesRegexp` splits them and passed through
`removeTestPathPrefixes` as the writer does. A baseline line equal to the next
unconsumed code line is that line's echo. The writer emits every code line
once, in order. Its other lines cannot equal that next code line: an assertion
would have to repeat the source verbatim, and the blank line the writer inserts
comes only before a code line that is not blank. If a section's source is
unknown or stops matching, the reader falls back to `parse`'s reading for the
rest of the section, so it never drops a line it cannot prove is an echo.

The two gate readers use it (`types_suite::CheckerTypes::judge` and
`verdict::case_rows`, through `types_producer::expected_for_case`). The 70-odd
probes in `examples/` keep `parse`. They have no case loaded at that point,
and a probe's denominator that is off by an echo in two cases does not
change what it ranks.

Considered and rejected: dropping every `>` line that does not follow the
walker's own output. That uses the port's answer to decide what the oracle
said, which is the circularity ADR-0006 exists to avoid.

### 2.4 Not fixed: `compiler/referenceTypesPreferedToPathIfPossible`

The unit is `/.src/node_modules/@types/node/index.d.ts` with `@types: *` and
`@noImplicitReferences`. It reaches native's program only as an automatic
type directive from the type root `/.src/node_modules/@types`, which is under
the native harness's current directory `/.src`. The producer compiles in `/`
(`CURRENT_DIRECTORY`, a recorded choice: `program_and_config_for_case` §539),
so the type root it looks in is `/node_modules/@types`, and the file is never
loaded. Moving the producer to `/.src` would re-root every case. That is a
whole-corpus change to every line's program, not a fix for one case, and is
left as a follow-up (§4).

### Measured (base `5ef1036`)

| | base | after |
|---|---:|---:|
| `verdictdump` rows (aligned lines) | 552,533 | 556,291 |
| RIGHT | 544,668 | 548,210 |
| `checker_types` (plain) passed | 8,247 / 9,538 | 8,275 / 9,538 |
| unaligned plain FAIL cases | 81 | 50 |
| `coverage`: `checker_types` | 8,244 (committed snapshot) | 8,275 / 9,538 |
| `coverage`: `checker_types_configured` | 1,643 (committed snapshot) | 1,659 / 1,928 |
| `coverage`: `diagnostics` / `diagnostics_configured` | | 4,537 / 5,502, 837 / 1,089 |

The committed snapshots trail the base by three plain cases (8,244 against
`casequery`'s 8,247 at `5ef1036`); the plain delta above is `casequery`'s.

- **3,758 newly aligned lines in 69 entries** (31 plain, 38 configured):
  3,542 RIGHT, 212 WRONG, 4 GAP. The WRONG and GAP lines are now scored where
  they were invisible.
- **28 plain cases FAIL→PASS**, none the other way. The other three that
  now align (`deferredConditionalTypes2`, `intersectionConstraintReduction`,
  `importAttributes10`) are blocked by WRONG lines.
- **Zero RIGHT losses, zero missing keys.** For every key in both dumps the
  `want` text is identical (`join` on columns 1 and 3 prints nothing), so no
  key moved to a different line.
- Diagnostics: the diagnostics path does not call the producer. Both dumps
  agree on the key/verdict columns (§5).

## 3. Parser and scanner divergences (49 cases, for main)

Each case's first misaligned line is a node the parser builds differently from
native. Walking it faithfully cannot fix it. Grouped by construct, with the
first misaligned line (`want` is native, `got` is the port's node):

| # | Construct | Cases | Witness (`want` / `got`) |
|---|---|---|---|
| P1 | `<` after an expression: empty type arguments, `new <T>`, type assertion vs. less-than | `emptyTypeArgumentList`, `emptyTypeArgumentListWithNew`, `expressionWithJSDocTypeArguments`, `importCallExpressionWithTypeArgument`, `intTypeCheck`, `newExpressionWithCast`, `parserTypeAssertionInObjectCreationExpression1` | `newExpressionWithCast`: `>new <any : boolean` / `TypeAssertionExpression <any>Test2()` |
| P2 | `yield`/`await` outside or at the edge of their context | `castOfYield`, `YieldExpression8_es6`, `YieldExpression18_es6`, `YieldStarExpression1_es6`, `YieldStarExpression2_es6`, `FunctionDeclaration10_es6`, `await_unaryExpression_es2017_3`, `await_unaryExpression_es6_3`, `dynamicImportsDeclaration` | `castOfYield`: `><number> : number` then `> : any` / `<number> yield` (native: operand missing, `yield 0` a separate statement) |
| P3 | JSX parse and recovery (`.js`/`.jsx`/`.tsx`) | `conflictMarkerTrivia3`, `parseJsxElementInUnaryExpressionNoCrash1`, `…NoCrash2`, `…NoCrash3`, `parseUnaryExpressionNoTypeAssertionInJsx1`, `…Jsx2`, `…Jsx3`, `jsxCheckJsxNoTypeArgumentsAllowed` | `conflictMarkerTrivia3`: `><div> : any` / `JsxElement <div><<<<<<< HEAD` (native ends the element before the marker) |
| P4 | scanner: bigint after a leading dot; unterminated regex end; lone `#` | `bigintPropertyName`, `parseBigInt`, `parserMissingToken2`, `parserRegularExpressionDivideAmbiguity4`, `tsxAttributeInvalidNames`, `privateNameHashCharName` | `parseBigInt`: `>.1n : 0.1` / `NumericLiteral .1`; `parserMissingToken2`: `>/ b : RegExp` / `/ b;` |
| P5 | decorator on an expression | `decoratorOnArrowFunction`, `decoratorOnFunctionExpression`, `manyCompilerErrorsInTheTwoFiles` | `decoratorOnFunctionExpression`: `>dec : <T>(target: T) => T` / `ClassExpression @dec` |
| P6 | reserved word or keyword where a name is expected | `ambientModuleDeclarationWithReservedIdentifierInDottedPath`, `…DottedPath2`, `reservedWords2`, `es6ImportNamedImportParsingError`, `importCallExpressionIncorrect2`, `typeofImportDefer`, `derivedClassSuperCallsInNonConstructorMembers`, `parseErrorIncorrectReturnToken` | `ambientModule…DottedPath`: `>debugger : typeof debugger` / an empty `Identifier`; `derivedClass…`: `a: super();` parses as a method `super` |
| P7 | arrow function with missing tokens | `arrowFunctionsMissingTokens`, `ArrowFunction3`, `parserX_ArrowFunction3` | `ArrowFunction3`: `>(a) : any` / `ArrowFunction (a): => { }` |
| P8 | other recovery | `nullishCoalescingOperator5`, `asOperatorASI`, `mappedTypeProperties`, `taggedTemplateChain`, `thisTypeInFunctionsNegative` | `nullishCoalescingOperator5`: `>a ?? b` / `a` under `b \|\| c`; `asOperatorASI`: `>10 : 10` / `10as \`Hello world\`` |

`bd` cannot run in this container (`box-protocol.md` §1). These are written
for the integrator to file as parser issues on main, one per row. Each row's
witness is the smallest case in it.

## 4. Follow-ups

- **Producer current directory.** `/` vs native `/.src` (§2.4). It costs
  `referenceTypesPreferedToPathIfPossible` here. Any case whose type roots or
  `node_modules` lookups depend on the current directory is exposed the same
  way. Measure before changing it: every line's program moves.
- **Probes on `parse`.** The probes in `examples/` read baselines with
  `types_baseline::parse`. A probe that reports alignment in
  `deferredConditionalTypes2` or `intersectionConstraintReduction` will be
  off by one there.
