# Checker parity lanes

Epic `tsr-2zk`: reach 99.9% case parity with pinned tsgo (`vendor/typescript-go`
@ `5b1047d`) on `checker_types` and `diagnostics`, with TSR faster than tsgo.

Baseline at `586c2ec0`, measured by `cargo run --release -p tsr-conformance --bin coverage`:

| suite | passed | rate |
|---|---|---|
| `checker_types` | 7,365/9,538 | 77.22% (lines 464,069/478,855 = 96.91%) |
| `diagnostics` | 2,880/5,488 | 52.48% |

Progress after each integrated lane (same command, on `main`; diagnostics counts
judged RIGHT cases only, EMPTY cases are excluded by the suite):

| merged | `checker_types` | `diagnostics` | smoke ratio (macOS) |
|---|---|---|---|
| baseline `a6afac52` | 7,365 (77.22%) | 2,880 (52.48%) | 1.31 |
| perf `33c8e121` | 7,365 | 2,881 | 1.21 |
| names-modules `9cb129a9` | 7,365 | 2,912 | 1.16 |
| misc-checks `51fcf59f` | 7,365 | 2,948 | 1.16 |
| js `c1f545cd` | 7,366 | 2,954 | 1.17 |
| relate-report `caad9396` | 7,374 | 3,093 | 1.16 |
| decls `b5db8d9f` | 7,374 | 3,196 | 1.18 |
| perf-2 `ed40c6c8` | 7,374 | 3,196 | 1.13 |
| harness `829611e9` | 7,374 | 3,219 | 1.14 |
| parser `ae656118` | 7,464 | 3,318 | 1.18 |
| property `d599c504` | 7,466 | 3,375 | 1.15 |
| iteration-destructure `06f25e03` | 7,488 | 3,427 | 1.20 |
| js-2 (local) `a08cbcf2` | 7,493 | 3,427 | 1.14 |
| misc-2 (local) `8f64006d` | 7,493 | 3,444 | 0.99 |
| js-3 (local) `ab3bb035` | 7,493 | 3,459 | 1.08 |
| cycles (local) `8731aa41` | 7,493 | 3,462 | 1.09 |
| calls + calls-2 (local) `675c4501` | 7,513 | 3,535 | 1.18 |
| flow (local) `d9f5b9f4` | 7,513 | 3,569 | 1.14 |
| calls-3 (local) `a13f5083` | 7,513 | 3,592 | 1.17 |
| cloud round 1 (`#4`, other integrator) `d5b4fe56` | 7,602 | 3,846 | — |
| relate-3 (local) `2fbd279f` | 7,603 | 3,854 | 1.17 |
| calls-4 (local) `b201a70b` | 7,603 | 3,865 | 1.20 |
| relate-3 held fix `4471f456` | 7,607 | 3,876 | 1.07 |
| calls-5 (local) `4290b311` | 7,607 | 3,901 | 1.19 |
| relate-4 (local) `e3ec2563` | 7,607 | 3,926 | 1.30 |
| flow-2 (local) `36c9bb5b` | 7,607 | 3,942 | 1.18 |
| relate-5 (local) `dad4d010` | 7,607 | 3,955 | 1.15 |
| types-writer (local) `2feb57a3` | 7,678 | 3,955 | 1.18 |
| calls-6 (local) `9c0cc090` | 7,679 | 3,967 | 0.99 |
| relate-6 (local) `2396df9b` | 7,679 | 3,971 | 1.15 |
| contextual (local) `0a5d310d` | 7,679 | 3,981 | 0.82 |
| types-writer-2 (local) `e5f61737` | 7,720 | 3,982 | 1.17 |
| property-3 (local) `0fbc6228` | 7,720 | 3,998 | 1.13 |
| contextual-2 prerequisites + PR #5 (other integrator) `48e300a4` | 7,928 | 4,160 | 1.15 |
| types-writer-4 (local) `abbaa1d9` | 7,928 | 4,163 | 1.13 |
| property-4 (local) `e528e745` | 7,928 | 4,171 | 1.16 |
| contextual-3 (local) `ab7f56ce` | 7,930 | 4,173 | 1.15 |
| laziness (local) `d417a8c9` | 7,931 | 4,173 | 1.18 |
| decls (local) `43576bf1` | 7,931 | 4,194 | 1.18 |
| types-misc (local) `50dfae18` | 7,941 | 4,195 | 1.11 |
| laziness-2 (local) `ea1306d3` | 7,944 | 4,195 | 1.24 |
| types-misc-2 (local) `6ccb0d0f` | 7,983 | 4,197 | 1.21 |
| calls-7 (local) `4497370d` | 7,988 (83.75%) | 4,203 (76.39%) | 1.23 |
| measurement `6d55ae54` (2026-10-06) | 8,042 (84.32%) | 4,221 (76.72%) | see representative measurement below |

