# Real-file I/O attribution and loader ownership

Source `a7901196865ecd82e04bd685490a5cfb0c31fe01` was measured in an isolated
worktree. No production code change was retained. The evidence supports settling
file preparation ownership before implementing parallel loading; it does not
establish a speed improvement or the required TSR/native median ratio of 0.50.

## Existing cache boundary

The CLI already constructs `CachedFileSystem` in
`crates/tsr-execute/src/compile.rs`. Its separate file-existence,
directory-existence, directory-entry and realpath caches are in
`crates/tsr-vfs/src/cached.rs`. Reads pass through to the underlying filesystem.
The pinned native compiler host uses the corresponding cached filesystem in
`internal/execute/tsc.go`, `internal/compiler/host.go` and
`internal/vfs/cachedvfs/cachedvfs.go`.

Independent metadata queries and live reads are part of that boundary. Adding a
second cache, inferring negative parent results, or replacing queries with a
directory listing needs semantic evidence beyond these counts.

## Source and controls

[loader-io-counts.json](loader-io-counts.json) contains source and binary hashes,
options, scope and input fingerprints, resource observations and public controls.
[loader-io-probe.patch](loader-io-probe.patch) is a temporary diagnostic patch
against the measured source. It was replayed successfully against exact source
bytes and removed from the worktree after measurement.

The normal binary SHA-256 is
`73f964393383a635e91cd944dd7e1acc1da67f539871cd78ee03bdc1da36c590`;
the probe binary is
`2b76dbbb8a18ac377017dd454c60a9dfd9af1470a627cd8a83e630d59042ad43`.
The native encoding oracle is pinned to
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

The real project used fresh processes with `--noEmit --incremental false
--composite false --pretty false`. Normal, counters-disabled, counters-enabled
and repeated enabled runs preserved all 118 normalized diagnostics. Separate
`--listFiles` controls preserved the ordered 13,097 loaded files and their input
fingerprint across all four roles. Independent checked-scope telemetry preserved
the ordered 1,341 actually checked files and input fingerprint across disabled,
enabled and repeated probe runs. The normal binary has no checked-scope marker;
its checked count was verified with extended diagnostics. This is not a claim of
normal-binary checked identity telemetry.

The root config hash remained unchanged. Effective configuration was captured
after the initial counts, and its fingerprint is recorded; this was not a
pre-run snapshot. Private app diagnostics, config contents and file lists remain
local; the checked-in evidence contains counts and fingerprints.

## Located costs

These intervals are from two instrumented observations, not paired normal-binary
timing results. All non-timing counters repeated exactly.

| Operation | Calls / unique paths | First interval | Repeat interval |
| --- | ---: | ---: | ---: |
| File metadata | 41,242 / 41,241 | 718.31 ms | 711.26 ms |
| Directory metadata | 6,161 / 6,158 | 40.03 ms | 43.27 ms |
| Realpath method body | 1,233 / 1,233 | 13.07 ms | 12.75 ms |
| Directory-entry method body | 222 / 222 | 10.09 ms | 10.91 ms |
| Successful file reads | 14,217 / 14,216 | 157.48 ms | 152.61 ms |
| Byte decoder helper | 14,217 | 23.95 ms | 23.79 ms |

All reads succeeded and returned 77,124,938 bytes. Every app read used the plain
UTF-8 branch; four reads were empty. The decoder's borrowed UTF-8 conversion
logically copied those bytes into owned strings. This is cumulative copy volume,
not allocator event counts or retained heap. The helper interval ends before
destruction of the input byte vector, so it is not a strict upper bound on all
savings from an owned-buffer conversion.

File and directory metadata timers cover individual `std::fs::metadata` calls.
Read and decoder timers are disjoint. Realpath and directory-entry timers cover
their full method bodies, including internal work. Atomic counters, clocks,
path sets and extra encoding classification add observer overhead; measured RSS
includes that overhead. Neither these intervals nor probe wall times establish
saved normal wall time.

One subsequent normal extended-diagnostics run reported loader 3.375 s, file
read 1.512 s, parse 0.584 s, discovery 1.166 s, bind 0.167 s and check 2.086 s.
Resolver 1.061 s is included in discovery and must not be added again. The large
read difference from the instrumented observations illustrates host variation
and measurement-boundary limits. This single locator does not establish a
concurrency benefit or a precise savings forecast.

## Physical encoding checks

[loader-io-controls.py](loader-io-controls.py) creates eight fixed physical byte
fixtures and runs normal, disabled, enabled, repeated and pinned native binaries
serially. All eight complete normalized diagnostic comparisons matched native
across 40 processes: UTF-8, UTF-8 BOM, UTF-16 LE/BE, odd UTF-16 trailing byte,
lone UTF-16 surrogate, malformed UTF-8 and empty file. Probe checked-root/input
controls and repeated non-timing counters matched. Surrogate and malformed
UTF-8 bytes occur in comments; this does not prove all malformed-byte span or
source representation semantics. Native Go strings can retain invalid UTF-8,
whereas the current Rust decoder replaces it.

The restored source passed all 37 release VFS tests, including existing decoding,
read errors, cache clearing/disabling, live-read and symlink mutation controls.
Full resolution-field/trace comparisons and full type corpora were not rerun for
this diagnostic-only evidence package.

To replay the public controls, build normal and temporary-probe binaries from
the recorded source and provide the pinned native executable:

```sh
python3 docs/architecture/loader-io-controls.py \
  --normal /absolute/path/to/normal-tsr \
  --probe /absolute/path/to/probe-tsr \
  --tsgo /absolute/path/to/pinned-tsgo \
  --output /absolute/path/to/new-output-directory
```

The output directory must not already exist. This runner is a correctness
control, not a timing gate.

## Task handoff

`tsr-1yb.2.1.3` retains the remaining attribution work: broader native resolution
controls, complete allocation/destruction boundaries and a defensible normal
benefit estimate. Its parent `tsr-1yb.2.1` remains in progress. No decoder or cache
implementation was selected from this evidence.

New P1 `tsr-1yb.19` settles file preparation ownership and deterministic replay
before production loader concurrency in `tsr-1yb.5`. The current
`FileLoader` borrows a caller-owned arena and uses one mutable `NodeTable` and
`NodeMap`. Parser `parse_into` shares those structures; dense node IDs derive
from table indices. A frozen program's checker sharing contract does not prove
mutable parsing, arena or host sharing safe.

The task audits native `filesParser.start` and `getProcessedFiles`, package
identity, claiming and trace replay; chooses an independent preparation unit;
and proves lifetime, node-domain, parent/flags/JSDoc, host-cache and deterministic
merge behavior in a small compiling seam or records an evidence-backed refusal.
It is independently ready. Production loader work retains its existing worker
ownership and measurement gates, with `.19` added as a prerequisite. No unsafe
allocator sharing, implicit ID crossing or unmeasured large ID reservation is
authorized. Retained production changes still require full-project benefit and
no previously RIGHT corpus losses.
