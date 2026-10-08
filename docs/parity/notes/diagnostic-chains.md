# Diagnostic-chain recovery — tsr-2zk.22

## Native boundary and recovery

Pin: `vendor/typescript-go` at `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Recovery base: `0e7824dd`. Inspected saved `box/parity-diagnostic-final` history,
including `b6d2104f` and `7c69cc64`; recovered only diagnostics representation,
comparison/formatting, assignreport/relater producer hunks and dedicated tests.
No old branch merge, snapshot/configuration changes, or compiler/execute edits.
Historical receipts in the saved branch are not current passes.

Pinned `ast.Diagnostic`, `NewDiagnosticChain`, `CompareDiagnostics` and
`EqualDiagnostics` preserve ordered explanation trees and independently located
related notes. Comparison orders paths (UTF-8 bytes), start, end, head code,
ordered arguments, descending recursive child counts, child arguments, then
related-note counts/content. Child codes deliberately affect equality but not
comparison. Category and runtime flags affect neither. Native compiler
`SortAndDeduplicateDiagnostics`/`compactAndMergeRelatedInfos` merge adjacent equal
heads/trees and sort/compact their related notes using full native equality.
Do not use comparator equality as diagnostic equality.

`Diagnostic::text()` remains the localized head. The diagnosticwriter port
flattens insertion-ordered children with two spaces per level and the requested
newline; only pretty output prints related information at its own source image.
`LocatedDiagnostic::compare` uses externally supplied Program primary locations.
`sort_and_deduplicate_diagnostics` requires attached primary files.
`compact_and_merge_related_infos(Vec<Diagnostic>) -> Vec<Diagnostic>` exposes
native already-sorted compaction without performing a second sort. Its input must
already use `compare_diagnostics`, not a head-only order. Both functions consume
owned diagnostics; external file/index side tables must be rebuilt or carried at
the consumer boundary, never zipped against pre-compaction positions.

Runtime category, unnecessary and deprecated booleans are stored in each
Diagnostic, initialized from the template by native NewDiagnostic semantics.
Explicit setters accept runtime/serialized values; getters never infer current
payloads from the catalogue. `set_category` mirrors native SetCategory; formatter
severity and error-summary counting read the runtime category. NoEmit state is
independent, stored in optional details; Clone retains all runtime state.
NewDiagnosticChain initializes its parent's payload from the parent's message,
not the child, and inherits only child location/related information. Exported
consumer APIs include `category`, `reports_unnecessary`, `reports_deprecated`,
`skipped_on_no_emit` and their setters.

## Ownership, publication, context and work

Trees belong to individual diagnostics, not a semantic relation cache. Optional
boxed details keep head-only diagnostics free of tree allocation; related lists
use Arc/COW and source images use shared `Arc<DiagnosticFile>` with one line map
per Program source. Insertion publishes completed reporting data, never an active
recursive assumption. Comparison/flattening traverse existing ordered data;
compaction copies related notes only for duplicate groups. No new cache.

`assignreport::relation_diagnostic` mirrors the direct type-parameter arm of
native `Relater.reportRelationError`: original Checker-local source/target IDs,
existing display generalization and base-constraint resolution, Assignable query
on generalized source first, original source only if needed. TS5075 explains
constraint assignability; otherwise TS5082 explains arbitrary instantiation.
Those verdict-only queries select an explanation, not the outer verdict.
Existing constraint owner/publication and alias-binding context remain intact;
printed names are presentation, never keys. Variance marker types are excluded
by the native branch. Deferred indexed targets and recursive errorChain
preservation/reset are not implemented at this direct-target seam.

Signature TS2849 comes from `Relater::one_signature_related_to` at the native
minimum-count decision, not a call-site arity reconstruction. Inline pending
(minimum, count) metadata belongs to one reporting Relater and its ordered direct
TypeId pair, relation kind and existing intersection-target context. Verdict-only
walks allocate no explanation. Only completed NotRelated renders a Diagnostic;
success/Unknown alternative candidates restore saved explanation state. Strict
arity failures do not publish this child. Concrete signature/rest/option and
receiver/alias context stay with existing readers. Nested parameter/return
wrappers and recursive rollback require a serialized reporting contract before
expanding this boundary. Worker executions/hits/copy bytes are unmeasured; no
speed or reuse claim is justified.

## Current verification

Receipts: repository-ignored `target/recovery-diagnostics/`, not `/tmp`.

- Real CLI failing-before/passing-after: four generic return failures, signature
  assignment and signature argument failure, plus a valid generic-return control.
  `--ignoreConfig --strict --noEmit --pretty false` output matches pinned native
  byte-for-byte after recovery. Six missing explanation lines reproduced before.
- Standalone Go pinned control confirms longer-tree precedence, child-code
  comparison/equality distinction, UTF-8 argument order, serialized runtime
  payloads versus new parent payloads, clone noEmit preservation and CRLF nesting.
- Dedicated representation and checker behavior regressions pass; release
  workspace tests pass; diagnostics/checker all-target clippy and workspace fmt
  check pass. Existing parser/binder `rss` example-name collision warning remains.
- Fresh unfiltered keyed dumps: types 477970 before/after, 469785 protected RIGHT;
  diagnostics 10570 before/after, 9190 protected RIGHT/EMPTY_RIGHT. Duplicate-key
  rejection and presence-first comparison find zero vanished or changed protected
  keys. Complete dump bytes match. Type dump multiline payloads are parsed at
  keyed record boundaries, not assumed to be one physical line per record.
- Read-only runner using all sixteen existing Suite implementations discovers
  12444 cases without writing protected snapshots. Checker types 8051/9538,
  diagnostics 4222/5502; skipped populations 2906 and 6942. These are legacy
  assertions, not exact complete-message/full-configuration >=99.9% proof.
- Fresh-process 21-pair candidate/baseline median wall ratios: domain-model
  1.0016, generic-imports 1.0170. Diagnostic fingerprints match. These observations
  do **not** prove zero slowdown. Candidate/pinned-tsgo domain-model observed wall
  ratio 1.0654, not <=0.50. Harness leaves verified_wall_ratio null because complete
  cross-tool query-input coverage and actual checker work/worker budgets remain
  unverified. Builds and fixture setup were outside timed samples.

## Allocation-free primary-location consumer API

`sort_and_deduplicate_located_diagnostics(Vec<(String, Diagnostic)>)` returns
`Vec<(String, Diagnostic)>`. It consumes the existing compiler tuple rows;
empty paths denote globals. Paths and diagnostic ownership move together without
key copies. Generic borrowed views are private worker machinery, not a second
public consumer convention.

This is the canonical native sort/compact worker: attached-file sort and
already-sorted compaction call the same internals, not parallel comparator
conventions. Explicit primary paths drive both comparison and equality. Rows and
consumer metadata move together in place; no key copies, primary source-image
copies or per-primary boxed details allocations. Related notes still carry their
independent attached shared images. Merged nonempty related lists are the only
compaction publication that needs diagnostic details. The existing Program/file
image owner remains responsible for source text and line maps at rendering.

## Runtime writer audit and first-failure chain correction

At the pinned native revision, `reportsUnnecessary`/`reportsDeprecated` have
exactly two writers: `ast.NewDiagnostic` copies the respective Message booleans
into per-diagnostic fields; `NewDiagnosticFromSerialized` copies explicit runtime
payloads. Checker does not mutate those two fields. Therefore the existing TSR
Diagnostic constructors are the actual semantic writer, not a serialization-time
flag inference. Checker comma diagnostics use that constructor and publish
TS2695 from their generated input; generic relation heads/children publish
both false. The comma-producer regression exposed an input-schema bug: the
generator omitted serde's native `reportsUnnecessary`/`reportsDeprecated` names.
The canonical generator now reads those two camelCase fields; regeneration
corrects eleven catalogue entries, including TS2695. No code-based flag setter
or serialization-time inference replaces the native constructor. Actual comma
production retains its unnecessary flag; explicit deserialization setters remain
separate. Deprecated suggestion producers still require their own native ports.
The full oracle must read stored getters for every chain node and related note.

Current native/TSR control reproduced a chain overwrite for one source signature
with three required parameters and target overloads of one then two parameters.
TSR reported TS2849 got 2; native got 1. Owned signatures worker now returns at
the first fully failed target as native signaturesRelatedTo does, preserving that
first explanation and avoiding subsequent target comparisons. Unknown still
retains the existing unsupported-state policy; successful alternatives restore
saved reporting state. This changes no cache/key/receiver context. A permanent
regression covers the competing failure counts. The temporary failing semantic
flag probe was removed from the permanent suite pending the generated-message
owner's cutover; its observed failure remains recorded, not repinned to false.

Current correction gates: actual overload/comma CLI bytes match pinned native
including first target's got 1 child; dedicated tests, workspace release tests and
scoped all-target clippy pass. Fresh dumps preserve 469785 RIGHT type keys and
9190 RIGHT/EMPTY_RIGHT diagnostic keys, zero missing/changed. Sixteen read-only
suites rerun; this remains legacy coverage, not strict metadata parity. Fresh
21-pair domain-model candidate/recovery-baseline observed wall ratio 0.9642,
diagnostic fingerprints match; complete-work verified ratio remains null. No
native <=0.50 or literal zero-regression release claim.

## Call-before-construct reporting boundary

Pinned structuredTypeRelatedToWorker at relater.go 3864 compares properties,
then calls, then constructs, then indexes, visiting the next worker only when the
previous result is non-false. TSR related_signatures previously evaluated calls
and constructs eagerly, so a call TS2849 Expected 2 was overwritten by construct
Expected 3. Existing owned worker now returns immediately on completed call
NotRelated. No cache, new metadata or type identity; reporting still belongs to
the existing direct ordered TypeId pair/Relater. Unsupported call work remains
unsupported. Construct comparison is the skipped expensive boundary, not a second
reporting query. Actual dual-signature CLI reproduced the discrepancy before and
matches complete pinned native bytes after. Regression, workspace release tests
and scoped clippy pass. Fresh full keyed dumps preserve all 469785 RIGHT type
and 9190 RIGHT/EMPTY_RIGHT diagnostic keys, zero missing/changed; sixteen read-only
suites rerun. Domain-model 21-pair candidate/recovery-baseline observed wall ratio
0.9781 with matching diagnostic fingerprints, verified complete-work ratio null.
No claim of native <=0.50 or strict full-configuration parity.

## Property relation reporting — tsr-2zk.1

Native `propertyRelatedTo`/`isPropertySymbolTypeRelated` at relater.go 4310/4334
reports the completed member failure then prepends TS2326. `reportError` at 4831
compresses property / relation / property into TS2200 dotted names, with quoted
property names converted to bracket paths. Owned
`properties_related_to_with_optionals` now collects reporting data during that
same receiver-mapped member comparison, not a second member/type traversal.

Identity/owner: ordered concrete Checker-local member TypeIds from existing
property getters, current relation/options/intersection-target context, private
walk-local Relater. A temporarily selected reporting pair returns to its parent's
pair. Completed simple failures and completed property/signature chains publish
an explanation; success/Unknown restores saved reporting data. Unavailable
recursive explanations stay absent. No new semantic cache or cross-Checker key.
Verdict-only paths bypass reporting-state movement. Reporting allocations occur
only for completed failure text/trees; existing simple failure sites publish an
inline boolean without an extra query. First failed member stops later worker
execution. Related declaration images remain unchanged. Completed subtrees move
rather than clone; native dotted compression reuses the inner chain and removes
the intervening relation head.

`Diagnostic::message_chain_mut()` is the minimal model seam for attaching the
consumer's chosen error span to each completed child after reporting. Both owned
assignment/argument consumers perform this cutover; primary tuple collector APIs
and external file identities are unchanged. Property names reuse existing
callable_property_name presentation, preserving quoted member syntax in direct
controls. Arbitrary alias/qualification, mapped late-bound symbols, recursive
union/generic constraint wrappers, optionality/private/member-related notes and
signature return/parameter wrappers are not certified by these controls.

Actual pinned CLI failing-before/passing-after controls cover TS2322 and TS2345
with two differently failing properties (first x retained), nested x.y compression,
quoted ["x-y"].z and x["y-z"] paths, and a signature-valued property with native
nested TS2849. Complete stdout matches native after; permanent regressions assert
full trees, ordering and child spans plus successful/overload rollback controls.
Targeted/workspace release tests and scoped all-target clippy pass. Fresh unfiltered
dumps preserve all 469785 RIGHT type and 9190 RIGHT/EMPTY_RIGHT diagnostic keys,
zero missing/changed; sixteen read-only suites complete without snapshot edits.
21-pair candidate/recovery-baseline observed wall ratios: domain-model 0.9494,
generic-imports 0.9950; diagnostic fingerprints match. An interrupted generic
measurement was discarded and replaced by a completed run. These comparisons
include earlier recovery work, not isolated property-port attribution. Verified
complete-work ratios remain null; no native <=0.50 or literal no-regression release
claim. Receipts `target/recovery-diagnostics/property-*`.

## Contravariant parameter reporting — tsr-2zk.1

Pinned compareSignaturesRelated at relater.go 1568 first runs non-strict bivariant
forward comparison without reporting, then reverse comparison with reportErrors;
completed reverse failure prepends TS2328 using getParameterNameAtPosition. Owned
one_signature_related_to now selects that concrete parameter pair for the existing
recursive worker, consumes its completed simple/property/signature explanation,
and restores parent reporting pair/state on success/Unknown. No second traversal
or persistent reuse. Ordinary/array-rest names use actual Signature parameters;
tuple-rest label publication and callback-signature recursive reporting remain
unsupported rather than receiving guessed names. Existing optional/rest/strict
arity verdict policy is retained. Verdict-only paths use the previous worker path.

Actual CLI assignment and argument controls with object parameter types now match
native complete TS2328/TS2322/TS2326 trees; primitive parameter mismatch also
matches. Dedicated tests assert contravariant direction, names, nested messages,
head codes and child spans. Workspace release tests complete and pass (not an
interrupted run), scoped all-target clippy passes, sixteen read-only suites finish.
Fresh keyed dumps retain 469785 protected RIGHT type and 9190 RIGHT/EMPTY_RIGHT
diagnostic keys, zero missing/changed. Domain-model 21-pair candidate/recovery-
baseline observed wall ratio 0.9361; complete-work verification remains null.
Current return control still lacks native return-type chains and property-return
marker compression (f().x); no complete parameter/return cluster claim. Native
return marker writer/reduction must be implemented together before publication.
Receipts target/recovery-diagnostics/parameter-*.

## Return worker and elided marker publication — tsr-2zk.1

Owned one_signature_related_to now selects actual completed source/target return
TypeIds for the existing recursive relation worker. Native reverse bivariant
query remains verdict-only; forward failure publishes the return relation chain.
Inline return marker (construct/call, arguments/no arguments) mirrors native
compareSignaturesRelated 1615 marker emission without rendering the marker.
properties_related_to consumes it at native reportError's reduction boundary:
function relation/property wrappers disappear and returned-by f().x retains the
actual inner property failure. No syntax-based signature reconstruction or
second semantic traversal. Marker state is walk-local, restored with successful
candidate/member/parameter states, never stored as a completed semantic cache.

Actual complete CLI control now matches native TS2322/2345 object parameter,
object return and property-return f().x trees. Permanent return test asserts both
uncompressed return chain and native returned-by compression. Workspace release
tests finish and pass, scoped clippy passes, sixteen read-only suites finish.
Fresh full keyed dumps preserve 469785 protected RIGHT type and 9190 diagnostic
keys without disappearance or changes. 21-pair domain recovery-baseline observed
wall ratio 0.9581, diagnostics match, verified complete-work ratio null. This
bounded worker publication does not certify nested callback predicates, tuple-
rest labels, arbitrary alias normalization, cyclic reporting re-entry or full
compiler/configuration exact parity. No whole-case conversion count is claimed
by legacy head-code verdicts, which cannot observe these full-message gains.
Receipts target/recovery-diagnostics/return-* and parameter-return-*.

## Outer return-path reduction follow-up — tsr-2zk.1

Pinned reportError's property reduction recognizes returned-by explanations too;
addToDottedName wraps constructor heads in parentheses before appending a property.
Owned worker now mirrors both operations. Actual control previously emitted an
extra property/relation wrapper for x.f() and unparenthesized new make().value;
complete CLI now matches native x.f(), (new make()).value and f(...).value output.
No relation/query/publication or allocation path changes; existing completed tree
is reduced in place. Dedicated regressions and completed workspace release tests,
scoped clippy pass. Full fresh dumps preserve 469785 RIGHT type/9190 diagnostic
keys, zero vanished/changed; sixteen read-only suites complete. Domain 21-pair
observed candidate/recovery-baseline wall 1.0007 with equal diagnostics; this does
not prove no slowdown. Verified complete-work ratio remains null. Frozen parent
catalogue-aware collector candidate is not the measured source: these receipts
apply only to this recovery branch's cumulative owned delta from 0e7824dd.

## Callback parameter reportErrors propagation — tsr-2zk.1

Pinned compareSignaturesRelated 1568 passes reportErrors through the reversed
callback signature worker, then wraps failure with outer TS2328. Owned worker
now does so instead of explicitly disabling reporting. Completed inner chains
move beneath the outer actual parameter names; success/Unknown restores saved
error/signature/marker state. Unsupported callback resolution becomes Unknown,
not successful metadata. Ordinary parameter names only; tuple-rest suppliers
remain unproved. No side traversal/new cache/public identity.

Actual nested callback control gains both native TS2328 wrappers, contravariant
object head and TS2326/TS2322 detail. Complete callback and nested-property CLI
bytes match native. Permanent behavior regression passes; completed workspace
release tests/clippy pass. Fresh full dumps preserve all 469785 RIGHT type and
9190 diagnostic keys, zero vanished/changed; sixteen suites complete. Domain
21-pair cumulative recovery-baseline observed wall 1.0241, diagnostics equal,
verified complete-work null. This measured increase does not pass no-hotpath-
regression acceptance and is not attributed exclusively to this small delta;
parent must evaluate the queued cumulative candidate with its performance owner.
No release-ready or full-cluster claim. Receipts callback-*.

## Callback arity chain context — tsr-2zk.1

Pinned compareSignaturesRelated wraps any completed callback failure in outer
TS2328, including callback arity TS2849. TSR's inline arity payload previously
escaped that wrapper and appeared directly under TS2322. Existing callback
publication now materializes that completed arity payload at the native wrapper
boundary, using actual outer parameter names. No new arity query, member walk,
cache or metadata. Verdict-only paths unchanged. Actual callback arity CLI
reproduces missing wrapper before and matches complete native bytes after;
permanent behavior regression, completed workspace release tests/clippy pass.
Fresh full dumps preserve all protected 469785 RIGHT type and 9190 diagnostic
keys; sixteen suites complete. Domain21 candidate/recovery-baseline observed wall
1.0145, diagnostics match, complete-work verification null: no no-slowdown/release
claim. Previous callback cumulative performance gate was also unmet; integration
must retain this risk, not interpret correctness controls as a speed win.
Receipts callback-arity-*; prior immutable hash
1d95f230e71e40b1b759f39a7b5205b3890e7fa7 confirmed by bundle list-heads.

## Signature-valued return arity — tsr-2zk.1

Native return comparison retains completed inner signature TS2849 under the
return TS2322 head and publishes its elided return marker. TSR previously let
inline arity bypass both. Owned return publication now consumes that existing
payload, without another signature query, and feeds the existing marker reduction.
Actual standalone return and property-return make() controls reproduce missing
head/marker before and match full native bytes after. Permanent behavior test,
completed workspace release tests/clippy pass. Fresh full keyed dumps preserve
469785 RIGHT type and 9190 diagnostic keys with no vanished/changed keys; sixteen
suites complete. Domain21 observed cumulative recovery-baseline wall 0.9814,
diagnostic fingerprints match; complete-work ratio null. Earlier positive
cumulative deltas remain evidence against claiming universally no regression.
Receipts return-signature-*; previous stable commit
3db66430793266adfdf8f99cb5c15b5ee77ef7d2.

## Source primitive constraint chain — tsr-2zk.1

Pinned source TypeVariable branch at relater.go 3665 first checks constraint with
reporting disabled, then compares constraint-with-this with reporting when target
is not a type parameter and constraint is not unknown. Primitive constraints have
no receiver-this substitution. Owned source-parameter worker now preserves the
original reporting pair, runs its existing constraint verdict worker with reporting
disabled, and publishes the completed primitive failure as inner TS2322. No new
constraint query/cache or syntax special-case. Object/polymorphic-this and composite
constraints remain unsupported at this seam rather than receiving guessed mappings.

Actual assignment/argument U extends string to number controls gain native
`Type 'string' is not assignable to type 'number'.` with consumer-chosen spans.
Dedicated tests and complete reachable CLI match native. The combined TS2344
control still emits no TSR Box<U> constraint error in this recovery base: its
non-owned caller is missing. Parent instantiation writer must call existing
report_relation_failure with head TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1 and
original U/number pair; this port supplies the owned inner constraint detail once
that caller reaches it. No whole instantiationExpressions conversion is claimed.

Workspace release test log completes all doc-tests; enclosing multi-command call
timed out during clippy, not during workspace tests. Interrupted clippy discarded;
completed replacement clippy passes. Fresh full dumps retain all 469785 type RIGHT
and 9190 diagnostic keys, zero vanished/changed; sixteen suites complete. Domain21
observed cumulative recovery-baseline wall1.0446, diagnostics match, verified work
null. No-hotpath-regression acceptance remains unmet. Receipts constraint-source-*.

## Exact TS2344 owned-consumer seam proof

Curated source-constraint commit: 58bebc06158a23ddbcc07f5480618d9b8057e245.
An owned assignreport module regression parses/binds a real U extends string and
number parameter pair, invokes report_relation_failure with caller-provided
TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1, and verifies exactly TS2344 plus inner
TS2322 string/number chain and inherited span. The targeted release test passes.
This proves the reachable owned consumer; it does not fabricate the absent
non-owned instantiation-expression caller on the recovery base. Parent can use
this unchanged existing API directly; no new shared fields/contracts. Existing
native CLI constraint control supplies the native complete message reference.
This follow-up adds only the consumer behavior regression/docs, no runtime or
performance change; prior source-qualified gates remain those in constraint-source
receipts and still do not pass no-hotpath-regression acceptance.

## Immediate constraint identity preservation — tsr-2zk.1

Actual U extends T extends string control exposed collapsed intermediate reporting:
TSR displayed only string/number, native retains T/number then string/number.
Owned existing cycle walk already records those immediate Checker-local TypeIds;
completed primitive-constraint failure now wraps them in reverse order. No new
semantic traversal, cache or verdict computation. Cycles/unsupported constraints
still return Unknown before publication; tree allocation is completed reporting
only. Actual assignment/argument CLI matches native complete three-level output;
permanent regression verifies both parent codes and ordered inner heads. Completed
workspace release tests/clippy pass; full keyed dumps preserve all protected RIGHT
keys; sixteen suites complete. Observed domain21 cumulative baseline wall1.0400,
diagnostics match, complete-work null: no-hotpath acceptance still unmet. Parent
check_return_statement reserved and unchanged. Receipts constraint-chain-*.

## Explicit signature-this recursive reporting — tsr-2zk.1

Pinned compareSignaturesRelated 1503 tries forward this relation without errors,
then reverse with reportErrors and wraps completed failure in native incompatible
this-types message. Owned signature worker now selects the actual this TypeIds
for that reverse recursive object worker and restores saved reporting state on
success/Unknown. Verdict-only path remains unchanged; no synthetic traversal or
receiver remapping. Actual explicit-this control previously lacked four nested
rows; now complete CLI matches native and permanent test asserts reverse receiver
direction/wrapper/inner property failure. Full fresh protected RIGHT keys retain
presence/verdicts; sixteen read-only suites complete, targeted tests/clippy pass.
Initial workspace run timed out before tsr-vfs completion and is NOT a pass;
replacement workspace run completes and passes. Domain21 observed cumulative
recovery-baseline wall1.0081 with equal diagnostics; no-hotpath gate remains
unproved, verified work ratio null. Parent check_return_statement untouched.
Receipts this-chain-* and this-chain-workspace-tests-complete.log.

## Constraint retry correction — supersedes 58bebc/6e2 reporting shortcut

Removed primitive-flag-gated manufactured details. Source TypeVariable branch now
retains its immediate constraint identity, runs native no-report fast comparison,
then runs the existing reporting relation worker on supported identity-this
constraints. Completed recursive child/simple failures produce their native
relation head; intermediate type parameters arise from actual recursive workers,
not wrapping a prewalk's list. Failed cached recursive results re-enter the
reporting worker when reportErrors is selected; active Maybe and Unknown retain
their prior meanings. No failure cache is treated as a cached diagnostic chain.

Missing prerequisite: parent reports canonical get_type_with_this_argument,
but neither Box checkout nor fetched public main544b4441 contains it. Required
signature is (TypeId, TypeId, bool)->TypeId. Native needApparentType=false is
identity except references/intersections. Those reference/intersection explanation
retries remain unsupported here; their completed verdict stays intact. Initial
returning Unknown discarded three previously RIGHT cases; that failed gate was
corrected, not claimed as passing. No primitive-only fallback remains.

Exact TS2344 module regression and immediate-constraint CLI pass after real retry;
workspace release completes, scoped clippy completes after a localized test-module
placement lint correction. Fresh protected RIGHT keys all preserved zero vanished/
changed. Domain21 observed cumulative baseline1.0056, verified complete-work null:
no-hotpath acceptance remains unproved. Parent must integrate its canonical helper
and verify reference/intersection reporting before claiming full native3677/3678.
Receipts constraint-retry-*.

## Signature-valued explicit-this arity — tsr-2zk.1

Existing reverse-this comparator owned completed inner signature arity TS2849,
but it escaped the native this wrapper and inner relation head. Same reporting
boundary now consumes inline arity metadata before constructing those wrappers;
no extra query/traversal/cache or source-error suppression. Actual complete CLI
matches native after reproduced missing two wrapper rows; permanent behavior test
passes. Fresh full protected RIGHT keys preserve presence/verdicts; sixteen suites
complete. Initial workspace call timed out during doc-tests and is not a pass;
completed replacement workspace and clippy pass. Domain21 cumulative baseline
observed wall0.9973 with equal diagnostics, verified complete-work null; earlier
positive deltas still prevent a global no-regression acceptance claim. Parent
reserved return/signature fields untouched. Receipts this-arity-*.

## Serialized integration prerequisites

1. The execute collector now consumes the canonical owned `(path, Diagnostic)`
   tuple sort/merge. Recoverable config errors continue into syntax and semantic
   checking while retaining extended-file origins. Positioned extends TS6053 and
   TS18002 use actual parsed initializer spans; an unreadable resolved `.json`
   path instead reports global TS5083. Each duplicate extends property contributes
   its own resolution errors, but only the last property's resolved paths load.
2. Oracle owner: consume runtime getters and compare complete flattened trees,
   native spans/lengths/order and independently located related information across
   all configurations. Historical/legacy scores cannot certify these payloads.
3. Checker/grammar owner: publish noEmit marks at native errorSkippedOnNoEmit and
   grammarErrorOnNodeSkippedOnNoEmit; Program owner filters only marked semantic
   diagnostics under noEmit. This lane does not invent message-code filters.
4. Integration/signature owners: serialize native reportErrors, recursive chain
   wrapping, normalization and rollback. Nested object/parameter/return chains,
   deferred indexed generic targets, related declaration TS2728 and constraint
   advice TS2208 remain unproved. No synthetic diagnostic traversal substitutes
   for those producers.
5. Calls/contextual/inference owners: reverseMappedTypeContextualTypeNotCircular
   needs concrete reverse-mapped member/candidate/contextual types before the
   existing signature reporter can consume that failure. No converted target is
   claimed. Performance owner must provide equivalent complete-work evidence.

Issue remains in progress. Recovery is a bounded owned implementation, not
campaign completion or permission to merge the saved branch wholesale.

## Config reporting and source eligibility cutover — tsr-2zk.22.1.1

Pinned operations: `GetDiagnosticsOfAnyProgram` (program.go:1782–1827),
`EmitFilesAndReportErrors` (execute/tsc/emit.go:73–131), `sourceFileMayBeEmitted`
(emitter.go:452–504), and `filesParser.start` (filesparser.go:245–301).

List-only output retains config and syntax diagnostics, skips semantics, then
prints files before the pretty summary. TypeScript-only lists create no checker;
the existing JavaScript syntax worker still needs one private checker, whose
actual construction is observed. No semantic worker is invoked in that branch.
The canonical no-input producer now uses `!files && !references` only at the
top-level config; the driver no longer inserts a second unconditional TS18003.

External-source classification lives in the existing completed Program metadata
vector, aligned with retained source files. Its key is the retained source's
canonical path/task identity, not a printed pathname or package alias. The
existing claimed task owns optional minimum depth and started-subtask state.
Minimum depth publishes before recursion/elision. An unloaded task can be revived
by an equal-depth non-eliding reference; already-started children do not restart,
matching the pinned native gate. Package aliases never overwrite the replay-first
source's classification. The expensive work remains source loading/parsing and
its existing bounded preparation; no second membership cache or member image.

Declaration and external sources are non-emittable. JSON sources require outDir
and native common-directory/output identity rules. NoEmit is a later per-emitter
decision, not source eligibility. An actually empty emitter set has an unskipped
native result and diagnostics exit 2; list-only and noEmitOnError instead exit 1.
Nonempty emission remains unsupported by the driver. Project-reference redirects
and the internal NoEmitForJsFiles option have no producer here; this change does
not certify those surfaces or complete native emission work.

Actual native/TSR CLI controls match output and status for positioned extends,
declaration-only empty emit, non-object empty config, duplicate extends, and
list-only config plus syntax. Receipts are retained under `.git/tsr-recovery/`.
Fresh executable launch stalls were sampled at `_dyld_start`, before Rust main;
those delays are retained separately, not attributed to checker work. Full
configured exact parity and verified <=0.50 performance remain unmet.

## Imported assignment semantic result — tsr-2zk.22.1.1.1

Correct root loading exposed a previously RIGHT imported `c++` operand becoming
any. Native `checkIdentifier` (5b1047d:11076–11094) tests the local/export symbol's
Variable flag, not an alias target's value type. The semantic getter now returns
errorType for every non-variable assignment target except JavaScript ValueModule;
the existing diagnostic walk retains the sole reporter. The old partial semantic
test covering only functions/classes/enums/namespaces is removed.

Ordinary reads do not fetch extra local/export flags: that work stays inside the
assignment-kind branch. The regression distinguishes imported value reads from
increment/assignment targets. Actual CLI reports native TS2632 at both targets.
The repaired full legacy population retains all formerly RIGHT IDs and all keys;
newly loaded CommonJS source rows still expose separate unsupported exports and
unresolved-assignment roots. Source-matched 41-pair project checks preserve full
output/options/scope without a hot-path slowdown; these are not a <=0.50 native
speed certificate or full configured-parity acceptance.

## Collector and pool-observer rebase — tsr-2zk.22.1

Integration preserves the pool-observer changes from `8091f493` alongside native
list-only diagnostic collection. Ordinary checks retain private pool ownership.
JavaScript list-only syntax uses one checker and records its construction as
owner zero; TypeScript list-only collection creates no checker.

Actual CLI controls retain the domain-model TS2322, distinct extended-config
diagnostics, positioned missing-config diagnostic and list-only semantic exclusion.
The physical JavaScript trace finishes with one constructed checker, zero full
checks and no unfinished spans. Thirteen execute trace/config controls and two
physical CLI trace controls pass; scoped all-target clippy passes.

Frozen full legacy dumps retain every protected RIGHT type and RIGHT/EMPTY_RIGHT
diagnostic key, zero losses and zero missing keys. Current counts are 469,892 RIGHT
type rows, 7,124 WRONG and 960 GAP; diagnostics are 4,224 RIGHT, 1,278 WRONG,
4,968 EMPTY_RIGHT and 100 EMPTY_WRONG. These are not full-configuration parity.

Twenty-one prior-TSR pairs observe wall ratio 1.00781; eleven pinned-native pairs
observe 1.56656. Diagnostics, options and loaded scope agree; complete performed
work remains unverified. Neither observation proves no slowdown, native speed
lead or the <=0.50 release target.


## Union-source constituent chains — tsr-2zk.1

Pinned `unionOrIntersectionRelatedTo` (relater.go:2853) and
`eachTypeRelatedToType` (:2932): with `reportErrors`, a non-primitive source
union relates each constituent as the diagnostic pair and returns on the first
failure, whose nested `reportErrorResults`/`reportRelationError` link
(`Checker::nested_relation_error`) sits above its completed explanation. A
primitive source union (boolean, enum) or a primitive side of a target-union
pair passes no nested reporting, so the pair's own link is the whole
explanation (`simple_error`). Key/owner: walk-local `property_error` on the
active `diagnostic_pair`; no cache, publication only after a completed
`NotRelated`. Declines (publishes nothing): a constituent after an undecided
one, equal display names, exactOptionalPropertyTypes, object-to-non-object
pairs (primitive/wrapper notes), unconstrained type-parameter sources (TS2208
note), and target unions needing `getBestMatchingType`. Expensive work: one
extra walk only on reported failures, as native.

## Type-argument variance chains — tsr-2zk.1

Pinned `relateVariances` (relater.go:3266) keeps the `typeArgumentsRelatedTo`
(:3903) chain when a measured variance check fails without structural fallback:
the first failed argument is related again as the diagnostic pair
(contravariant reversed, bivariant via the covariant check) and its nested
`reportRelationError` link is published (`report_type_arguments_failure`).
Declines: unmeasured (default-covariant) variances, since native may carry
Unmeasurable/Unreliable fallback flags this port lacks; any invariant
parameter, where native discards the variance chain for a structural
elaboration not run here; an undecided earlier argument. Walk-local
`property_error`, no cache; extra walk only on a reported failure.

## Nested reportRelationError links and unmatched properties — tsr-2zk.1

`propertiesRelatedTo` (relater.go:4153) checks `getUnmatchedProperty` before
member types; on the reporting pair the walk now publishes
`reportUnmatchedProperty`'s TS2741-form child when
`unmatched_property_report` certifies exactly one name. Several names fail the
pair without an explanation: member-table order is not yet native declaration
order for every type (members lane). The property arm's inner head is now the
nested `reportRelationError` link (`nested_relation_link`): generalized source,
type-parameter explanation, TS2820 string-literal suggestion
(`getSuggestedTypeForNonexistentStringLiteralType`; union constituents arrive in
CompareTypes order so the tie-break never prefers a later candidate) and
`getChainMessage(0)` missing-property suppression (`chainArgsMatch`). A declined
link keeps the previous plain head.

## Related information for relation reports — tsr-2zk.1

Before this port the checker published no related information at all, so every
native case carrying `!!! related` was inexact. Pinned functions:
`NewDiagnosticForNode`/`createDiagnosticForNode` (utilities.go:22) →
`Checker::diagnostic_for_node`; `createDiagnosticChainFromErrorChain`
(relater.go:402) + `ast.NewDiagnosticChain` → `Diagnostic::
publish_chain_related_information` (every link shares the walk's list, gathered
children-first; a `new_chain` parent's inherited list counts once);
`reportUnmatchedProperty` (relater.go:4345) TS2728 at the single missing
property's first declaration; `elaborateElement` (relater.go:588) TS6500/TS6501
on its one direct report (`RelationReport::Reported`, never on an inner
elaboration), skipping default-library declarations; `elaborateArrowFunction`
(relater.go:666) TS6502 and TS1356; `elaborateDidYouMeanToCallOrConstruct`
(relater.go:480) TS6212/TS6213. `check_object_literal_member` is this port's
elaborateElement for known members and decorates the same way.
`type_symbol_declaration` names `type.symbol` only where this port can: an
anonymous type's symbol, a class/interface/enum, or an alias written as a type
literal/function type (the literal's `__type` symbol); otherwise no record.

Side table `Checker::diagnostic_files` (convention record): native op is the
`*SourceFile` pointer `createDiagnosticForNode` stores; key = `SourceFile`
`NodeId` of this Checker's node table, value = `Arc<DiagnosticFile>` built from
the host's `file_path` + `source_text` (`None` = host without text, a completed
decline: no record, never a misplaced one). Owner: the Checker, program
lifetime; filled only on reporting paths, so relation queries never pay. The
expensive work (line-start index of one file) runs once per file with related
information.

Measurement still needed outside this lane: TSR chain links carry no file of
their own (native links copy the head's file and span), which the exact oracle
must render as the head location; related information for TS2448/TS2449/
TS2554/TS2813/TS2814/TS9027 belongs to other lanes and can use
`diagnostic_for_node`.

## Type-parameter constraint note — tsr-2zk.1

`reportErrorResults` (relater.go:4744) closes every reported pair whose source
is an unconstrained type parameter with a declaration with TS2208 "This type
parameter might need an `extends {target}` constraint" when the clone
constrained to the target has a non-circular base constraint
(`hasNonCircularBaseConstraint`). Port: `type_parameter_constraint_note`, called
from `relation_diagnostic` (top-level and nested links) and the signature
this/parameter/return links the relater builds directly. Unconstrained means
no written constraint, not `infer`, not polymorphic `this`.
`base_constraint_avoids` walks `computeBaseConstraint`'s arms (union,
intersection, template literal, string mapping, non-mapped `keyof`; another
type parameter ends the walk, its constraint names the original, not the
clone). A clone inside a union/intersection is treated as circular, so
`T & string` publishes no note although native settles it; indexed access,
conditional and substitution targets decline. The unconstrained source's
constraint retry now marks the pair's own link as its explanation
(`simple_error`), as native relates the `unknown` constraint without
`reportErrors`; `nested_relation_link` no longer declines such sources.

## Relation chain link locations — tsr-2zk.1

Native chain links are built bottom-up by `ast.NewDiagnosticChain`
(ast/diagnostic.go:152), each copying its child's file and location, so every
link of a reported relation chain sits at the error node in its file (the
exact oracle compares these records). TSR builds chains top-down with no file
on links; `report_relation_chain` now calls `Diagnostic::locate_message_chain`
with the reported file's image (`Checker::diagnostic_file`, the same
`diagnostic_files` table). Chains reported by other lanes' sites (calls'
TS2769, declarations' TS2416/TS2430 heads, JSX) need the same call at their
report site.

## Primitive source against index signatures — tsr-2zk.1

`structuredTypeRelatedToWorker` (relater.go:3864) relates a primitive source
through its apparent type but keeps the primitive pair key and passes
`sourceIsPrimitive` to `indexSignaturesRelatedTo`, which then skips the
any-valued string-index shortcut: `string -> { [k: string]: any }` fails on the
missing string index of `String`, `"x" -> { [n: number]: any }` holds through
`String`'s number index. The arm previously answered Unknown unless a property
failed (`primitive_structured_related_to`). With `reportStructuralErrors`
false for a primitive source, a failed primitive pair's own reportRelationError
link is its whole explanation (`simple_error`), so nested chains now carry it.

## Leaf-pair links and wrapper notes — tsr-2zk.1

`structuredTypeRelatedToWorker` reports nothing for an object source against a
primitive target, a concrete source against a type-parameter target, or a
nullable source against an object target; native's explanation is then the
failed pair's own `reportRelationError` link, so these arms now set
`simple_error` and nested chains carry the link. `reportErrorResults`
(relater.go:4705) puts its own link between the structural child and the pair:
`tryElaborateErrorsForPrimitivesAndObjects` (TS2692) for the global
`String`/`Number`/`Boolean`/`Symbol` type against its primitive, or — only for
a non-primitive target, since the object-to-primitive arm comes first — TS2696
for the global `Object` source (`wrapper_object_note`, used by nested links and
both top-level reports). `nested_relation_link` no longer declines object
sources against non-object targets. Open: the global `Object` source's
missing-property child under TS2696 (`unmatched_property_report` still
declines that source for the top-level head).

## Index signature explanations — tsr-2zk.1

`indexSignaturesRelatedTo`/`typeRelatedToIndexInfo`/`membersRelatedToIndexInfo`/
`indexInfoRelatedTo` (relater.go:4578-4690) with `reportErrors` now explain the
reporting pair: "'K' index signatures are incompatible" (or the two-key form)
over the value pair's nested link, "Property 'p' is incompatible with index
signature" over the member's link, and "Index signature for type 'K' is
missing in type 'S'". Native returns on the first failed target info or member,
so the walk now does too (a later undecided member no longer turns a decided
failure into Unknown). `relate_explained` is the shared "relate as the
diagnostic pair, answer its nested link" step; walk-local state, no cache, the
extra walk happens only on the reported pair. Publication requires every
earlier part decided, as native could have failed there first.

## Construct-signature compatibility explanations — tsr-2zk.1

`signaturesRelatedTo` (relater.go:4441) reports "Cannot assign an abstract
constructor type to a non-abstract constructor type" and
`constructorVisibilitiesAreCompatible` (:4526) "Cannot assign a 'V' constructor
type to a 'W' constructor type" on the reporting pair
(`constructor_visibility_mismatch`). Not published: "Type 'S' provides no
match for the signature 'T'" for a source without signatures, which needs
`signatureToString`'s default colon style; `Checker::signature_to_string`
(signatures.rs) prints arrow style only.
