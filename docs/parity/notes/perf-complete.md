# Complete-work performance and diagnostic evidence

This lane owns tsr-2zk.17, diagnostic-order child tsr-2zk.17.9, queued comparator
consumer tsr-2zk.22.1 and native temporary-overlay telemetry tsr-2zk.17.10.
Pinned native: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.

## Diagnostic-order evidence root (.17.9)

The benchmark fingerprint now preserves rendered diagnostic order, duplicate
multiplicity, codes, messages, printed positions, paths and continuation text.
It does not expose unprinted span lengths. Native Program.SortAndDeduplicateDiagnostics
establishes order; sorting again in the harness hid reversals. Native ten-TS2318
reversal control now rejects reordered output. Duplicate two-file roots emit the
same two TS2322 diagnostics once in both compilers. With --noLib native emits ten
TS2318 globals while TSR emits two file TS2322 diagnostics: a real checker
initializeChecker global-type producer gap, not harness equivalence.

Frozen source 5dd3bad84d12991e1ba169d2d5687321e1989740; TSR binary SHA256
210ad47f20e6bf4f286acd4151974744178d19a96842944bfc392859b9892e52;
native SHA256 7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302.
Types: 477970 verdicts (469765 RIGHT, 7212 WRONG, 993 GAP). Diagnostics: 10570
(4221 RIGHT, 4968 EMPTY_RIGHT, 1281 WRONG, 100 EMPTY_WRONG).
Full before/after keys and verdicts agree, zero RIGHT type-line losses and zero
RIGHT/EMPTY_RIGHT diagnostic losses. Fresh post-root actual-child receipts with
1800-second deadlines: types exit 0 in 250.136s, diagnostics exit 0 in 222.197s,
full scratch-bound coverage exit 0 in 776.315s; no timeout. Earlier kill-0 waits
are not completion evidence. Full coverage: 12444 discovered sources,
checker_types 8042/9538 (84.32%, 98.10% lines), diagnostics 4221/5502 (76.72%).
This is not complete variant/message-chain/span-length coverage.

Workspace release tests completed successfully; pinned 1.96.0 clippy all-targets
-D warnings and fmt --check clean. 56 script tests passed after duplicate/global
controls. Stable emitted an existing fetch_update deprecation warning. Initial
report tests under build/corpus contention timed out; isolated report tests passed.
Coverage uses proot scratch snapshot binding; mount namespaces were denied.
No corpus/snapshot/model/checker/parser/binder/setup/lockfile changes were made.

## Performance (.17): observations, not accepted gains

Fresh alternating self-comparisons after local verification finished:
domain-model-large wall 1.00444763, CPU 0.99988340 (21 pairs);
generic-imports wall 1.00267985, CPU 1.00137345 (21 pairs);
domain-model wall 0.99704000, CPU 1.00183399 (41-pair noise follow-up).
Compiler binary unchanged, no speed gain. Local native run: TSR 1.0232078s,
native 0.6371528s, observed 1.60590646 (21 pairs). External load not excluded.
Integrator-reported current source 5dd3bad8 observation: 2.662059755 (21 pairs),
TSR samples 0.644–2.589s/native 0.227–1.851s; earlier corpus-contended 3.6683.
All are observations, not uncontended receipts. Complete input/actual-work
flags remain false, verified ratio null, target_verified false. Global .17 stays
open until verified equivalent complete work reaches median wall <=0.50.

Actual production perf capture: 2146 samples, none lost. Checker shares 42.68%,
17.24%,16.68%,15.38%; caller 7.88%,dependency parser 0.14%. Aggregated self:
malloc 5.31%,BindResult::resolve_name 4.89%,cfree 2.61%,SymbolTableField::get
2.42%,check_node 2.24%,get_type_at_flow_node 2.10%. Owned Program lookups on
checker-0: source_file_by_path 0.75%,mode_for_usage_location 0.61%,resolution
0.61%,resolved_module_in_mode 0.28%. resolution reads existing tables, not the
resolver; mode lookup reads metadata/usage syntax. PreparedNames::new 0.14%,
ordered bind wrapper 0.23%. Native BindSourceFiles queues unbound files; TSR
private bind results require ordered relocation/global publication. Removing it
requires binder identity changes and measured publish_file work, not scheduling
intuition. No duplicate cache, path heuristic, shifted timer or speculative
scheduler change. Pinned pool defaults to 4, serial 1, or explicit Checkers;
clamped to filecount/256, not available-core count.

