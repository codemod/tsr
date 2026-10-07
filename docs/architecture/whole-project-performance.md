# Whole-project performance

`bd tsr-1yb` requires complete project checks to finish in **at most half the
wall time of pinned tsgo**. Parser and binder benchmarks in ADR-0009 remain
useful, but do not establish this CLI target.

## Reproduce

Build both compilers before timing them. The harness accepts a prebuilt native
binary so that a Go build cannot contaminate the measured interval:

```sh
cargo build --release --bin tsr
cargo run -p xtask -- perf-project \
  --project /path/to/project/tsconfig.json \
  --tsgo /path/to/pinned/tsgo \
  --samples 5 --output /tmp/whole-project-perf.json
```

Add `--mode single` to request `--singleThreaded true` on both sides. The default
measures each CLI's normal scheduling (see [Native checker pool](#native-checker-pool)).
The real project's path is opt-in; private sources and detailed output are not
committed. `benches/projects/generic-imports` is a small public smoke fixture,
including an intentional assignment error. It verifies the measurement plumbing;
it is too small to establish throughput on large projects.

`benches/projects/domain-model` is the public representative project: about
7,000 lines in 42 modules, generated deterministically by
`python3 scripts/generate_perf_project.py` (rerun it and diff to verify the
committed bytes). It uses cross-module imports, generic interfaces and classes,
abstract members, discriminated unions narrowed by `switch`, mapped and
conditional types, overloads, optional chaining and async functions, and ends
with one intentional TS2322 control. Both CLIs report exactly that diagnostic;
keep it that way when changing the generator, because a benchmark whose
diagnostics differ is not equivalent work. The first generator draft exposed a
TSR false positive (an object literal against an interface that extends an
instantiated generic base with an optional member) and avoids that shape.

`benches/projects/domain-model-large` is the same generator at
`--modules 200` (~35,000 lines in 202 files), committed so that a public ratio
measures checking rather than startup and library parsing: on the two smaller
projects checking is 1–40% of the run. Both CLIs report exactly its one
intentional TS2322. Its first measurements, the cross-tool wall attribution
and the per-work checker comparison are in
[the perf lane notes](../parity/notes/perf.md).

The harness runs one warmup per tool and alternates process order across measured
pairs. Every check is a fresh process with `noEmit`, `incremental false`,
`composite false`, and plain diagnostics. It does **not** flush the OS file cache:
these are warm-filesystem measurements. POSIX `wait4` measures CPU and peak RSS
for each individual child rather than reusing cumulative resource usage.

Each sample also records its child PID, launch timestamp and exact command.
The [cache-isolation controls](benchmark-cache-isolation.md) demonstrate native
incremental diagnostic replay and verify that absent, valid, stale, poisoned and
malformed build info cannot change the disabled-incremental fixture checks.
Fresh process identity alone does not exclude persisted incremental reuse.

The JSON contains every sample, medians, p95 and ranges, binary fingerprints,
source/oracle/project revisions, effective configs, loaded-file identities, and
diagnostic fingerprints. It persists each sample immediately. A timeout or an
unsupported compiler invocation fails the run. Schema version 2 fingerprints
observed input state before and after each child, including preflights and warmups.
Capture time, including setup and loaded-path discovery observations, is
recorded separately from child wall/CPU/RSS.

`observed_wall_ratio` is always an observation. `verified_wall_ratio` is null
until complete cross-tool input coverage and actual performed checker work are
verified, as well as matching options, loaded scope and stable diagnostics.
Logical symlink paths remain distinct; only known bundled-library prefixes are
normalized. `target_verified` also requires matching diagnostics and a ratio at
most 0.50. The current harness does not yet collect complete query coverage or
actual checked-work/worker telemetry, so its verified ratio remains null even
when the public smoke project's loaded lists and diagnostics agree. Full-corpus
correctness verification is separately required before the epic can close.

`--require-comparable` fails after saving evidence when performed work is
unverified. Until the missing coverage and worker controls are implemented, use
reports as observations; this flag cannot currently produce a passing speed gate.

## Current trace capture and equivalence certificate (tsr-2zk.17 / tsr-1yb)

`--capture-work` runs separate **untimed** current-binary invocations before
warmups and pairs. Build TSR once with `cargo build --release --bin tsr
--features work-trace`; supply the prebuilt pinned native binary. Capture writes
fresh `<output>.work/{tsr,tsgo}/` directories, raw traces and supervising receipts.
The report retains complete CLI stdout/stderr, warmups, observed input snapshot
rows, effective configs, source/producer hashes and full trace file identities.
Never reuse an artifact directory. Capture overhead, type dumping and builds are
outside the five-or-more fresh-process median pairs. Ambient `TSR_WORK_TRACE`
and its binary-hash claim are removed from ordinary children: otherwise a shell
setting silently forces serial checking in timed runs. No security settings are
changed. `fresh_launch_delay` is a separate, unmeasured/null row on this Linux Box;
it is not a measured macOS launch-delay correction.

```sh
python3 scripts/whole_project_perf.py \
  --project benches/projects/domain-model-large/tsconfig.json \
  --tsgo target/tsgo-pinned --capture-work --samples 5 --warmups 1 \
  --output /tmp/current-perf.json
python3 scripts/checker_work_trace.py --native-trace \
  --trace /tmp/current-perf.json.work/tsgo/trace.json \
  --receipt /tmp/current-perf.json.work/tsgo/receipt.json \
  --output /tmp/native-validation.json
```

### Pinned native operation and ownership

