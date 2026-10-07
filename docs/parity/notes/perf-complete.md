# Complete-work performance lane — tsr-2zk.17

## Pinned boundary and reproduced evidence defect

Native source: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
`internal/compiler/program.go` `SortAndDeduplicateDiagnostics` clones, sorts
with `ast.CompareDiagnostics`, and compacts related information. The compiler
consumer publishes that ordered sequence. The performance harness previously
sorted rendered diagnostic entries again before hashing them. A pinned native
invocation with two source files and `--noLib` emitted ten TS2318 diagnostics;
reversing the emitted entries retained the old fingerprint. The candidate
fingerprint rejects this reversal. A two-file control with standard libraries
emits matching ordered TS2322 diagnostics from TSR and native.

Integrator Beads request: track this native diagnostic-order evidence root
separately from tsr-2zk.17's global complete-work/performance root. The separate
issue ID is not available on this Box; commits reference the assigned parent.

Additional current controls: passing the same two root paths twice emits the
same two TS2322 diagnostics once in both compilers (exact stdout/exit agreement).
With `--noLib`, pinned native emits ten global TS2318 diagnostics; TSR instead
emits the two file TS2322 diagnostics. This is a real global-diagnostic producer
gap, not harness equivalence. Route native checker initializeChecker global-type
validation to the checker owner; do not hide it or infer completion from file
output. Harness controls require global order and duplicate multiplicity to
change fingerprints. No harness deduplication is performed.

This is an evidence correction, not a compiler optimization or a conformance
case conversion. No cache, checker identity, AST, or diagnostic producer changes.
The harness owns one ordered rendered sequence per fresh child invocation;
identity includes normalized project paths, codes, messages, printed line/column
positions, complete captured continuation text, duplicate multiplicity and order.
CLI output does not expose span lengths, so a full-span fingerprint still needs
the structured diagnostic oracle; this harness does not claim one. Absent output is an empty sequence, not completed semantic success.
Timeout/failure remain separate child outcomes. No active or completed semantic
cache publication is inferred from this fingerprint. Existing binary/input
capture happens outside child timing; the fingerprint does not establish actual
checked-work, missing span lengths, complete related-information structure, or
complete query-input equivalence. Verified ratio remains null.

## Frozen current baseline and profile

Startup source is the integration commit `5dd3bad84d12991e1ba169d2d5687321e1989740`.
Native checkout SHA was checked directly. Offline bootstrap used a private venv
with tomlkit 0.13.3; existing Cargo configuration was preserved before bootstrap.
Release CLI and unfiltered diagnostic/type dumps were frozen before edits.

- Frozen TSR binary SHA256:
  `210ad47f20e6bf4f286acd4151974744178d19a96842944bfc392859b9892e52`.
- Built pinned tsgo SHA256:
  `7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`.
- Type dump: 477970 tab-delimited verdict records, 469765 RIGHT, 7212 WRONG,
  993 GAP. Physical lines are 477976 because rendered source can contain newlines.
  SHA256 `25ed9bd55f95956c4515044bf7f41334799bbcc6e2a186eaf6ad36824621660a`.
- Diagnostic dump: 10570 records; RIGHT 4221, EMPTY_RIGHT 4968, WRONG 1281,
  EMPTY_WRONG 100. SHA256
  `ff08a03bb448a639ac7e219d8e7e4b2921b6aa5cf95230c4744efba413da9482`.

Fresh-process 21 alternating pairs on Linux x86_64, domain-model-large:
TSR median wall 1.0360096 s, pinned tsgo 0.6474923 s, observed ratio
1.60003385. The post-edit run after all verification workers finished measured
TSR 1.0232078 s, native 0.6371528 s, observed ratio 1.60590646 (21 pairs).
Loaded scope, exposed options, diagnostics and sampled inputs match.
Complete query-input coverage and actual-work equivalence do not: this is not a
verified release ratio or evidence for the <=0.50 target. The first measurement
precedes the evidence-only edit; compiler binary bytes are unchanged. Finishing
this Box's verification workers does not establish absence of external machine
load; neither run is an uncontended receipt.

Integrator-reported current measurement at source `5dd3bad8`: 21 alternating
pairs on domain-model-large, observed TSR/native wall ratio 2.662059755.
Individual walls vary from 0.644–2.589 s for TSR and 0.227–1.851 s for native;
concurrent external machine load makes this observation-only. The earlier
capture with corpus workers observed 3.6683. Scope/options/diagnostics match;
complete-input and actual-work flags remain false, as does target_verified.
These are separately attributed observations, not speed wins or contradictory
uncontended receipts.

A subsequent `perf record -F 999 -g` captured 2146 samples with no lost samples.
Checker thread shares: checker-0 42.68%, checker-3 17.24%, checker-1 16.68%,
checker-2 15.38%; caller 7.88%, dependency parser 0.14%. The largest individual
symbol was `BindResult::resolve_name` on checker-0 (3.68%), followed by
`Checker::get_type_at_flow_node` (2.10%). This is sample attribution, not counts
of worker execution/reuse, and does not justify adding a cache or changing
file affinity. The existing CLI pool already mirrors native `createCheckers`
and `forEachCheckerGroupDo`: private checkers, Program file index modulo count,
read-only Program, no cross-checker semantic-cache merging. No speculative
scheduling change was made. Aggregating the actual same profile by symbol across
all threads gives self samples: malloc 5.31%, BindResult::resolve_name 4.89%,
cfree 2.61%, SymbolTableField::get 2.42%, Checker::check_node 2.24%,
get_type_at_flow_node 2.10%. Native owner-affinity and the actual production
parallel run were preserved. Unreliable unwound parent addresses are not treated
as worker counts or expensive traversal evidence; no scheduler speed claim is
made from this profile.

