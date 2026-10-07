# Native diagnostic message chains — tsr-2zk.22

## Implemented boundary

Native pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

`internal/ast/diagnostic.go`: `Diagnostic`, `NewDiagnosticChain`, ordered
`AddMessageChain`/`SetMessageChain`, related-information setters, comparison and
recursive equality. `internal/diagnosticwriter/diagnosticwriter.go`: flattening
and pretty related-location rendering. `internal/checker/relater.go`:
`reportRelationError`'s direct type-parameter explanation, consumed by
`assignreport.rs::report_relation_failure` and `report_argument_failure`.

The diagnostic retains a localized head (`text()`), distinct from the flattened
tree. CLI formatting walks every child in insertion order, two spaces per level,
with the caller's newline. Plain output does not render related information;
pretty output renders its independently attached source, cyan squiggle and
four-space snippet indent. No generated catalogue edits.

Comparison first uses file path, start, end, head code and ordered arguments,
then descending child counts recursively, then child argument contents, then
related-information count/content. Native comparison deliberately ignores child
codes; native equality includes them. A longer explanation sorts before a
shorter one. Category and child locations do not participate in chain equality.

## Identity, ownership, publication, context and work

Diagnostic trees are per-error owned values, not semantic relation results or a
cache. Head-only diagnostics keep absent boxed details and allocate no new tree
storage. Explanation insertion publishes completed reporting data only; there
is no active recursive diagnostic assumption. Related lists are reference-counted
and copy-on-write, allowing `NewDiagnosticChain` to inherit related information
without eagerly copying it. Independently located related diagnostics can share
one `Arc<DiagnosticFile>` per Program source image. Its line map is constructed
once per image, not once per related error. Existing primary locations still
come from Program/Checker side tables and `LocatedDiagnostic`.

Direct generic-target reporting reads concrete Checker-local `TypeId`s and the
existing base-constraint and assignability workers. Receiver, aliases, strict
options and literal generalization stay in the current Checker domain; printed
names are presentation, never lookup keys. The target constraint is tested
against generalized source first, then original source, exactly in native order.
Native TS5075 explains assignability to a constraint; TS5082 explains arbitrary
instantiation when neither test succeeds. The parent code and span remain the
caller's. Synthetic variance marker types do not receive user diagnostics.

No new semantic reuse is introduced. Actual constraint-worker executions,
completed hits, active repeats and diagnostic-tree copy bytes are unmeasured.
Integrator Beads follow-up request: record those counters under tsr-2zk.22 before
extending this reporting boundary to cached diagnostic-producing relations.
Unsupported structural/signature explanation work remains unsupported rather
than a completed diagnostic chain. The existing three-valued relation does not
promote Unknown into successful constraint assignability.

## Reproduced controls and verification

Baseline frozen before edits: release TSR SHA-256
`866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`.
Unfiltered diagnostic dump SHA-256
`ff08a03bb448a639ac7e219d8e7e4b2921b6aa5cf95230c4744efba413da9482`;
unfiltered type dump SHA-256
`25ed9bd55f95956c4515044bf7f41334799bbcc6e2a186eaf6ad36824621660a`.
Both after dumps are byte-identical: zero former RIGHT type-line losses and zero
RIGHT/EMPTY_RIGHT diagnostic-case losses. These dumps do not judge full messages.

Pinned tsgo directly reproduced missing TSR children for unconstrained `T`,
`T extends number`, `T extends 1`, and `T extends string` with a number source.
After the port, CLI output matches all four complete messages, codes, locations
and order. A primitive mismatch stays head-only; a valid generic return stays
clean. Corpus source `compiler/genericParameterAssignability1.ts` run with
`--strictNullChecks --noEmit --pretty false --ignoreConfig` is byte-identical to
pinned tsgo after the port. This is a full-message improvement, not a legacy
case-verdict conversion. No broad converted-case count is claimed.

Seven permanent boundary tests pass. `cargo test --workspace --release` passes;
`cargo clippy --workspace --all-targets -- -D warnings` passes;
`cargo fmt --all -- --check` passes on Rust 1.96.0. Cargo reports the existing
parser/binder `rss` example output-name collision during workspace tests.

All sixteen coverage suites ran unfiltered using a throwaway read-only runner
calling the same `run_suite` and suite implementations as `coverage`; the stock
coverage binary unconditionally writes forbidden shared snapshots. Population:
12444 discovered cases; checker_types 8042/9538, 469765 RIGHT type lines;
diagnostics 4221/5502, clean 4968/5068. Skipped populations remain explicit:
checker_types 2906, diagnostics 6942. This is not >=99.9% compiler parity proof.

