# Completed call effects

This bounded port follows native `getEffectsSignature` at typescript-go
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Beads `tsr-1yb.11.3.1` owns
characterization, `tsr-1yb.11.3.3` the completed-negative slice, and
`tsr-1yb.11.3.2` the unfinished lazy predicate prerequisite. The source baseline
is `b027f09de213d1c18adb7fc3b55ad8e16a1724f3`; the runtime candidate is
`f0b075b8fb471e7c3660660c0b4b6e1118fb837b` (a later test semicolon is lint only).

## Native operation and the work it avoids

`internal/checker/flow.go:2047` reads checker-private `signatureLinks` by call
node. An absent `effectsSignature` runs callee typing, apparent-type and
signature selection. The writer publishes an effects signature or
`unknownSignature`; the latter is a completed negative, distinct from absence.
`relater.go:2016` lazily resolves predicates using the target/mapper, composite,
written annotation, or eligible body. A written ordinary annotation excludes
body inference. Before recursive body inference native publishes
`noTypePredicate`, then finishes the predicate slot.

TSR already memoizes flow work for a reference and caches resolved call
signatures. Different references can revisit one call after the reference-local
flow region is discarded. Those caches do not avoid all repeated callee typing
and materialized signature lists. The earlier CPU profile locates this boundary
at 17.5% of running samples on generated400 and 0.18% on API; inclusive samples
are not avoided-work counts or a saving bound. See [the profile](checker-native-cpu-gap.md).

## Identity, owner, publication and context

A private `Checker` owns `completed_no_effects_calls: FxHashSet<NodeId>` for its
lifetime. No set crosses checker/store owners, no borrowed signature survives,
and no global semantic cache or lock is introduced. The entry certifies only
completed unknown. Absence includes uncomputed, deferred, inferred, positive,
and unsupported work; it is never interpreted as a completed negative.

Publication follows existing callee/apparent/signature resolution. Every
candidate must have an explicit nonpredicate annotation, a materialized nonerror
and nonnever return, and a declaration that is not context sensitive. An empty
list certifies nothing. A returned `Some(Vec<Signature>)` alone is not proof of
native completion. Written annotations prevent a later body-inferred predicate;
TSR's signature builder already follows that rule. JSDoc-only and inferred
signatures are deliberately excluded from this port, rather than claimed to be
excluded by native.

Both lookup and publication require an empty alias evaluation stack and zero
mapped-template depth. This is a TSR admission restriction: native's call-node
identity is not being redefined as a mapper key. Foreign contexts run the
existing worker. Cold receiver, alias/origin, static/instance, optional target,
position and diagnostic handling remain there. The existing pure-function
refusal is unchanged and publishes no entry.

The obvious broader implementation would cache every `None`. That fails when
a foreign alias frame temporarily refuses an original pending signature and a
later original-frame query completes an inferred predicate. General effects
completion therefore waits for the lazy predicate publication work in
`tsr-1yb.11.3.2`. Positive generic signatures and target-refusal negatives are
also outside this slice.

## Executable controls and historical correctness verification

`effects_signature_queries.rs` checks 14 native-supported declaration returns
under cold/reversed/prechecked/repeated queries in four fresh private checkers.
The native strict declaration output was independently identical with one and
four checkers. Assertions, predicates, written and inferred never, ordinary
identifier/dotted/optional/generic targets, loops, contextual calls, late lazy
returns and recursion are represented. Semantic return/member identities are
asserted; the native union spelling is checked for the two never controls.

The private refusal control installs a TSR alias frame, receives two refused
answers with the original lazy return still pending, then completes the string
predicate in the original frame with the callable TypeId preserved. It is a TSR
publication-state control, not a claim that native exposes an equivalent frame
installation API. A second private control proves completed ordinary negatives
and exclusion of predicates/never in independent checker owners.

Before interruption, an illegally broad all-None cache mutation compiled and
failed the refusal control at `inferred predicate after original-frame
completion`. Restoration passed all 189 checker library tests and the public
query test. Full release workspace tests, all-target Clippy with warnings denied,
and formatting passed. The complete type corpus was byte-identical: 477,970
assertions, 469,785 RIGHT, 995 GAP, 7,190 WRONG. The complete eligible diagnostic
corpus was byte-identical: 10,570 cases, 4,968 EMPTY_RIGHT, 4,221 RIGHT, 1,281 WRONG,
100 EMPTY_WRONG. Thus there were no previously RIGHT losses on those source pins.
The type result SHA256 was
`8d1491a9eb7f2f7aaa098dc5223e35a13d8108518c51d7793f3fea08c14b636d`.
Temporary raw corpus/review artifacts were cleared during interruption; these
are historical verified tool outcomes, not preserved raw evidence. Recovery
rebuilt both ordinary CLI binaries with the exact same SHA256 identities.

## Retention measurements

Two separately confirmed rounds each used one warmup pair and five alternating
measured pairs per workload (60 measured CLI children total). Complete stdout,
stderr, exit status and normalized diagnostics matched in every warmup and
sample; observed ordered loaded files, config and input snapshots also matched.
Every predeclared wall/RSS gate passed. Memory is median child peak RSS,
measured using wait4; the host remains shared with other user processes.

