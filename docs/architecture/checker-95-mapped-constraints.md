# Constrained mapped inference

Baseline e4fde75a: 448,476/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferToMappedType (inference.go:948) now traverses union/intersection key constraints.
For a key parameter K it infers keyof source with MappedTypeConstraint priority,
then follows K's constraint to perform homomorphic reverse inference. If that
constraint provides no reverse inference, source property/index value types are
unioned and inferred against the mapped template. Constraint cycles use an identity
visiting set. Existing inference variance and priority state is restored after
key inference; there are no fixture names or special printed-type candidates.

Both reverse caches include source, target and constraint identity, matching
ReverseMappedTypeKey. Mapped constraint nodes resolve keyof operators semantically
under their current mapper, including constraints of type parameters. Intersection
parts remain available before union distribution, matching getLimitedConstraint's
origin lookup. Reverse members whose literal keys fail the remaining intersection
constraint are filtered out. Numeric/computed source declaration names participate
in that filter without inferring their types from rendered spellings.

## Accepted measurement and limits

Checkpoint 2e3ef200: 448,493/478,855 correct assertions (93.66%),
6,572/9,538 complete cases (68.90%). Another 6,420 assertions are needed for 95%.
Aligned verdicts: 474,243 total; 448,493 right; 4,466 gap; 21,284 wrong.
Against e4fde75a: 4 WRONG→RIGHT and 13 GAP→RIGHT, zero RIGHT losses;
three GAP→WRONG remain. Gains: mappedTypes2 7, isomorphicMappedTypeInference 6,
mappedTypeInferenceAliasSubstitution 4. Newly wrong: mappedToToIndexSignatureInference
returns string[] instead of E[]; reverseMappedTupleContext has two unknown member
answers instead of literal string tuples. These remain counted in the deficit.

Five pinned declaration controls cover uniform template inference, Pick<T,K>,
transformed constrained templates, chained key constraints and filtered reverse
members. The filtered extra property intentionally produces tsgo TS2353 while its
emitted declaration retains only a:number. Release workspace tests and clippy with
warnings denied pass; 3,379 anchors resolve; snapshot refreshed; whitespace passes.

A numeric/quoted-key declaration probe now matches pinned tsgo: reversing
`{ 1: { value: number }, "2": { value: string } }` through `Pick<T, K>` infers
`K` as the string-literal union `"1" | "2"`, while the reconstructed `T` keeps
the numeric member syntax. `resolveReverseMappedTypeMembers` copies declarations
and name types but not `valueDeclaration`; consequently `resolved_keyof_type`
must treat a numeric declaration on a reverse mapped object as its copied string
name instead of re-deriving an ordinary numeric `keyof` key. A focused control
in `reverse_mapped.rs` distinguishes those answers. Ordinary `keyof { 1: T }`
continues to produce the number literal `1`. This closes tsr-6.10's written
numeric/quoted-key mismatch; computed-key inference remains outside this unit.

Exact optional reverse inference preserves the distinction between an absent
property and an explicitly written `undefined`. Native `inferFromProperties`
(`internal/checker/inference.go:829–835`) removes `missingType` from source and
target member types only when their property symbols are optional. It does not
remove ordinary `undefined`. The Rust structural property walk now applies the
same symbol-driven normalization before recursive inference: under
`exactOptionalPropertyTypes`, reversing `{ p?: string }` yields `{ p: string }`,
while both `{ p?: string | undefined }` and required
`{ p: string | undefined }` preserve `undefined`. Without the exact-optional
option, the property walk removes no constituent; the optional mapped template's
ordinary `undefined` then makes all three controls infer `{ p: string }`, matching
native. Focused controls distinguish both modes and explicit `undefined`; no
printed type spelling is inspected.

Partially inferable source flags, expanding reverse recursion, full member
widening and contextual nested indexed-access simplification remain incomplete.
The original intersection origin is retained on mapped metadata; general union
origin provenance across arbitrary alias transformations remains incomplete.
ts r-6.9 remains in progress (issue id tsr-6.9).

Code review: skipped (ce-code-review unavailable). The sequential main-thread rule
conflicts with independent review requirements. Manual review checked constraint
cache identity, priority restoration, cyclic constraints, original intersection
filtering and source declaration key types. This is not independent review.

Evidence: /tmp/tsr-95-mapped-constraints-filter-verdict.{tsv,log},
/tmp/tsr-95-mapped-constraints-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-mapped-constraints{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
4d27bc6b2fa29b07475716062675ff05f3f06c99ef6536a648c16fda0f1917a6.
Goal remains active.
