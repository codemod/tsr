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