At native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`,
`Program.GetSemanticDiagnostics` calls `collectCheckerDiagnostics` and grouped
`collectCheckerDiagnosticsFromFiles`. `checkerPool.forEachCheckerGroupDo` retains
one task and exclusive lock per private checker, original per-owner file order,
and Program file identity association `i % checkerCount`. `createCheckers` waits
for all constructors before publishing associations. Options select four
checkers by default, one for `singleThreaded`, or the requested `checkers`, clamped
to `[1, min(fileCount, 256)]` (at least one even for an empty Program).

`getBindAndCheckDiagnosticsWithChecker` applies `SkipTypeChecking`, retrieves
bind plus `Checker.GetDiagnostics`, handles plain JS/JSDoc and comment directives;
`getSemanticDiagnosticsWithChecker` applies no-emit filtering and adds include
processor diagnostics. `SkipTypeChecking` includes project-reference sources
and native default-library identity, not just declaration-file counts.
`Checker.getDiagnostics` calls `checkSourceFile`; its private `sourceFileLinks`
key is the concrete Program `*ast.SourceFile`, with `typeChecked` and
`unusedChecked` separate publication states. The full worker performs grammar,
source elements, deferred nodes/diagnostics and exports before publishing
`typeChecked`; cancellation must be observed separately, not inferred from a
returning trace span. The unsampled `checkSourceFile` B/E span lies **inside**
`!links.typeChecked`; it counts actual initial workers, not all queries or hits.

`--generateTrace` preserves native threading. `checker.Tracer` attaches private
`checkerId`; trace `pid=1`/`tid` are synthetic, not OS identities. The supervisor
binds raw bytes, actual child PID/command, binary hash, source files and stable
observed inputs. `getVariancesWorker` B/E end args legitimately add `variances`;
the reader preserves input identity rather than rejecting that output field.
`checkExpression`, `structuredTypeRelatedTo` and other X events are 10 ms sampled:
never reinterpret sampled counts as all executions, hits or exclusive inner
worker cost. The longest full-file duration includes nested work and scheduling.

TSR's shipped producer instead observes a single private checker and forces
pool size one when tracing. Its full inventory retains declaration/JS/JSON,
parsed directive, source-node identity, byte count and eligible/excluded policy
facts; begun and completed counts are distinct for full-file, symbol-type,
declared-type and variable-type-worker operations. Symbol/declared queries are
not cache misses. Native `getTypeOfSymbol` dispatches by concrete symbol flags;
`getTypeOfVariableOrParameterOrProperty` consults private `valueSymbolLinks`,
executes its worker only for absent `resolvedType`, and may defer publication
for context-sensitive parameters or prefer a type published during recursion.
Receiver, instantiated target/mapper and written alias identity are not captured
by these counters and cannot authorize cross-symbol/checker reuse. This harness
adds no semantic cache or mutable semantic table.

### Native inner-operation checkpoint and next boundary

`--checkpoint-output /tmp/checkpoint.json` exports a curated source-qualified
handoff: certifier hashes, binary/config identities, all median pairs and warmup
count, receipt/trace hashes, qualified producer source hashes, observed operation
counters and explicit incomplete gates. It does not import caller acceptance
claims. `native_operation_boundaries` retains unsampled checker spans with
begin/end args, concrete private checker ID, native type ID, parent operation,
containing full-source path and inclusive duration. Sampled checker spans are
retained separately; they cannot enter the unsampled ranking or satisfy missing
worker counters. Equal 202 full-file completions do not prove complete checker
operation equivalence.

Pinned `checker/relater.go:getVariancesWorker` is the currently observable
unsampled inner boundary: `c.varianceLinks.Get(symbol)` is private-Checker-owned,
keyed by concrete native `*ast.Symbol`. Absent `variances` enters the worker;
publication first writes an empty provisional slice so recursive re-entry returns
that active image, then publishes the completed ordered variance vector.
`createMarkerType` uses concrete symbol/type-parameter identity and an ordered
simple mapper, with separate alias-vs-interface instantiation. Relation reliability
flags and `inVarianceComputation`/resolution stack context are preserved/restored.
Trace args use the declared type ID plus checker ID and arity; these **do not**
uniquely reconstruct the symbol, alias, mapper, receiver or publication state.
The trace lies inside the absent-cache branch and records no completed hits or
active repeats. No cross-checker cache or table reuse is added by this reader.

Latest Linux large-project checkpoint, same prebuilt binaries and source
`62b3caf7a9f4aed279381c10a7fc983e5b7f86bb`: five alternating fresh pairs plus
warmups, medians TSR 0.8077273999999761 s / native 0.585535899999968 s,
**observed ratio 1.3794669122764638; <=0.50 UNMET**. Native capture has 202
completed initial full workers, 15 completed variance workers and 203 completed
emit-entry spans. Its largest observed unsampled inner worker was variance,
checker 2 / native type 180 / arity 1, `src/model002.ts`, 465200 ns inclusive,
completed vector `["out"]`. This is not the globally most expensive inner
operation: 348 sampled `structuredTypeRelatedTo` events cannot establish all
relation executions, and native symbol/declared/variable workers and metadata
forcing have no corresponding current counter. The next expensive-work root is
therefore **complete relation-worker attribution**, not optimization of the
smallest available unsampled span. The existing runtime/checker owner must emit
query/hit/active-repeat/worker/publication counters at those native boundaries;
this scripts-only worker cannot create that evidence by counting sampled wrappers.

```json
{
  "checkpoint": "/tmp/recover-native-current-checkpoint.json",
  "whole_project_perf_sha256": "9f596f250dbcc6a35c91969834bf21b1ea3ef237e0bfbaf2612721e1f54f08a1",
  "checker_work_trace_sha256": "c4decf980b1dfd631530c2f85b0371fd5e39a7573c3388d8517bf14c8357072c",
  "observed_wall_ratio": 1.3794669122764638,
  "verified_wall_ratio": null,
  "complete_checker_operation_coverage": false,
  "actual_timed_work_equivalence": false,
  "speed_target_verified": false,
  "proof_gap_count": 9
}
```

26 trace-reader and 27 harness tests passed; actual native reader and latest
five-pair capture/checkpoint CLI ran. `--require-comparable` exited 1. Mac parent
reports `_dyld_start` launch stalls exceeding 15 minutes: those are separate
fresh-launch observations, not Linux checker timings or semantic completion
proof. No macOS security setting changes or launch-delay subtraction occurred.

### Malformed receipts must not hide the other producer

Comparison CLI receipt parsing/validation now runs independently for TSR and
native. Malformed JSON or unreadable receipt on one side no longer aborts before
collecting the other producer's failure. Both original reason arrays survive,
qualifications remain empty, requested rejection JSON is saved and comparison
exits 1. This is bounded failure capture, not acceptance of invalid evidence.

37 trace tests passed, including actual malformed-left/empty-right and reverse
CLI controls. Actual qualified large comparison ran; fresh frozen expected-hash
five-pair/warmup smoke saved `/tmp/recover-independent-receipt-checkpoint.json`
and comparable exit 1. No locally qualified full-corpus zero-loss or new-parent
source-build receipt is available. The native pinned 202-file observation and
>0.50 large ratios still cannot pass the release certificate.

### Cross-tool receipt identity enforcement

Comparison acceptance now also requires equal captured effective config objects,
equal input snapshot rows and matching normalized loaded Program identity sets.
Matching completed worker paths alone cannot pass with changed options, captured
input bytes or a different loaded population. Invalid producer receipts short-
circuit these accesses; all existing worker/scope/receipt rejection gates remain.
The raw equality is deliberately conservative: serializer differences are not
silently treated as equivalent compiler semantics.

36 trace tests passed, including equal-worker controls with different options,
inputs or loaded scope. Actual CLI changed-option receipt saved rejection JSON
and exited 1 at `/tmp/recover-cross-option-rejection.json`; matched existing large
captures passed only bounded comparison acceptance. Fresh frozen expected-hash
five-pair/warmup capture smoke saved `/tmp/recover-cross-identity-checkpoint.json`
and exited 1 for unverified complete-work comparability. No full-corpus zero-loss
or source-to-build attestation is available locally; those release requirements
remain false, not replaced by the native 202 completed-worker observation.
Verified <=0.50 remains unmet.

### Comparison command must reject differing completed scopes

`artifact_integrity_valid` describes bounded producer captures, not cross-tool
scope acceptance. The comparison now separately publishes `comparison_valid`,
requiring both producer validations and a nonempty matching completed-worker path
set. The CLI uses that gate for its exit status; two individually valid captures
with different completed paths now save scope differences/reasons and exit 1,
not 0. Empty scope also fails. This does not promote matching paths to complete
semantic work or a speed certificate.

35 trace tests passed, including an actual comparison CLI with valid TSR/native
captures but different completed paths: bounded artifact integrity true,
comparison validity false, differences preserved, exit 1. Existing invalid-receipt
and unfinished-worker controls retain failure output. Actual matched large
comparison ran; fresh frozen-hash/input five-pair/warmup smoke saved
`/tmp/recover-scope-exit-checkpoint.json` and comparable exit 1. No full corpus
run was performed here: parent zero prior-RIGHT report is not a local corpus
receipt and vanished-case verification remains unproved. Existing native pinned
202 observations and >0.50 ratios remain rejected release evidence.

### Native private-owner thread invariant and executable negative receipt

Pinned `tracing.traceThreadKey.defaultThreadID` assigns checker index `i` the
synthetic thread `2+i`; native event process ID is 1. The reader now rejects
checker-bearing events with a different synthetic owner/thread or process ID.
This closes an identity migration that could otherwise avoid a per-thread
full-worker overlap check. These are source-qualified synthetic identities,
not actual OS thread/PID assertions. Existing worker-activity, ordinary-path and
invalid-receipt comparison fixes remain necessary.

34 trace tests passed; actual native large reader accepted the source-qualified
capture. Actual comparison CLI combining an empty TSR receipt and a native
out-of-Program full worker persisted **both** rejection arrays, artifact validity
false and exit 1 in `/tmp/recover-owner-negative/rejection.json`. Fresh expected-
hash/frozen source-input CLI capture ran five pairs plus warmups and saved
`/tmp/recover-owner-checkpoint.json`, source `07f6ad7e`, checkpoint SHA-256
`45a65dfe819e686190499ca01ab75ebdae10419bf274d671b8a90a05e1392888`.
Harness SHA-256 `513c21cb74197c79e547df5ca157348c7fbff60ab3421e6e1f60b33b99f569f0`;
reader SHA-256 `71556acb401fcb5b3d9ff16dbf986bae0217c538af7c08922a570216d2ecfe5e`.
Comparable exit 1 and target false; no release source-build or semantic completion
certificate was created. Native 202 large-file observations and >0.50 ratios
remain unmet release evidence, not count-based completion proof.

### Shared build artifacts are not doc/test verification

The parent reports all 201 final-family controls plus tests passed, with a possible
rustdoc E0460 after concurrent builds reused the same target. This is not evidence
of a checker bug. Cargo's build lock does not protect the entire subsequent test
or rustdoc execution from another worktree rebuilding dependency artifacts;
sccache reuse is separate from shared target mutability. The parent owns serialized
doc/lint verification after concurrent source-target builds finish. No doc pass,
skip, or semantic regression is claimed by this scripts-only worker.

Each controlled child now carries `executed_binary_identity` with exact executed
frozen path, before/after SHA-256, stability and false source-build provenance.
This applies to preflights, captures, warmups and timed checks. Static operation
catalogues remain unobserved coverage descriptions, never actual metadata forcing
flags. 30 harness tests passed and an actual frozen five-pair/warmup capture smoke
ran; comparable gate exited 1. Evidence `/tmp/recover-child-binary-perf.json`.
No compiler rebuild was launched here during the parent's serialized verification.

### Native completed-worker inventory and partial failure evidence

The native reader now requires each full-worker source path to belong to the
supervising receipt's exact loaded Program inventory. It rejects overlapping
full-file spans on a private checker, duplicate completed sources even across
different checker owners, duplicate loaded identities and regressing B/E boundary
chronology. Metadata timestamps remain legitimately backdated; sampled X events
are not converted into boundary chronology. Returned worker rows/counters are
retained even when later spans fail or remain unfinished, explicitly marked
partial-on-failure. Artifact validity remains false; partial observations cannot
enter a comparison identity-match gate or release certificate.

33 trace tests passed, including out-of-inventory workers, overlap, cross-owner
duplicate source and unfinished capture after a real completed worker. The actual
native large-artifact reader ran. A fresh frozen/hash-bound large-project run
completed five pairs plus warmups with current script source
`002b3e53b4633eafb8d0b8cc19d1a14732361708` and the original qualified compiler
binaries (not a rebuilt parent candidate). Native observed 202 completed initial
workers; all broader operation/timed-work gates remain false. Latest observed
ratio **1.4344651882444954**, verified null, 12 unmet obligations,
**<=0.50 UNMET**; comparable exit 1. Existing 1.379467 observation remains historical.

```json
{
  "checkpoint": "/tmp/recover-native-inventory-checkpoint.json",
  "checkpoint_sha256": "e959f2289f65e041a22f346a77e8ffa3f7904d51e69231f3b1b682157a5f77b6",
  "harness_sha256": "460951504741b3efdccb1f319c1a50197f22918ccb1850d3352f8e4251793b49",
  "trace_reader_sha256": "c8f48a13c66ebb73dfba6869c9c97e29725eaa868880730c51db9b44e6105ffa",
  "source_file_workers_observed": 202,
  "complete_checker_operation_coverage": false,
  "actual_timed_work_equivalence": false,
  "speed_target_verified": false
}
```

Full raw evidence: `/tmp/recover-native-inventory-perf.json` and `.json.work/`;
standalone native validation `/tmp/recover-native-inventory-validation.json`.
No compiled hot-path allocations or runtime/checker/output-filter changes were
introduced by these Python capture controls.

### Assignment-only hot-path correction is not a scope shortcut

The parent reports the initial native import-alias assignment correction added
two unconditional symbol-table/merged reads per identifier: domain CPU ratio
1.029 despite wall 0.9993. It subsequently moved those reads inside the native
`assignmentKind != None` branch, with no semantic change and no additional
read-only identifier flags lookup. That is a specific expensive-work boundary
correction, not justification for compiler-output filtering or skipping source
checks. The earlier causal diagnostic ratio (wall 0.9958 / CPU 1.0019) predates
this assignment fix and cannot certify its cost. The parent reports exact full
parity with zero prior-RIGHT losses for this stage; this Box has not inspected
its full corpus receipt and does not promote the release obligations.

The comparison JSON now exposes `counter_attribution` with symbol/declared
**queries** separate from source-file/variable-type **actual workers**. Native
unobserved queries/workers remain null. Assignment flag reads and read-only
extra reads are explicitly null on both sides: existing traces do not observe
that boundary, so query totals cannot prove a zero-cost read-only path or a cache
hit/copy-byte reduction. No diagnostic/output filters were added. 32 trace tests
passed; actual comparison CLI exported
`/tmp/recover-query-worker-attribution.json` from the existing qualified large
captures. The parent-corrected assignment binary is absent here; shared recovery
TSR still hashes `b2e104692f21571c489d3c88ad636fb11b6a64c957f219ec92bf47bcf6150fa3`.
Rerun candidate timing only after its full frozen expected binary hash and source
build receipt arrive. No new-candidate timing is claimed. Verified global <=0.50
remains unmet.

### Oracle release blockers remain independent of file-worker counts

The release certificate separately records three still-unverified obligations:
complete oracle selection/corpus receipt, resolution of native Code -1 failures,
and global diagnostic spans/metadata cutover. A selection-debug receipt cannot
certify a complete corpus, and a native failure is not an empty successful check.
The parent/oracle owner's reported blockers leave all three obligations false;
202 validated initial source-file workers prove only the observed completed
file-worker boundaries, not unsupported semantic work or a bootstrap certificate.
The certificate now enumerates 19 obligations, with 12 unmet even when the
existing scope/options/output/input/sample controls match. Historical gap counts
above describe their original captures, not the current release checklist.

Binary-capture failures now persist requested report and checkpoint JSON with
partial freeze evidence, expected/source identities, original failure reason,
nonzero exit and false work/target gates. A wrong expected binary hash does not
produce a traceback-only lost capture. 30 harness tests passed; the actual CLI
wrong-hash consumer control preserves both outputs. A frozen five-pair/warmup
capture/checkpoint smoke ran and still exits 1 for unverified comparability.
Evidence `/tmp/recover-release-obligation-checkpoint.json`. No parent legacy
regression fix or global metadata cutover is locally certified by this smoke;
large Linux <=0.50 remains unmet.

### Freeze compiler bytes before any CLI proof

Shared Cargo targets may be overwritten by another worktree's baseline build.
The harness now copies each supplied binary to fresh `<output>.binaries/{tsr,tsgo}`
paths before preflight, outside child timing. It hashes source bytes before and
after copying, rejects changes during capture, hashes the frozen copy, and marks
it read/execute-only. All preflight, trace, warmup and measured invocations use
those frozen paths. Original and frozen paths remain observed input identities;
source replacement during a run rejects the run. `--tsr-sha256` and
`--tsgo-sha256` bind an expected source-qualified build and reject a stale shared
target before any invocation. Reports/checkpoints/receipts retain freeze metadata;
matching hashes are still not source-to-binary build attestation. A copy frozen
from an already stale binary is not the current candidate without the expected
hash and qualified build receipt.

The parent reports `diagnostics-final-current-tsr` was frozen with a `b283…` hash
before the causal baseline build. A later duplicate-config invocation used an
old baseline from a shared primary target symlink: it is not a current-candidate
CLI regression. The parent's earlier five CLI proofs were executed before that
baseline replacement. Exact parent frozen hash/build/source/options evidence is
not present on this Box; no abbreviation is accepted as a hash argument here.
All new parent candidate gates must use its canonical rebuild and frozen full
hash, not the overwritten target path.

29 harness tests passed, including an actual frozen CLI retaining current output
after its shared source path is overwritten with a baseline and expected-hash
rejection of the wrong build. Existing source-replacement mutation controls still
reject changed input. Actual hash-bound five-pair/warmup trace/perf smoke ran
using the original qualified recovery binaries and frozen paths; comparable gate
exited 1. Evidence `/tmp/recover-frozen-binary-checkpoint.json` and
`/tmp/recover-frozen-binary-perf.json`. The pending parent candidate is not locally
verified by this run. Native 202 worker observations and failed large ratios
remain bounded evidence, not release verification.

### Mixed-source candidate qualification

Reports, receipts and curated checkpoints retain `checkout_identities`: HEAD,
SHA-256 and byte count of staged+unstaged tracked deltas against HEAD, untracked
file snapshots, and a combined identity. Source checkout identities are captured
before the run and again after all pairs, outside child timing; mismatch prevents
comparability. This does not detect transient edits. Nested submodule deltas are
not recursively audited; the pinned native repository is captured independently.
Neither a clean HEAD nor equal checkout identities attests that supplied binaries
were built from them. The scripts' `build_provenance_verified` and
`causal_baseline_verified` remain false; the parent-reported matched causal
experiment below is separately qualified and does not promote these gates.

Parent's earlier mixed AST candidate (unmerged mask/unary changes) and reported
AB41 CPU ratios domain 1.0308 / generic 1.0098 were **not** attributed to the
diagnostic patch or labeled a speed win. The earlier binding comparison
(domain wall 1.117 / CPU 1.031) was confounded by pending semantic files and is
not diagnostic-causal evidence.

The parent subsequently reports a matched causal baseline `9bc2c9dd`, identical
pending flow/binary/expressions/AST five-source SHA-256 identities, and 41 pairs
for the final diagnostic/config/loader candidate:

| Parent-reported causal candidate/baseline | Domain | Generic |
|---|---:|---:|
| Median wall ratio | 0.9958165 | 0.9860849 |
| Median CPU ratio | 1.001948 | 0.995279 |

Complete output, loaded scope and effective options were stable. This supports
**no evidenced genuine diagnostic hot-path regression**, not a speed win or
release-equivalence claim. Source-qualified parent receipts are retained under
`.git/tsr-recovery/diagnostic-causal*` on the parent machine; they are absent from
this Box and were not independently inspected here. Exact five-source hashes,
commands and binary identities must come from those receipts, not guessed from
the baseline revision. The scripts-only identity smoke below still uses the
original prebuilt binaries and does not locally reproduce the parent's candidate.

The parent also reports one full-parity prior-RIGHT loss after corrected JS
source population exposed an unresolved CommonJS symbol root, assigned to a
runtime worker. No-prior-RIGHT-loss acceptance is therefore unmet; do not revert
correct source population or relax the performance scope to hide the loss.
The Linux large-project ratio 1.379467 and verified TSR/native <=0.50 target
remain unmet. Nothing in the matched diagnostic control establishes broader
semantic operation completion or clears the native worker-counter proof gaps.

28 harness tests passed, including equal-HEAD staged/unstaged/untracked source
controls. Actual five-pair/warmup smoke with capture/checkpoint completed, checkout
stable, observed ratio 0.9133487218123627, verified ratio null, causal baseline
false and comparable exit 1. Artifact `/tmp/recover-source-qualified-checkpoint.json`;
harness SHA-256 `963936a7530d6a79110f84f117da4555a71de7e7e258462eae5db9a9920db42a`.
The large Linux ratios above remain failed speed observations; this small identity
smoke supersedes none of them and verifies no frozen-parent diagnostic behavior.

### Invalid receipts must produce comparison rejection artifacts

The comparison now constructs qualified receipt metadata only after both
validators succeed. Valid JSON `{}` receipts previously caused direct required
field access to raise before `--output` was saved. TSR/native validation failures
now survive intact, qualifications remain empty, aggregate artifact validity and
identity matching are false, and the CLI exits 1 with a rejection JSON. No blanket
exception suppression replaces validation. Worker-activity and canonical-library
identity fixes remain required.

32 trace-reader tests passed, including actual comparison CLI controls for empty
TSR receipt, empty native receipt, and both empty: output persisted, both original
failure arrays retained as applicable, nonzero exit and no traceback. Actual
existing large-artifact comparison also ran successfully with bounded integrity;
`/tmp/recover-receipt-gated-comparison.json`. None of these gates certifies complete
semantic work or changes the <=0.50 unmet performance target.

### Ordinary paths must not become bundled-library aliases

A second review blocker found that the comparison normalized every basename
under bundled-library-looking prefixes. It now applies the same restriction as
`whole_project_perf.file_identity`: only `lib\.[\w.]+\.d\.ts` under known bundled
prefixes becomes `<typescript-lib>/<basename>`. Ordinary paths remain exact;
`/a/typescript-go/internal/bundled/libs/model.ts` and the corresponding `/b/`
path, or `bundled:///libs/model.ts`, are distinct. `model.d.ts` is likewise not a
canonical library. No new alias heuristic was introduced. Worker-activity
validation and both producer rejection arrays remain required before matching.

