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
measures each CLI's normal scheduling, which is serial for TSR at `8f8f4e1a`.
The real project's path is opt-in; private sources and detailed output are not
committed. `benches/projects/generic-imports` is a small public smoke fixture,
including an intentional assignment error. It verifies the measurement plumbing;
it is too small to establish throughput on large projects.

The harness runs one warmup per tool and alternates process order across measured
pairs. Every check is a fresh process with `noEmit`, `incremental false`,
`composite false`, and plain diagnostics. It does **not** flush the OS file cache:
these are warm-filesystem measurements. POSIX `wait4` measures CPU and peak RSS
for each individual child rather than reusing cumulative resource usage.

The JSON contains every sample, medians, p95 and ranges, binary fingerprints,
source/oracle/project revisions, effective configs, loaded-file identities, and
diagnostic fingerprints. It persists each sample immediately. A timeout or an
unsupported compiler invocation fails the run. Input contents are fingerprinted
before and after sampling to catch source edits during the run.

`observed_wall_ratio` is always an observation. `verified_wall_ratio` is null
when reported options, loaded-file lists, or stable inputs do not agree. Logical
symlink paths remain distinct; only known bundled-library prefixes are normalized.
`target_verified` additionally requires matching diagnostics and a ratio at most
0.50. These checks do not yet prove all semantic work is equivalent: actual
checked-file telemetry and full-corpus correctness verification are still required
before the epic can close.

`--require-comparable` makes a mismatched-work run fail after saving its evidence.
It is useful when wiring the harness into CI; it does not require matching
diagnostics and cannot by itself prove the speed target has been met.

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

The CLI now honors `--extendedDiagnostics` and reports actual checks plus coarse
host-clock phase times. Program time combines discovery/resolution, parsing and
binding. Reporting includes diagnostic extraction, source indexing, comment
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
