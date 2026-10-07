# Full configured native oracle

## Ordered related-information contract: completed measurement

The stricter oracle completed all 14,965 configured rows on the same frozen
c8185606 checker: **7,211/14,965 exact RIGHT (48.185767%)**, 7,072 WRONG,
671 native failures/deadlines, 11 actual failures/deadlines, **0 missing rows**.
Compared with the primary-contract checkpoint: 387 of its 7,598 RIGHT rows are
not RIGHT under the expanded contract; **0 primary diagnostic/type losses**.
These 387 are newly exposed detail mismatches, not a checker regression or a
relabeled primary-contract rate. Prior archive before c818 remains unavailable.

The artifact sequence now compares head metadata, ordered recursive message
chains and ordered recursive related-information records. Each detail includes
list position/path, file, UTF-16 start/length, code, category, own localized
message and flags (unnecessary, deprecated, skipped-on-no-emit). No sorting or
bag/set reduction is applied to chain/related rows. Missing detail source text
fails the producer rather than inventing a location. Native operations:
`Diagnostic.MessageChain`, `RelatedInformation`, `Localize` at pinned 5b1047d;
producer ownership is per native compilation / TSR Program, with no added
semantic cache. Completion is published only after all ordered rows are emitted.

Largest first-difference groups: missing/additional records 2,334; type
printing/selection 1,498; TS5108 537; **related information 493**; TS2322 488;
TS5095 132; TS2339 93; TS2345 77; **chain structure 46**. These remain
first-difference attribution, not proven semantic root causes.

Full receipts: `target/full-oracle-c8185606-related/` (ignored; copy before Box
destruction). All workers use immutable content-addressed copies.

- Native binary: `a2ad1af8c6a364365cb22c7ca36356ff07dfd59aa775602b5c7fe8fb6fb66555`
- Actual binary: `4d5a1afe2da4399a4851372a8105c6c170cd39d70ab2132e898b4798322b9ad9`
- Manifest: `eb65eec9185759a098264aa811cba809e6bfac9d7d69d868e728f7753da89ea6`
- Results: `3827df21256c7b35f47b7452292b0b13fa19dc9ce4ba28e25c3e3115f4356a8a`
- Summary: `696689eae29d1c91412c1062bd2bae8969a603736a6a91dbdacb116fd8cc7587`
- Input manifest unchanged: `3c5d62305d52908bbd3e66b39003801b5b86273446dbc11c750dbd611f17e847`

Actual CLI exercised every supported configured case. Positive related-info
control `00050` (`abstractPropertyInConstructor.ts`) emits ordered TS2728
`'prop' is declared here.` at UTF-16 start 479, length 4, category 3, attached
to TS2729. All conformance library/integration and example tests passed.
Performance <=0.50 remains unmet; this stricter correctness measurement is not
a performance certificate and does not measure newer parent main.

## Published completed r2/r3 receipts (primary contract)

