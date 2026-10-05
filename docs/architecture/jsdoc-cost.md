# JSDoc cost and discovery contract

`tsr-1yb.9.1` attributes eager documentation work without changing production
parsing or storage. The next experiment belongs to `tsr-1yb.9.2`; the full
comparable TSR/tsgo median wall target <=0.50 remains unmet and unverified.

The frozen Rust source is `24aebf06072b9847667d39b21097be395c85134c`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. [The report](jsdoc-cost.json)
records exact archive, patch, binary, fixture, effective-option and raw-artifact
hashes. Rust used rustc 1.96.0; native controls used Go 1.26.0. The ordinary
release profile uses opt-level 3, no LTO, 16 codegen units and panic abort.
All observer edits live in an archived source tree. The canonical crates and
vendor are unchanged by this work.

## Observations and limits

The read-only real project is
`/Users/mohebifar/dev/codemod/app/apps/nextjs`. Each timed command is a new
complete CLI process with `--project tsconfig.json --noEmit --incremental false
--composite false --pretty false --extendedDiagnostics --listFiles`. Three
serial off/on pairs alternate order; the observer is the same binary in both
modes. An ordinary binary supplies a separate three-process baseline.

| Median measurement | Next.js | Public mixed TS/TSX/declarations/checkJs |
|---|---:|---:|
| Ordinary full CLI wall | 4.459624 s | 0.031531 s |
| Probe binary, observer off wall | 4.524933 s | 0.031344 s |
| Observer on wall | 4.796990 s | 0.033767 s |
| Paired on-minus-off wall overhead | 0.297458 s | 0.002422 s |
| Inclusive file parser intervals | 627.050 ms | 17.781 ms |
| JSDoc body intervals | 230.293 ms | 9.345 ms |
| Parser arena requested bytes | 302,949,415 | 8,011,479 |
| JSDoc arena requested bytes | 61,104,711 | 2,155,884 |
| Sum of sparse table capacity bytes at parse return | 7,211,424 | 268,416 |
| Reported loaded / checked / parsed files | 14,050 / 1,397 / 14,746 | 70 / 6 / 70 |
| Complete diagnostics | 117 | 0 |

The median paired overhead is not the difference of the two medians. Next.js
off/on user CPU medians are 3.981610/4.011418 s, system CPU
0.456252/0.729945 s, peak RSS 1,184,071,680/1,148,518,400 bytes. Public RSS is
35,356,672 bytes in both medians. These variable maxima establish no memory
improvement. Individual CPU/RSS/wall samples and per-file/category intervals
remain in the report. Declaration inputs dominate measured documentation work;
the public fixture includes default libraries as well as its 512-interface file.

Removing **all** observed body work would be an optimistic observer-scale
230 ms ceiling, about 5.1% of the observer-off wall. It is not a predicted gain:
the intervals include clock/counter overhead, some documents must stay eager,
lazy queries may recreate work, and attachment/table growth lies outside these
intervals. Eligible plain-text cost has not yet been separated. No production
candidate or 2x improvement is retained here.

`Arena::allocated_bytes` measures requested sizes, excluding chunk reservation,
padding, other heap allocations, temporary vectors and node-map/table copies.
Comment-range bytes measure source exposure, not copied bytes. Sparse table
capacity is measured at each parse return, including duplicate loader parses
later discarded; its sum is not retained live Program storage or RSS. Filename
and source copies occur before the file timer. Body timers cover range scanning,
recursive JSDoc grammar and arena requests; attachment and semantic consumers
are separate. Trace writes happen after the measured parser interval but inside
the loader's broader parse phase. Do not add overlapping phase measurements.

Ordinary/probe-off/probe-on preserve complete diagnostics, reported counts and
ordered physical source payloads. Archive default-library path spellings differ;
every ordered realpath and SHA256 payload matches. Loaded sources and root
config remain byte-stable before/after each sample. This is partial input
coverage: missing-path queries, all manifests and lazy forcing are not proved.
Separate intrusive worker traces qualify artifact integrity and directly observe
the same ordered 1,397 Next.js and six public full-file workers off/on. They do
not qualify every semantic operation, cross-tool work or full forcing.

