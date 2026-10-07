# Exact full-corpus oracle (lane: oracle)

Beads: tsr-2zk.47.3, tsr-2zk.47.1, tsr-2zk.47.2, tsr-2zk.47.3.1.

## Command

```sh
scripts/parity_gate.sh oracle /tmp/oracle/base          # committed tree only
scripts/parity_gate.sh oracle-compare /tmp/oracle/base /tmp/oracle/cand
```

`oracle` = `full_oracle_run run DIR [--workers N] [--deadline S] [--filter SUBSTRING]`.
The report directory must be empty: nothing resumes. Outputs: `identity.tsv`,
`plan.tsv`, `results.tsv`, `summary.md`, `cases/NNNNN/{request,native,actual}.tsv`
(the TSR copy is deleted when byte-identical to native).

`oracle-compare` (`full_oracle_run gate`) exits 1 when a key that is `EXACT` in
the base is not `EXACT` in the candidate or is absent from it (tsr-2zk.47.2). It
refuses runs whose native revision, corpus revision, native binary SHA-256,
plan SHA-256, deadline or filter differ, and either run that is not `complete`.

## Identity (`identity.tsv`)

Source commit (a dirty `crates/`, `xtask/`, `Cargo.*` aborts unless
`--allow-dirty`, which is recorded), pinned native revision and clean checkout,
corpus submodule revision and clean checkout, SHA-256 of both frozen worker
binaries (copied read-only into `bin/` before use) and of the three producer
sources, workers, deadline, filter, worker environment, plan SHA-256, results
SHA-256, `status` (`running` → `complete`; a failed discovery writes
`discovery-<outcome>` and publishes no population).

## Population (expected-independent)

`full_oracle_native.go` (a `go test -overlay` of the pinned
`internal/testrunner`, checkout untouched) emits the plan in `runCompilerTests`
order: regression then conformance runner, `EnumerateTestFiles` order,
`GetFileBasedTestConfigurations` order. `skippedTests` rows are `LISTED_SKIP`;
a configuration expansion that fails (`t.Fatal`/panic) is one
`DISCOVERY_FAILED` row. Discovery is bounded (900 s); timeout or failure aborts
the run. No committed baseline is read by either producer.

## Per-case outcome

Two bounded processes (default deadline 60 s, kill + reap):

- native: `getCompilerFileBasedTest` → `newCompilerTest` →
  `SkipUnsupportedCompilerOptions` (skipped → `NATIVE_SKIPPED`) → records of
  `c.result.Diagnostics` and `DoTypeAndSymbolBaseline`'s walk;
  `TS_TEST_PROGRAM_SINGLE_THREADED=true` (one checker, the test default).
- TSR: `full_oracle::actual` with the native configuration map verbatim. Case
  program = `types_producer::program_and_config_for_case` at `/.src`; one
  configured checker; `compileFilesWithHost` collection (config, parse, JS
  syntax, `bind_and_check_diagnostics` with the program's bind diagnostics,
  include processor, isolated-declaration diagnostics) then
  `sort_and_deduplicate_located_diagnostics`; type rows via
  `types_producer::render_file` through the same checker,
  `hadErrorBaseline = !diagnostics.is_empty()`, over `toBeCompiled ++
  otherFiles` filtered to loaded files (JSON included), skipped by
  `@noTypesAndSymbols`.

Outcomes: `EXACT`, `WRONG`, `NATIVE_FAILED`, `NATIVE_TIMEOUT`, `TSR_FAILED`,
`TSR_TIMEOUT`, `DISCOVERY_FAILED` are all in the denominator; only
`NATIVE_SKIPPED` and `LISTED_SKIP` (no native baseline exists) are excluded.

## Exactness

