# Exact full-corpus oracle (lane: oracle)

Beads: tsr-2zk.47.3, tsr-2zk.47.1, tsr-2zk.47.2, tsr-2zk.47.3.1.

## Commands

```sh
scripts/parity_gate.sh oracle-native /tmp/oracle/native                 # once per native identity
scripts/parity_gate.sh oracle-tsr /tmp/oracle/native /tmp/oracle/base   # committed base tree
scripts/parity_gate.sh oracle-tsr /tmp/oracle/native /tmp/oracle/cand   # committed candidate
scripts/parity_gate.sh oracle-compare /tmp/oracle/base /tmp/oracle/cand
```

`oracle-native` = `full_oracle_run native DIR [--workers N] [--deadline S] [--filter SUBSTRING]`;
`oracle-tsr` = `full_oracle_run tsr NATIVE_DIR DIR [--workers N] [--deadline S]
[--filter SUBSTRING] [--per-process] [--allow-dirty]`. Output directories must be
empty: nothing resumes. Native: `identity.tsv`, `plan.tsv`, `native.tsv`,
`cases/NNNNN/{request,native}.tsv` (console output kept only for failures).
TSR: `identity.tsv`, `results.tsv`, `summary.md`, `workers/N.stderr`, and
`cases/NNNNN/{native,actual}.tsv` only for non-exact cases (`actual.stderr`
for failures).

`oracle-compare` (`full_oracle_run gate`) prints one `LOST`, `MISSING` or
`GAINED` line per case and exits 1 when a key `EXACT` in the base is not `EXACT`
in the candidate or is absent from it (tsr-2zk.47.2). It refuses reports that
are not complete TSR reports or whose native identity (revisions, native
producer sources, native binary, plan, native results, native deadline and
filter) or TSR deadline and filter differ.

## Identity (`identity.tsv`)

Native run: pinned native revision and clean checkout, corpus submodule
revision and clean checkout, SHA-256 of the two Go producer sources and of the
frozen native test binary (copied read-only into `bin/`), deadline, filter,
worker environment, plan SHA-256, `native.tsv` SHA-256 (each row carries its
artifact's SHA-256), `status` (`running` → `complete`; a failed discovery
writes `discovery-<outcome>` and publishes no population).

TSR run: refuses a native directory that is not `complete`, whose native or
corpus revision differs from the checkout's, whose Go producer sources changed
since, or whose binary, plan or results no longer hash to the recorded values;
every native artifact is re-hashed before it is compared. Records the source
commit (a dirty `crates/`, `xtask/`, `Cargo.*` aborts unless `--allow-dirty`,
which is recorded), the native directory and the SHA-256 of its identity, the
inherited native keys, the frozen TSR worker binary's SHA-256, `tsr_mode`,
deadline, filter, results SHA-256 and `status`.

## Population (expected-independent)

`full_oracle_native.go` (a `go test -overlay` of the pinned
`internal/testrunner`, checkout untouched) emits the plan in `runCompilerTests`
order: regression then conformance runner, `EnumerateTestFiles` order, a
source's configurations sorted by name (`GetFileBasedTestConfigurations`
builds them from a Go map range, so its own order is random per process; the
set is not). `skippedTests` rows are `LISTED_SKIP`; a configuration expansion
that fails (`t.Fatal`/panic) is one `DISCOVERY_FAILED` row. Discovery is
bounded (900 s); timeout or failure aborts the run. No committed baseline is
read by either producer.

## Per-case outcome

Native, once per native identity (bounded process, default deadline 60 s, kill
and reap): `getCompilerFileBasedTest` → `newCompilerTest` →
`SkipUnsupportedCompilerOptions` (skipped → `NATIVE_SKIPPED`) → records of
`c.result.Diagnostics` and `DoTypeAndSymbolBaseline`'s walk;
`TS_TEST_PROGRAM_SINGLE_THREADED=true` (one checker, the test default).

TSR, per candidate: `full_oracle::actual` with the native configuration map
verbatim. Case program = `types_producer::program_and_config_for_case` at
`/.src`; one configured checker; `compileFilesWithHost` collection (config,
parse, JS syntax, `bind_and_check_diagnostics` with the program's bind
diagnostics, include processor, isolated-declaration diagnostics) then
`sort_and_deduplicate_located_diagnostics`; type rows via
`types_producer::render_file` through the same checker, `hadErrorBaseline =
!diagnostics.is_empty()`, over the harness `toBeCompiled ++ otherFiles` filtered
to loaded files (JSON included), skipped by `@noTypesAndSymbols`.

Default `serve` mode: one long-lived `full_oracle_actual --serve` per worker
slot, one case per request line, each case on a fresh thread (no thread-local
state crosses cases; the only process-wide input is the immutable bundled-lib
text). A per-case deadline kills the worker (`TSR_TIMEOUT`); a panic or abort
ends the worker, the case is `TSR_FAILED` with that case's stderr, and the slot
starts a new worker, so no case runs after another case's unwinding.
`--per-process` runs one process per case; the two modes must publish
byte-identical artifacts (checked over the full corpus, below).

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

## Measurement at `c4bd3a6d` (origin/main `23711d1d` + oracle; 2026-10-08, 14 workers, 60 s deadlines)

Native run: native `5b1047d1`, corpus `4d4f005c`, Go producer
`decc2e4b…` / `59076d08…`, native binary `807a9a1e…`, plan `04db1d44…`
(reproduced byte-identically by a second discovery), native results
`83802b0f…`; 368 s wall, once. TSR report: worker `9ca79c73…`, results
`af5af5cf…`.