31 trace-reader tests passed, including consumer comparisons that reject equal
ordinary basenames across different logical paths and preserve only canonical
library normalization. The actual existing large-artifact comparison CLI ran
and accepted bounded artifact integrity. Evidence
`/tmp/recover-path-identity-comparison.json`. Full semantic work, source-to-binary
identity and the <=0.50 speed certificate remain unverified; this fix changes no
large-project performance observation.

### Worker-activity rejection must survive comparison

Review found that a valid base TSR artifact could survive failed worker activity
validation (bad construction timestamps or incorrect activity peaks). The old
comparison read only `artifact_integrity_valid`, discarded rejection reasons and
could exit 0. **Do not use the pre-fix comparison gate in `bf9eea3a`.** The
comparison now requires TSR artifact **and worker-activity** validity plus native
artifact and trace validity, with no producer rejection reasons. It retains
`producer_validation`, both `producer_reasons` arrays and qualified flattened
reasons before any identity match claim. Missing required validity fails closed.
Curated checkpoints also retain activity/native validity and reasons instead of
presenting base artifact validity alone.

30 trace-reader and 28 harness tests passed. The regression runs the actual
comparison CLI against valid base traces with invalid constructor timestamps or
peaks: exit 1, identity match false and original worker rejection reasons retained.
Both producers' failures are preserved. The actual large-artifact comparison
still passes bounded artifact validation; its semantic-work and target gates
remain false. Fresh five-pair/warmup capture/checkpoint smoke also ran with
`--require-comparable` exit 1. Evidence `/tmp/recover-worker-gate-comparison.json`
and `/tmp/recover-worker-gate-checkpoint.json`. The large observed 1.379467 ratio
and <=0.50 unmet target are unchanged; 202 initial worker completions still do
not prove full semantic operation completeness.