An earlier 7.3139 s calibration overlapped our native compilation and was
discarded. The clean ordinary baseline range is 4.453123–4.517397 s. Trace-v1
measurements were superseded after same-PID freshness validation was added;
the table and report use v2. Neither discarded data set scores an optimization.

## Native boundaries and current Rust consumers

At the pinned native source, `parser/jsdoc.go`'s `withJSDoc` retains cheap
deprecated flags and eagerly parses TS link/see metadata, while ordinary TS,
TSX and declarations defer documentation until `Node.JSDoc(file)`. JS stays
eager. `parser/reparser.go` materializes JS typedef/template/overload declarations
and `KindJSImportDeclaration` for `@import`; import discovery consumes them
during loading. `ast/ast.go`'s `resolveJSDoc` publishes into a file-owned cache
under a mutex; repeated and concurrent queries return the same document identity.
Native's sanctioned mutable cache does not supply a Rust ownership design.

The private [native tests](native-jsdoc-boundaries-test.go) cover seven eager
boundary cases, three JS reparse cases and eight concurrent queries. Five
repetitions, race, the parser package and vet pass. Deliberately deferring links,
omitting deprecated flags and deferring JS each fail their intended assertion;
the native file is restored exactly before final checks. Clean native CLI
default/single controls load `base.ts` through `@import`. Native declaration
emit produces six files and preserves generic identity, two overloads, typedef
properties, imported type and link/deprecated text. This does not establish Rust
declaration-emit parity. Three existing Rust raw-comment printer controls pass.

Current Rust boundaries to preserve:

| Consumer | Current dependency |
|---|---|
| `tsr-parser/src/jsdoc.rs`: `parse_leading_jsdoc`, `parse_jsdoc_comment` | Eager scanner save/restore, parsed tags, host attachment and sparse JSDoc table |
| `tsr-compiler/src/lib.rs` and `loader.rs` parse sites | Default JSDoc-enabled `ParseOptions`, shared parsed-file storage |
| `loader.rs`: `collect_jsdoc_import_references` | JS-only `@import` dependency discovery before resolution; TS handling is a separate fidelity boundary |
| `Program::bind`, `binder::bind_source_file_with_jsdoc` | Tag/type-host publication and declarations before checking |
| Checker `set_jsdoc`, symbols/signatures | Host-to-doc and doc-to-host maps; `@returns`, `@this`, `@template`, typedef/import and signature queries |
| Compiler module host `jsdoc_template_parameters` | Templates with typedef-owned templates excluded |
| Printer leading comments | Original source ranges, not JSDoc table storage |

Rust arenas are not Sync; parsed files use immutable borrowed nodes with an
owning arena. Existing NodeIds index Program domains. A private lazy parse
cannot allocate into a published shared arena or introduce its IDs into shared
node maps. Cached results may not contain another checker's TypeIds.

## Strong negative controls expose a separate fidelity gap

[The executable negative control](jsdoc-cost-negative-controls.py) appends four
invalid uses to the same public fixtures. Native reports three TS2322 errors
and a TS2769 overload error with related detail. TSR ordinary/off/on report
zero. The native fingerprint is
`d2ee8fbc2944029add17ee06fbe2c62b68b60681b8913a780e64e6907eb7f959`;
the TSR empty fingerprint is
`4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945`.
The full normalized entries and source hashes are in the report.

P1 `tsr-6.65` owns this existing template/typedef/overload/imported-type
diagnostic gap. The failing semantic boundary is not yet localized. Clean
positive controls prove observer preservation, not equivalent native checking.
Any JS deferral experiment must preserve the negative diagnostics after that
gap is fixed. TS-only attribution can continue independently.

## Bounded experiment and ownership handoff

The first candidate is deferred **untagged plain-text documentation in ordinary
TS/TSX/declarations only**, selected after measuring its eligible cost. Keep JS,
semantic or unrecognized tags, deprecated/link/see and imports eager. Preserve
standalone parser defaults and raw comment emit. Use a conservative classifier;
ambiguous trivia remains eager. This keeps the discovery boundary narrow while
testing whether lazy documentation is worth its representation cost.

