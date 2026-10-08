# Round 5 — TS2322 / TS2345 root-cause census (lane r5-triage2322)

Epic `tsr-2zk`. Analysis lane: no checker code. The integrator turns the
ranked buckets below into lanes.

**Status: PARTIAL** — population and mechanical split are final; the
root-cause table is filled in as the slices finish.

## Method

- Tree: `f90fcef` (integration head = `405b55c` + a `main` merge; the
  population below differs from the brief's `405b55c` counts by one case on
  each of two rows, see §1).
- `diagverdictdump` unfiltered; rows kept: `WRONG`/`EMPTY_WRONG`, key without
  `(` (configuration variants excluded); expected/actual
  `BaselineDiagnostic` multisets diffed. 351 cases carry at least one TS2322
  or TS2345 difference.
- `examples/r5census.rs` (added by this lane, instrument only) prints one
  row per differing line: side, position, code, the
  `report_assignability_failure` gate under `TSR_ASSIGN_PROBE=1`
  (`NEVER` = no TS2322 report attempted at that position; `DECLINED` = the
  relation answered `Unknown`; `NOTREPORTABLE`; `REPORTED`), the native
  message chain from the pinned reference baseline, TSR's rendered message,
  and the source line. With `CODES=` empty it covers every code, which is how
  a TS2322 extra is paired with the native diagnostic it displaced (e.g.
  TS2353 at the property).
  ```text
  TSR_ASSIGN_PROBE=1 CODES= CASES=cases.txt cargo run --release -p tsr-conformance --example r5census
  ```
- **Native oracle.** Native tsgo could not be built in this container: the
  pinned `go.mod` asks for the Go 1.26 toolchain and `proxy.golang.org` is
  not in the egress allowlist (`403 Host not in allowlist`). The native
  answer per line is therefore the pinned reference baseline
  (`testdata/baselines/reference/submodule/**.errors.txt`, which *is* native
  output at `5b1047d`) and the native function was identified by reading the
  pinned Go source, not by a debug print. Repros were run through TSR with
  `examples/probefile`.

## 1. Population

| | brief (`405b55c`) | this tree (`f90fcef`) |
|---|---|---|
| plain cases failing on TS2322 alone | 162 | 160 (missing in 118, extra in 54) |
| plain cases failing on TS2345 alone | 57 | 56 (missing in 54, extra in 2) |
| cases with any missing TS2322 | 184 | 184 (423 lines) |
| cases with any extra TS2322 | 117 | 116 (224 lines) |
| cases with any missing TS2345 | 83 | 82 (182 lines) |
| cases with any extra TS2345 | 20 | 20 (35 lines) |
| mixed-code cases touching TS2322/TS2345 | ~380 | 135 (of 1,172 plain WRONG/EMPTY_WRONG) |

The brief's "TS2322 alone: 162" uses the per-code set view; here "alone"
means the multiset difference contains no other code.

## 2. Mechanical split (gate × side)

| side | code | gate | lines | cases |
|---|---|---|---|---|
| EXTRA | 2322 | – | 224 | 116 |
| EXTRA | 2345 | – | 35 | 20 |
| MISS | 2322 | DECLINED (relation `Unknown`) | 161 | 62 |
| MISS | 2322 | NEVER (no report attempted) | 220 | 110 |
| MISS | 2322 | NOTREPORTABLE | 33 | 17 |
| MISS | 2322 | REPORTED (elsewhere) | 8 | 7 |
| MISS | 2345 | (call path; probe n/a) | 182 | 82 |

Of the 259 extra lines, 144 TS2322 + 14 TS2345 have **no** native
diagnostic of any code within three lines (TSR's relation or one of its
operand types is wrong); the rest pair with a native TS2353 (28), TS2741 (21),
TS2739/2740 (8), a TS2322/TS2345 at another column (23), TS2820 (4), …
(reporter or excess-property path).

## 3. Ranked root causes

*(in progress)*