### Runnable cross-tool operation-gap comparison

The trace reader accepts `--compare-native-trace` and
`--compare-native-receipt` alongside the TSR `--trace`/`--receipt`. It validates
both actual captures before comparing **full-worker path identity sets**, not
counts. Same-count/different-path and invalid/native-empty controls fail scope
matching. The full TSR Program policy rows and native completed source/checker
rows remain attached, along with structured effective configs and full CLI
stdout/stderr in supervising receipts. Native Program eligibility facts are
explicitly null, not inferred from the emitted worker population. No diagnostic
policy or loaded-file scope is narrowed to produce matching counters.

Each operation row identifies the pinned native operation, private Checker key
owner, begun/returned observations and actual executions where an existing
worker span observes them. Native symbol/declared/variable counters, both sides'
completed-cache-hit/active-repeat counters and result-copy bytes are **null**,
never zero. Symbol and declared query totals are not worker executions. Every
operation-equivalence gate remains false. `--capture-work` attaches this runnable
comparison to the full report and curated checkpoint; the checkpoint now also
retains machine facts and full input-reference hash rows.

```sh
python3 scripts/checker_work_trace.py \
  --trace /tmp/recover-operation-current-perf.json.work/tsr/work.ndjson \
  --receipt /tmp/recover-operation-current-perf.json.work/tsr/receipt.json \
  --compare-native-trace /tmp/recover-operation-current-perf.json.work/tsgo/trace.json \
  --compare-native-receipt /tmp/recover-operation-current-perf.json.work/tsgo/receipt.json \
  --output /tmp/operation-comparison.json
```

Latest comparison run: Linux x86_64, 14 CPUs, fixture
`benches/projects/domain-model-large/tsconfig.json` SHA-256
`17e4763f027443707e62545430fbd9d14f58ad25a05ed776542c4c1c7e29cb73`.
Harness checkout `25c942e2065b6ea0c6c49182ce00d0c8f5372b16`; reused binaries
built before the scripts-only commits (hashes above), **not** a claimed rebuild
from that checkout. Native remains pinned at `5b1047d`. Five fresh pairs plus
warmups: TSR 0.8125586999999541 s, native 0.5840470999996796 s, observed
1.3912554312835383; **<=0.50 UNMET**, nine gaps, verified ratio null.
Both observed full-worker path sets match with 202 completed initial workers;
native policy and broader checker-operation completion remain unsupported.
Native largest observed unsampled inner span: variance worker, checker 3,
type 315 / arity 2, `src/core.ts`, 2456800 ns inclusive, output `["out","out"]`.

```json
{
  "checkpoint": "/tmp/recover-operation-current-checkpoint.json",
  "checkpoint_sha256": "2db86fc4c265983e7afb21f7d9a129665996bece2e215fcdc1028e117952d0b9",
  "harness_sha256": "2d792fa1b600822a1f8844377820988e584c61f48ff3b1bad6ba811eb7ba110c",
  "trace_reader_sha256": "e0951023ed67b8f146e90f242acd9a79ee921b596ae53655a86ed1d27a2c61dc",
  "tsr_trace_sha256": "6872023f11e1331dcafce59d88a2a621d4f0b35a0dc8eb14a57f2fac20e54cd2",
  "native_trace_sha256": "6c55c1e8ee08bac9388a25e812accd49ad76a8ea9bb0696f5bdf9e22d738211b",
  "tsr_receipt_sha256": "8858a771318e751b8d13cea956227af6c12ec10a79b25ca42f8242c3911a0efe",
  "native_receipt_sha256": "1f8f6a791ab10ca1f8bb0dff5d1c0c43ae07372dee654432d5658cfdf97bb999"
}
```

28 trace-reader and 27 harness tests passed; the standalone comparison CLI and
fresh five-pair capture/checkpoint CLI ran. Comparable gate exited 1. Missing
native executed-worker/cache-hit/copy-byte producers remain the highest proof
gap, queued under the existing performance issue for the runtime owner; no
scripts-only estimate replaces them.

### Emit eligibility is not checking eligibility or output equivalence

The parent owns the runtime `noEmit`/list-only cutover. Current TSR has no
emitter; neither a fake generated file nor a native empty `EmittedFiles` result
proves equal work. The performance certificate separately requires matching
explicit `--noEmit` effective options and leaves native emit eligibility,
skipping and actual work unverified. The native reader retains every completed
`emit` operation's input/output args and duration, including the Program-level
span with no path. These are emitter-entry observations, not generated bytes,
`EmitSkipped` observations or exclusive cost.

Pinned `compiler/emitter.go:sourceFileMayBeEmitted` policy, in order:

1. Reject JS when `!forceJsEmit && noEmitForJsFiles`.
2. Reject declaration files.
3. Reject `host.IsSourceFileFromExternalLibrary(file)`.
4. Accept remaining files when forcing DTS or JS emission.
5. Reject a source with `GetProjectReferenceFromSource(file.Path()) != nil`.
6. Accept remaining non-JSON sources.
7. Reject JSON without `outDir`.
8. For JSON with `rootDir` or a config path, derive native common source directory
   and output path; reject when native case-sensitive/current-directory-aware
   path comparison says output is the input. Otherwise accept JSON.

External-library status is membership in
`Program.sourceFilesFoundSearchingNodeModules` keyed by native `tspath.Path`,
not a substring test. A root under `node_modules` need not have that membership;
do not confuse the separate `/node_modules/` exclusion in other Program paths
with this helper. Faithful runtime telemetry must carry that host membership,
project-reference source association, JS/declaration/JSON facts, forcing mode,
options, config/current-directory/case policy and native derived JSON output
identity. Neither library filename normalization nor a declaration-file flag
substitutes for those facts. This scripts-only slice cannot manufacture them.

`Program.Emit` selects eligible files and queues one emitter per file. Even
under `noEmit`, `emitter.emit` is entered; JS/declaration routines apply the
no-emit gate and publish `EmitSkipped`. `CombineEmitResults([])` starts from
`EmitResult{}`: zero eligible emitters yield a false skipped flag, not true.
`noEmitOnError`, forced emission and cancellation are additional Program gates.
Thus equal `noEmit` prevents intentionally timing generated output, but does not
prove TSR executed native emitter setup/skipping work. Preserve that limitation
when integrating the parent's corrected diagnostic and list-only behavior.

Emit-boundary verification: 25 trace-reader and 27 harness tests passed; the
actual native reader retained 203 completed large-project emit spans (202
per-source plus one Program-level span) despite matching `noEmit`. A fresh
five-pair plus warmup smoke completed with both capture readers valid, matching
explicit effective `noEmit`, observed ratio 0.905128675613575, nine unmet proof
obligations, verified ratio null and `--require-comparable` exit 1. Evidence:
`/tmp/recover-performance-emit-smoke.json` and
`/tmp/recover-native-emit-validation.json`. These observations precede the
parent's pending runtime integration and cannot certify that integration.

### Evidence and remaining proof

The recovery Box has no `tsr-1yb` row or history; `tsr-2zk.17` is the live related
performance issue (already claimed by the parent). Do not replace that issue or
close it on an observation. The certificate now enumerates 16 independent obligations
and reports an explicit unmet-constraint count, never accepts caller booleans as
proof, and remains false for empty checks or matching coarse fingerprints.

Recovery large-project run, source `62b3caf7a9f4aed279381c10a7fc983e5b7f86bb`,
Linux x86_64, pinned native revision above, one warmup per tool and five
alternating fresh pairs, unchanged observed inputs, matching effective options,
loaded identities and the complete intentional TS2322 diagnostic text:

```json
{
  "issue": "tsr-2zk.17",
  "source_sha": "62b3caf7a9f4aed279381c10a7fc983e5b7f86bb",
  "oracle_sha": "5b1047d10d32e7d5b446be4de56b126ff42f82bb",
  "project": "benches/projects/domain-model-large/tsconfig.json",
  "binary_sha256": {
    "tsr": "b2e104692f21571c489d3c88ad636fb11b6a64c957f219ec92bf47bcf6150fa3",
    "tsgo": "8c7a0a760284a95e69eb55df16a2ed2472a30924ba00e226cfcd1e80d4af5400"
  },
  "pairs": 5,
  "warmups_per_tool": 1,
  "median_wall_seconds": {"tsr": 0.8182695000000422, "tsgo": 0.5722015000000056},
  "observed_wall_ratio": 1.4300373207690547,
  "verified_wall_ratio": null,
  "target_verified": false,
  "proof_gap_count": 8,
  "untimed_full_worker_completions": {"tsr_serial": 202, "native_threaded": 202},
  "tsr_untimed_query_completions": {
    "declared_type_query": 411443, "symbol_type_query": 176462,
    "variable_type_worker": 14249
  },
  "highest_observed_native_full_worker": {
    "path": "benches/projects/domain-model-large/src/main.ts",
    "checker_id": 0, "duration_ns": 100021100
  }
}
```