## Native temporary overlay experiment (.17.10)

Ownership extension permits temporary Go overlays outside tracked vendor. Actual
successful controls use Go -overlay with copies in /tmp/box/native-overlay:
core collector,compiler/checkerpool.go,checker/checker.go,osvfs/os.go,cmd main.
Default owner scheduling/file affinity is preserved. Added observations:
constructor intervals and checker identities; file association; exclusive lease,
checker-group/parallel scope; initialization; source-file query versus actual
worker and completed publication; symbol queries and variable workers; OS file
reads/contents,successful and negative existence, directory entries and realpath;
invocation PID/args/environment and normal completion. No output suppression.

Native domain-model controls default,serial,explicit two-checker all exited 1
normally and matched noninstrumented native stdout/stderr/exit exactly. Pool
counts were 4/1/2, matching existing TSR checker_pool::checker_count for the
same default/singleThreaded/explicit-two options and loaded file count. Default
count identity is explicitly 4, never host core count; file affinity remains
Program file index modulo selected count. Full workers were 42 in each;
record counts 171709/108845/131222;
negative file probes 75/7/43. Every work begin/end paired and each stream ended
normally. Different query counts demonstrate why serial receipts cannot certify
production admission. Collector buffering is outside semantic state but adds
observer synchronization/allocations: instrumented runs are correctness-only,
never timed ratios. Initial unbuffered large-project capture timed out at 180s,
801878 records, no completion: rejected, not proof. Buffered controls completed.

### Minimal serialized TSR checker interface request

Integrator must route these additions through ONE checker owner; this lane edits
no checker files. Existing WorkObserver begin/end(token,panicking) and Operation
can remain the worker-span interface. Required minimal extension:

- Add InitializeChecker, SourceFileQuery, SourceFileWorker and explicit
  SourceFileCompleted; preserve query vs actual worker vs successful publication.
- Add bounded cache publication observations at the existing symbol/declared/
  variable worker owners: absent,active,completed-success/failure,unsupported;
  query identity must include private checker,symbol/declaration identity,
  receiver/alias/options context when relevant. No printed-name keys.
- A per-checker observer is supplied by owned checker_pool::check_program_files
  configure callback carrying owner index. Owned compile.rs must stop forcing
  one checker ONLY after the sink/consumer accepts per-owner contexts; preserve
  native file affinity and constructor/group intervals. Existing serial receipt
  remains separately labeled until that atomic cutover.
- Emit actual full-check completion in check_source_file's publication branch,
  not merely driver invocation; initialization scope around real constructor
  initialization, not a guessed driver timer. Native counterparts are
  checker.initializeChecker/checkSourceFile/getTypeOfVariableOrParameterOrPropertyWorker.

Observed four operations do not certify all forcing. Full native input coverage
still needs bundled libs (outside osvfs), config/resolver logical queries before
cache admission,Stat/WalkDir,environment reads and option provenance. Complete-input
oracle .47 and host extension .16.59.1 retain their ownership; coordinate event
coverage rather than invent duplicate caches. Native side interface currently
uses sequence,monotonic elapsed_ns,owner pointer; span begin has operation and
file/symbol/pool identity, end references token. Export final schema only after
these missing boundaries and TSR hooks are serialized. Temporary overlays are
not committed as a production implementation or a complete proof artifact.

## Comparator consumer dependency (.22.1)

Model commit b6d2104f is absent locally; fetch of short revision fails (no remote
ref). Transfer full reachable model commit before atomic compile.rs cutover.
Native program.go SortAndDeduplicateDiagnostics: clone pointer list,sort full
CompareDiagnostics,longer chains first,group EqualDiagnosticsNoRelatedInfo,
concat related infos,sort/dedup full equality,clone retained head only when
related info exists. Preserve Program source image; no per-diagnostic file clones
or head-only sort shim. No guessed API imports or model-file edits made.
