# Resolver candidate construction attribution

Base `181b5e2941268337f66875fad21caf59dd1b9f66`, pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, Rust 1.96.0.
This closes the bounded locating work in `tsr-1yb.2.1.3.1.1` and hands a
candidate to `tsr-1yb.2.1.3.1.2`. No production optimization was selected.
The comparable whole-project TSR/native median wall target remains <=0.50
and unverified. [Aggregate evidence](resolver-construction-performance.json)
records exact source, patch, binary and driver hashes.

The final app controls agree on complete normalized diagnostics (122), ordered
loaded identities (14,015), options and observed input snapshots. The scope-only,
disabled, enabled and repeated variants directly report the same 1,364 checked
identities/order. The pristine normal binary has no checked marker; its loaded
list must not be described as direct performed-checking telemetry.

## What the probe measures

The [temporary patch](resolver-construction-probe.patch) wraps unchanged
constructor expressions. It starts a clock immediately before the expression
and stops it before inspecting the returned shape or recording an origin.
Disabled wrappers execute the expression once. Shape inspection records output
length and capacity without cloning it. Two controls verify single execution
and preservation of string/non-string results.

Constructor rows cover request keys, conditions, relative normalization,
ancestor traversal, extension/suffix filenames, package paths and export/import
processing. These constructors contain no nested observer timers or filesystem
calls; their intervals can be summed within one process. Inclusive resolver,
package-info and export intervals contain child operations and observer work.
Never add them to constructor or metadata intervals. Metadata syscall intervals
remain a separate decomposition, not another additive historical savings claim.

The final enabled/repeat samples observe 123.079/124.476 ms across instrumented
constructors, with 61,622,699 cumulative returned payload bytes. These are output
sizes across calls, not allocation counts, actual copied bytes, live retained
storage or RSS. Vector capacity includes its element slots and returned string
capacities. The typed canonical `Path` key exposes length but no capacity:
`package_cache_key` capacity zero is an unavailable placeholder. Internal path
component allocations and allocations inside normalization remain uncounted.

| boundary | calls | enabled ms | repeat ms | output payload bytes |
|---|---:|---:|---:|---:|
| relative normalization | 20,111 | 41.637 | 42.101 | 1,956,869 |
| canonical package key | 160,229 | 24.129 | 24.294 | 12,966,660 |
| package.json filename | 160,229 | 23.466 | 23.797 | 12,966,672 |
| extension filename | 70,525 | 6.877 | 6.996 | 6,602,128 |
| node_modules folder | 23,751 | 3.472 | 3.567 | 1,566,755 |

Earlier initial-probe samples locate relative normalization at 39.655–43.749 ms.
Their constructor counts/output sizes match the final probe, and their loaded
sources/root config/full diagnostics match. They lack the final queried-input
controls and have different source/binary identities. Keep these observations
distinct. The aggregate preserves each generation and its resource samples.

## Observer and input limits

These fresh processes are serialized attribution controls, not alternating
normal baseline/candidate performance pairs. The final normal/scope/disabled
wall observations are 4.191/4.156/4.262 s; enabled/repeat are 4.843/4.788 s.
Enabled CPU is about 4.056–4.068 s user plus 0.604–0.607 s system; peak RSS is
1,260–1,262 MB, versus 1,123–1,184 MB across normal/scope/disabled observations.
The enabled probe has material overhead. These independent peaks cannot be
subtracted to establish retained observer storage, and these runs do not isolate
a causal overhead percentage or normal saved-wall ceiling.

An untimed private preflight exports observed file/directory metadata query
paths. The driver fingerprints their kind, file bytes and realpath plus loaded
sources/root config before/after each configuration and full-check process.
Enabled/repeat query sets equal the preflight. There are 51,018 paths: 15,407
files, 3,122 directories and 32,489 missing candidates. Raw paths, manifests,
effective app configuration and diagnostics remain in local scratch files.