The historical run above had eight unmet proof obligations; the current
certificate adds native emitter-work equivalence as a ninth even when explicit
no-emit options match. Remaining obligations: source-to-binary build attestation; complete
cross-tool query inputs and bundled library bytes; actual **timed** Program
inventory/eligibility; timed full worker completion/cancellation; initialization
and all metadata forcing; thread-preserving private-checker ownership; structured
diagnostic spans/chains/related information/filtering; full-corpus >=99.9% exact
parity with no prior RIGHT loss. Implementing their producers requires the
checker/compiler/oracle owners: this scripts-only slice cannot faithfully add
facts that those producers do not emit. No accepted equivalent-work certificate
or speed win exists. The public smoke's observed ratio 0.938068 is likewise not
a verified ratio. Raw local evidence is `/tmp/recover-performance-{smoke,large}.json`
and their `.json.work` directories; these machine-local paths are not portable
attestations. The separate single-threaded large-project run also completed five
pairs plus warmups, both trace readers accepted its actual CLI captures, and
`--require-comparable` exited 1 as required: observed ratio 2.222994513623873,
verified ratio null. Verification: 24 trace-reader and 27 harness tests passed;
the native and TSR trace-reader CLIs accepted the default large-project artifacts.
The large TSR NDJSON is about 145 MiB, so it is not committed as a script fixture. The embedded JSON preserves the source-qualified observation after
Box destruction; full transient capture must be exported by the parent if needed.

## Resolver input manifests

`bd tsr-1yb.1.2.1` adds `--input-manifest /tmp/inputs.json`. Supply paths from
the actual config/host/resolver observations, including extended configs,
queried package manifests, successful and failed file candidates, directories
whose entries affect discovery, and relevant logical symlink paths. Relative
names are interpreted against the project's config directory:

```json
{
  "schema_version": 1,
  "provenance": {
    "source_sha": "full-source-revision-of-the-query-producer",
    "producer": "source-qualified config/host/resolver query capture"
  },
  "paths": [
    "tsconfig.base.json",
    "node_modules/example/package.json",
    "node_modules/example/missing.d.ts",
    "node_modules/example",
    "linked-package/index.ts"
  ]
}
```

Provenance is recorded caller metadata, not an attestation that paths are
complete or that a binary was built from that revision. The report separately
records the harness hashes, compiler binary hashes, flags, effective configs,
source/oracle revisions and whether the declared producer revision matches the
harness checkout. Include the query producer binary/patch hashes and capture
command in provenance when using temporary instrumentation. Existing
[resolver construction controls](resolver-construction-performance.md) export
local observed-path rows; their `path` values can populate this format, keeping
their original producer identity and partial-coverage limits.

The harness always adds the root config, both compiler binaries, the manifest
file and the union of physically listed loaded sources. Loaded paths join the
reference after discovery; their state before that first observation is not
proved. Every full check then validates the same union. Path spelling remains
exact: `alias/../file` follows a symlink before its parent segment, while
`file/.` and `file/` fail when `file` is regular. Lexical normalization would
observe a different input. Files are streamed into
SHA-256 hashes; missing/file/directory/other kinds remain distinct. Snapshots
record realpaths, symlink spellings along logical ancestors, and directory
entry names/kinds/link targets. Special files are not read. Snapshot errors
invalidate evidence rather than masquerading as missing files. Schema 2's
loaded-input digest uses per-file hashes and is not interchangeable with the
older concatenated-byte digest.

Keep the report outside observed directories: writing an artifact there changes
their entries and correctly invalidates the run. A changed input stops sampling,
saves the failed observation and any measured/rejected full-check sample, and
leaves `target_verified` false. Warmup changes cannot be forgotten before timed
samples start. Fingerprinting warms OS caches; it is not a cold-cache benchmark.

All supplied manifests are reported as partial. Directory-entry capture does
not recursively hash unqueried descendants. Bundled library bytes, environment,
unobserved queries and transient changes between snapshots remain unproved.
`complete_input_equivalence_verified` and `actual_checked_work_verified` remain
false. Parent `bd tsr-1yb.1.2` still owns permanent cross-tool performed-work
telemetry; `bd tsr-1yb.1.2.2` owns cross-tool query capture and its coverage
proof; `bd tsr-1yb.1.1.1` owns CLI trace delivery. The tracked empty-suffix
resolution defect `bd tsr-6.59` is also unaffected.