Exact = equal complete record streams: every diagnostic in published order
(file, UTF-16 start/length, code, category, flattened message), its metadata,
message chain and related information recursively; every `.types` section and
row (file, placement line, node text, type text). Rows plus the unit text
determine the `.types` file, so equal rows are equal baselines. Comparisons are
raw except the printed file name, which both sides pass through
`removeTestPathPrefixes`; TSR's `/.ts-lib/` lib mount is spelled as the native
`bundled:///libs/` first. A file-less diagnostic's position is published as `-`
on both sides (no baseline or CLI prints it; native uses -1 or 0). Symbols
baselines, JS/declaration emit and source maps are outside this oracle.

## Failure classes

First difference, diagnostic half before type half. Diagnostic half: equal
groups in another order → `order-only`; else by `(file, start, len, code)`
multisets: only native → `missing`, only TSR → `extra`, both with the same
`(file, code)` → `span-only`, same spans → `code-at-same-span`, else
`missing+extra`; same locations: chain records → `chain`, related records →
`related-info`, category → `category`, head text → `message-text-only`, else
`metadata`. Type half: section list → `section-set`/`no-sections`; first
differing row → `type-text` (same node and line), `extra-row`/`missing-row`
(one-row shift), `line-placement`, `node-selection`. Details carry the code or
`native -> tsr` type text.

## Measurement at `52c2759a` (2026-10-07, 12 workers, 60 s deadline)

Identity: native `5b1047d1`, corpus `4d4f005c`, native binary
`9703f9c2…5c279` (rebuilt byte-identically), TSR worker `bad836c0…ac707`,
plan `83e5f41b…c8579`, results `f1bed3ba…285a4`. Wall ≈ 25 min.

**Exact 7,698 / 12,797 = 60.15%.** Population 14,960 rows: 14,915 configured
cases + 45 `skippedTests`; 2,118 configurations are `SkipUnsupportedCompilerOptions`
skips (ES5 target, AMD/UMD/System, node10/classic resolution, `baseUrl`,
`outFile`, `esModuleInterop`/`allowSyntheticDefaultImports`/`alwaysStrict` false).
Both excluded sets have no native baseline. 0 discovery failures (the 8
`module: none` sources are in `skippedTests`), 0 native failures.

| outcome | cases |
|---|---|
| EXACT | 7698 |
| WRONG | 5096 |
| TSR_TIMEOUT | 2 |
| TSR_FAILED | 1 |
| NATIVE_SKIPPED (excluded) | 2118 |
| LISTED_SKIP (excluded) | 45 |

WRONG halves: diagnostics only 2,871, types only 1,135, both 1,090.

Primary class (diagnostic half first; details = first differing code / `native -> tsr` type):