| Round | Workload / checkers | Baseline wall | Candidate wall | Candidate/baseline | RSS ratio |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | Generated400 / 1 | 3.324 s | 2.853 s | 0.8583 | 1.0304 |
| 2 | Generated400 / 1 | 3.322 s | 2.884 s | 0.8682 | 1.0081 |
| 1 | Generated400 / 4 | 1.700 s | 1.256 s | 0.7387 | 1.0280 |
| 2 | Generated400 / 4 | 1.704 s | 1.259 s | 0.7386 | 0.9848 |
| 1 | API / 4 | 7.079 s | 7.001 s | 0.9890 | 0.9981 |
| 2 | API / 4 | 7.260 s | 7.139 s | 0.9834 | 1.0109 |

The generated result is a retained 13–14% one-checker and 26% four-checker
improvement. API meets the no-regression gate; its small difference and sample
spread do not establish a material real-project speedup. The gates were at
least 20 ms generated median improvement at both checker counts, median peak
RSS ratio at most 1.05, and API median wall ratio at most 1.0 in both rounds.
No build, observer or corpus run overlapped the ordinary timing cohorts.

A separate ordinary extended-diagnostics control preserved generated
465 loaded / 465 parsed / 402 checked files and API 9,861 loaded / 10,555 parsed /
615 checked files. Counts are the CLI's performed-work reports; equality is not
proof of complete native-equivalent lazy work.

## Actual worker proof and replay

A task-only private per-call observer records requests, actual callee/signature
worker entries, completed hits, first publications and returned signs. Its global
atomic numbers output owners only, never semantic identities. The same observer
binary runs with completed-negative reads disabled, enabled twice in fresh
processes, and output recording disabled. Input fingerprints match the ordinary
timing inputs before and after each process. Outputs and status match the frozen
ordinary baseline in every mode. Two enabled runs have identical aggregate
counts; observer durations are excluded from the timing table.

| Workload / 4 checkers | Requests | Signature workers, reuse disabled | Signature workers, enabled | Completed hits | First publications |
| --- | ---: | ---: | ---: | ---: | ---: |
| Generated400 | 1,691,802 | 1,127,194 | 2,400 | 1,124,794 | 1,200 |
| API | 63,579 | 19,145 | 6,032 | 13,113 | 1,128 |

The decrease in both signature and callee worker entries equals completed hits.
Generated negative/positive return counts remain 1,691,802/0; API remains
61,472/2,107. Private checker/call key populations remain 2,803 and 5,991 across
four owners each. Owner numbers are not checker worker indices. The disabled
control still records first publications but bypasses every retained read,
restoring the historical baseline signature-worker counts. Counts certify
avoided execution, not allocation or result-copy bytes.

The broader premature-None mutation was rebuilt during recovery: it compiled,
ran exactly the foreign-frame refusal test and failed with exit 101 at the
later inferred-predicate expectation. Restoration and current quality checks
are recorded in the machine evidence.

[Machine evidence](checker-effects-completion.json) retains every ordinary
child statistic, full-output hashes, source/binary/config/input identities,
actual worker aggregates, historical corpus results and their raw-artifact-loss
limit. Private app file listings and diagnostic bodies are not published.
Current raw files are local under `target/effects-completion/`.

The local experiment commit f0b075b8 is a source identity, not a promised remote
ref. This landing contains its exact runtime files; the test semicolon is lint
only. To replay timing, build baseline b027f09d and the landing containing this
report in isolated checkouts, then use `scripts/generate_perf_project.py --modules 400
--out <project>` and `scripts/dependency_parse_perf.py --baseline <baseline>
--candidate <candidate> --baseline-source <sha> --candidate-source <sha>
--project <project> --output <raw> --samples 5 --checkers <1-or-4>` twice.
Use the real project's directory for its separate four-checker control.

For counts, apply [the observer patch](checker-effects-observer.patch) only in
an isolated candidate checkout, build a separate CLI, and run with
`TSR_EFFECTS_PROBE_DIR=<output>`; set `TSR_EFFECTS_PROBE_DISABLE_REUSE=1` for
the disabled-read control. Per-owner TSVs retain per-call executed stages.
For the publication falsifier, apply [the premature-None patch](checker-effects-premature-none.patch)
to an isolated ordinary candidate and run `cargo test --release -p tsr-checker
--lib effects_refusal_in_foreign_alias_frame_is_not_a_completed_negative`.
Expect compilation success and test failure. Remove both patches before normal
validation or timing; neither is production code.

API retains four existing false parser diagnostics on ioredis `function:`
parameters; native accepts them. TSR/TSR equality can qualify this bounded
optimization, but does not establish equivalent complete native work or the
release target of TSR/native median wall ratio at most 0.50.

## Main integration

Main gained native cheap-JSDoc terminator and ambient-export grammar fixes at
`0e7824dd` during measurement. The landing rebased cleanly. The combined code
passes189 checker library tests,1 effects query test,3 ambient-export tests and
1 scanner lazy-tag test, strict all-target workspace Clippy and formatting. Its
ordinary CLI preserves both workloads' complete normalized diagnostics, exit
status, stderr and loaded/parsed/checked counts. These integration controls are
recorded separately: corpus and timing numbers remain bound to b027f09d/f0b075b8.