Run the public controls with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts -p test_whole_project_perf.py -v
```

They exercise manifest byte changes, missing candidate creation, symlink
retargeting, extended/root config changes, directory additions/removals,
compiler replacement and warmup mutations through real child processes. A
matching loaded-list/options/diagnostics control verifies that incomplete work
cannot pass `--require-comparable` or publish a verified ratio.

The [sanitized validation receipt](benchmark-input-controls.json) records 30
passing script tests, five alternating public pairs in each worker mode, and a
single-pair private-app scale check. The app validates more than 51,000 observed
paths, including directory entries, while retaining explicit partial coverage.
The receipt uses the frozen CJS candidate and pinned native binaries; these runs
validate the input protocol and reporting, and do not claim current-source
throughput or progress against the native 0.50 target.

## First paired real-app baseline

Source `8f8f4e1a4fb197c266f79dc5751b3537967e5a68`, pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, Codemod Next.js app, default
scheduling, one warmup and five measured pairs on the same Mac:

| Measurement | TSR | tsgo |
|---|---:|---:|
| Median wall time | 7.321 s | 3.224 s |
| Wall range | 7.215–7.465 s | 3.149–3.454 s |
| Median user CPU | 6.391 s | 16.915 s |
| Median system CPU | 0.925 s | 1.924 s |
| Median peak RSS | 1.118 GB | 4.995 GB |
| Listed files | 13,560 | 13,098 |
| Diagnostics | 141 | 0 |

The observed ratio is **2.271**, against a target of **0.50**. Diagnostic output
was stable within each tool. **This is not a verified equal-work ratio.** Besides
different loaded files, reported options differ in `paths`,
`allowImportingTsExtensions`, `disableSourceOfProjectReferenceRedirect`, and
`tsBuildInfoFile`. Some may be omissions in TSR's config serialization rather than
actual ignored options; establish that in source instead of silently dropping them
from the comparison. The build-info path does not authorize incremental reuse:
both incremental and composite were explicitly disabled.

Detailed local evidence is `/tmp/tsr-1yb-nextjs-baseline.json`. The initial
baseline predates the harness's input-content fingerprint addition. Subsequent
confirmation must use that check. These observations supersede the earlier single
timings as evidence of the problem, not as proof that work matches or that the
optimization is complete.

## Initial phase attribution

The CLI honors `--extendedDiagnostics` and reports actual checks plus
host-clock phase times. Program time combines discovery/resolution, parsing and
binding; opt-in loader statistics now separate those costs below. Reporting
includes diagnostic extraction, source indexing, comment
directives and formatting. Compilation time excludes process startup and final
teardown, so use the external harness for the end-to-end target.

A separate full-app probe on the same source with the new instrumentation reported
13,560 loaded files and **1,341 actually checked**: 0.031 s config, 3.696 s
program construction, 0.000 s checker initialization, 3.700 s checking and 0.089 s
reporting. A `noCheck` control performed zero checks, took 3.283 s wall and
0.926 GB peak RSS. These are locating probes, not a new confirmed benchmark.
Both program construction and checking need improvement; parallelizing only the
checker cannot reach half native's total wall time while loading alone costs
over three seconds. Detailed source and native counter attribution remain in
`bd tsr-1yb.2`.

## Package identity alignment

The loader now follows native `filesparser.go:getProcessedFiles`: the first
source file with a complete package identity wins during deterministic replay.
Name, version, submodule and resolved peer dependencies all participate. Imports
through duplicate physical paths point to the first source file, and replay skips
the duplicate's dependencies. `deduplicatePackages: false` keeps both instances.
Discovery still parses duplicates, as native does; this does not avoid all work
on those copies.

On the real app this removes all 463 TSR-only listed paths, leaving 13,097 TSR
files versus 13,098 native files. Native additionally loads
`@lingui/conf/dist/index.d.ts`; effective-config differences also remain. The
performance comparison therefore remains unverified. The full assertion audit
loses no previously RIGHT rows: four WRONG rows become RIGHT and six newly
aligned rows are RIGHT. One newly aligned global-merge row remains WRONG.
The diagnostic harness checks each canonical source-file identity once and
attributes its spans to the canonical test unit. The pinned global-merge
diagnostic control passes; the full diagnostic suite retains 2,799/5,488 passes.

A separate five-pair comparison alternated saved pre-change and candidate
binaries on the same app, with fresh processes, one warmup per binary, no emit
and incremental/composite disabled. Median wall times were **7.418 s before and
7.428 s after** (ratio 1.0014); all 141 diagnostic fingerprints matched.
This is neutral within observed noise, not a speed improvement. Local evidence
is `/tmp/tsr-1yb-dedup-paired.json`, with binary hashes and every sample.

Compiler controls verify canonical import identities, skipped duplicate-only
dependencies, and separate versions/submodules/peers. A distinct-package class
assignment control also exposes a preexisting checker false negative: with
deduplication disabled native reports incompatible members but both the saved
pre-change binary and candidate accept them. This is tracked as `bd tsr-6.47.1`;
the passing source-file identity controls do not prove that diagnostic works.

## Loader cost attribution

Run the built CLI with `--extendedDiagnostics --noEmit --incremental false
--composite false` and the same project. Add `--noCheck` for a loader-only
control. The added clocks and counters run only when extended diagnostics is
enabled; these fields are locating measurements, not a throughput benchmark.

A separate real-app `noCheck` probe after package alignment reported:

| Measurement | Time |
|---|---:|
| Loader total | 2.960 s |
| File reads | 0.162 s |
| Package/module metadata | 0.057 s |
| Parser, including JSDoc | 0.547 s |
| Discovery, including resolver calls | 2.172 s |
| Resolver calls, within discovery | 2.041 s |
| File/module indexing | 0.054 s |
| Binding and global merges | 0.156 s |
| Program total | 3.194 s |

The resolver made **46,031 module/type-directive queries**, taking about 69% of
the loader time in this probe. Resolver time is a subset of discovery time;
do not add it to discovery again. It excludes lib-replacement queries during
setup. Loader total includes setup/replay but excludes final task-storage
disposal; program total includes that disposal and the unclassified overhead.

The loader parsed **13,560 files**, retained 13,097, and performed zero checks.
A separate full-check probe made the same 46,031 queries, attributed 2.853 s to
the resolver, and checked 1,341 files. Both full-check instrumentation versions
preserved the existing 141 diagnostic fingerprints. Probe timing varies and
does not demonstrate a code speedup. Local evidence is
`/tmp/tsr-1yb-resolver-profile.json`.

This ranks resolver probe/cache reuse ahead of parser or binder work. Count
filesystem probes and reusable native directory/module keys in `bd tsr-1yb.10`
before implementing a cache; package-JSON caching already exists and its trace
behavior is observable. Lazy JSDoc remains a measured candidate, with a smaller
upper bound than the resolver cost on this workload.

## Native resolution query caches

Of 46,031 discovered task queries, 22,265 module requests and 45 type-reference
requests repeated complete native cache keys. The resolver now reuses both
successful and failed results with those keys. Tracing bypasses query-cache
reads, matching pinned tsgo, so traced walks remain observable. Caches are scoped
to one resolver/options/project snapshot; config lookup uses separate semantics.
See [module resolution](module-resolution.md#per-project-query-caches).

Five fresh-process pairs alternated an uncached reference and cache candidate,
both including checker commit `830c97e6`. Both retained the same 13,097 files,
had stable input-content fingerprints, and produced identical 123 diagnostics:

| Measurement | Before | Cached |
|---|---:|---:|
| Median wall | 7.255 s | 6.622 s |
| Wall range | 7.208–7.282 s | 6.616–6.631 s |
| Median user CPU | 6.376 s | 6.022 s |
| Median system CPU | 0.871 s | 0.596 s |
| Median peak RSS | 1.110 GB | 1.093 GB |

This is an **8.7% wall reduction** against the uncached TSR reference, with a
substantial reduction in system CPU. RSS ranges overlap; do not claim a memory
improvement from these medians. The complete 474,251-row assertion verdict files
are byte-identical: 458,036 RIGHT, 2,445 GAP, 13,770 WRONG. Six controls verify
actual filesystem-probe elimination, mode/directory/inferred distinctions,
traced walks, symlink/package identity and fresh-snapshot behavior. A temporary
full-app comparison of cached and freshly computed complete resolver results
also found no differences; that verification instrumentation was removed.

An earlier comparison used a reference predating the concurrent template
assertion fix. Its 141-to-123 diagnostic reduction and four assertion gains
belong to that checker fix and are not cache benefits. The isolated comparison
above supersedes it. Detailed local evidence, including distinct binary hashes,
is `/tmp/tsr-1yb-resolution-cache-isolated-paired.json`.

A separate five-pair pinned-native comparison observed a TSR median of 6.705 s
and tsgo median of 3.230 s (observed ratio 2.076). Both used fresh processes,
warmed filesystem inputs and disabled incremental/composite reuse. TSR retained
13,097 files and reported 123 diagnostics; tsgo retained 13,098 and reported
none. The remaining native-only Lingui declaration and effective-config output
differences keep the benchmark **incomparable**. This does not verify the 0.50
target. Local evidence is `/tmp/tsr-1yb-nextjs-resolution-cache.json`; follow-up
alignment is tracked in `tsr-1yb.1.1`.

## Native compiler-host filesystem cache

A temporary wrapper measured actual backing-host operations during program
loading after query caching; configuration discovery was outside this probe.
It found 114,266 repeated metadata
and real-path probes. The CLI now wraps its compilation host with native
`cachedvfs` semantics; content reads remain uncached.

| Operation | Before calls | Cached backing calls |
|---|---:|---:|
| File existence | 72,745 | 41,238 |
| Directory existence | 78,858 | 6,155 |
| Real path | 11,066 | 1,010 |
| Content read | 14,213 | 14,213 |

All cached backing-call paths were unique. This workload made no directory-entry
queries; unit controls cover cached entries, including empty results. Repeated
underlying calls accounted for 0.240 s in the locating probe. Tracking overhead
is excluded from that sum, and the instrumented total is not a speed benchmark.
The temporary wrapper was removed before the final build.

Five fresh-process pairs compared source `f1389da9` with and without this wrapper:

| Measurement | Before | Cached |
|---|---:|---:|
| Median wall | 6.629 s | 6.478 s |
| Wall range | 6.525–6.661 s | 6.363–6.494 s |
| Median user CPU | 5.999 s | 6.062 s |
| Median system CPU | 0.619 s | 0.398 s |
| Median peak RSS | 1.123 GB | 1.101 GB |

The measured wall reduction is **2.3%**; system CPU fell by about 36%, offset in
part by cache lookup/storage CPU. RSS ranges overlap, so the result establishes
no memory reduction. Both sides retained the same 13,097 files, stable content
fingerprints, effective options and 123 diagnostics. The public generic-imports
fixture also retains identical diagnostics and three checked files; output
differs only in phase times. The resolution and loader oracle suites now use the
cached host, exercising all 95 resolver transcripts and 96 loader cases.
All 474,251 assertion rows are byte-identical against the same-source reference:
458,472 RIGHT, 2,436 GAP, 13,343 WRONG. These tallies include concurrent JSDoc
checker changes, whose gains are independent of this cache. CLI baseline replay
also has identical output: 33 of 43 judged cases pass, with ten existing failures.

Local evidence is `/tmp/tsr-1yb-cached-vfs-paired.json`; its `tsgo` tool slot holds
the saved **TSR reference binary**, not native tsgo. This is an isolated TSR
comparison and does not prove the overall 0.50 target. The full-app CLI trace
control emitted zero bytes on both versions and is invalid as trace evidence;
trace forwarding is tracked in `tsr-1yb.1.1.1`.

## Registered type-parameter membership

A symbolized sample delayed into checking on source `52585a08` located a large
allocation cost: 853 of 3,482 main-thread samples were in registry-vector
collection called by conditional evaluation. Temporary counters found that
conditional, mapped-context and conditional-extends queries copied **795 million
parameter IDs** across 37,629 requests. The registry reached 42,150 entries,
while the largest visited type graph had 299 nodes. These samples and counters
locate the cost; their instrumented durations are not throughput measurements.

Conditional and mapped-context predicates now query the existing checker-owned
registry directly. Conditional-extends evaluation only collects candidate IDs
after the graph confirms it contains a registered parameter. Both predicates
use the same graph walk, traversal order and cycle guard as explicit inference
parameter queries; those explicit queries retain their printed-name fallback.
No answers are memoized, so changes to the current registry remain visible.

Five alternating fresh-process pairs compared the clean saved `52585a08` TSR
binary with this change, with one warmup per binary:

| Measurement | Before | Direct membership |
|---|---:|---:|
| Median wall | 6.452 s | 4.987 s |
| Wall range | 6.380–6.606 s | 4.951–4.997 s |
| Median user CPU | 6.051 s | 4.564 s |
| Median system CPU | 0.400 s | 0.388 s |
| Median peak RSS | 1.132 GB | 1.083 GB |

This is a **22.7% wall reduction** against the saved TSR reference. Both sides
retain the same 13,097 files, effective options, input-content fingerprints and
123 diagnostics. All 474,251 assertion rows are byte-identical: 458,472 RIGHT,
2,436 GAP and 13,343 WRONG, with zero previously RIGHT losses.

Focused controls cover registered versus unregistered same-named parameters,
cyclic references, nested signatures, constraints/defaults, primitives, explicit
printed-name recovery and registry updates. The bounded unresolved Flatten
fixture produces the same diagnostic as pinned native; the MCP fixture checks
clean with all three binaries. The public generic-imports fixture retains the
same three TSR diagnostics and three actual checked files; its existing native
comparison reports one diagnostic, so that fixture does not establish parity.

Local evidence is `/tmp/tsr-1yb-parameter-membership-paired.json` and
`/tmp/tsr-1yb-parameter-membership-assertion-audit.json`. The benchmark's `tsgo`
slot contains the saved **TSR reference**, not native tsgo. This confirms the
isolated optimization, while the native 0.50 target remains unverified and
workload alignment remains open.

## Indexed Program file metadata

After the registry change, a delayed symbolized sample on `cfcbcfab` still
attributed 148 self samples to `Program::jsx_factory_namespace`, with another
13 inclusive samples in declaration-file lookup. The sampled main thread had
2,563 samples across the end of loading, checking, reporting and teardown.
Those counts locate the repeated scans; they are not whole-process cost shares.

Both host queries now use the existing immutable `files_by_source_file` index.
They preserve the old root/referenced-file boundary, which excludes the bundled
lib prefix. Missing IDs still yield no namespace and a false declaration-file
answer. Each constructor allocates source roots in the program's shared node
table, so distinct parsed files have distinct IDs even when their paths
canonicalize alike. Package redirects query the canonical retained file's ID.

Five alternating fresh-process pairs compared saved `cfcbcfab` TSR with this
change, with one warmup per binary:

| Measurement | Before | Indexed |
|---|---:|---:|
| Median wall | 5.043 s | 4.842 s |
| Wall range | 5.012–5.234 s | 4.791–4.849 s |
| Median user CPU | 4.590 s | 4.417 s |
| Median system CPU | 0.437 s | 0.415 s |
| Median peak RSS | 1.096 GB | 1.112 GB |

The isolated wall reduction is **4.0%**. RSS ranges overlap; this result does
not establish a memory change. Both sides retain the same 13,097 loaded files,
effective options, stable content fingerprints and 123 diagnostics.

Controls cover distinct per-file JSX pragmas, same-path inputs with different
source IDs, TS/TSX and `.d.ts`/`.d.mts`/`.d.cts` classification, imported files,
the bundled-lib boundary, unknown IDs and canonical package redirects. The
excluded bundled-lib metadata domain is a preexisting fidelity question tracked
in `tsr-1yb.7.2.1`, under the checker port epic; this lookup experiment preserves
it rather than using a semantic change as performance evidence.

All 474,251 assertion rows are byte-identical to the same-source reference:
458,472 RIGHT, 2,436 GAP and 13,343 WRONG, with zero previously RIGHT losses.
The 84 affected compiler/execute tests, including the shared-Program ownership
controls, pass, as do all-target clippy and formatting checks.

Local evidence is `/tmp/tsr-1yb-program-file-queries-paired.json`; its `tsgo`
slot contains the saved **TSR reference**, not native tsgo. The isolated gain
does not verify the overall native 0.50 target.

A separate five-pair pinned-native run after this change observed **5.559 s TSR
versus 3.305 s tsgo**, ratio **1.682**. It used the same TSR binary as the isolated
comparison above, but alternated against native rather than the saved TSR
reference. Do not combine medians from those different pairings into a ratio.
TSR user/system CPU medians were 4.552/0.458 s and peak RSS 1.130 GB; native
medians were 17.364/1.640 s and 5.023 GB. The additional TSR wall time over CPU in
this pairing has not been attributed.

The native comparison remains **incomparable**: 13,097 versus 13,098 files,
123 versus zero diagnostics, and effective-config differences. Input content
remained stable; processes were fresh and incremental/composite reuse disabled.
The 0.50 target is still unmet and unverified. Local evidence is
`/tmp/tsr-1yb-nextjs-after-file-index.json`. Worker memory/scaling and remaining
loader CPU are the next larger opportunities; checked-scope telemetry and
workload alignment remain required before verifying the overall target.

## Package JSON string validation

A two-second early-loader sample with `noCheck` on the saved `efddbfe0` binary
captured 1,666 main-thread stacks. UTF-8 validation had 630 self samples, all
under `tsr_module::json::Parser::string`. For each ordinary string character,
that reader validated the entire remaining document suffix, then decoded one
character. The public parser already receives a valid `&str`; repeated suffix
validation makes this part of reading large package files quadratic.

The reader now retains that validated text alongside its byte view and uses
safe string slicing to decode the next character. It preserves cursor-boundary
failure, escapes, declaration order, duplicate-key last position and existing
malformed-input behavior. It adds no unsafe conversion, cache or dependency.

Five alternating fresh-process pairs compared saved source `760513fa` with this
change, after one warmup per binary:

| Measurement | Before | Validated text view |
|---|---:|---:|
| Median wall | 4.935 s | 4.140 s |
| Wall range | 4.858–5.041 s | 4.061–4.194 s |
| Median user CPU | 4.492 s | 3.711 s |
| Median system CPU | 0.397 s | 0.384 s |
| Median peak RSS | 1.093 GB | 1.077 GB |

The isolated wall reduction is **16.1%**. RSS ranges overlap, so the result
establishes no memory reduction. Both binaries retain 13,097 loaded files,
effective options, stable input-content fingerprints and identical complete
123 diagnostics. Separate extended-diagnostics controls confirm 1,341 actually
checked files and 13,560 parsed files on both sides.

All 474,251 assertion rows are byte-identical to the same-source reference:
458,508 RIGHT, 2,433 GAP and 13,310 WRONG, with zero previously RIGHT losses.
The tallies include concurrent checker fixes already present in the reference;
they are not benefits of this optimization. All 49 module tests, 95 native
resolver transcripts and 96 loader cases pass, as do all-target module clippy,
formatting and whitespace checks. Focused controls cover mixed 2/3/4-byte UTF-8,
escaped delimiters and surrogate pairs, and malformed strings after multibyte
characters.

A separate candidate `noCheck` locating probe attributed 0.423 s to resolution
and 1.624 s to Program construction. Its sample has no whole-suffix UTF-8
validation stacks under JSON string parsing. The captured stages differ from
the earlier early-loader sample, so these profile times and counts do not
establish a throughput ratio; the paired full-check result above does.

Local evidence is `/tmp/tsr-1yb-json-utf8-paired.json`,
`/tmp/tsr-1yb-json-utf8-assertion-audit.json` and
`/tmp/tsr-1yb-json-utf8-checked-scope.json`. The paired harness's `tsgo` slot
contains the saved **TSR reference**, not native tsgo. This isolated win does
not verify the overall native 0.50 target, and production checking remains serial.

A separate five-pair pinned-native observation after this change measured
**4.287 s TSR versus 3.414 s tsgo**, observed ratio **1.256**. This run is
incomparable: TSR retains 13,097 files and reports 123 diagnostics; native
retains 13,098 and reports none, with the same native-only Lingui declaration
and effective-config differences as before. Inputs remain stable and
incremental/composite reuse is disabled. Do not combine the isolated 4.140 s
median with this native run's median. The 0.50 target remains unmet and
unverified. Local evidence is `/tmp/tsr-1yb-nextjs-json-utf8-native.json`.

## Structured property-name enumeration experiment

A checker-phase sample on `81216b80` captured 1,699 main-thread stacks.
Disjoint attribution to the nearest TSR owner placed 138 samples under
`collect_structured_property_names`, including 61 allocator self samples.
The captured interval covers part of checking; these counts are not a share
of complete CLI time.

Temporary opt-in counters recorded 222,029 structured visits across 3,615
owner symbols, with at most 25,471 visits to one owner. The existing loop
copied 3,262,955 own names (30,779,136 name bytes) into temporary vectors.
Only 34,699 copies were subsequently discarded as duplicates. Temporary
vector capacity summed to 116,079,456 bytes over the run; this is neither
total heap allocation nor peak live memory. Instrumentation was removed
before building the comparison binaries.

The candidate enumerated immutable binder members directly, allocating a
name only when adding it to the result. It removed the intermediate vector
while retaining most final name copies. Three independent five-pair runs,
each alternating fresh processes after one warmup per binary, produced:

| Reference source | Before median wall | Candidate median wall | Outcome |
|---|---:|---:|---|
| `81216b80`, initial | 5.068 s | 4.987 s | 1.6% observed reduction |
| `81216b80`, confirmation | 5.123 s | 4.949 s | 3.4% observed reduction |
| `4bf5fe96`, integration | 4.929 s | 4.971 s | 0.9% observed increase; rejected |

Every sample is retained. The confirmation run includes large final-pair
outliers (9.840 s before, 5.962 s candidate); no memory benefit is established.
The integration result fails the isolated wall-time decision's unchanged
0.020 s absolute threshold. The production loop was restored rather than
shipping a gain that did not repeat on current source.

On `4bf5fe96`, both binaries retain identical effective options, input
fingerprints, 13,097 loaded files and complete 123 diagnostics. Separate
extended-diagnostics controls confirm 1,341 checked and 13,560 parsed files
on both sides. All 474,251 assertion rows are byte-identical: 459,381 RIGHT,
2,198 GAP and 12,672 WRONG, with zero previously RIGHT losses. These totals
reflect the concurrent fidelity checkpoint, not an optimization benefit.
Retained unit controls cover diamond inheritance and duplicate names,
type-parameter exclusion, computed members, stable repeat ordering, cycles
and unfollowable bases.

Local evidence is `/tmp/tsr-1yb-property-names-paired.json`,
`/tmp/tsr-1yb-property-names-confirmation.json`,
`/tmp/tsr-1yb-member-current-paired.json`,
`/tmp/tsr-1yb-member-current-audit.json` and
`/tmp/tsr-1yb-member-current-telemetry.json`. Each paired harness's `tsgo`
slot holds a saved TSR reference. These runs establish no native speed ratio.

The repeated visits justify investigating completed structured-member reuse
under `tsr-1yb.4.1.1`. Native `resolveStructuredTypeMembers` retains members
per concrete type, including instantiated arguments and completion state;
an owner-symbol-only name cache would not implement that contract. The
overall native 0.50 target remains unmet and unverified.

The [structured-member reuse contract](checker-member-cache-contract.md)
records the native publication states, current TSR storage and executable
identity/re-entry controls. It hands concrete-type attribution and the
measured builder implementation to `tsr-1yb.4.2`; it introduces no production
cache or throughput claim.

The [ambient lookup experiment](ambient-module-lookup-performance.md) reduces
temporary quoted-key construction under `tsr-1yb.7.6`. Two independent
five-pair TSR comparisons confirm 1.943% and 0.948% median wall reductions,
with identical complete corpus results and actual checked-file identities.
The separate pinned-native comparison remains incomparable; the required
verified native wall ratio stays 0.50.

## Native checker pool

The CLI now checks with native `checkerpool.go` scheduling: `singleThreaded`
gives one checker on the calling thread, otherwise `checkers` (default 4,
at most 256, at most the file count) checkers run on their own workers.
Program file `i`, libraries included, belongs to checker `i % count`; each
checker visits only its own files in program order, and publishes only
diagnostics located in files it owns, as native `GetSemanticDiagnostics`
asks the file's associated checker. Checkers share the program, binder and
node tables read-only and keep private type stores. The opt-in work trace
observes one private checker, so a traced run keeps a pool of one.
`checker_pool.rs` records the ownership and work boundary.

Nine alternating pairs against pinned tsgo on the 14-CPU Linux box:

| Project | Before TSR | Pool TSR | tsgo | Ratio before | Ratio after |
|---|---:|---:|---:|---:|---:|
| domain-model | 403 ms | 259 ms | 219 ms | 1.838 | 1.181 |
| generic-imports | 143 ms | 144 ms | 123 ms | 1.191 | 1.171 |

Diagnostics match tsgo on both projects. The smoke fixture is dominated by
library loading (66 files, 2.8 MB), not checking.

## Embedded default libraries

After the checker pool, the smoke fixture spent most of its program time on
library I/O: 66 opens and reads of 2.8 MB, plus a copy of each text into the
arena. Native's default build embeds the libraries (`internal/bundled/embed.go`)
and its CLI wraps the OS file system with `bundled.WrapFS`, so library loading
opens nothing. `tsr_vfs::BundledFileSystem` ports that wrapper: `build.rs`
embeds every pinned `vendor/.../internal/bundled/libs/*.d.ts`, the CLI's
default library path is native's `bundled:///libs`, and
`FileSystem::read_static` lends the loader the embedded text so it is never
copied. `--listFiles` and library locations now print native's paths.
`TSR_LIB_PATH` still selects an on-disk directory.

| Project | Before TSR | Embedded TSR | tsgo | Ratio before | Ratio after |
|---|---:|---:|---:|---:|---:|
| domain-model | 259 ms | 215 ms | 219 ms | 1.181 | 0.983 |
| generic-imports | 144 ms | 101 ms | 121 ms | 1.171 | 0.828 |

Nine alternating pairs on the same 14-CPU Linux box; loaded scope, options
and diagnostics match tsgo on both. This box's file opens are unusually slow
(about 0.1–0.7 ms each), which inflates the I/O share relative to a local SSD.

## Concurrent root reads and lazy diagnostic indexing

Two smaller boundaries follow native. `run_compilation` built a line map and a
text copy for every program file before any diagnostic existed; it now indexes
only files that hold a diagnostic, as native `ECMALineMap` is computed on first
use. The loader reads all root files ahead of its walk through
`FileSystem::read_files`, which `OsFileSystem` answers on concurrent workers
(native `filesParser.start` loads queued tasks concurrently) and every other
host answers serially. Parsing, node numbering and replay order stay on the
walk; see [threading](threading.md#not-yet-built-in-production). Valid
BOM-less UTF-8 now becomes the read `String` without a second copy.

| Project | Embedded TSR | + lazy index | + root reads | tsgo | Final ratio |
|---|---:|---:|---:|---:|---:|
| domain-model | 215 ms | 207 ms | 199 ms | 216 ms | 0.921 |
| generic-imports | 101 ms | 94 ms | 96 ms | 122 ms | 0.786 |

The remaining program time on the smoke fixture is parsing (37 ms) and binding
(12 ms) of the 2.8 MB of libraries on one thread, where native parses files
concurrently. Parallel parsing needs per-worker arenas and node-id assignment
that keeps today's deterministic numbering (`bd tsr-1yb.5`); it is outside the
loader and is the next lever toward the 0.50 target.

## Attribution after the native work boundaries

`--extendedDiagnostics` on the final tree (same Linux box, warm cache):

| Phase | generic-imports | domain-model |
|---|---:|---:|
| Config | 2 ms | 3 ms |
| Loader (reads, discovery) | 8 ms | 18 ms |
| Parse (serial, JSDoc eager) | 39 ms | 46 ms |
| Bind (serial) | 12 ms | 16 ms |
| Checker pool (4 workers) | 5 ms | 81 ms |
| Reporting | 0 ms | 2 ms |
| Compilation | 66 ms | 165 ms |

Nine pairs against pinned tsgo: generic-imports 95 ms vs 127 ms (ratio
0.752), domain-model 198 ms vs 216 ms (ratio 0.916). Process creation and exit
add about 25 ms to both tools on this box. The levers left are outside the
loader and execute crates; each was measured with a throwaway patch that was
not committed:

- **Lazy JSDoc for TypeScript files.** Native `withJSDoc`
  (`internal/parser/jsdoc.go:56`) defers JSDoc parsing for non-JS files to
  first access (`parseJSDocForNode`). Skipping JSDoc for library files alone
  cut parse time from 37 to 21 ms and compilation from 65 to 48 ms on
  generic-imports.
- **Pattern ambient modules collected once.** `has_pattern_ambient_module`
  scans every global name for `*` on each module-specifier query; native
  collects `patternAmbientModules` once in `initializeChecker`
  (`checker.go:1318`). Caching the answer cut serial check time from 229 to
  175 ms and pooled check time from 81 to 60 ms on domain-model.
- **Parallel parse and bind** (`filesParser.start`, `BindSourceFiles`) need
  per-worker arenas and deterministic node and symbol id assignment
  (`bd tsr-1yb.5`, ADR-0034).

## Pattern ambient modules collected once

A frame-pointer `perf` profile of 20 domain-model runs put 19.5% of all
samples in `module_specifier_unfindable` and the `memchr` under it:
`has_pattern_ambient_module` scanned every global name for `*` on each
module-specifier query, including the ones `get_type_of_alias` makes for
every imported call or `new`. Native fills `c.patternAmbientModules` once in
`initializeChecker` (`checker.go:1318`). The checker now takes the same
answer once at construction (`Checker::has_pattern_ambient_modules`; owner:
the checker, one answer per program, published at construction, never
invalidated because the binder's globals are immutable for the checker's
lifetime). Pooled check time on domain-model fell from 75 to 56 ms.

| Project | TSR before | TSR after | tsgo | Ratio before | Ratio after |
|---|---:|---:|---:|---:|---:|
| domain-model | 191 ms | 173 ms | 215 ms | 0.897 | 0.805 |
| generic-imports | 93 ms | 94 ms | 118 ms | 0.786 | 0.790 |

Fifteen alternating pairs each on the 14-CPU Linux box; diagnostics match
tsgo and are identical across 20 runs of each project. All conformance
verdicts are byte-identical.

## Comment directives without a rescan

Native collects `@ts-expect-error`/`@ts-ignore` while its parser scans each
comment (`processCommentDirective`, `scanner.go:972`); the CLI rescanned every
checked file's text afterwards. Every directive spells `@ts-`
(`scanner.go:1003`), so `directives_in` now returns at once for a text
without it. Reporting time on domain-model fell from 2 ms to under 1 ms; the
conformance harness keeps its own reader and is unaffected.

## Remaining levers, measured (perf-2)

Linux 14-CPU box, after the two changes above. Domain-model compilation is
about 140 ms: config 3, loader 62 (reads 8, metadata 2, parse 44, reference
walks 4), bind 15, checker pool 56, reporting <1. Process creation and exit
take another ~12 ms (`tsr -v` is 11.8 ms against 6 ms for `/bin/true` on
this VM). Each lever below was measured with a throwaway probe that was not
committed:

| Lever | Owner | Measured effect |
|---|---|---|
| Lazy JSDoc for TS files (`withJSDoc`, `parseJSDocForNode`) | parser | parse of the 63 libs plus sources, min of 15: 32.3 → 16.6 ms (generic-imports), 36.3 → 20.6 ms (domain-model) |
| `PossiblyContainsDynamicImport`/`ImportMeta` source flags (`parser.go:3027`, `:5183`-`:5195`) gating the loader's two full-tree walks (`references.go:16`, `parseoptions.go:102`) | parser | the walks cost 3.2 ms (generic-imports) and 3.9 ms (domain-model) on the main thread, almost all over library files that set neither flag |
| Parallel parse of files (`filesParser.start`) | program/AST | `lib.dom.d.ts` is 26.8 of 36.3 ms of parsing, so file parallelism saves at most 9.5 ms (5.5 ms on generic-imports), 6.7 ms after lazy JSDoc |
| Parallel bind (`BindSourceFiles`) | binder | `lib.dom.d.ts` is 7.7 of 15 ms of binding: at most 7 ms |
| `std::env::var` debug probes on checker hot paths (`symbols.rs`, `flow.rs`, `declared.rs`, `members.rs`, `optionality.rs`) | checker | 0.6% of domain-model samples in `getenv`, which takes the process environment lock across the pooled checkers |
| Allocator (mimalloc v3 tried as the global allocator) | — | 155 → 144 ms on domain-model, but peak RSS 47 → 117 MB; the gain is transparent huge pages (`no_thp` erases it: 155 ms) and with eager arena commit off it is slower than glibc (159 ms). Not adopted |
| `dist` profile (thin LTO, one codegen unit) | — | within noise of `release` on both projects |
| Leaving program, arena and checkers to process exit (native `os.Exit(runMain())`) | driver | 159.8 → 158.5 ms on domain-model, inside the ±1.1 ms run-to-run error; the kernel still unmaps the pages at exit. Not adopted |

Native has the same `lib.dom.d.ts` floor: its reported parse time on
generic-imports (40 ms) is that file on one goroutine. On these benchmarks
the parse phase therefore improves through per-file parse cost (lazy JSDoc,
the flag-gated walks) rather than through file parallelism.

### Why parallel parse and bind are not built yet

Deterministic parallel parsing needs every file's node ids before its nodes
are written, and the parser writes each id into the node itself
(`node_id: Option<NodeId>`, a plain field generated by `xtask/src/gen_nodes.rs`,
read directly at about 900 sites outside generated code). A file's base id
is the node count of every file before it in walk order, which is unknown
until those files are parsed. Parsing into worker-local tables and rebasing
afterwards would mutate ids through shared references, which the
`Sync`, `Cell`-free AST (ADR-0012) rules out; reparsing to count costs as
much as the parse it saves. The workable designs each cross lanes:

1. Make the generated id slot rebasable (an atomic slot behind a
   `node_id()` accessor) and migrate every direct field read; workers then
   parse with local ids into leased child arenas, and the coordinator
   assigns bases in walk order and rebases ids, parents and JSDoc keys, so
   numbering stays exactly today's.
2. Bind each file into its own store and merge with `SymbolId`/`FlowId`
   offsets. `BindResult` keeps dense node-indexed vectors (`node_symbols`,
   `node_flow`) and performs global merges during binding, in file order, so
   the merge is binder semantics rather than an entry-point change.

Both are recorded here for `tsr-1yb.5`; on the two public projects their
combined ceiling is about 16 ms of the domain-model wall and 9 ms of
generic-imports, below the parser-side levers above.
