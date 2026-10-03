# Benchmark cache isolation

`tsr-1yb.1.5` checks whether a standalone tsgo run can reuse persisted compiler
work in the full-project benchmark. It can in incremental mode. With both
`--incremental false` and `--composite false`, the pinned CLI takes its full
compilation path instead. OS file caches and legitimate caches within each
compiler process remain part of the workload.

This is a benchmark prerequisite, not a speed improvement or proof of the 2x
target. Actual cross-tool checked-file telemetry and representative CI workloads
remain `tsr-1yb.1.2` and `tsr-1yb.1.3`.

## Pinned native dispatch

Source pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

| Source | Cache boundary |
| --- | --- |
| `vendor/typescript-go/internal/core/compileroptions.go`, `IsIncremental` | Incremental **or** composite enables incremental compilation. Disabling only incremental is insufficient. |
| `vendor/typescript-go/internal/execute/tsc.go`, command-line dispatch | Watch has a separate path. Otherwise `IsIncremental` selects `performIncrementalCompilation`; the remaining path calls `performCompilation`. |
| Same file, `performIncrementalCompilation` | Calls `incremental.ReadBuildInfoProgram`, creates a new Program, and compares it with the stored snapshot. Fresh process identity alone therefore does not prove a full check. |
| Same file, `performCompilation` | Creates a compiler host and Program without loading a build-info Program. |
| `vendor/typescript-go/internal/compiler/program.go`, `NewProgram` / `initCheckerPool` | Creates this Program's checker pool. |
| `vendor/typescript-go/internal/compiler/checkerpool.go` | Reuses checker-local results within that Program; default checker count is four, single-thread count is one. This reuse does not cross independent CLI processes. |

The audit applies to this pinned standalone CLI, without `--watch` or `--build`.
It does not describe a language server, a long-lived watch process, or every
future native release.

## Reproducible controls

Build the compilers first. Run each mode into a new, exclusively owned directory:

```sh
python3 -B scripts/check_cache_isolation.py \
  --tsgo /path/to/pinned/tsgo \
  --work-dir /tmp/tsr-cache-proof-default \
  --output /tmp/tsr-cache-proof-default.json

python3 -B scripts/check_cache_isolation.py \
  --tsgo /path/to/pinned/tsgo --mode single \
  --work-dir /tmp/tsr-cache-proof-single \
  --output /tmp/tsr-cache-proof-single.json
```

Existing work directories are refused. The script copies
`benches/projects/generic-imports`, adds one deliberate TS2322 sentinel to each
of its two initially clean source files, and configures incremental/composite
**true** in that copy. Full-check commands override both to **false**, and both
tools' `--showConfig` output must confirm those effective values. The config
names an explicit `cache.tsbuildinfo`; controls also place build info at the
default `tsconfig.tsbuildinfo` path. No real application or machine-wide cache
is modified.

The native positive control first creates valid build info, then starts a fresh
incremental process. That process must replay the three diagnostics with zero
instantiations. Next, the control changes one stored TS2322 message argument
without changing source versions or options. A fresh incremental process must
replay `CACHE_ISOLATION_POISON`. This establishes that the build-info fixture
really can influence the compiler, rather than merely placing an irrelevant
file next to a project.

For the stale-state control, native builds a version of `schema.ts` with a
different sentinel type/error. The original source bytes are then restored
before any full-check samples. The resulting build info contains genuine old
source versions and different diagnostics.

Five alternating full-check pairs run for each of these states:

1. Both build-info files absent.
2. Valid native incremental build info.
3. Native build info from the previous source version.
4. Valid native build info with poisoned cached diagnostics.
5. Malformed build-info bytes.

Each sample retains its exact command, child PID, launch timestamp, exit status,
timeout status, CPU/wall/peak RSS, complete normalized diagnostics, integer
statistics, and build-info size/hash/mtime before and after the check. Complete
messages are sorted as units; related/continuation text is retained. Loaded
identities and input hashes are recorded separately.

Every fixture `.ts` file must produce its own sentinel. TSR must also report
three actual checked files. Native must repeat the same nonzero instantiation
count as its full-check baseline. Diagnostics alone are insufficient: the
positive incremental control replays identical messages while doing zero
instantiations. File sentinels prove visits to this fixture's three sources;
they do not replace general checked-file telemetry for arbitrary projects.

Build/setup, report writes, and build-state observations are outside the timed
compiler processes. Preflight and setup warm the filesystem. The first observed
disk state is uncontrolled, and no run is labelled disk-cold. These small,
instrumented fixture timings are excluded from the verified release ratio.

## Verified observations

Compiler source: `debb68b68494915d7ae002c4590f3d4b78f584cc`; compiler code and
release binaries were unchanged during this harness-only work.

| Binary | SHA-256 |
| --- | --- |
| TSR release CLI | `9787054f832ca8d6680eaee29b1d03bb12d2e104de901fed0e9e66736bed67c9` |
| Pinned native CLI | `118d4efb63e59af858d6d0f5a718a8d2809d9f5551303a62caa5be3d5364c7ce` |

Both modes passed all five states: **50 alternating pairs / 100 full-check
processes** in total, with distinct recorded PIDs. All samples retained three
complete diagnostics and loaded the same 66 identities. Native performed 238
instantiations per full default-mode check and 164 per single-thread check;
incremental replay performed zero in both modes. The mode difference is observed
work, not a claim about latency or a reason to combine unlike modes. The native
poison replay controls passed. Sources, loaded input fingerprints, binaries, and
full-check build-state snapshots stayed unchanged.

The complete diagnostic fingerprint was
`e1d63ad306915cc23e6ef7c70517cbff06350ef91f3d625ca34f91ef9cf7992c`.
TSR still omits `tsBuildInfoFile` from `showConfig`, and the proof report retains
that difference. It does not silently declare the complete option sets equal.

Local artifacts are `/tmp/tsr-cache-isolation-default-final.json` and
`/tmp/tsr-cache-isolation-single-final.json`, with the retained owned fixture
directories of the same stems. The failed `default-v1` report preserves the
initial whole-message ordering comparison failure; that was corrected before
the successful controls.

The real Next.js application was also run through the existing whole-project
harness for five fresh-process pairs plus warmups. All ten measured samples
recorded distinct PIDs and explicit disabled incremental/composite flags. The
existing `.cache/tsbuildinfo.json` retained its hash, 12,121,322-byte size and
mtime; the default `tsconfig.tsbuildinfo` remained absent. Application inputs
were unchanged. Local evidence is `/tmp/tsr-cache-isolation-nextjs-v1.json` and
`/tmp/tsr-cache-isolation-nextjs-state-v1.json`.

Observed medians were TSR **3.576491 s** and native **3.142558 s**, an observed
ratio of **1.138083**. This remains **incomparable**: TSR loads 13,097 files and
reports 121 diagnostics; native loads 13,098 and reports none. The extra native
file is `@lingui/conf/dist/index.d.ts`, and the report retains the existing
`paths`, `allowImportingTsExtensions`, `disableSourceOfProjectReferenceRedirect`
and `tsBuildInfoFile` configuration serialization differences. The verified
ratio remains null, `target_verified` remains false, and the original target
remains comparable full-project TSR/native median wall **≤0.50**.

Focused verification: 17 Python evidence tests passed, including identical
diagnostics with zero-instantiation replay, skipped sources, altered work counts,
timeouts, changed build state, foreign paths, cached poison and protection of an
existing workspace. No compiler implementation changed; this package makes no
new conformance coverage or speed claim.