Both completed runs report source `c8185606e3b972d59d345b6e45d789586d993af8`,
native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`, RIGHT 7,598, TOTAL 14,965
(**50.771801%**), PRIOR_RIGHT 7,598, PRIOR_RIGHT_LOSSES 0, MISSING_ROWS 0.
These are primary diagnostics/types results, **not related-information parity**.
Native failure/deadline rows (671) and actual failure/deadline rows (11) stay in
the denominator. New parent main is not included.

r2 manifest SHA-256: `48229a30f9516a166069f635aa770422d5e6473d340940f5a11c446695373ee5`.
r2 results SHA-256: `85c3be4ef82a10d194a1f780e0890e9ee610c55dec60fef644d94973d4dc8675`.
r2/r3 summary SHA-256: `b14ccab91dbf03090780d8006c01926e990eb7c43c62b72211fd69b91fc87e8a`.

Direct f32 command, independent of corpus production:

```sh
TSR_ORACLE_MODE=actual \
TSR_ORACLE_CASE="$PWD/vendor/typescript-go/_submodules/TypeScript/tests/cases/conformance/types/typeParameters/typeArgumentLists/instantiationExpressions.ts" \
TSR_ORACLE_VARIANT='' \
TSR_ORACLE_OUTPUT="$PWD/target/native-f32-control/native.tsv" \
GOMAXPROCS=1 \
target/full-oracle-c8185606-r3/worker-ce73346778b98c91f3d042ad052ed9dd82fca0b602e9111b7598e8817fa69936 \
-test.run='^TestFullOracle$'
```

Observed output: PASS; 286 type rows; 16 diagnostics; COMPLETE. Native rows
202/203 are exactly:

```text
fs : ((a: string) => string) | { x: string; }
f<string> : ((a: string) => string) | { x: string; }
```

Input SHA-256 `86cdd089559185d1d1df073e20c9a1bc57c15c75fba6faac5502354a3ff50235`;
worker SHA-256 `ce73346778b98c91f3d042ad052ed9dd82fca0b602e9111b7598e8817fa69936`;
artifact SHA-256 `594197cc6af8083070ed47a53d8bfdd33c73e3c9a5601e4548f92742ce8093ed`.
Configuration: target es2015, strict true, declaration true. Real native
`tsbaseline` walker and type query/printing; not the stale bundled baseline.

## Immutable-worker rerun (r3)

The full c8185606 rerun in `target/full-oracle-c8185606-r3/` completed with
**7,598/14,965 RIGHT**, 6,685 WRONG, 671 native failures/deadlines and 11 TSR
failures/deadlines. Prior r2 RIGHT retention: **0/7,598 losses; 0 missing rows**.
Workers execute read-only content-addressed **copies**, not build paths or hard
links; rebuilding examples cannot change the executable used by a running
measurement. `producer-sources.tsv` records producer source hashes.

- Native binary: `ce73346778b98c91f3d042ad052ed9dd82fca0b602e9111b7598e8817fa69936`
- Actual binary: `7f644d0e4467e2260b19eb9f95fe7722e690f1dc1859d458a29dd5618d733594`
- Manifest: `0f3465e5ae6ef2884ad6b63d925486fc7332909bb417b6a785cfc24d007c2cde`
- Results: `313a4e7b92935d12d52a19e851f813c33c15b9dadc98489ad6c85a54abfbf0dc`
- Summary: `b14ccab91dbf03090780d8006c01926e990eb7c43c62b72211fd69b91fc87e8a`

Fresh full native `instantiationExpressions.ts` output (target es2015, strict
true, declaration true) contains 286 type rows and 16 diagnostics. Rows 202/203
are `fs` and `f<string>`, both printing
`((a: string) => string) | { x: string; }`. The bundled reversed ordering is not
current-pin authority; no comparator was changed to fit it. Full receipt:
`target/native-instantiationExpressions-currentpin/receipt.tsv`; artifact SHA-256
`594197cc6af8083070ed47a53d8bfdd33c73e3c9a5601e4548f92742ce8093ed`, source
SHA-256 `86cdd089559185d1d1df073e20c9a1bc57c15c75fba6faac5502354a3ff50235`.


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

The fresh r2 run reproduced 7,598/14,965 exact RIGHT. Its prior gate compares
all 7,598 RIGHT rows from the first c8185606 run: **0 losses, 0 missing rows**.
The input/configuration manifest matches exactly. No older db726c9c RIGHT archive
or saved oracle branch is reachable; that older gate remains unverified.
The preserved failed machine was not accessed or destroyed.

## Running

```sh
cargo build --release -p tsr-conformance \
  --example full_oracle_actual --example full_oracle_run
target/release/examples/full_oracle_run "$PWD" "$PWD/target/full-oracle-c8185606-r2" \
  "$PWD/target/full-oracle-c8185606"
```

The runner verifies compiler inputs against c8185606 with a git diff gate;
only oracle-owned files, this document and Beads metadata may differ. Oracle-only
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
expression text and printed type in walker order. For the historical primary-contract r2/r3 measurements, related-information
bags were not compared separately. The newer ordered-detail contract above
compares them. Neither contract certifies emission or every compiler API.

## Receipts and process ownership

Ten workers run at most ten child processes. Each child has file-backed stdout
and stderr and null stdin; parent-owned descriptors close after spawn. A
60-second deadline kills and waits for the child. Every process failure/deadline
has a denominator row. No EMFILE occurred in this full run.

Native resume requires COMPLETE output and a receipt binding native revision,
producer binary SHA-256, source SHA-256, variant, full input-manifest SHA-256 and
artifact SHA-256. Actual receipts also bind request and output artifact hashes.
Only complete equivalent native artifacts are reused. Actual output is always
freshly produced and source/binary bound. Old output is removed before a fresh
attempt, so a failed child cannot inherit a previous COMPLETE marker.

Full fresh receipts/output are in `target/full-oracle-c8185606-r2/` (approximately 1.1 GiB,
ignored by git). **Transfer this directory before destroying the Box; fetching
this branch does not transfer ignored artifacts.** The measurement used:

- Native oracle binary SHA-256:
  `ce73346778b98c91f3d042ad052ed9dd82fca0b602e9111b7598e8817fa69936`.
- Actual oracle binary SHA-256:
  `d34db4feb4198898af6457055fe7abb01fa30a4d132f034a8e65e217c7f4700d`.
- Input manifest SHA-256:
  `3c5d62305d52908bbd3e66b39003801b5b86273446dbc11c750dbd611f17e847`.

Durable report SHA-256 identities:

- `manifest.tsv`: `48229a30f9516a166069f635aa770422d5e6473d340940f5a11c446695373ee5`
- `results.tsv`: `85c3be4ef82a10d194a1f780e0890e9ee610c55dec60fef644d94973d4dc8675`
- `prior-right.tsv`: `3880a9603a5280bd0e3d13e26927009c2f9d0316419a5b7c667cd4c637d3a3bd`
- `summary.tsv`: `b14ccab91dbf03090780d8006c01926e990eb7c43c62b72211fd69b91fc87e8a`

`results.tsv` enumerates every source/configuration result. The prior RIGHT
ledger binds both prior artifacts. The previous c8185606 run remains in its own
directory and retains its original binary identities; no checkpoint is relabeled.

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
