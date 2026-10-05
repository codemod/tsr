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
| calls + calls-2 (local) `675c4501` | 7,513 (78.77%) | 3,535 (64.41%) | 1.18 |

From 2026-10-05 ~09:45 PDT the Box service stopped answering (`Not connected`;
new machines did not start), so lanes continued as local workers. Smoke ratios
after that point were taken with load average ~60 on 16 cores and are noise.

The smoke ratio is `perf-project` on `benches/projects/generic-imports`, 9
samples, observed (not verified). `benches/projects/domain-model` reads 1.42 on
macOS and 0.79 on the Linux box after perf-2; the 0.50 target is not met.

`lanes/<lane>.txt` lists every failing case at that commit, assigned to exactly
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
