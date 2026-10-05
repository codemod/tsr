# Whole-project performance

`bd tsr-1yb` requires complete project checks to finish in **at most half the
wall time of pinned tsgo**. Parser and binder benchmarks in ADR-0009 remain
useful, but do not establish this CLI target.

## Reproduce

Build both compilers before timing them. The harness accepts a prebuilt native
binary so that a Go build cannot contaminate the measured interval:

```sh
cargo build --release --bin tsr
cargo run -p xtask -- perf-project \
  --project /path/to/project/tsconfig.json \
  --tsgo /path/to/pinned/tsgo \
  --samples 5 --output /tmp/whole-project-perf.json
```

Add `--mode single` to request `--singleThreaded true` on both sides. The default
measures each CLI's normal scheduling (see [Native checker pool](#native-checker-pool)).
The real project's path is opt-in; private sources and detailed output are not
committed. `benches/projects/generic-imports` is a small public smoke fixture,
including an intentional assignment error. It verifies the measurement plumbing;
it is too small to establish throughput on large projects.

`benches/projects/domain-model` is the public representative project: about
7,000 lines in 42 modules, generated deterministically by
`python3 scripts/generate_perf_project.py` (rerun it and diff to verify the
committed bytes). It uses cross-module imports, generic interfaces and classes,
abstract members, discriminated unions narrowed by `switch`, mapped and
conditional types, overloads, optional chaining and async functions, and ends
with one intentional TS2322 control. Both CLIs report exactly that diagnostic;
keep it that way when changing the generator, because a benchmark whose
diagnostics differ is not equivalent work. The first generator draft exposed a
TSR false positive (an object literal against an interface that extends an
instantiated generic base with an optional member) and avoids that shape.

The harness runs one warmup per tool and alternates process order across measured
pairs. Every check is a fresh process with `noEmit`, `incremental false`,
`composite false`, and plain diagnostics. It does **not** flush the OS file cache:
these are warm-filesystem measurements. POSIX `wait4` measures CPU and peak RSS
for each individual child rather than reusing cumulative resource usage.

Each sample also records its child PID, launch timestamp and exact command.
The [cache-isolation controls](benchmark-cache-isolation.md) demonstrate native
incremental diagnostic replay and verify that absent, valid, stale, poisoned and
malformed build info cannot change the disabled-incremental fixture checks.
Fresh process identity alone does not exclude persisted incremental reuse.

The JSON contains every sample, medians, p95 and ranges, binary fingerprints,
source/oracle/project revisions, effective configs, loaded-file identities, and
diagnostic fingerprints. It persists each sample immediately. A timeout or an
unsupported compiler invocation fails the run. Schema version 2 fingerprints
observed input state before and after each child, including preflights and warmups.
Capture time, including setup and loaded-path discovery observations, is
recorded separately from child wall/CPU/RSS.

`observed_wall_ratio` is always an observation. `verified_wall_ratio` is null
until complete cross-tool input coverage and actual performed checker work are
verified, as well as matching options, loaded scope and stable diagnostics.
Logical symlink paths remain distinct; only known bundled-library prefixes are
normalized. `target_verified` also requires matching diagnostics and a ratio at
most 0.50. The current harness does not yet collect complete query coverage or
actual checked-work/worker telemetry, so its verified ratio remains null even
when the public smoke project's loaded lists and diagnostics agree. Full-corpus
correctness verification is separately required before the epic can close.

`--require-comparable` fails after saving evidence when performed work is
unverified. Until the missing coverage and worker controls are implemented, use
reports as observations; this flag cannot currently produce a passing speed gate.

## Resolver input manifests

`bd tsr-1yb.1.2.1` adds `--input-manifest /tmp/inputs.json`. Supply paths from
the actual config/host/resolver observations, including extended configs,
queried package manifests, successful and failed file candidates, directories
whose entries affect discovery, and relevant logical symlink paths. Relative
names are interpreted against the project's config directory:

```json
{
  "schema_version": 1,
  "provenance": {
    "source_sha": "full-source-revision-of-the-query-producer",
    "producer": "source-qualified config/host/resolver query capture"
  },
  "paths": [
    "tsconfig.base.json",
    "node_modules/example/package.json",
    "node_modules/example/missing.d.ts",
    "node_modules/example",
    "linked-package/index.ts"
  ]
}
```

Provenance is recorded caller metadata, not an attestation that paths are
complete or that a binary was built from that revision. The report separately
records the harness hashes, compiler binary hashes, flags, effective configs,
source/oracle revisions and whether the declared producer revision matches the
harness checkout. Include the query producer binary/patch hashes and capture
command in provenance when using temporary instrumentation. Existing
[resolver construction controls](resolver-construction-performance.md) export
local observed-path rows; their `path` values can populate this format, keeping
their original producer identity and partial-coverage limits.

The harness always adds the root config, both compiler binaries, the manifest
file and the union of physically listed loaded sources. Loaded paths join the
reference after discovery; their state before that first observation is not
proved. Every full check then validates the same union. Path spelling remains
exact: `alias/../file` follows a symlink before its parent segment, while
`file/.` and `file/` fail when `file` is regular. Lexical normalization would
observe a different input. Files are streamed into
SHA-256 hashes; missing/file/directory/other kinds remain distinct. Snapshots
record realpaths, symlink spellings along logical ancestors, and directory
entry names/kinds/link targets. Special files are not read. Snapshot errors
invalidate evidence rather than masquerading as missing files. Schema 2's
loaded-input digest uses per-file hashes and is not interchangeable with the
older concatenated-byte digest.

Keep the report outside observed directories: writing an artifact there changes
their entries and correctly invalidates the run. A changed input stops sampling,
saves the failed observation and any measured/rejected full-check sample, and
leaves `target_verified` false. Warmup changes cannot be forgotten before timed
samples start. Fingerprinting warms OS caches; it is not a cold-cache benchmark.

All supplied manifests are reported as partial. Directory-entry capture does
not recursively hash unqueried descendants. Bundled library bytes, environment,
unobserved queries and transient changes between snapshots remain unproved.
`complete_input_equivalence_verified` and `actual_checked_work_verified` remain
false. Parent `bd tsr-1yb.1.2` still owns permanent cross-tool performed-work
telemetry; `bd tsr-1yb.1.2.2` owns cross-tool query capture and its coverage
proof; `bd tsr-1yb.1.1.1` owns CLI trace delivery. The tracked empty-suffix
resolution defect `bd tsr-6.59` is also unaffected.

