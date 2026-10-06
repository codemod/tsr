# Concrete reference construction cost selection

At frozen Rust `3cf335646c185182e360326b847ff9dd8cbb8075`, the existing class/interface reference cache answers **3,566,827 of 3,666,301 requests (97.29%)**. The remaining **99,474 final reference mints have 99,474 distinct checker-local ordered keys**, with zero repeated mints. This family does not justify another construction cache. This is a locating decision under `tsr-1yb.4.2.1`; native completed-member population, its broader prerequisites and `tsr-1yb.4.2` remain unfinished.

[Sanitized receipt](checker-concrete-builder-cost.json) and [private observer replay](checker-concrete-builder-cost.patch) preserve the source, builds, exact counts, failures and controls. The patch is instrumentation for a private checkout, not production code. No canonical compiler/runtime change or speed gain is retained.

## Ordinary workload and sampling

Fresh ordinary release builds bind all 651 Rust source hashes and 108 embedded library hashes. Native contract source remains `5b1047d10d32e7d5b446be4de56b126ff42f82bb`; no new native workload timing is performed.

The real Next.js project runs with explicit project, pretty false, noEmit, incremental false, composite false, listFiles and extendedDiagnostics. Two default observations are 18.606 and 18.763 seconds (median 18.684); two single-worker observations are 22.439 and 22.880 (median 22.659). All load 14,051 files and preserve four normalized parser diagnostics. Extended diagnostics report 1,397 checked files, but checked identities are not directly verified. Physical listed sources plus the root config are fingerprinted before/after each child. This is partial input coverage, not complete filesystem query coverage or native performed-work equivalence. Other agents' host activity is uncontrolled, so these are observations, not a confirmed benchmark ratio.

Four sandboxed sample attaches fail with exit255. An unsandboxed owned child is successfully sampled at seconds4–7, keeping its main thread and four checker workers distinct. The main thread waits for the pool. Worker nearest owners include structured member-name traversal, heritage preparation, reference preparation, generic instantiation and parameter graph walks; allocator frames are prominent. These interval counts include waits and are neither CPU percentages nor savings estimates. A second delay12 attach reaches an already exited child and yields no profile. Both setup failures are retained separately from compiler results.

## Actual work boundaries

The observer captures its environment switch once per Checker and never supplies an answer. It observes class/interface targets only:

- The reference wrapper counts all calls, nesting roots and hits at the existing `instantiations` lookup.
- The actual final named-reference mint records `(SymbolId, ordered Vec<TypeId>)` in that Checker's private observation ledger. Special alias/mapped routes are excluded by target flags; the all-class/interface partition reconciles on this workload.
- The declared-class/interface mint body is counted separately by symbol. This symbol-only counter does not certify native semantic identity or a completed member image.

Each observer ledger is capped at 100,000 keys per Checker; qualified runs have zero overflow. No key crosses workers or becomes a semantic memo. Timing is root-inclusive elapsed time, includes clock/observer work, and sums separate owners; declared durations can overlap reference roots and must not be added as separate savings.

| Qualified default count | Total across four Checker owners |
| --- | ---: |
| Reference calls / roots |3,666,301 /3,666,301|
| Existing reference-cache hits |3,566,827|
| Actual final reference mints / distinct keys |99,474 /99,474|
| Repeated actual reference mints |0|
| Declared mint entries / distinct symbols |9,465 /8,217|
| Repeated declared symbol entries |1,248|
| Timed reference-root intervals |0.412485508s|
| Timed declared-body intervals |0.001384367s|

Count and time mode totals repeat exactly per owner, excluding duration fields. Disabled, count and timed default children preserve diagnostics, ordered loaded files and observed physical inputs. Their walls are 11.141, 11.415 and 10.842 seconds; this later set cannot establish speed or observer overhead against the earlier normal observations. The probe is not a candidate optimization.

The next single/off child finishes with unchanged output and counts of loaded/checked files, but its observed physical input snapshot changes. The driver stops with a retained failed gate. No original before/after snapshot rows were persisted by this driver; exact changed-file attribution is unavailable. Several app source and Next-generated files have contemporaneous mtimes, which is not a content-diff proof. There are no qualified single-mode natural counters or a second default timed observation. Do not replace this failure with an unqualified retry on the live project.

## Fixed controls and replay qualification

Thirty terminal public CLI children cover ordered/swapped reference arguments, repeated empty references, same-spelled namespace identities, receiver inheritance and recursive generic members in default and single modes. Each fixture/mode runs the ordinary binary, observer disabled, count, time and count again. All complete stdout/exit results agree and are clean; observer stderr is separated. Repeated count ledgers agree. The three fixtures mint respectively six, three and three reference keys, with positive existing-cache hits and zero repeated mints. This certifies the observer's bounded behavior, not native key/publication parity or a full corpus pass.

A reader mutation that removes one cache-hit increment fails the count partition check. Fresh two-file replay passes `git apply --check`, applies and reproduces both observed hashes exactly. Both release builds complete with source unchanged during compilation. No full checker package, full type/diagnostic corpus or strict lint gate is claimed in this locating turn. Documentation checks resolve 4,492 upstream references and have zero dangling section citations. The issue-ID gate fails on 191 historical IDs at 737 reported locations; their containing files are byte-identical to frozen main. Printed line coordinates are approximate; an initial exact-line reader assumption failed and was corrected. New task references resolve; no global issue-ID pass is claimed.

For replay, archive the exact Rust revision above in a private directory with its pinned vendor/submodule and embedded libraries. Build and freeze an ordinary release CLI, apply the patch, then build and freeze a separate observer CLI. Run the fixed fixture sources preserved in the receipt with `--ignoreConfig --noEmit --pretty false --target es2022 --strict false --skipLibCheck true`; add `--singleThreaded true` for single mode. `TSR_CONSTRUCTION_COST=count` enables aggregate `CONSTRUCTION_COST` stderr records; `time` adds duration measurements; unset disables output. For a stable real project, use the workload flags above and persist complete physical snapshots before/after each child. Stop on input drift. Hash both executables and every changed source before comparing results.

## Decision and remaining work

Do not add a second class/interface reference cache: the current owner already eliminates repeated actual final mints, and the measured root interval is small relative to whole checking. Repeated symbol-only declared mint entries account for a very small observed body interval and do not justify a new semantic cache either.

This does not reject native completed-member reuse: the current constructor paths do not populate that field-complete image. [Native publication/reset requirements](checker-native-member-completion.md) and [private contextual ownership handoff](checker-member-mapped-contextual.md) remain separate. A future builder choice needs actual member-field population/consumer cost and qualified native state, rather than reusing these reference-call counts as native completion misses. Existing graph-walk and allocator owners remain responsible for their candidates.

No production fix for the reported 20–30x PR #5 regression is established. `tsr-1yb.34` remains unresolved, and the complete equivalent-work TSR/pinned-tsgo median wall target of at most 0.50 remains unmet.