## Current measurement and exclusive remote ownership

Integration issue `tsr-2zk.47`; pinned native commit
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Fresh release coverage discovered
12,444 compiler/conformance sources. Type assertions: 469,765/478,855
(98.10%); clean diagnostic cases: 4,968/5,068. These remain **coarse,
incomplete oracles**: configuration variants and native divergence cases are
excluded; diagnostic scoring compares file/line/column/code, not complete
message trees, span lengths or ordering. Neither rate certifies the full-corpus
99.9% objective.

Representative `domain-model-large`, macOS arm64, default scheduling,
21 alternating fresh-process pairs: TSR median 0.3886385 s; pinned native
0.1382433 s; observed wall ratio **2.8113**. Loaded scope, effective options,
stable diagnostics and diagnostic fingerprints match. Complete query-input
coverage and actual performed checker work remain unverified; verified ratio
is null and the <=0.50 release target is unmet. Raw capture is local derived
data under `/tmp/tsr-parity-20261006/`, not a portable release receipt.

Ten remote boxes requested from this source revision; ownership is whole-file
exclusive. A provisioning request is not evidence of a running worker.

| box | Beads issue | exclusive subsystem |
|---|---|---|
| `parity-full-corpus` | `tsr-2zk.47` | configuration expansion and full diagnostic/type oracle |
| `parity-alias` | `tsr-2zk.16.2` | declared type bodies, alias/mapper representation, mapped/indexed/union types |
| `parity-calls` | `tsr-2zk.9.7` | calls, signatures, overloads and inference |
| `parity-contextual` | `tsr-2zk.11.5` | contextual typing, object/accessor laziness and binding patterns |
| `parity-symbols` | `tsr-2zk.38` | binder, symbol/module resolution and augmentation publication |
| `parity-parser` | `tsr-2zk.17.1` | parser/scanner and lazy JSDoc demand |
| `parity-printing` | `tsr-2zk.39` | checker state, accessible symbol chains, printing and node reuse |
| `parity-property` | `tsr-2zk.4.12` | properties, member images, inherited receivers and readonly metadata |
| `parity-diagnostic-chains` | `tsr-2zk.22` | diagnostic representation/rendering and relation reporting |
| `parity-performance` | `tsr-2zk.17` | compiler/CLI scheduling and complete-work performance evidence |

Only the integration owner changes shared dispatch (`check.rs`,
`expressions.rs`, `lib.rs`) and `types_producer.rs`. Exported contract changes
are serialized through their single owner, then all callers migrate during
integration. Each commit must preserve all previously RIGHT type assertions
and RIGHT/EMPTY_RIGHT diagnostic cases; full parity and fresh-process A/B
performance are rerun after each serialized merge. Historical lane tables
below retain their original measurement populations, not current rankings.


### Current session dispatch and measurement

Current source `5dd3bad8`: fresh unfiltered coverage remains 8,046/9,538
`checker_types` cases (84.36%) and 4,221/5,502 diagnostic cases (76.72%).
The type verdict dump records 469,796 RIGHT, 7,181 WRONG and 993 GAP aligned
rows; its population differs from the coverage assertion denominator and is
used only for the no-RIGHT-loss transition gate. The exact expanded native
population, complete diagnostic messages/spans/order and independent type
selection remain the `parity-full-corpus` owner's required work.

The current failed-case intersection contains 510 native-operation root groups.
Existing Beads records cover 384; 126 additional bounded roots were created
with native evidence and current target lists. Counts rank blocked scope, not
promised conversions, and can overlap across prerequisite roots.

Ten Box slots were dispatched with the exclusive ownership above. Two machines
failed before agent startup during Rust installation with a null exit status
and a closed exec connection (`tsr-2zk.50`); they were destroyed and replaced by
`parity-calls-r2` and `parity-performance-r2`. Provisioning and setup are not
running-worker evidence. The other workers retain their original names.

Fresh 21-pair `domain-model-large` timing observes a TSR/pinned-native median
wall ratio **2.6621**, with matching scope, options and stable diagnostics.
Sample ranges show concurrent-load effects; complete input and performed-work
proof remain false, `verified_wall_ratio` remains null, and the <=0.50 target
is unmet. Raw current captures are local derived artifacts, not a release
certificate. The stronger oracle and performance work are not optional scope.