**Exact 7,884 / 12,797 = 61.61%** (7,698 at `52c2759a`; the gate against the
report of origin/main `d779b044` lists 147 gains, 0 losses). Population 14,960
rows: 14,915 configured cases + 45 `skippedTests`; 2,118 configurations are
`SkipUnsupportedCompilerOptions` skips (ES5 target, AMD/UMD/System,
node10/classic resolution, `baseUrl`, `outFile`, `esModuleInterop`/
`allowSyntheticDefaultImports`/`alwaysStrict` false). Both excluded sets have
no native baseline. 0 discovery failures, 0 native failures or timeouts.

| outcome | cases |
|---|---|
| EXACT | 7884 |
| WRONG | 4910 |
| TSR_TIMEOUT | 2 (`recursiveConditionalCrash3`, `relationComplexityError`) |
| TSR_FAILED | 1 (`constructorWithIncompleteTypeAnnotation`: reversed span 6665..6663) |
| NATIVE_SKIPPED (excluded) | 2118 |
| LISTED_SKIP (excluded) | 45 |

WRONG halves: diagnostics only 2,738, types only 1,111, both 1,061.

| primary class | cases | top details |
|---|---|---|
| diag:missing | 1684 | TS2322 177, TS2339 87, TS2345 68, TS2741 43 |
| types:type-text | 1109 | any -> error 117, number -> any 42, number -> error 29 |
| diag:chain | 520 | TS2322 286, TS2345 38, TS2430 32, TS2416 25 |
| diag:related-info | 438 | TS2403 45, TS2741 41, TS2554 36, TS2322 35 |
| diag:missing+extra | 339 | TS2322 37, TS2353 17, TS2345 15, TS2304 10 |
| diag:extra | 307 | TS2304 68, TS2322 51, TS1254 44, TS18048 10 |
| diag:span-only | 288 | TS2322 136, TS1163 12, TS1206 12, TS2390 10 |
| diag:message-text-only | 180 | TS2339 46, TS2314 20, TS2411 18, TS2322 10 |
| diag:code-at-same-span | 43 | TS2322 4, TS2741 4, TS1109 3, TS2693 3 |
| types:section-set | 2 | `augmentExportEquals2`, `referenceTypesPreferedToPathIfPossible` |

Type half (2,172 WRONG cases with a type difference): type-text 2,139,
node-selection 22 (parser recovery), missing-row 5, extra-row 4, section-set 2.
First type-text difference by answer: both print a type 1,239; TSR `any` where
native has a type 455; TSR `error` 396; native `any`/`error`, TSR a type 39;
same union members in another order 11.

| code | missing | extra | same location |
|---|---|---|---|
| TS2322 | 424 | 283 | 470 |
| TS2345 | 120 | 33 | 91 |
| TS2339 | 141 | 15 | 84 |
| TS2304 | 57 | 122 | 1 |
| TS2741 | 73 | 9 | 68 |
| TS2552 | 6 | 89 | 46 |
| TS1005 | 33 | 24 | 41 |
| TS2403 | 15 | 0 | 66 |
| TS2554 | 29 | 7 | 45 |
| TS2769 | 40 | 3 | 27 |
| TS7006 | 56 | 9 | 4 |
| TS2353 | 55 | 2 | 8 |
| TS1254 | 0 | 48 | 0 |

Systemic next lanes, by cases: (1) assignability reporting — missing
TS2322/TS2345/TS2741 heads and elaboration chains; (2) type answers — 851
first rows where TSR prints `any`/`error`; (3) related information (438); (4)
TS2322 error-node spans (136 span-only); (5) TS2304/TS2552 extras (211) and
program/option diagnostics TSR has no producer for (TS5053, TS5055, global
TS2318).

## Gate cost and mode identity

`serve` and `--per-process` reports at the previous main (`d779b044` + oracle)
are byte-identical on all 14,960 rows (outcome, classes, details, native and
TSR artifact SHA-256); wall 954 s vs 1,529 s on 14 workers. Native artifacts
are reused; the per-candidate cost is the TSR run alone: median 1.06 s per
case, p99 1.55 s, about 13,200 worker-seconds; 910 s wall at `c4bd3a6d`.

That cost is TSR checking the bundled libraries. The native harness never
does: `CompileFiles` defaults `SkipDefaultLibCheck` to true
(`harnessutil.go:99`), and `program_and_config_for_case` does not. With that
default ported (plus `createProgram`'s `SingleThreaded`) the run takes 125 s
wall at `c4bd3a6d` (0.1 s per case; serve/per-process byte-identical at the
previous main), so a 16-core gate fits in 10 minutes. The port is **held**: it
gains `genericPrototypeProperty2` and `plainJSReservedStrict` but loses
`compiler/sliceResultCast.ts`, whose EXACT depends on checking `lib.es5.d.ts`
first. For `x: [number, string] | [number, string, string]`, native prints
`x.slice` as a union of two distinct instantiated signatures (also with
`@skipDefaultLibCheck: false`); TSR prints that union only when
`lib.es5.d.ts` was checked before the case, and one signature otherwise. The
union projection (`get_type_of_property_with_this_argument`, `members.rs`) must
keep one instantiation per tuple receiver (`getTypeWithThisArgument`)
independent of check order; that is a checker fix outside this lane. The
legacy dumps (`parity_gate.sh compare`) show no transition from the held
port.