| class | cases | top details | examples |
|---|---|---|---|
| diag:missing | 1683 | TS2322 177, TS2339 86, TS2345 68, TS2741 43 | `compiler/TransportStream.ts`, `compiler/abstractClassUnionInstantiation.ts`, `compiler/abstractPropertyInConstructor.ts` |
| types:type-text | 1133 | any -> error 121, number -> any 42, number -> error 29, string -> error 21 | `compiler/abstractClassInLocalScopeIsAbstract.ts`, `compiler/acceptSymbolAsWeakType.ts`, `compiler/accessorDeclarationEmitJs.ts` |
| diag:chain | 516 | TS2322 283, TS2345 38, TS2430 32, TS2416 25 | `compiler/accessorAccidentalCallDiagnostic.ts (target=es2015)`, `compiler/addMoreOverloadsToBaseSignature.ts`, `compiler/aliasInstantiationExpressionGenericIntersectionNoCrash1.ts` |
| diag:related-info | 437 | TS2403 45, TS2741 41, TS2554 36, TS2322 35 | `compiler/anonymousClassExpression2.ts`, `compiler/anyIdenticalToItself.ts`, `compiler/arityErrorRelatedSpanBindingPattern.ts` |
| diag:span-only | 430 | TS2322 136, TS1036 18, TS1029 17, TS1042 17 | `compiler/ClassDeclaration10.ts`, `compiler/ClassDeclaration11.ts`, `compiler/ClassDeclaration14.ts` |
| diag:missing+extra | 358 | TS2322 38, TS2353 17, TS2345 15, TS2304 10 | `compiler/abstractPropertyNegative.ts (target=es2015)`, `compiler/allowImportClausesToMergeWithTypes.ts`, `compiler/ambientWithStatements.ts (alwaysstrict=true)` |
| diag:extra | 308 | TS2304 68, TS2322 51, TS1254 44, TS18048 10 | `compiler/abstractPropertyBasics.ts (target=es2015)`, `compiler/ambientModuleWithTemplateLiterals.ts`, `compiler/arrowFunctionsMissingTokens.ts` |
| diag:message-text-only | 182 | TS2339 45, TS2314 20, TS2411 18, TS2345 10 | `compiler/aliasBug.ts`, `compiler/allowSyntheticDefaultImports10.ts`, `compiler/arrayAssignmentTest2.ts` |
| diag:code-at-same-span | 47 | TS2322 4, TS2741 4, TS1109 3, TS2693 3 | `compiler/awaitCallExpressionInSyncFunction.ts`, `compiler/classImplementsClass4.ts`, `compiler/declarationEmitInvalidReferenceAllowJs.ts` |
| tsr:timeout | 2 | | `compiler/recursiveConditionalCrash3.ts`, `compiler/relationComplexityError.ts` |
| types:section-set | 2 | | `compiler/augmentExportEquals2.ts`, `compiler/referenceTypesPreferedToPathIfPossible.ts` |
| tsr:error | 1 | reversed span 6665..6663 | `compiler/constructorWithIncompleteTypeAnnotation.ts` |

Type half over every WRONG case with a type difference (2,225): type-text
2,190, node-selection 22 (parser recovery: `castOfYield`, `parseBigInt`),
missing-row 7, extra-row 4, section-set 2. First type-text difference by
answer: both print a type 1,279; TSR `any` where native has a type 458; TSR
`error` 401; native `any`/`error`, TSR a type 44; same union members in another
order 9.

Codes in differences (cases per kind; same location = other text/chain/related):

| code | missing | extra | same location |
|---|---|---|---|
| TS2322 | 425 | 285 | 471 |
| TS2339 | 144 | 17 | 84 |
| TS2345 | 120 | 34 | 91 |
| TS2304 | 58 | 125 | 1 |
| TS2741 | 73 | 9 | 68 |
| TS2552 | 6 | 91 | 46 |
| TS1005 | 34 | 30 | 41 |
| TS2403 | 15 | 0 | 66 |
| TS2554 | 29 | 7 | 45 |
| TS2769 | 40 | 3 | 27 |
| TS7006 | 56 | 10 | 4 |
| TS2353 | 55 | 2 | 8 |
| TS1036/1029/1042/1183 | 25/25/19/19 | 25/23/18/18 | 0 |
| TS1254 | 0 | 48 | 0 |
| TS5055 | 35 | 0 | 0 |

Every sampled class (TS2322 elaboration, grammar spans, TS1254, TS2552, parser
recovery, `noLib` global TS2318/TS5053) is a TSR output difference, not an
oracle artifact. Systemic next lanes, by cases: (1) assignability reporting —
TS2322/TS2345/TS2741 missing heads and missing elaboration chains (`chain` 516,
TS2322 283); (2) type answers — 859 first rows where TSR prints `any`/`error`;
(3) related information (437; TS2403/TS2741/TS2554 "declared here"); (4)
grammar-check spans (TS1036/1029/1042/1183 equal-count span shifts) and TS2322
error-node spans; (5) program/option diagnostics TSR has no producer for
(TS5053, TS5055, global TS2318).

TSR worker medians (process wall, 12 concurrent): native 0.20 s, TSR 1.24 s
per case. Not a performance certificate (unequal work: TSR re-parses and binds
the bundled libraries per process).
