# Reference-spelling retention candidate

`tsr-1yb.16.3.3.1` qualifies one isolated six-line candidate against Rust
`a49a171a5dbae69caf6f51355edd1b88e18ae209`, native `5b1047d` and nested
TypeScript fixtures `4d4f005`. The private candidate commit is
`2fd26c80bd0f57cedac41e1d21c5bfa501de35c8`. The
[patch](reference-preflight.patch) is a proposal; this prototype delivery
does not change canonical checker code or claim saved wall time.

## Change and boundary

In `get_instantiated_type_reference`, resolve every explicit argument first.
Then prepare spelling only for a partially written list or an argument NodeId
already present in the private checker's `qualified_written_text`. The guard
precedes allocation of the base name, vector and argument strings. Bare
registration, errors, the entire retained loop and ordered dependent defaults
remain unchanged.

The [written-node contract](reference-spelling-reuse.md) proves this exact
predicate against the current read-only Rust formatter. Native rendering can
instantiate types; this guard does not authorize skipping native rendering or
changing a future formatter with semantic callbacks. No mapper, TypeStore,
allocator or scheduler change is part of the candidate.

## Fidelity and actual work

The refreshed baseline and candidate complete unfiltered `verdictdump` and
`diagverdictdump`. Both outputs are byte-identical. The type summary contains
475,538 unique assertion identities: 464,093 RIGHT, 1,604 GAP, 9,841 WRONG;
zero previously RIGHT assertions are lost. Physical type output has 475,600
lines because some rendered assertions span lines; line count is not the
assertion denominator.

All 55 candidate oracle children preserve the baseline inputs, native outputs,
Rust displays and query-order observations. All 44 ordinary public CLI children
preserve normalized diagnostics, reported counts and loaded-file order in both
worker modes. Existing `tsr-6.23.1`, `tsr-6.67` and `tsr-6.66` defects remain
explicit: qualified defaults still produce two TSR TS2322 errors versus native
acceptance. These controls do not establish complete cross-tool performed work.

The archived bad mutations still create seven/five new native mismatches. All
22 fixture children finish, and exact candidate source and helper-binary
restoration is verified after rebuilding. Required written spelling is protected.

The existing [allocation observer](reference-preparation.md) is reused in a
separate archive, with its original hooks, meter and reader. Four current-source
baseline/candidate children have identical 1,397 directly checked identities;
actual admitted/private/checking peaks are 4 in default mode and 1 in single
mode. Entries and outer entries at all three selected semantic origins remain
equal. This proves the observed boundaries, not all native lazy work or CPU
utilization.

| Observed spelling work | Default baseline → candidate | Single baseline → candidate |
| --- | --- | --- |
| Preparations | 127,968 → 58,488 | 113,864 → 57,787 |
| Rendered strings | 254,105 → 154,533 | 235,388 → 153,627 |
| Cloned written strings | 132 → 132 | 78 → 78 |
| Retained preparations | 58,488 → 58,488 | 57,787 → 57,787 |
| Returned string bytes | 2,884,009 → 534,244 | 2,165,053 → 527,865 |
| Discarded returned bytes | 2,349,765 → 0 | 1,637,188 → 0 |

These bytes describe returned string payload, separately from semantic entries,
allocation requests, resize/copy traffic and RSS. They are not saved wall time.
The observer's allocator and atomics substantially perturb execution; all timing
decisions must use ordinary release binaries with observation disabled.

## Evidence and handoff

[The receipt](reference-preflight.json) pins source, patch, helper, binary,
fixture and harness hashes; native declarations and diagnostics; complete
public diagnostic entries; actual checked identities; allocation/request and
spelling counts; and raw proof paths/hashes. Raw receipts stay in the local
archive named there. The first missing archive helper, zero-row corpus wiring,
phase/path guards and missing nested meter are retained as setup failures, never
silently treated as passing observations. Only the corrected corpus with the
verified nested fixture checkout qualifies.

To reproduce, freeze the named source with both pinned vendor checkouts. Install
the existing archive example and apply the proposal only in that archive. Build
ordinary release CLI, `verdictdump`, `diagverdictdump` and reference example;
capture both full corpora before/after. Run the existing site-aware oracle,
public driver and archive mutation runner with resolved absolute arguments.
Their receipt hashes and original native artifact are recorded in the JSON.

For the observer, follow its linked reproduction recipe, then apply the proposal
to the instrumented baseline. This run removes the copied meter's test allocator
declaration; a standalone test shim supplies the parent allocator and recognizes
the original `jsdoc-setup-probe` cfg value. No production allocator is changed.
Four allocator tests and three receipt-reader tests pass; strict release checker
Clippy and candidate formatting pass.

The next task, `tsr-1yb.16.3.3.2`, owns the decision to retain or reject. Its fixed
protocol uses two independent five-pair rounds in default and single modes,
ordinary binaries, stable observed inputs/options, full output, wall/CPU/RSS and
variance. Each round must independently pass the existing 20 ms absolute gate
without a required-mode regression beyond that threshold, with at most 5%
median peak-RSS growth per mode. Sub-noise or contradictory rounds require
no-change. The initial four-child ordinary baseline is 3.352/5.232 seconds;
single samples range 5.120–5.344, so it is not a confirmation result.

Original comparable full-project TSR/pinned-tsgo median wall ≤0.50 acceptance
remains unmet and unverified. This component cannot close that goal.