First current-session code integration: `f9e8b84f9`, native `trySymbolTable`
local alias exclusions. Isolated committed-parent comparison gains 20 RIGHT
type assertions, loses none, and has no vanished verdict keys; diagnostics
remain unchanged. Full legacy coverage reports 8,051/9,538 type cases and
4,221/5,502 diagnostic cases. This excludes the unrelated local flow patch.
Workspace release tests (3,125), clippy, formatting and 4,497 native anchors
pass. Real script/module UMD controls match native site type printing.
Twenty-one alternating candidate/parent pairs observe wall ratio 0.9739 and
CPU ratio 1.0123; native comparison observes wall ratio 2.6821. Complete-work
verification remains false; neither observation certifies the release target.

The native-only oracle snapshot was rejected: it has no real TSR artifact
producer, retains expected-driven type selection and lacks bounded process
outcome publication. Replacement `parity-full-corpus-r2` owns the entire
conformance producer cutover (`tsr-2zk.47.3`), including the real TSR producer.
Its predecessor is stopped; provisioning remains distinct from running work.


Held from merge: node-reuse (`local/node-reuse-2`, +42 types cases, 348
WRONG→RIGHT lines, zero losses) because A/B child CPU read 1.025 / 1.036
(31 samples) — a hot-path slowdown; its owner is moving the reuse work to
print time.

From `b201a70b` on, each merge is also gated on interleaved A/B child CPU of
the previous `main` binary against the merged one on
`benches/projects/domain-model-large` (15 rounds, diagnostics identical); every
merge above read between 0.95 and 1.014.

From 2026-10-05 ~09:45 PDT the Box service stopped answering (`Not connected`;
new machines did not start), so lanes continued as local workers. Smoke ratios
after that point were taken with load average ~60 on 16 cores and are noise.

Two integrators now merge into `main`: this local one (worktree
`tsr-integ`, lanes run as local workers while the Box service is down) and the
cloud round-1 integrator (`docs/parity/box-protocol.md`). Local lanes in flight
at `2fbd279f`, so the cloud round does not duplicate them: `flow-2` (rebasing
TS7030/2355/2366, getExplicitThisType, TS2448/2449, TS2454 assumeInitialized,
TS2872/2873 onto main), `calls-4` (overload sets `tsr-2zk.9.6`, call-site
TS2344 `tsr-2zk.9.4`/`tsr-2zk.33`, TS2684), `relate-4` (checkAssignmentOperator
and missing elaboration sites, union-target elaboration, weak types TS2559,
TS4104). Beads issues from both integrators are merged in `031d801d`.

The smoke ratio is `perf-project` on `benches/projects/generic-imports`, 9
samples, observed (not verified). `benches/projects/domain-model` reads 1.42 on
macOS and 0.79 on the Linux box after perf-2; the 0.50 target is not met.

`lanes/<lane>.txt` lists every failing case at the commit named in its header
(refreshed at `06f25e0`: 3,442 failing cases, all still inside the lanes below), assigned to exactly
one lane (`T` = fails `checker_types`, `D` = fails `diagnostics`). Assignment:

- A case failing `diagnostics` goes to the lane owning its dominant missing or
  extra code (`jsx`/`salsa`/`jsdoc` directories and `.js` files go to `jsx`/`js`).
- A case failing only `checker_types` goes by the dominant want/got shape of its
  non-RIGHT lines; shapes without a clear owner go to `types-any-triage`, which
  splits them into root causes and reroutes them.

The lists say which tests a lane is judged on, not where the root cause lives.
Each lane owns the source files named in its Beads issue. Hub files
(`check.rs`, `expressions.rs`, `checker.rs`, `lib.rs`) are edited only inside
lane-specific functions or by adding functions; a needed change to a shared helper
or another lane's file is reported to the integrator, who serializes it.

| lane | issue | cases |
|---|---|---|
| relate-report | `tsr-2zk.1` | 696 |
| parser | `tsr-2zk.2` | 401 |
| decls | `tsr-2zk.3` | 301 |
| property | `tsr-2zk.4` | 221 |
| js | `tsr-2zk.5` | 221 |
| names-modules | `tsr-2zk.6` | 287 |
| flow | `tsr-2zk.7` | 124 |
| jsx | `tsr-2zk.8` | 114 |
| calls-inference | `tsr-2zk.9` | 227 |
| destructure | `tsr-2zk.10` | 124 |
| implicit-any-widening | `tsr-2zk.11` | 118 |
| operators | `tsr-2zk.12` | 59 |
| type-operators | `tsr-2zk.13` | 33 |
| contextual | `tsr-2zk.14` | 5 |
| misc-checks | `tsr-2zk.15` | 371 |
| types-any-triage | `tsr-2zk.16` | 692 |

`types-triage.md` splits the 730 types-only failing cases (`types-any-triage`
plus the `type-operators` and `contextual` types failures) into root-cause
clusters ranked by cases each finishes. Port sets and every verified cluster
that finishes at least three cases are filed as `tsr-2zk.16.1`–`tsr-2zk.16.54`,
labelled with the lane that owns the code to change.
