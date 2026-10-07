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

## Integration prerequisites — not edited outside ownership

1. `crates/tsr-execute/src/compile.rs`, diagnostic sorting/dedup block in the
   compile worker (around line 372): replace the head-only comparator with
   `LocatedDiagnostic::compare`, resolving existing Program source images;
   deduplicate by native comparator equality, not only head/span/args. The
   representation exposes `compare_diagnostics` for attached-file diagnostics.
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
