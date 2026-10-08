# Parity round 5 (tsr-2zk)

Dispatched 2026-10-08 after round 4 merged to `main` (PR #6, `ac56208f`), at
baseline `405b55ce`: plain checker_types 8,176/9,538, plain type lines 470,766
RIGHT, diagnostics 4,394/5,502, and configured rows as reported by ADR-0047.
Rules are round 4's (`docs/parity/round4.md`, `docs/parity/box-protocol.md`).
Each box gets one file set no other box touches. Lanes avoid the files main's
local boxes edited in the 36 hours before dispatch: `members.rs` (14 commits),
`contextual.rs` (10), `flow.rs` (9), `check.rs` (6, one-line hook calls only)
and `calls.rs` (5). Changes a lane needs in those files ship as measured diffs.

| Lane | Issue | Owns |
|---|---|---|
| r5-errorsplit2 | `tsr-2zk.960` (`.944`) | intrinsic/error contract, checkIdentifier unresolved arms in `expressions.rs`, writer rewrites in `types_producer.rs`; ADR-0048 |
| r5-report | `tsr-2zk.961` (`.956`, `.918`, `.1.2`) | report-head functions in `assignreport.rs`, the `jsx_component.rs` relation-report call site |
| r5-relater3 | `tsr-2zk.962` (`.927`, `.929`, `.917`) | relation arms in `relater.rs`, `variances.rs` |
| r5-iteration | `tsr-2zk.963` (`.957`) | `iteration.rs` |
| r5-triage2322 | `tsr-2zk.964` | analysis only: `docs/parity/notes/r5-triage2322.md` |
| r5-variants2 | `tsr-2zk.965` | `crates/tsr-conformance` harness |
| r5-typeparams2 | `tsr-2zk.966` (`.901`, `.911`, `.913`) | `declared.rs` type-parameter gathering functions |
| r5-perf4 | `tsr-2zk.967` (`.943`) | `perf_links.rs`, `resolution.rs`, the `get_property_of_type_ex` hook in `members.rs` |
| r5-operators3 | `tsr-2zk.968` (`.941`) | `nullable_operand.rs`, operator files |
| r5-declemit2 | `tsr-2zk.969` | the declaration-emit diagnostic files r4-declemit owned |

Sessions: the integrator's board, `board5.tsv` (one cloud session per lane).

### r5-operators3 finished; r5-intersections dispatched (`tsr-2zk.971`)

r5-operators3 (docs only, a5d6c35) found no operator-side remainder:
- **TS18046 unknown-operand diff:** held. Its one loss comes from an inference
  gap with overloaded callees (`tsr-2zk.970`).
- **in-operand diff:** measured lossless (+1 case). The integrator lands it in
  `assignreport.rs` while r5-report owns other functions there.
- **Other causes:** filed as `.971`-`.973`.

`r5-intersections` takes `.971` and owns `intersections.rs`. Main has not
touched that file in 36 hours.