## Integration prerequisites / Beads follow-up request

Record these missing boundaries under tsr-2zk.17 before extending reuse:

1. Native actual-work capture is outside this lane's ownership. Instrument the
   pinned `internal/compiler/checkerpool.go` constructor, file association,
   exclusive leases and checker-group execution; `internal/checker/checker.go`
   `initializeChecker`, `getTypeOfVariableOrParameterOrPropertyWorker`, symbol
   and declared-type worker boundaries; and the source-file check entry.
   Keep query entries, completed-cache hits, active repeats, worker executions,
   failure/unsupported work and private-checker identity distinct. A trace
   inventory is not proof that all forcing was observed.
2. Native complete inputs require `internal/compiler/fileloader.go` /
   `filesparser.go`, resolver, tsoptions/config and OS host observations for
   reads, negative existence queries, directory enumeration, realpath/case,
   package metadata, config extends/references, bundled library bytes and
   relevant environment. Preserve logical path and query result, not only
   loaded-file names. Native vendor files are not owned here.
3. TSR checker instrumentation additions require the checker owner:
   `crates/tsr-checker/src/work_trace.rs`, `symbols.rs`, `declared.rs`,
   `checker.rs`, `flow.rs`. Existing observer covers four operations but not
   initialization/all forcing, cache state or complete semantic workers.
   `crates/tsr-execute/src/compile.rs` explicitly forces the observed run to
   one checker; its serial sink cannot certify the default four-checker run.
   Do not remove that guard without per-private-checker observers and a
   versioned consumer contract. Production actual-work proof must preserve
   owner scheduling, not use the serial trace as a receipt for the production
   pool. Read pinned checkerpool.go lines 40–169 and core/workgroup.go lines
   20–87 for scheduling; filesparser.go lines 56–155 and 240–518 for frontend
   task ownership, loading and publication.
4. Profile-guided optimization investigation belongs to binder/checker owners:
   `crates/tsr-binder/src/lib.rs::resolve_name` / native name resolution and
   `crates/tsr-checker/src/flow.rs::get_type_at_flow_node` /
   `internal/checker/flow.go::getTypeAtFlowNode`. The profile names a consumer,
   not a proven algorithmic discrepancy; reproduce and count actual worker
   boundaries before choosing a fix.
5. Full strict conformance is owned by the conformance lane. Current dumps do
   not cover all variants, exact message-chain structure, span length or full
   population. No compiler case is claimed converted by this harness fix.

## Verification constraints

The normal coverage executable writes forbidden shared snapshots. Linux mount
namespace isolation was denied (`unshare: Operation not permitted`). A private
`proot` bind of the snapshot directory runs the actual full coverage executable
without changing tracked snapshots. No corpus, snapshots, toolchain, setup,
lockfile, checker, parser or binder file is edited by this lane.

## Completed verification

- Full unfiltered before/after dumps have identical keys and verdict counts;
  zero before-RIGHT type-line losses, zero before-RIGHT/EMPTY_RIGHT diagnostic
  case losses. No named compiler target cases were assigned or converted.
- Full coverage completed over all 12444 discovered sources: checker_types
  8042/9538 (84.32%, 98.10% lines), diagnostics 4221/5502 (76.72%). Skipped
  populations remain 2906 and 6942 respectively; these are not strict full
  variant/message/span-length coverage claims.
- `cargo test --workspace --release` completed successfully, including doc tests.
  `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings` and
  `cargo +1.96.0 fmt --all -- --check` completed cleanly. Stable release testing
  emitted a pre-existing deprecation warning for AtomicUsize::fetch_update in
  front_end.rs; that file was intentionally unchanged. Initial stable fmt/clippy
  commands lacked components; installed pinned 1.96.0 components were used.
- 25 whole-project harness tests, 9 report tests, and 22 focused trace/order
  controls passed. An initial concurrent report-suite run timed out six controls
  under corpus/build contention; the isolated report-suite run passed all nine.
- Interleaved fresh-process candidate/frozen-TSR comparisons, after corpus/tests
  finished: domain-model-large wall 1.00444763, CPU 0.99988340 (21 pairs);
  generic-imports wall 1.00267985, CPU 1.00137345 (21 pairs). Domain-model's
  initial 21 pairs had CPU 1.03759859 and wall 1.00664511; the required 41-pair
  noise check yielded CPU 1.00183399 and wall 0.99704000. Diagnostics matched
  throughout. There is no compiler change or speed gain: CLI binary SHA256
  remains identical to the frozen binary.
- The final pinned native run reports matching loaded scope/options/ordered
  rendered diagnostics, but `verified_wall_ratio: null` and `target_verified:
  false`. Actual checked-work and complete-query-input proof remain blocked by
  the ownership prerequisites above. No 99.9% parity or <=0.50 claim is made.