Four [mutation controls](test_resolver_construction_controls.py) detect manifest
byte changes, creation/kind changes for failed candidates, ancestor symlink
retargeting with identical file bytes, and unexpected files appended after
diagnostics. Directory contents, unobserved reads/environment and changes that
occur and revert between snapshots are not covered. Snapshotting warms the OS
cache outside timed processes; this is not cold-cache evidence. Full input
equivalence remains unproved; `tsr-1yb.1.2.1` owns the broader permanent gate.

## Native controls and quality

Seven physical projects run normal/scope/off/on/repeat TSR controls and both
native default/single modes. All TSR variants preserve complete output and
loaded/checked scope plus observed queried-input snapshots. Twelve of fourteen
native diagnostic comparisons match. The remaining pair deliberately exposes
existing `tsr-6.59`: TSR discards the empty module suffix, misses `dep.ts` and
reports TS2307 while native reports TS2322. Trace-forwarding diagnostics match,
but native emits the requested CLI resolution trace and TSR does not
(`tsr-1yb.1.1.1`). Neither failure is normalized away or called parity.

Controls include exports/imports and ancestor package scope, failed packages and
relative lookups, extension substitution, suffix ordering and symlink packages.
Abstract resolver/VFS tests cover case sensitivity and cache warming. Supported
canonical native suites pass 95 resolver and 96 loader cases, with committed
snapshots byte-identical. These sanitized suites do not establish every raw
failed-lookup/trace field or correct CLI trace forwarding. Unsupported corpus
cases are excluded by those suites rather than passed.

The final temporary source passes 49 module and 41 VFS release tests, strict
Clippy for both crates/tests and workspace formatting. The four Python mutation
controls pass. Full checker assertion/diagnostic corpus scores were not rerun:
main contains only reproduction/evidence documents and drivers. Any production
candidate must run those fidelity gates and preserve previously RIGHT results.

## Candidate handoff

`normalize_path_for_cjs_resolution` in `crates/tsr-module/src/util.rs` constructs
owned `Vec<String>` path components solely to inspect the last component for
`.`/`..`. The next candidate tests a borrowed inspection after the root boundary,
preserving `combine_paths`, normalization and native trailing-directory slash
behavior. A slash split alone is unsafe for URI authorities and UNC roots that
end in a dot. Characterize native empty/Unicode, trailing/repeated separator,
backslash, drive-relative/absolute, UNC and URI root/body cases first.

This avoids a specific component-vector construction rather than another cache.
Expected saved normal whole-project wall time is unknown. The ~42 ms helper
interval is locating evidence, not an allocation saving or retention verdict.
Retain a change only after complete scope/output/input controls, native fidelity,
no previously RIGHT losses and two independently confirmed serialized paired
whole-project rounds under the existing 20 ms retention gate. Otherwise restore
it. Broad unique-metadata attribution remains `tsr-1yb.2.1.3.1`.

## Reproduction

Use a separate checkout at the recorded base. Build pristine `normal`; apply
[scope patch](metadata-origin-scope.patch), build `scope`, and reverse it. Apply
the construction patch and build `probe`. Preserve each binary before replacing
the target output. Provide the pinned bundled libraries at the driver's
repository `vendor/typescript-go` path. Keep build and measurement processes
serial; use `CARGO_BUILD_JOBS=1` and `RAYON_NUM_THREADS=1` for canonical suites.

```sh
python3 docs/architecture/test_resolver_construction_controls.py
python3 docs/architecture/resolver-construction-controls.py \
  --normal /tmp/normal --scope /tmp/scope --probe /tmp/probe \
  --project /absolute/project/tsconfig.json --output /tmp/new-construction-run
python3 docs/architecture/resolver-construction-public-controls.py \
  --normal /tmp/normal --scope /tmp/scope --probe /tmp/probe \
  --tsgo /tmp/pinned-tsgo --output /tmp/new-construction-public
TSR_META_PROFILE=1 cargo test --release -p tsr-module -p tsr-vfs
TSR_META_PROFILE=1 RAYON_NUM_THREADS=1 cargo run --release -p tsr-conformance \
  -- module_resolution file_loader
```

The output directory must be new. Drivers verify critical JSON writes and retain
each completed control before starting another. Their raw outputs contain private
paths; publish only reviewed aggregates. The probe is diagnostic-only and must
be removed before normal timing acceptance or production delivery.