Initial fresh-process interleaved 21-pair measurements, release candidate versus
frozen TSR: domain-model wall 0.9844, CPU 0.9589; generic-imports wall 0.9996,
CPU 1.0043. Diagnostic fingerprints match. Pinned tsgo domain-model observed
wall ratio 1.0402, not <=0.50. All reports correctly leave verified_wall_ratio
unset: complete cross-tool query-input coverage and actual checker work/worker
budgets are unverified. Equal loaded scopes, equal option reports, stable input
observations and diagnostic fingerprints do not fill those gaps. These initial
candidate measurements precede commit attribution and are not release proof.

Committed-source rerun at `b6d2104fcb61f76f1b9ed6d3032c12d5facf849b`:
21 pairs domain-model wall 0.9958 / CPU 0.9851; generic-imports wall 1.0073 /
CPU 1.0193. A 41-pair generic-imports rerun gives wall 1.0020 / CPU 1.0178.
Both projects preserve diagnostic fingerprints. Generic-imports shows a small
positive measured delta; these results do not prove literally zero slowdown.
No verified native release ratio is claimed.

## Signature minimum-argument explanation — tsr-2zk.1.10

Pinned native `Relater.signatureRelatedTo` selects checkMode None for Assignable,
StrictTopSignature for Subtype, and StrictTopSignature|StrictArity for
StrictSubtype; it passes the same Relater recursive comparer, intersectionState,
reportErrors and reportError callback to `Checker.compareSignaturesRelated`.
That comparator checks top signatures first, then target effective rest. Without
StrictArity it fails when source minimum argument count exceeds target parameter
count and reports TS2849: `Target signature provides too few arguments. Expected
{0} or more, but got {1}.` StrictArity failures do not emit that explanation.
Generic contextual instantiation follows this arity test, not before it.

Owned `relater.rs::one_signature_related_to` now publishes the minimum/count
pair at that existing decision. The diagnostic consumer shares the same walk,
ordered source/target identities and Assignable relation with the returned
verdict; `assignreport.rs` attaches the native child to TS2322/TS2345 and inherits
its chosen span. It never recomputes arity at a call site. Overload alternatives
restore saved error state when a candidate succeeds or the result is Unknown.
Verdict-only walks allocate no diagnostic data; the pending arity explanation is
an inline pair of usize values. Only a completed failed reporting relation
renders arguments and allocates a Diagnostic. Initial storing a whole Diagnostic
in the Relater increased measured domain-model CPU by 3.46% in 41 pairs; replacing
it with inline metadata eliminated that measured regression.

This publication is per-Relater walk, not a persistent semantic cache. The
existing ordered TypeId pair and intersection-target context own relation keys;
concrete signature declarations, min counts, rest flags, erasure/canonicalization
and strict-function options remain in the existing signature readers. Only a
direct pair's signature comparator may publish this explanation; nested callback
parameter/return error contexts are not exposed until full native chain wrapping
and rollback are implemented. Follow-up request under tsr-2zk.1.10 for integrator
Beads: attribute
actual signature worker executions, result hits and metadata copy bytes before
expanding diagnostic-result reuse. No new result cache was introduced.

Two behavior regression tests cover real assignment/argument errors and distinct
optional-parameter, effective-rest-target and successful-overload controls.
Three actual CLI controls (assignment, argument and contextual object member)
match pinned tsgo byte-for-byte after the port, including head type printing,
child text, positions and ordering. Workspace release tests, all-target clippy
and fmt check pass. Both final unfiltered verdict dumps retain 477970 type keys
and 10570 diagnostic keys; absence-aware checks preserve all 469765 RIGHT type
keys and 9189 RIGHT/EMPTY_RIGHT diagnostic keys, zero missing/changed. All sixteen
stock coverage suites complete in scratch output: checker_types 8042/9538,
diagnostics 4221/5502. No legacy verdict conversions are claimed.

Final fresh-process baseline/candidate measurements: domain-model 41 pairs wall
0.9874 / CPU 0.9902; generic-imports 21 pairs wall 0.9901 / CPU 0.9941. Diagnostic
fingerprints match. Pinned-tsgo domain-model 21 pairs observed wall 1.0320;
verified ratio remains unset due to unverified complete input queries and actual
checker work. These working-tree measurements precede commit attribution and
are not verified release-ratio evidence.

Committed-source rerun at `e858e7cf01b21d338f8d514027588d5c179987ae`, 21
interleaved pairs each: domain-model wall 0.9854 / CPU 0.9411;
generic-imports wall 0.9848 / CPU 0.9910. Diagnostic fingerprints match. These
short-project observations show no measured slowdown, not complete-work parity
or a verified native <=0.50 ratio.

### Reverse-mapped corpus target prerequisite

