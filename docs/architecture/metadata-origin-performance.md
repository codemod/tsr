# Metadata demand by loader and resolver origin

The diagnostic experiment at `dc16a8dec98771b6bc1a74371b8b0321a2270d86`
locates most remaining disk file probes in relative-import extension lookup.
It does not select a production cache or concurrency change. All probe edits
were restored. The required comparable TSR/tsgo median wall ratio of 0.50 remains
unmet and unverified.

## What was measured

[metadata-origin-counts.json](metadata-origin-counts.json) records two distinct
private-project input snapshots, source/binary/toolchain hashes, full-output
fingerprints, per-origin operation counts, resource observations and public
controls. Private paths, config contents and diagnostic text are omitted.

[metadata-origin-probe.patch](metadata-origin-probe.patch) is a temporary patch
against the measured source. Thread-local scope guards label config discovery,
includes/extends, module/type requests, package scopes, exports/imports, ancestor
walks and extension candidates. They neither change answers nor cache keys.
The guard stays on its creating thread and restores the origin after unwind.
It reaches the compiler through an existing internal dependency; no dependency
or unsafe implementation was added.

Two boundaries are counted separately:

- Logical file/directory queries at `CachedFileSystem`, including reused answers.
- Actual `std::fs::metadata` calls at `OsFileSystem`, with found, wrong-kind,
  not-found and other-error outcomes.

The timer stops after `metadata`, before classification and bookkeeping. The
interval excludes cached lookup, path construction, reads and decoding. Observer
work includes clocks, a mutex, origin-string construction and retained path sets;
these affect CPU, wall and RSS. Per-origin unique sets may overlap; summing them
does not produce global uniqueness. Disk calls also include uncached config
queries, so subtracting disk calls from cached requests is not an exact hit count.

The exported patch includes a later test-only `FileSystem` import repair. The
record distinguishes measured and tested source hashes; rebuilding proved this
repair left the measured CLI binary byte-identical.

## Located demand

These are instrumented intervals from two observations per input snapshot,
not paired normal-binary speed acceptance rounds.

| Input snapshot | Disk file calls / unique paths | Relative extension calls | File metadata intervals |
| --- | ---: | ---: | ---: |
| Initial, 13,101 loaded files | 41,248 / 41,247 | 33,700 | 114.913 / 93.098 ms |
| Confirmation, 13,133 loaded files | 41,881 / 41,880 | 34,186 | 516.919 / 514.168 ms |

Relative extension candidates account for about 82% of disk file calls in both
snapshots. In the initial snapshot, 11,572 of these candidates exist and 22,128
do not. The confirmation has 11,734 existing and 22,452 missing candidates.
These failures are part of native extension fallback; the counts do not authorize
skipping them or replacing exact probes with directory listings.

The initial snapshot has 72,763 cached logical file requests and 78,888 cached
logical directory requests, versus 41,248 disk file and 6,161 disk directory calls.
The confirmation has 73,791 and 79,958 logical requests, versus 41,881 and 6,313
disk calls. This separates substantial existing reuse from the mostly unique
remaining disk requests. A second cache for identical paths does not address the
unique-path demand.

Source-file package scope contributes 2,089 initial disk file probes, of which
only 62 find a package.json. Node-modules extension lookup contributes 2,195
probes, of which 34 succeed. Root/path-reference lookup contributes 1,138
successful probes. Full origin trees and repeated counts are in the JSON rather
than folded into overlapping totals.

The syscall intervals vary substantially across snapshots and observations.
Source and input differences, filesystem state, and activity on the shared host
prevent attributing that variation to one cause. Own builds and processes were
serialized; other agents' host activity was not controlled. Neither the older
roughly 710 ms observation nor the initial roughly 100 ms observation is a fixed
current savings ceiling. No normal wall improvement is claimed from these data.

## Scope and input controls

Each snapshot ran a normal CLI, an independently built scope-only baseline,
the probe with counters disabled, the enabled probe and a repeated enabled probe.
All roles produced the same complete normalized 119 diagnostics within each
snapshot. Effective configuration, full ordered loaded-file sections and physical
input hashes matched across roles. Inputs and root config hashes stayed unchanged
during each set of controls. The snapshots are not combined into a before/after
comparison: loaded scope grew between them despite the diagnostic count remaining
119.

The scope-only baseline and all probe roles preserve ordered actual checking:
1,345 files initially and 1,346 in the confirmation. The checked-input fingerprints
match. The normal binary has no checked-identity marker; its diagnostic and loaded
scope evidence must not be described as direct normal-binary checked identity
telemetry. [metadata-origin-scope.patch](metadata-origin-scope.patch) places the
cheap marker immediately before checking, after the eligibility filters.

The driver now checks the entire loaded-file section, including unexpected extra
files. Initial saved output was independently audited with that stronger check.
A separate three-second startup sample reported 13,102 loaded files and 1,346
checks on another intervening input snapshot. It observed loading only, with
sampler overhead, and is excluded from acceptance and quantitative whole-run
shares. It is not a paired observation of either controlled snapshot.

## Correctness controls

All 49 resolver and 39 VFS release tests pass. The latter include two diagnostic
controls: nested/unwinding scopes restore their caller, and repeated cached
requests are counted separately from a single underlying metadata call.

Supported native trace suites pass 95/95 resolver and 96/96 loader cases with
the probe enabled; both snapshots match the source's committed snapshots byte
for byte. The canonical resolver suite sanitizes compiler version and package-json
cache messages and skips unsupported/no-baseline cases. It does not directly
compare every raw physical failed-lookup result field.

Three public physical controls match complete pinned-native diagnostics:
relative `.js` to `.ts` extension lookup; package exports/imports and a failed
package import; and a symlinked package. Every normal/scope/off/on/repeat role
preserves loaded order, actual probe checking, input/config fingerprints and
non-timing repeated counters. No full checker type/diagnostic corpus was run for
this diagnostic-only package.

## Replay and remaining work

Build and save a normal release CLI at the measured source. Apply the scope-only
patch, build and save that binary, then restore the source. Apply the full probe
patch and build the probe. Use the recorded toolchain and pinned libraries.

```sh
python3 docs/architecture/metadata-origin-controls.py \
  --normal /absolute/path/to/normal-tsr \
  --scope /absolute/path/to/scope-tsr \
  --probe /absolute/path/to/probe-tsr \
  --project /absolute/path/to/tsconfig.json \
  --output /absolute/path/to/new-output-directory

python3 docs/architecture/metadata-origin-public-controls.py \
  --normal /absolute/path/to/normal-tsr \
  --scope /absolute/path/to/scope-tsr \
  --probe /absolute/path/to/probe-tsr \
  --tsgo /absolute/path/to/pinned-tsgo \
  --output /absolute/path/to/another-new-output-directory
```

`tsr-1yb.2.1.3.1` remains in progress for surrounding candidate/path construction
cost, broader raw failed-lookup/casing/cache-warming evidence and a bounded
implementation handoff. Do not short-circuit native extension ordering from
miss counts, infer missing parents, or introduce a scope cache from these data.
Dynamic preparation replay and byte/memory policy remain prerequisites
`tsr-1yb.19.1` and `tsr-1yb.19.2`. A retained implementation must demonstrate
complete work/output, no previously RIGHT losses and independently confirmed
whole-project benefit.