Run the public controls with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts -p test_whole_project_perf.py -v
```

They exercise manifest byte changes, missing candidate creation, symlink
retargeting, extended/root config changes, directory additions/removals,
compiler replacement and warmup mutations through real child processes. A
matching loaded-list/options/diagnostics control verifies that incomplete work
cannot pass `--require-comparable` or publish a verified ratio.

The [sanitized validation receipt](benchmark-input-controls.json) records 30
passing script tests, five alternating public pairs in each worker mode, and a
single-pair private-app scale check. The app validates more than 51,000 observed
paths, including directory entries, while retaining explicit partial coverage.
The receipt uses the frozen CJS candidate and pinned native binaries; these runs
validate the input protocol and reporting, and do not claim current-source
throughput or progress against the native 0.50 target.

## First paired real-app baseline

Source `8f8f4e1a4fb197c266f79dc5751b3537967e5a68`, pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, Codemod Next.js app, default
scheduling, one warmup and five measured pairs on the same Mac:

| Measurement | TSR | tsgo |
|---|---:|---:|
| Median wall time | 7.321 s | 3.224 s |
| Wall range | 7.215–7.465 s | 3.149–3.454 s |
| Median user CPU | 6.391 s | 16.915 s |
| Median system CPU | 0.925 s | 1.924 s |
| Median peak RSS | 1.118 GB | 4.995 GB |
| Listed files | 13,560 | 13,098 |
| Diagnostics | 141 | 0 |

The observed ratio is **2.271**, against a target of **0.50**. Diagnostic output
was stable within each tool. **This is not a verified equal-work ratio.** Besides
different loaded files, reported options differ in `paths`,
`allowImportingTsExtensions`, `disableSourceOfProjectReferenceRedirect`, and
`tsBuildInfoFile`. Some may be omissions in TSR's config serialization rather than
actual ignored options; establish that in source instead of silently dropping them
from the comparison. The build-info path does not authorize incremental reuse:
both incremental and composite were explicitly disabled.

Detailed local evidence is `/tmp/tsr-1yb-nextjs-baseline.json`. The initial
baseline predates the harness's input-content fingerprint addition. Subsequent
confirmation must use that check. These observations supersede the earlier single
timings as evidence of the problem, not as proof that work matches or that the
optimization is complete.

## Initial phase attribution

The CLI honors `--extendedDiagnostics` and reports actual checks plus
host-clock phase times. Program time combines discovery/resolution, parsing and
binding; opt-in loader statistics now separate those costs below. Reporting
includes diagnostic extraction, source indexing, comment
directives and formatting. Compilation time excludes process startup and final
teardown, so use the external harness for the end-to-end target.

A separate full-app probe on the same source with the new instrumentation reported
13,560 loaded files and **1,341 actually checked**: 0.031 s config, 3.696 s
program construction, 0.000 s checker initialization, 3.700 s checking and 0.089 s
reporting. A `noCheck` control performed zero checks, took 3.283 s wall and
0.926 GB peak RSS. These are locating probes, not a new confirmed benchmark.
Both program construction and checking need improvement; parallelizing only the
checker cannot reach half native's total wall time while loading alone costs
over three seconds. Detailed source and native counter attribution remain in
`bd tsr-1yb.2`.

## Package identity alignment

The loader now follows native `filesparser.go:getProcessedFiles`: the first
source file with a complete package identity wins during deterministic replay.
Name, version, submodule and resolved peer dependencies all participate. Imports
through duplicate physical paths point to the first source file, and replay skips
the duplicate's dependencies. `deduplicatePackages: false` keeps both instances.
Discovery still parses duplicates, as native does; this does not avoid all work
on those copies.

On the real app this removes all 463 TSR-only listed paths, leaving 13,097 TSR
files versus 13,098 native files. Native additionally loads
`@lingui/conf/dist/index.d.ts`; effective-config differences also remain. The
performance comparison therefore remains unverified. The full assertion audit
loses no previously RIGHT rows: four WRONG rows become RIGHT and six newly
aligned rows are RIGHT. One newly aligned global-merge row remains WRONG.
The diagnostic harness checks each canonical source-file identity once and
attributes its spans to the canonical test unit. The pinned global-merge
diagnostic control passes; the full diagnostic suite retains 2,799/5,488 passes.

A separate five-pair comparison alternated saved pre-change and candidate
binaries on the same app, with fresh processes, one warmup per binary, no emit
and incremental/composite disabled. Median wall times were **7.418 s before and
7.428 s after** (ratio 1.0014); all 141 diagnostic fingerprints matched.
This is neutral within observed noise, not a speed improvement. Local evidence
is `/tmp/tsr-1yb-dedup-paired.json`, with binary hashes and every sample.

Compiler controls verify canonical import identities, skipped duplicate-only
dependencies, and separate versions/submodules/peers. A distinct-package class
assignment control also exposes a preexisting checker false negative: with
deduplication disabled native reports incompatible members but both the saved
pre-change binary and candidate accept them. This is tracked as `bd tsr-6.47.1`;
the passing source-file identity controls do not prove that diagnostic works.

## Loader cost attribution

Run the built CLI with `--extendedDiagnostics --noEmit --incremental false
--composite false` and the same project. Add `--noCheck` for a loader-only
control. The added clocks and counters run only when extended diagnostics is
enabled; these fields are locating measurements, not a throughput benchmark.

A separate real-app `noCheck` probe after package alignment reported:

| Measurement | Time |
|---|---:|
| Loader total | 2.960 s |
| File reads | 0.162 s |
| Package/module metadata | 0.057 s |
| Parser, including JSDoc | 0.547 s |
| Discovery, including resolver calls | 2.172 s |
| Resolver calls, within discovery | 2.041 s |
| File/module indexing | 0.054 s |
| Binding and global merges | 0.156 s |
| Program total | 3.194 s |

The resolver made **46,031 module/type-directive queries**, taking about 69% of
the loader time in this probe. Resolver time is a subset of discovery time;
do not add it to discovery again. It excludes lib-replacement queries during
setup. Loader total includes setup/replay but excludes final task-storage
disposal; program total includes that disposal and the unclassified overhead.

The loader parsed **13,560 files**, retained 13,097, and performed zero checks.
A separate full-check probe made the same 46,031 queries, attributed 2.853 s to
the resolver, and checked 1,341 files. Both full-check instrumentation versions
preserved the existing 141 diagnostic fingerprints. Probe timing varies and
does not demonstrate a code speedup. Local evidence is
`/tmp/tsr-1yb-resolver-profile.json`.

This ranks resolver probe/cache reuse ahead of parser or binder work. Count
filesystem probes and reusable native directory/module keys in `bd tsr-1yb.10`
before implementing a cache; package-JSON caching already exists and its trace
behavior is observable. Lazy JSDoc remains a measured candidate, with a smaller
upper bound than the resolver cost on this workload.

## Native resolution query caches

Of 46,031 discovered task queries, 22,265 module requests and 45 type-reference
requests repeated complete native cache keys. The resolver now reuses both
successful and failed results with those keys. Tracing bypasses query-cache
reads, matching pinned tsgo, so traced walks remain observable. Caches are scoped
to one resolver/options/project snapshot; config lookup uses separate semantics.
See [module resolution](module-resolution.md#per-project-query-caches).

Five fresh-process pairs alternated an uncached reference and cache candidate,
both including checker commit `830c97e6`. Both retained the same 13,097 files,
had stable input-content fingerprints, and produced identical 123 diagnostics:

| Measurement | Before | Cached |
|---|---:|---:|
| Median wall | 7.255 s | 6.622 s |
| Wall range | 7.208–7.282 s | 6.616–6.631 s |
| Median user CPU | 6.376 s | 6.022 s |
| Median system CPU | 0.871 s | 0.596 s |
| Median peak RSS | 1.110 GB | 1.093 GB |

This is an **8.7% wall reduction** against the uncached TSR reference, with a
substantial reduction in system CPU. RSS ranges overlap; do not claim a memory
improvement from these medians. The complete 474,251-row assertion verdict files
are byte-identical: 458,036 RIGHT, 2,445 GAP, 13,770 WRONG. Six controls verify
actual filesystem-probe elimination, mode/directory/inferred distinctions,
traced walks, symlink/package identity and fresh-snapshot behavior. A temporary
full-app comparison of cached and freshly computed complete resolver results
also found no differences; that verification instrumentation was removed.

An earlier comparison used a reference predating the concurrent template
assertion fix. Its 141-to-123 diagnostic reduction and four assertion gains
belong to that checker fix and are not cache benefits. The isolated comparison
above supersedes it. Detailed local evidence, including distinct binary hashes,
is `/tmp/tsr-1yb-resolution-cache-isolated-paired.json`.

A separate five-pair pinned-native comparison observed a TSR median of 6.705 s
and tsgo median of 3.230 s (observed ratio 2.076). Both used fresh processes,
warmed filesystem inputs and disabled incremental/composite reuse. TSR retained
13,097 files and reported 123 diagnostics; tsgo retained 13,098 and reported
none. The remaining native-only Lingui declaration and effective-config output
differences keep the benchmark **incomparable**. This does not verify the 0.50
target. Local evidence is `/tmp/tsr-1yb-nextjs-resolution-cache.json`; follow-up
alignment is tracked in `tsr-1yb.1.1`.

## Native compiler-host filesystem cache

A temporary wrapper measured actual backing-host operations during program
loading after query caching; configuration discovery was outside this probe.
It found 114,266 repeated metadata
and real-path probes. The CLI now wraps its compilation host with native
`cachedvfs` semantics; content reads remain uncached.

| Operation | Before calls | Cached backing calls |
|---|---:|---:|
| File existence | 72,745 | 41,238 |
| Directory existence | 78,858 | 6,155 |
| Real path | 11,066 | 1,010 |
| Content read | 14,213 | 14,213 |

All cached backing-call paths were unique. This workload made no directory-entry
queries; unit controls cover cached entries, including empty results. Repeated
underlying calls accounted for 0.240 s in the locating probe. Tracking overhead
is excluded from that sum, and the instrumented total is not a speed benchmark.
The temporary wrapper was removed before the final build.

Five fresh-process pairs compared source `f1389da9` with and without this wrapper:

| Measurement | Before | Cached |
|---|---:|---:|
| Median wall | 6.629 s | 6.478 s |
| Wall range | 6.525–6.661 s | 6.363–6.494 s |
| Median user CPU | 5.999 s | 6.062 s |
| Median system CPU | 0.619 s | 0.398 s |
| Median peak RSS | 1.123 GB | 1.101 GB |

The measured wall reduction is **2.3%**; system CPU fell by about 36%, offset in
part by cache lookup/storage CPU. RSS ranges overlap, so the result establishes
no memory reduction. Both sides retained the same 13,097 files, stable content
fingerprints, effective options and 123 diagnostics. The public generic-imports
fixture also retains identical diagnostics and three checked files; output
differs only in phase times. The resolution and loader oracle suites now use the
cached host, exercising all 95 resolver transcripts and 96 loader cases.
All 474,251 assertion rows are byte-identical against the same-source reference:
458,472 RIGHT, 2,436 GAP, 13,343 WRONG. These tallies include concurrent JSDoc
checker changes, whose gains are independent of this cache. CLI baseline replay
also has identical output: 33 of 43 judged cases pass, with ten existing failures.

Local evidence is `/tmp/tsr-1yb-cached-vfs-paired.json`; its `tsgo` tool slot holds
the saved **TSR reference binary**, not native tsgo. This is an isolated TSR
comparison and does not prove the overall 0.50 target. The full-app CLI trace
control emitted zero bytes on both versions and is invalid as trace evidence;
trace forwarding is tracked in `tsr-1yb.1.1.1`.

## Registered type-parameter membership

A symbolized sample delayed into checking on source `52585a08` located a large
allocation cost: 853 of 3,482 main-thread samples were in registry-vector
collection called by conditional evaluation. Temporary counters found that
conditional, mapped-context and conditional-extends queries copied **795 million
parameter IDs** across 37,629 requests. The registry reached 42,150 entries,
while the largest visited type graph had 299 nodes. These samples and counters
locate the cost; their instrumented durations are not throughput measurements.

Conditional and mapped-context predicates now query the existing checker-owned
registry directly. Conditional-extends evaluation only collects candidate IDs
after the graph confirms it contains a registered parameter. Both predicates
use the same graph walk, traversal order and cycle guard as explicit inference
parameter queries; those explicit queries retain their printed-name fallback.
No answers are memoized, so changes to the current registry remain visible.

Five alternating fresh-process pairs compared the clean saved `52585a08` TSR
binary with this change, with one warmup per binary:

| Measurement | Before | Direct membership |
|---|---:|---:|
| Median wall | 6.452 s | 4.987 s |
| Wall range | 6.380–6.606 s | 4.951–4.997 s |
| Median user CPU | 6.051 s | 4.564 s |
| Median system CPU | 0.400 s | 0.388 s |
| Median peak RSS | 1.132 GB | 1.083 GB |

This is a **22.7% wall reduction** against the saved TSR reference. Both sides
retain the same 13,097 files, effective options, input-content fingerprints and
123 diagnostics. All 474,251 assertion rows are byte-identical: 458,472 RIGHT,
2,436 GAP and 13,343 WRONG, with zero previously RIGHT losses.

Focused controls cover registered versus unregistered same-named parameters,
cyclic references, nested signatures, constraints/defaults, primitives, explicit
printed-name recovery and registry updates. The bounded unresolved Flatten
fixture produces the same diagnostic as pinned native; the MCP fixture checks
clean with all three binaries. The public generic-imports fixture retains the
same three TSR diagnostics and three actual checked files; its existing native
comparison reports one diagnostic, so that fixture does not establish parity.

Local evidence is `/tmp/tsr-1yb-parameter-membership-paired.json` and
`/tmp/tsr-1yb-parameter-membership-assertion-audit.json`. The benchmark's `tsgo`
slot contains the saved **TSR reference**, not native tsgo. This confirms the
isolated optimization, while the native 0.50 target remains unverified and
workload alignment remains open.

## Indexed Program file metadata

After the registry change, a delayed symbolized sample on `cfcbcfab` still
attributed 148 self samples to `Program::jsx_factory_namespace`, with another
13 inclusive samples in declaration-file lookup. The sampled main thread had
2,563 samples across the end of loading, checking, reporting and teardown.
Those counts locate the repeated scans; they are not whole-process cost shares.

Both host queries now use the existing immutable `files_by_source_file` index.
They preserve the old root/referenced-file boundary, which excludes the bundled
lib prefix. Missing IDs still yield no namespace and a false declaration-file
answer. Each constructor allocates source roots in the program's shared node
table, so distinct parsed files have distinct IDs even when their paths
canonicalize alike. Package redirects query the canonical retained file's ID.

Five alternating fresh-process pairs compared saved `cfcbcfab` TSR with this
change, with one warmup per binary:

| Measurement | Before | Indexed |
|---|---:|---:|
| Median wall | 5.043 s | 4.842 s |
| Wall range | 5.012–5.234 s | 4.791–4.849 s |
| Median user CPU | 4.590 s | 4.417 s |
| Median system CPU | 0.437 s | 0.415 s |
| Median peak RSS | 1.096 GB | 1.112 GB |

The isolated wall reduction is **4.0%**. RSS ranges overlap; this result does
not establish a memory change. Both sides retain the same 13,097 loaded files,
effective options, stable content fingerprints and 123 diagnostics.

Controls cover distinct per-file JSX pragmas, same-path inputs with different
source IDs, TS/TSX and `.d.ts`/`.d.mts`/`.d.cts` classification, imported files,
the bundled-lib boundary, unknown IDs and canonical package redirects. The
excluded bundled-lib metadata domain is a preexisting fidelity question tracked
in `tsr-1yb.7.2.1`, under the checker port epic; this lookup experiment preserves
it rather than using a semantic change as performance evidence.

All 474,251 assertion rows are byte-identical to the same-source reference:
458,472 RIGHT, 2,436 GAP and 13,343 WRONG, with zero previously RIGHT losses.
The 84 affected compiler/execute tests, including the shared-Program ownership
controls, pass, as do all-target clippy and formatting checks.

Local evidence is `/tmp/tsr-1yb-program-file-queries-paired.json`; its `tsgo`
slot contains the saved **TSR reference**, not native tsgo. The isolated gain
does not verify the overall native 0.50 target.

A separate five-pair pinned-native run after this change observed **5.559 s TSR
versus 3.305 s tsgo**, ratio **1.682**. It used the same TSR binary as the isolated
comparison above, but alternated against native rather than the saved TSR
reference. Do not combine medians from those different pairings into a ratio.
TSR user/system CPU medians were 4.552/0.458 s and peak RSS 1.130 GB; native
medians were 17.364/1.640 s and 5.023 GB. The additional TSR wall time over CPU in
this pairing has not been attributed.

The native comparison remains **incomparable**: 13,097 versus 13,098 files,
123 versus zero diagnostics, and effective-config differences. Input content
remained stable; processes were fresh and incremental/composite reuse disabled.
The 0.50 target is still unmet and unverified. Local evidence is
`/tmp/tsr-1yb-nextjs-after-file-index.json`. Worker memory/scaling and remaining
loader CPU are the next larger opportunities; checked-scope telemetry and
workload alignment remain required before verifying the overall target.

## Package JSON string validation

A two-second early-loader sample with `noCheck` on the saved `efddbfe0` binary
captured 1,666 main-thread stacks. UTF-8 validation had 630 self samples, all
under `tsr_module::json::Parser::string`. For each ordinary string character,
that reader validated the entire remaining document suffix, then decoded one
character. The public parser already receives a valid `&str`; repeated suffix
validation makes this part of reading large package files quadratic.

The reader now retains that validated text alongside its byte view and uses
safe string slicing to decode the next character. It preserves cursor-boundary
failure, escapes, declaration order, duplicate-key last position and existing
malformed-input behavior. It adds no unsafe conversion, cache or dependency.

Five alternating fresh-process pairs compared saved source `760513fa` with this
change, after one warmup per binary:

| Measurement | Before | Validated text view |
|---|---:|---:|
| Median wall | 4.935 s | 4.140 s |
| Wall range | 4.858–5.041 s | 4.061–4.194 s |
| Median user CPU | 4.492 s | 3.711 s |
| Median system CPU | 0.397 s | 0.384 s |
| Median peak RSS | 1.093 GB | 1.077 GB |

The isolated wall reduction is **16.1%**. RSS ranges overlap, so the result
establishes no memory reduction. Both binaries retain 13,097 loaded files,
effective options, stable input-content fingerprints and identical complete
123 diagnostics. Separate extended-diagnostics controls confirm 1,341 actually
checked files and 13,560 parsed files on both sides.

All 474,251 assertion rows are byte-identical to the same-source reference:
458,508 RIGHT, 2,433 GAP and 13,310 WRONG, with zero previously RIGHT losses.
The tallies include concurrent checker fixes already present in the reference;
they are not benefits of this optimization. All 49 module tests, 95 native
resolver transcripts and 96 loader cases pass, as do all-target module clippy,
formatting and whitespace checks. Focused controls cover mixed 2/3/4-byte UTF-8,
escaped delimiters and surrogate pairs, and malformed strings after multibyte
characters.

A separate candidate `noCheck` locating probe attributed 0.423 s to resolution
and 1.624 s to Program construction. Its sample has no whole-suffix UTF-8
validation stacks under JSON string parsing. The captured stages differ from
the earlier early-loader sample, so these profile times and counts do not
establish a throughput ratio; the paired full-check result above does.

Local evidence is `/tmp/tsr-1yb-json-utf8-paired.json`,
`/tmp/tsr-1yb-json-utf8-assertion-audit.json` and
`/tmp/tsr-1yb-json-utf8-checked-scope.json`. The paired harness's `tsgo` slot
contains the saved **TSR reference**, not native tsgo. This isolated win does
not verify the overall native 0.50 target, and production checking remains serial.

A separate five-pair pinned-native observation after this change measured
**4.287 s TSR versus 3.414 s tsgo**, observed ratio **1.256**. This run is
incomparable: TSR retains 13,097 files and reports 123 diagnostics; native
retains 13,098 and reports none, with the same native-only Lingui declaration
and effective-config differences as before. Inputs remain stable and
incremental/composite reuse is disabled. Do not combine the isolated 4.140 s
median with this native run's median. The 0.50 target remains unmet and
unverified. Local evidence is `/tmp/tsr-1yb-nextjs-json-utf8-native.json`.

## Structured property-name enumeration experiment

A checker-phase sample on `81216b80` captured 1,699 main-thread stacks.
Disjoint attribution to the nearest TSR owner placed 138 samples under
`collect_structured_property_names`, including 61 allocator self samples.
The captured interval covers part of checking; these counts are not a share
of complete CLI time.

Temporary opt-in counters recorded 222,029 structured visits across 3,615
owner symbols, with at most 25,471 visits to one owner. The existing loop
copied 3,262,955 own names (30,779,136 name bytes) into temporary vectors.
Only 34,699 copies were subsequently discarded as duplicates. Temporary
vector capacity summed to 116,079,456 bytes over the run; this is neither
total heap allocation nor peak live memory. Instrumentation was removed
before building the comparison binaries.

The candidate enumerated immutable binder members directly, allocating a
name only when adding it to the result. It removed the intermediate vector
while retaining most final name copies. Three independent five-pair runs,
each alternating fresh processes after one warmup per binary, produced:

| Reference source | Before median wall | Candidate median wall | Outcome |
|---|---:|---:|---|
| `81216b80`, initial | 5.068 s | 4.987 s | 1.6% observed reduction |
| `81216b80`, confirmation | 5.123 s | 4.949 s | 3.4% observed reduction |
| `4bf5fe96`, integration | 4.929 s | 4.971 s | 0.9% observed increase; rejected |

Every sample is retained. The confirmation run includes large final-pair
outliers (9.840 s before, 5.962 s candidate); no memory benefit is established.
The integration result fails the isolated wall-time decision's unchanged
0.020 s absolute threshold. The production loop was restored rather than
shipping a gain that did not repeat on current source.

On `4bf5fe96`, both binaries retain identical effective options, input
fingerprints, 13,097 loaded files and complete 123 diagnostics. Separate
extended-diagnostics controls confirm 1,341 checked and 13,560 parsed files
on both sides. All 474,251 assertion rows are byte-identical: 459,381 RIGHT,
2,198 GAP and 12,672 WRONG, with zero previously RIGHT losses. These totals
reflect the concurrent fidelity checkpoint, not an optimization benefit.
Retained unit controls cover diamond inheritance and duplicate names,
type-parameter exclusion, computed members, stable repeat ordering, cycles
and unfollowable bases.

Local evidence is `/tmp/tsr-1yb-property-names-paired.json`,
`/tmp/tsr-1yb-property-names-confirmation.json`,
`/tmp/tsr-1yb-member-current-paired.json`,
`/tmp/tsr-1yb-member-current-audit.json` and
`/tmp/tsr-1yb-member-current-telemetry.json`. Each paired harness's `tsgo`
slot holds a saved TSR reference. These runs establish no native speed ratio.

The repeated visits justify investigating completed structured-member reuse
under `tsr-1yb.4.1.1`. Native `resolveStructuredTypeMembers` retains members
per concrete type, including instantiated arguments and completion state;
an owner-symbol-only name cache would not implement that contract. The
overall native 0.50 target remains unmet and unverified.

The [structured-member reuse contract](checker-member-cache-contract.md)
records the native publication states, current TSR storage and executable
identity/re-entry controls. It hands concrete-type attribution and the
measured builder implementation to `tsr-1yb.4.2`; it introduces no production
cache or throughput claim.

The [ambient lookup experiment](ambient-module-lookup-performance.md) reduces
temporary quoted-key construction under `tsr-1yb.7.6`. Two independent
five-pair TSR comparisons confirm 1.943% and 0.948% median wall reductions,
with identical complete corpus results and actual checked-file identities.
The separate pinned-native comparison remains incomparable; the required
verified native wall ratio stays 0.50.

## Native checker pool

The CLI now checks with native `checkerpool.go` scheduling: `singleThreaded`
gives one checker on the calling thread, otherwise `checkers` (default 4,
at most 256, at most the file count) checkers run on their own workers.
Program file `i`, libraries included, belongs to checker `i % count`; each
checker visits only its own files in program order, and publishes only
diagnostics located in files it owns, as native `GetSemanticDiagnostics`
asks the file's associated checker. Checkers share the program, binder and
node tables read-only and keep private type stores. The opt-in work trace
observes one private checker, so a traced run keeps a pool of one.
`checker_pool.rs` records the ownership and work boundary.

Nine alternating pairs against pinned tsgo on the 14-CPU Linux box:

| Project | Before TSR | Pool TSR | tsgo | Ratio before | Ratio after |
|---|---:|---:|---:|---:|---:|
| domain-model | 403 ms | 259 ms | 219 ms | 1.838 | 1.181 |
| generic-imports | 143 ms | 144 ms | 123 ms | 1.191 | 1.171 |

Diagnostics match tsgo on both projects. The smoke fixture is dominated by
library loading (66 files, 2.8 MB), not checking.