Direct pinned CLI `compiler/reverseMappedTypeContextualTypeNotCircular.ts` emits
TS2322 at editable's property (line 11 column 3 in the raw source), printing
`(state: any, props: any) => {}` against `Selector<unknown, {}>` plus TS2849
Expected 2 / got 1. TSR still emits nothing. A current type trace proves the
call result and mapStateToProps are both any, where the oracle requires
`Selector<unknown, { editable: {}; }>`; this port does not claim target conversion.

Required non-owned contracts: `inference.rs::reverse_homomorphic_mapped_type`,
`reverse_mapped_member_type_worker` and `complete_reverse_mapped_type` must
publish the inferred T member {}; `calls.rs::choose_overload` must retain the
instantiated `createStructuredSelector` candidate and its argument diagnostic
context instead of unresolved any; `contextual.rs::contextual_type_for_object_literal_named_element`
must supply concrete `Selector<unknown, {}>` to editable. Native counterparts:
`inference.go::inferToMappedType`, `inferReverseMappedTypeWorker` and reverse
member resolution; `checker.go::getContextualTypeForObjectLiteralElement` and
signature applicability/overload resolution. Integrator must route that atomic
producer contract to its owners. No calls.rs, signatures.rs, inference.rs,
contextual.rs or objects.rs edits were made, and no diagnostic side pass was
substituted. Once that target reaches the existing member elaboration, the
owned signature worker can supply the arity explanation proved by the direct
contextual-member control. That last statement is an inference, not a converted
corpus result.

## Reporting-context integration review

Exact native branch: `Relater.reportRelationError` at relater.go lines
4751–4785 first obtains display names, generalizes literal source unless target
is never or admits top-level singletons, obtains `getBaseConstraintOfType(target)`,
then tests `c.isTypeAssignableTo(generalizedSource, constraint)` and, only if that
fails, `c.isTypeAssignableTo(source, constraint)`. Both calls explicitly select
Checker.assignableRelation even when the outer Relater has another relation.
`Checker.isTypeRelatedTo` reads the shared relation result and otherwise calls
`checkTypeRelatedTo` with nil errorNode; `checkTypeRelatedToEx` borrows a separate
Relater from the same private Checker and disables reportErrors. The outer
Relater's stacks/errorChain are not reused by those explanation queries.
The default branch resets `r.errorChain` before TS5082; the successful constraint
branches prepend TS5075 to existing errorChain. Outer relation decision is not
recomputed by this operation.

TSR's callers are `report_argument_failure` and `report_relation_failure`, using
Assignable and the original source/target TypeIds passed to those functions.
Their top-level failure decision precedes `relation_diagnostic`. It retains those
identities; `assignability_source_for_error_display` computes generalized source
only for presentation/explanation queries. `base_constraint_of_type` is the
existing Checker-owned cache keyed by target TypeId plus sorted current alias
bindings; active resolution is a ResolvedBaseConstraint frame. Failed pops
publish None and may emit TS2313; completed Some/None is cached for that context.
Each `is_type_assignable_to` launches the existing verdict Relater with
Assignable, fresh private per-call results/stacks and no reporting worker. This
is the same native explanation query relation, not a syntax-based side pass or
second structural diagnostic traversal. Head-only reporting adds neither type
clones nor explanation allocations; only actual explanations allocate children.

Integration limit: TSR has no outer recursive reporting Relater/errorChain at
this seam. Original versus native normalized type/alias identity, prior-chain
preservation/reset, overflow behavior and Unknown versus native failed relation
results are not proven generally. Direct generic return/argument controls prove
only the documented direct target domain. Do not merge this as a complete
reportRelationError port; the recursive publication/context contract remains an
integration prerequisite. No new reuse is justified by these controls.

Pinned CompareDiagnostics uses `strings.Compare` for paths and `slices.Compare`
for string arguments; those are UTF-8 byte comparisons, not UTF-16 comparisons.
A standalone native Go control returns -1 for U+E000 versus U+10000 in both
operations; the added regression checks head and nested argument order. UTF-16
would reverse that pair and violate this pin. Span comparison retains both
start and end (therefore full length), before code/args/tree/related comparison.
Latest workspace release tests, all-target clippy and workspace fmt check pass.

## Absence-aware and real-compiler receipt

After native compaction was committed, both verdict dumps were rerun unfiltered.
The receipt loads every keyed row, rejects duplicate keys, and checks each
protected baseline key for presence before comparing verdicts; no inner join
can hide vanished rows. Types: 477970 keyed rows before/after, 469765 protected
RIGHT rows, zero missing and zero changed. Diagnostics: 10570 keys before/after,
9189 protected RIGHT/EMPTY_RIGHT keys, zero missing and zero changed. A whole
prior-RIGHT case disappearing necessarily loses its protected type keys.

