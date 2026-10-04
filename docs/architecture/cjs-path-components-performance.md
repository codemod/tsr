# Borrowed CJS path-component inspection

Base `9c176c03edc550045e9d4d266f215b4bb7ccf541`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, Rust 1.96.0, macOS arm64.
`tsr-1yb.2.1.3.1.2` removes owned component construction from
`normalize_path_for_cjs_resolution`. The normal whole-project improvement is
confirmed in two five-pair rounds. This is an isolated TSR improvement; the
comparable TSR/pinned-native median ratio <=0.50 remains unverified.
[Evidence](cjs-path-components-performance.json) retains every sample,
source/binary identity, scope fingerprint and auxiliary retention receipt.

## Native boundary and implementation

Native `internal/module/resolver.go:2048` combines the containing directory and
specifier, calls `GetPathComponents`, and checks the final component for `.` or
`..` before normalizing. `internal/tspath/path.go:133` keeps the root separate
and removes exactly one trailing empty component.

TSR now borrows the body after `get_root_length`, removes one trailing slash
and inspects its final segment. It preserves the existing combine/normalize
operations, owned return type and native trailing-directory separator rule.
URI/UNC roots stay separate from body components; repeated trailing separators
are not indiscriminately trimmed. No cache key, resolver probe order, tracing,
worker or checker storage changes are involved.

The pinned native helper was copied exactly into a scratch Go program with
unchanged `tspath`/`stringutil` package files, compiled using existing Go 1.26.0
with network access disabled. Native source hashes are retained. Native and both
Rust helpers agree on 45,852 deterministic path pairs, including prior seeded
normalization controls and 20,000 generated pairs with seed 196572. The measured
input/output hashes distinguish this set from a newly generated subset.
Nineteen native golden controls are committed with the helper: empty/relative,
dot/parent, repeated/trailing separators, backslash, drive, UNC, URI authority,
file URL, untitled and Unicode boundaries.

The earlier [constructor locator](resolver-construction-performance.md) observed
~42 ms in 20,111 relative-normalization calls. That interval selected a candidate;
it was not a predicted normal saving. Returned payload sizes did not count the
internal component-vector allocations. The retained result below comes from
normal executables and includes the entire CLI.

## Whole-project result

Fresh processes use `--project`, `--noEmit`, `--incremental false`,
`--composite false`, `--pretty false`, and `--extendedDiagnostics` on the local
Next.js project. Builds, setup, input fingerprints and untimed controls are
outside timed intervals. Each round alternates five baseline/candidate pairs;
round two reverses the starting order. All 20 measured PIDs are distinct.

| round | median wall baseline → candidate | saved wall | gain | median user CPU baseline → candidate |
|---|---:|---:|---:|---:|
| 1 | 4.193297 → 4.149874 s | 43.423 ms | 1.04% | 3.646543 → 3.633852 s |
| 2 | 4.192729 → 4.128836 s | 63.893 ms | 1.52% | 3.674301 → 3.612261 s |

Both rounds exceed the unchanged 20 ms isolated retention gate; `decide.mjs`
returns keep/none for each auxiliary wall-time receipt. Those receipts use a
separate isolated wall metric, leave the fixed native-ratio spec unchanged,
and do not establish release-target completion. RSS medians overlap and vary:
1.175→1.175 GB in round one, 1.124→1.158 GB in round two. There is no retained
memory or RSS improvement claim. Other agents' host activity was not controlled;
all samples and CPU/system/RSS observations remain in the evidence.

Every sample retains 122 complete normalized diagnostics, 14,015 loaded files,
1,364 checked files and 14,617 parsed files. Ordered loaded lists and options
match before/after timing. Separate baseline/candidate scope binaries directly
report the same 1,364 file-check worker entries in the same order. Their loaded
identities and complete diagnostic fingerprints also match. The scope patch is
removed, and the rebuilt normal CLI matches the timed candidate byte-for-byte.

Observed input snapshots cover 51,018 loaded/root-config/metadata-query paths,
checking kinds, file bytes and realpaths before/after each timed process. The
seed snapshot came from equivalent pristine production source in the preceding
locator. Directory entries, unobserved reads/environment and changes that revert
between snapshots remain outside coverage (`tsr-1yb.1.2.1`). Fingerprinting warms
the OS cache; these are warmed-input measurements, not cold-cache evidence or
complete input-equivalence proof. No native throughput ratio is inferred.

## Fidelity and quality

Unfiltered same-source assertion dumps are byte-identical: 474,251 rows,
460,045 RIGHT, 2,094 GAP and 12,112 WRONG. No previously RIGHT losses.
Assertion SHA-256:
`20517b7b532d890a5b8f31e9315058c8114ca731780c70a6136d9e9e0e5a5e94`.

All 10,570 eligible diagnostic cases are byte-identical: 2,856 RIGHT,
4,804 EMPTY_RIGHT, 2,632 WRONG and 278 EMPTY_WRONG. Diagnostic dump SHA-256:
`96a082ea8cddf0c2eb7b1ec3a0fb3cfa290540b3798cd306e1f1ea238944f781`.
The existing suite excludes varied configurations, known divergences and absent
baselines; it compares sorted diagnostic occurrences rather than full messages.
The app comparison separately preserves complete normalized messages.

Affected-crate release tests pass 204 cases, with one existing ignored test;
this includes 50 module, 22 path and 37 VFS tests. Strict module all-target
Clippy and workspace formatting pass. Supported canonical suites remain
95/95 resolver and 96/96 loader with committed snapshots byte-identical.
These sanitized controls do not close existing CLI trace-forwarding
`tsr-1yb.1.1.1` or empty-suffix `tsr-6.59` gaps.

## Reproduction

Build normal baseline/candidate binaries from the recorded source states with
`CARGO_BUILD_JOBS=1`. Run corpus comparisons without `TSR_FILTER` and with
`RAYON_NUM_THREADS=1`, using `verdictdump` and `diagverdictdump`. Compare entire
TSV bytes; split records on literal newline rather than Unicode line separators.
The golden tests and native source anchors retain the portable boundary.

The [general pair driver](cjs-path-components-pairs.py) takes explicit binary,
project and private preflight-manifest paths:

```sh
python3 docs/architecture/cjs-path-components-pairs.py \
  --baseline /tmp/baseline --candidate /tmp/candidate \
  --project /absolute/project/tsconfig.json \
  --input-manifest /tmp/private-input-manifest.json --output /tmp/new-pairs
```

Supply a fresh manifest from the construction locator and include all loaded
sources/root config. The output directory must be new. Raw output includes
private paths; publish only reviewed aggregates. The measured driver was the
scratch predecessor; the generalized exported driver has a distinct hash and
its public smoke replay verifies plumbing rather than representative throughput.