Shared Program storage may hold only immutable source ranges, source-file/host
identity and classification flags. Source text must outlive those ranges, and
discovery/binder publication must finish before workers see the Program. A lazy
query result needs a private owner with its own arena and local-ID domain;
borrowed results cannot outlive that owner or be installed into Program NodeMap
or binder hosts. A checker-local map keyed by canonical source-file/host/range
may preserve repeated result identity. Language-service callers need an explicit
query/session owner if that identity must survive a checker. `unsafe Sync`,
post-publication shared-arena allocation and globally cached TypeIds are not
acceptable shortcuts. Native's mutex cache must not be copied mechanically.

Before retaining `.9.2`, measure eligible range/tag costs and lazy hit/miss
requests, including doc query and emit workloads; verify native boundary and
strong negative controls, complete corpus RIGHT preservation and loaded/actual
work equivalence. Require two independently confirmed fresh-process whole-CLI
paired rounds above noise on representative projects, CPU/RSS included. Record
no-change if the small eligible opportunity cannot repay added storage and query
work. The existing measured all-body ceiling is the only opportunity estimate
available in this attribution slice.

## Reproduction

Use a new scratch directory. Archive the exact source above into two trees,
with the pinned vendor checkout available as `vendor/typescript-go`. Apply
`jsdoc-cost-probe.patch` to the probe tree with `git apply --check` then
`git apply`; the patch includes the observer module. Build ordinary release
`--bin tsr` in one tree and `--features jsdoc-cost-probe,work-trace --bin tsr`
in the other, using distinct `CARGO_TARGET_DIR`s. Set
`TSR_WORK_TRACE_BUILD_SHA=24aebf06072b9847667d39b21097be395c85134c` for the probe
build. Preserve the same libraries and effective configuration. Do not rebuild
or run other heavy jobs during timing. New builds require newly recorded hashes.

From the canonical repository, substituting absolute scratch paths:

```sh
rtk proxy python3 docs/architecture/jsdoc-cost-controls.py fixtures /tmp/jsdoc-public
rtk proxy python3 docs/architecture/jsdoc-cost-pairs.py --normal /tmp/normal-target/release/tsr --probe /tmp/probe-target/release/tsr --project /tmp/jsdoc-public --output /tmp/jsdoc-public-pairs
rtk proxy python3 docs/architecture/jsdoc-cost-pairs.py --normal /tmp/normal-target/release/tsr --probe /tmp/probe-target/release/tsr --project /Users/mohebifar/dev/codemod/app/apps/nextjs --output /tmp/jsdoc-next-pairs
rtk proxy python3 docs/architecture/jsdoc-cost-work-controls.py --binary /tmp/probe-target/release/tsr --source /tmp/probe-source --source-sha 24aebf06072b9847667d39b21097be395c85134c --project /tmp/jsdoc-public --output /tmp/jsdoc-public-work
rtk proxy python3 docs/architecture/jsdoc-cost-negative-controls.py --normal /tmp/normal-target/release/tsr --probe /tmp/probe-target/release/tsr --native /tmp/native-tsgo --output /tmp/jsdoc-negative
rtk proxy python3 -m unittest discover -s docs/architecture -p test_jsdoc_cost_controls.py
```

Repeat the worker command with the Next.js project into a new output directory.
The readers reject incomplete/signaled children, replayed PID/time, partial rows,
impossible intervals/bytes, noncanonical paths, FIFO traces and missing parse
coverage. Trace creation refuses to overwrite an existing file.

For native tests, archive the exact native pin and copy the durable Go test to
`internal/parser/jsdoc_cost_boundary_test.go` in that private tree. Run
`go test ./internal/parser -run 'TestJSDoc' -count=5`, the same selection with
`-race`, full `go test ./internal/parser` and `go vet ./internal/parser`, using
an isolated GOCACHE. Build native `./cmd/tsgo`. In the generated public fixture,
run normal/default and `--singleThreaded` noEmit commands, then
`--declaration --emitDeclarationOnly --outDir /tmp/jsdoc-emit --incremental false
--composite false --pretty false`. Keep emitted payload hashes and complete
diagnostics. These controls establish native boundaries rather than a native
throughput comparison.
