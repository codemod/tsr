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

## Serialized integration prerequisites

1. Integration owner: replace execute's head-only collector sorting/dedup with
   full native comparison/equality/related merge; attach shared Program source
   images, not a source copy per diagnostic. Config dedup needs the same audit.
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
