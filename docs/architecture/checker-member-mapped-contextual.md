# Mapped contextual member ownership

The private `tsr-1yb.33.1` replay at frozen TSR `0b18d357` removes the
remaining prior-correct losses in both unfiltered corpora. It combines the
archived member publication candidate with the qualified signature-admission
seam and repairs the actual mapped contextual read. This is a correctness
prerequisite for builder `tsr-1yb.4.2.1`; the PR #5 slowdown and complete-work
TSR/native median wall target of at most 0.50 remain unresolved.

The [receipt](checker-member-mapped-contextual.json) retains commands, terminal
outcomes, source and binary hashes, full public outputs, changed corpus rows,
failed setup attempts and explicit test-expectation corrections. The [combined
replay](checker-member-mapped-contextual.patch) applies to `0b18d357`, including
12 source/test paths. Canonical runtime is unchanged. The source boundary of
release/full-corpus qualification is the 11-path candidate before the later
one-file integration-test correction; its runtime bytes are identical.

## The wrapper owns the contextual read

In `thislessFunctionsNotContextSensitive3`, `Partial<ExtensionConfig<O>>` has
an identity-optionality wrapper whose Named.members points at ExtensionConfig.
Its actual reference is Partial with the instantiated ExtensionConfig<O>
argument. The old contextual branch called publication_property directly with
the wrapper and bypassed the typed property reader. It read the declaration
owner and reconstructed ExtendableConfig with its own Options parameter.
That Options became a contravariant inference candidate for O, overriding the
concrete option returned by addOptions. The inferred Extension<Options> erased
18 previously RIGHT type rows and two required duplicate-property diagnostics.

Passive traces preserve the original 132 stdout rows byte for byte and inspect
stored IDs only. They show correct inherited O substitution before the wrong
wrapper read; missing base arguments and applicability retry are not the cause.
Probe compiler failures are preserved as setup failures, and all probes are
removed before regression tests, builds and final corpus runs.

Native getTypeOfConcretePropertyOfContextualType at pinned `5b1047d`
checker.go:30638 reads the instantiated symbol and removes only optional
missing. The existing Rust concrete_contextual_property_type already owns the
ordinary semantic property reader, instantiated values and that optionality.
The fix exposes it within the crate and uses it in the existing Some-property
contextual branch. Union/intersection discrimination and the None fallback keep
their existing owners. No cache, name-based exception or inference override is
added.

## Regression qualification

The regression test in contextual.rs compiles and fails before the fix on the
actual missing numeric property. It passes after the fix across method/arrow,
cold/checked-first and positive/negative combinations: eight parsed programs,
24 warm numeric-property reads, and the required TS2322 only for the negative.
Restoring the prior raw-owner reader while keeping the final test compiles and
fails the same semantic assertion. Exact restoration passes 205 library tests.
A driver initially looked for the assertion on stdout rather than stderr;
its correction consumes the original terminal semantic failure without rerun.

The complete checker package initially stops at three member integration
failures. The entry prototype also fails those three; frozen main passes all
18 member tests. Fresh 36 native/main/final CLI children plus native base
resolution at checker.go:19167/19220 show that two tests pin obsolete generic
inheritance gaps and the circular-class test pins the old shortcut walker.
The private test correction checks concrete number values, original declaration
suppliers in both base orders, rejected circular inheritance, surviving own
members and a positive acyclic control. Original failing tests/outcomes remain
in the receipt. The corrected complete checker package passes 1523 tests
across 101 result blocks, with no failures and three pre-existing ignored tests. These
ignored controls concern call/index signature printing, unnamed call/construct
signature members and generic identity aliases. No ignore is added by this
candidate; full execution of those controls is not claimed. Native public circular-base TS2339 and interface conflict
elaboration still differ; the correction does not assert full diagnostic parity.

## Complete public and corpus results

The 344-child public matrix contains 43 inputs, native/main/entry/final tools
and default/single modes. All 172 complete default/single output pairs agree.
Full native agreement across 86 case/mode pairs is main56, entry62 and final66;
no main/native agreement is lost. The new method/arrow negative controls match
native, and the original duplicate TS2783 errors return. Signature merged and
subtypesOfUnion TS2411 controls stay correct.

All 10 remaining wrong case/variant pairs are preserved as complete pairs, including
independent accessor writes, invalid bases, derived-interface compatibility,
constructor return compatibility, assertion overlap, generic diagnostic names
and diagnostic elaboration. Required negative errors and diagnostic bodies
are compared; an empty output alone is not acceptance. The additional 36 legacy
contract children have six native agreements for both main and final across
12 case/mode pairs, with no lost agreement and all 18 full mode pairs equal.

Fresh baseline/final binaries run all 477,970 type rows and 10,570 diagnostic
cases without a fixture filter. Types improve 41 WRONG-to-RIGHT and eight
GAP-to-RIGHT; five already-WRONG payloads change; zero prior RIGHT rows are
lost. Diagnostics improve four WRONG-to-RIGHT and two EMPTY_WRONG-to-EMPTY_RIGHT;
zero prior passing cases are lost. Row keys, diagnostic expectations and
populations remain equal. All 54 type and six diagnostic changed rows are kept.
The older frozen538 eighteen-type/one-diagnostic refusal remains historical;
these are new source-bound runs, not a relabeling of the old gate.

## Retention limits and continuation

The replay remains private. Full corpus parity does not certify all native
metadata, every public diagnostic, canonical workspace integration or a cost
benefit. The archived member candidate still has an unused containing-field
warning and unformatted source; no strict-lint or global-format pass is claimed.
Only the newly changed contextual/symbols files were formatted. Three scoped
simplification passes reused the existing helper and left the preliminary
property lookup because removing it would change the None/union fallback.

The next optimization gate is current expensive member construction attribution
for builder4.2.1, preserving the actual published handles, stored fields,
receiver identity, mapper ownership and natural reset continuation. Measure
complete equivalent public work, wall/CPU/RSS and fresh-process medians before
retaining a faster implementation. Unit requests, corpus elapsed times and
private performance scaffolding are not speed claims. Receiver6.69.2 and the
remaining diagnostic owners keep their scope; signature27/28 handoffs remain
qualified at their own frozen source boundaries.

The passive delivery passes 4,438 upstream anchors (zero unresolved) and section
validation (zero dangling). The global issue-ID check still fails on 191
historical IDs; all 737 failing citation lines exist unchanged in frozen main.
The continuation issue IDs resolve in authoritative Beads. An initial legacy
PATH caused the validator to skip; that setup outcome and the corrected failing
global check are both retained. The local lite review covers only the five
passive paths, finds no actionable defect, and makes no independent-review claim.
The replay initially omitted Git new-file metadata; the corrected patch applies
to a fresh consumer with all 12 source hashes exact, without runtime changes.