A real CLI fixture exercises four TS2322 generic-return failures and two TS2345
generic-argument failures (unconstrained T and T extends number). Running both
release TSR and pinned tsgo with `--ignoreConfig --strict --noEmit --pretty false`
produces byte-identical complete output: heads, nested explanations, displayed
paths, line/columns, ordering and requested newlines. This is relation producer
smoke, not constructor-only proof. Explicit end/length fidelity remains limited
by existing producer span contracts; no full-length oracle claim is made.

Stock `cargo run --release -p tsr-conformance --bin coverage` also completed all
sixteen suites from a `git archive` scratch checkout of `f3a6bfbe`, with the
pinned vendor tree linked read-only and snapshots emitted only into that scratch
checkout. Its result is 12444 discovered cases, checker_types 8042/9538 (98.10%
lines), diagnostics 4221/5502. The original snapshot tree was not edited. Mount
namespace isolation was attempted first and denied (`Operation not permitted`);
scratch coverage resolved the limitation rather than skipping the gate.

## Native compaction follow-up

The representation now also exports `sort_and_deduplicate_diagnostics`, ported
from pinned `compiler.SortAndDeduplicateDiagnostics` and
`compactAndMergeRelatedInfos` (`internal/compiler/program.go`). It sorts by the
complete comparator, groups adjacent diagnostics using native equality excluding
related information, then concatenates, sorts and deduplicates related notes
using full native equality. Comparator-equivalent child trees with different
codes remain distinct; different chains are never collapsed merely because
heads/spans match. Input primary files must be attached before using this API.
This does not silently cut over the non-owned CLI collector.

Two added regression tests cover merged ordered related notes and preservation
of comparator-equivalent but unequal chains; all seven diagnostics boundary
tests pass. Diagnostics clippy and workspace fmt check pass. A standalone
release smoke prints one merged head and related notes in `a`, `z` order.
This uncalled collector API changes no production checking path; the prior full
verdict/performance evidence does not certify its future CLI integration.

The reported self-referential getter TS7023 failure belongs to the scheduled
atomic accessor publication/printing/objects port. No diagnostic side pass,
error suppression, or accessor producer edit was added here. Contextual handoff
`b9c48897` is not recorded as a checker fix or a converted case.

## Integration prerequisites — not edited outside ownership

1. `crates/tsr-execute/src/compile.rs`, diagnostic sorting/dedup block in the
   compile worker (around line 372): replace the head-only comparator with
   `LocatedDiagnostic::compare`, resolving existing Program source images;
   compact with the native equality/related-info merge operation, not only
   head/span/args or comparator equality. The representation now exports
   `sort_and_deduplicate_diagnostics` for attached-file diagnostics and
   `compare_diagnostics` for complete ordering.
   Otherwise native longer-chain preference can be discarded. Remove the stale
   no-chain comment in that block. Config diagnostic dedup around line 180 must
   also be audited before claiming global native equality.
2. `crates/tsr-conformance/src/diagnostics_suite.rs` and
   `errors_baseline.rs`: strict oracle must compare full flattened trees,
   lengths, related locations and stable native order. Current unchanged scores
   cannot observe this lane's message improvements.
3. Relation reporting needs the native `reportErrors` contract across recursive
   `Relater::is_related_to`, result publication and rollback. Owned relater.rs
   currently runs verdict-only workers. Its `one_signature_related_to` calls
   the non-owned signature comparator; signature parameter/return chain writers
   must be integrated by that owner, not reconstructed in a second reporting
   traversal. `compiler/typeParameterArgumentEquivalence` currently still lacks
   the parameter incompatibility + nested type-parameter explanation; directly
   reproduced against pinned tsgo. `compiler/typeParameterAssignmentCompat1`
   still lacks instantiated type-argument explanations. No converted cases
   are claimed for either.
4. Native `reportUnmatchedProperty` appends a declaration-related TS2728, and
   `reportErrorResults` may append TS2208 constraint advice. Producing these
   requires Program-owned source-image publication at checker diagnostic writers
   (`checker.rs`/`check.rs`), including cross-file declaration identity and the
   native `createDiagnosticForNode` span contract. The storage/renderer exists;
   no fake global related note or per-error source copy is substituted.
5. The direct type-parameter branch must eventually include deferred indexed
   access targets whose object is a type parameter, as native does; the source
   and target diagnostic normalization/reporting contract needs serialization
   with the indexed-access owner before extending that boundary.

A nested object control `{ x: { y: string } }` to `{ x: { y: number } }` still
lacks native `The types of 'x.y' are incompatible between these types` and the
nested TS2322. This is an explicit unresolved relation-chain writer, not a
successful full-cluster completion. `elaborateError`, native dotted-name
compression/return-signature rewriting, recursive chain rollback and all
related-information producers are not certified by these direct-target tests.
