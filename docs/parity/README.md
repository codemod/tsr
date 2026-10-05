# Checker parity lanes

Epic `tsr-2zk`: reach 99.9% case parity with pinned tsgo (`vendor/typescript-go`
@ `5b1047d`) on `checker_types` and `diagnostics`, with TSR faster than tsgo.

Baseline at `586c2ec0`, measured by `cargo run --release -p tsr-conformance --bin coverage`:

| suite | passed | rate |
|---|---|---|
| `checker_types` | 7,365/9,538 | 77.22% (lines 464,069/478,855 = 96.91%) |
| `diagnostics` | 2,880/5,488 | 52.48% |

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
