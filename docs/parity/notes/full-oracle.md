# Full configured native oracle

## Measurement: c8185606

Checker source: `c8185606e3b972d59d345b6e45d789586d993af8`.
Native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The prior `db726c9c` checkpoint is distinct and was not recovered or relabeled.

| Result | Configured rows |
|---|---:|
| Exact ordered diagnostic/type agreement | 7,598 / 14,965 (50.77%) |
| Different ordered records | 6,685 / 14,965 |
| Native failure or deadline | 671 / 14,965 |
| TSR failure or deadline | 11 / 14,965 |

The source corpus contains 12,444 cases. Configuration enumeration uses the
pinned native `GetFileBasedTestConfigurations` and `compilerVaryBy`; aliases,
wildcards, exclusions and cross-products are native operations. When native
cannot enumerate an unsupported source configuration, the source remains one
failed row. Its unenumerable variants are not claimed to have been measured.
Native failures are not clean expectations or exclusions.

Largest **first-difference** clusters (not semantic root-cause certificates):

| Cluster | Rows / 14,965 | Tracking |
|---|---:|---|
| Missing/additional ordered records | 2,358 | `tsr-3at`, P0 |
| Type printer or selection | 1,542 | `tsr-9z5`, P1 |
| Native failure/deadline | 671 | `tsr-5qi`, P1 |
| First differing TS5108 | 537 | parent diagnostic work |
| First differing TS2322 | 515 | parent relation work |
| First differing TS5095 | 132 | parent configuration work |
| First differing TS2339 | 97 | parent member work |
| First differing TS2345 | 85 | parent call/relation work |

No previous RIGHT/vanished archive or saved oracle branch was reachable in this
Box checkout (`git fsck` found no unreachable commits). The former-RIGHT gate
therefore remains **unverified**, not passed. Issue `tsr-2zk.47.3.1` stays open.
The preserved failed machine was not accessed or destroyed.

## Running

```sh
cargo build --release -p tsr-conformance \
  --example full_oracle_actual --example full_oracle_run
target/release/examples/full_oracle_run "$PWD" "$PWD/target/full-oracle-c8185606"
```

The runner deliberately requires the c8185606 checker checkout. Oracle-only
working files are compiled into the two examples without changing shared module
exports. Native Go producers are installed via `go test -overlay`, never by
editing the pinned submodule. Native libraries and corpus support inputs must be
tracked-clean. Native artifact selection uses the actual native baseline walker,
not TSR's chosen nodes or checked-in `.types` baselines.

`full_oracle_actual SOURCE REQUEST OUTPUT` is the real TSR artifact CLI.
Requests supply native-expanded option values only. Actual type sections derive
from actual input units; expected expression/type rows do not choose TSR work.
The existing Program builder, configured Checker, diagnostic filtering,
declaration analysis and type producer are reused. Missing compiler behavior
(including unsupported global/declaration diagnostics) remains observable as a
mismatch; there is no mock or native-output fallback.

Primary records compare file, UTF-16 start and span length, diagnostic code,
category and flattened chained message in native sort order; types compare file,
expression text and printed type in walker order. Related-information bags are
not compared separately. This does not certify emission or every compiler API.

## Receipts and process ownership

Ten workers run at most ten child processes. Each child has file-backed stdout
and stderr and null stdin; parent-owned descriptors close after spawn. A
60-second deadline kills and waits for the child. Every process failure/deadline
has a denominator row. No EMFILE occurred in this full run.

Native resume requires COMPLETE output and a receipt binding native revision,
producer binary SHA-256, source SHA-256, variant and full input-manifest SHA-256.
Only complete equivalent native artifacts are reused. Actual output is always
freshly produced and source/binary bound. Old output is removed before a fresh
attempt, so a failed child cannot inherit a previous COMPLETE marker.

Full receipts/output are in `target/full-oracle-c8185606/` (approximately 1.1 GiB,
ignored by git). **Transfer this directory before destroying the Box; fetching
this branch does not transfer ignored artifacts.** The measurement used:

- Native oracle binary SHA-256:
  `74e3ccc001856f18a533ca2f913398b7e0d21729748f967120071f12cbcba70d`.
- Actual oracle binary SHA-256:
  `afd5df8dd235e6eb090e3b7e06f302b061523e4f892228d6c8e73ecc39434ab7`.
- Input manifest SHA-256:
  `3c5d62305d52908bbd3e66b39003801b5b86273446dbc11c750dbd611f17e847`.

A later source-formatting/receipt-safety rebuild was smoke-tested byte-identical
on `compiler/2dArrays`; it is not relabeled as the measured binary.

## Verification and performance

Actual CLI smoke: `compiler/2dArrays` emitted 21 ordered type rows plus COMPLETE;
its native comparison is exact RIGHT. All conformance library/integration tests
passed. The full legacy suite ran over 12,444 sources: checker types 8,054/9,538,
diagnostics 4,224/5,502. Generated snapshots were restored; no shared checker,
verdict, type producer or suite caller was edited.

The TSR/pinned-native median wall ratio <=0.50 target remains **unmet**. Concurrent
corpus production, native pre/post harness compilation, separately built TSR
programs and reused native timings are not equivalent complete-work samples.
No performance certificate is issued.
