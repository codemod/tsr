# Private member publication: compiling rejection

The `tsr-1yb.33` experiment allocates actual private symbols and retains their
publication history. It compiles, but is **rejected** at TSR
`88a0d3cec2df7d1e43aff6684349eea1c6256446` / native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Scalar query history is
Merged/Merged/Single instead of native Single/Merged/Single. **Correction:**
the distinct `this` test confused supplier reads with the synthesized result;
its alleged second parity failure was an incorrect expectation, as the
qualification below explains.
No runtime change, production cache, coverage gain or speedup is retained.
Equivalent complete-work TSR/native median wall <=0.50 remains unverified.

The [natural-publication continuation at d417a8c9](checker-natural-member-publication.md)
now passes the previously pending circular-default field/reset control. Its
broader retention gate still fails on 63 type assertion and eight diagnostic
case RIGHT losses. The historical experiments below remain unchanged.

The [receipt](checker-private-member-publication.json) contains full test/build
outputs, source and binary bindings, mutation payloads, executed drivers and all
ordinary CLI diagnostics. The [rejected patch](checker-private-member-publication-rejected.patch)
applies to the exact Rust revision above, including the two failing qualification
tests. It is a replay asset for an isolated archive, not an implementation to
copy into main. [The ownership contract](checker-published-members.md) remains
the required boundary; its proposed native completion protocol is not certified
by this experiment.

## What the experiment actually builds

One exclusive Checker owns the new `MemberPublication` store. Member tables
retain actual `SymbolRef` handles, keyed by concrete TypeId and receiver context;
positive intersection results have a separate retained cache. Private links
carry target, an ordered mapper-operation handle, independently lazy read/write
types and containing type. The bound read adapter consults `symbol_types`, so
observing invariant reuse does not independently populate another bound read
cache. Private IDs never become binder indexes.

`CheckerSymbols::new_member_from` copies declaration metadata without invoking
`clone_symbol` or recording a merge redirect. Instantiation checks already
resolved read/write invariance before allocating a private symbol. Instantiated
symbols retain the original target and ordered composition; a merged clone
keeps INSTANTIATED clear, its own target identity, the first supplier's mapper
and write, declaration parent and containing type. The intersection read in
`members.rs::property_type_via_shape` consumes a stored handle rather than a
classification reconstructed from declaration/read equality.

The store is deliberately a **property-only experiment**. Its image frames,
revision and `done` state describe that surface, not native MembersResolved or
field-complete structured members. The reset hook is attached to Rust base
resolution completion and withdraws only a target-image certificate; escaped
symbols and positive composite history remain. The active-prefix control pushes
a resolution frame, and the clone control explicitly invokes a reset. Those are
synthetic protocol controls, not qualification of native's naturally continuing
outer worker. Native natural prefix/reset behavior stays established by the
[recursive-base producer](checker-native-recursive-bases.md); reproducing it in
the Rust writer remains unfinished.

Other unqualified details include native MapsThisOnly/isThisless admission,
objectFlags-based invariant eligibility, full mapper identity, synthesized
declaration/flag propagation, reduced apparent container identity, unions,
augmentation, mapped/index/signature surfaces and deferred multi-supplier work.
The constructor/store shape is useful evidence, not a waiver for these gaps.

## Tests and the missing preparation path

The pre-implementation red adapter uses the existing bound-symbol getter. It
cannot return the inherited generic handle at all, although the type getter can
compute a read. The compiling store then exposes real private handles and
preserves repeated lookup identity. The final candidate library run has
**188 passes and two failures**; after exact restoration, all **185 baseline
library tests pass**.

| Observation | Result |
| --- | --- |
| Seven fixture families × three fresh query orders | Historical run reports 18/21; the three distinct-self failures incorrectly compare the result with the native supplier expectation |
| Scalar cold / expression-first / checked-first | Merged / Merged / Single; native is Single / Merged / Single |
| Shared inherited `self` | One supplier identity and Both read/expression retained |
| Distinct `self` | Synthesized Left & Right result/expression is native behavior; suppliers individually read Both. The historical assertion expected Both from the result and is corrected below. |
| Equal/unequal generic value and distinct equal-valued declarations | Single string / synthesized never / synthesized string retained |
| Both accessor orders | Merged clone retains first string/number write independently of number read |
| Clone and explicit reset | Own-target versus link-target, flags, parent, mapper/write and warm identity controls pass |
| Active prefix | Readable active own member is not completed; synthetic resolution-frame control passes |

Five mutations each fail a previously passing dedicated control: collapsing
instantiations to bound declarations, treating a clone's link target as its
getTargetSymbol result, flushing all composite history on reset, certifying an
active prefix, and admitting setters from invariant reads alone. Mutants compile;
the receipt does not count compiler errors or an already-failing broad test as
sensitivity. All six candidate sources restore after this batch, and the full
candidate library rerun reproduces exactly the two qualification failures.

A fresh extension of the existing native producer records constructor state and
call stacks **without forcing additional read/write types**. All three scalar
orders still pass. Cold lookup constructs unresolved scalar suppliers during
raw reduction, then constructs suppliers with the original read already resolved
during later apparent preparation. Expression-first constructs unresolved
suppliers before the adapter and retains its earlier clone. This trace records
17 constructor attempts including checked-first declaration work; these are
deliberately driven tests, not ordinary workload or saved-worker counts.

Native `getReducedApparentType` (`checker.go:21860`) runs reduction before and
after `getApparentType`; `getReducedType` (`checker.go:21819`) can build composite
properties while detecting a never intersection. Property expression preparation
has its own apparent-type entry (`checker.go:11258`, apparent preparation at
`checker.go:11265`; corrected from the alias-marking anchor at 28462). Consequently, retaining
symbols behind `(type, receiver)` without wiring those construction paths cannot
recover the native history. Adding a cold-query special case or pre-forcing a
scalar type would hide the missing representation rather than port it.
The [native trace patch](checker-private-member-publication-native-trace.patch)
applies on the preceding shared-property archive with its existing two helper
files. Both touched native files restore exactly, and all three previously
qualified source hashes match. This trace did not capture a new full-archive
before-hash inventory. Its initial audit mistakenly compared that three-file
map with all 55,105 files; the failure and correction remain in the receipt.
The driver's initially null revision field is likewise retained and qualified
against the preceding native pin and exact checker source hash.

## Ordinary output and delivery

Twenty-seven public fixtures/assignment variants execute native, frozen baseline
and prototype in default and single-thread modes: **162 completed fresh CLI
invocations**. Launch PIDs, full stdout/stderr, exit status, ordered listed inputs,
options, input bytes and binary hashes are retained. Modes agree for each tool;
all **54 baseline/prototype full-output pairs are byte-identical**, including
already-WRONG cases. Thirty-two pairs match native diagnostic payloads; 22 retain
11 existing fixture/variant gaps in accessor writes, optional/private reads,
generic cycles, invalid bases, override reporting and circular defaults.
There are no changed diagnostic pairs to triage in this matrix.

The private archive restores all 650 baseline Rust files byte-identically and
removes the added Rust module. Both replay patches pass apply-check. The 108
pinned bundled library inputs are present for CLI compilation. The initial
build-cache copy was stopped; Cargo rebuilt missing artifacts. Canonical runtime
files remain unchanged. Full unfiltered corpora, workspace lint and whole-project
timing were not run for this rejected private prototype; no production retention
or no-RIGHT-loss claim follows from these bounded outputs.

`tsr-1yb.33.1` owns the remaining compiling repair after the preparation progress
below: original receiver identity and natural field-complete publication
continuation. It consumes this failure and existing native producers, preserving
the receiver `tsr-6.69.2` and signature `.27`/`.28` owners. The concrete builder
`tsr-1yb.4.2.1` depends on it and still requires current expensive construction
counts, full fidelity and measured ordinary benefit before retention. No costly
native-style builder reuse candidate is qualified by this experiment.

## Prepared reference progress at 55ed1a2a

The continuation of `.33.1` is an isolated experiment frozen at TSR
`55ed1a2a09b1a77841cbac34b21db53eae659718` and the same native pin. The
[receipt](checker-member-preparation.json) and [replay patch](checker-member-preparation.patch)
preserve the compiling implementation and its pending natural control. They
are progress assets; no runtime implementation is retained on main, and `.33.1`
remains in progress.

Concrete prepared TypeIds now retain ordered ordinary arguments plus the final
original `this` argument. Raw references implicitly pad their own receiver;
already prepared references retain their explicit argument. Apparent
intersection construction and raw reduction publish separate concrete images.
Reduction enumerates properties before the later apparent construction, whereas
the expression consumer prepares the apparent type first. This produces native
**Single / Merged / Single** scalar history, including readonly/private variants,
without testing query order or pre-forcing scalar links. Warm identity, first
accessor mapper/write and the clone's own target remain intact.

The fresh existing native producer passes all **39** parsed-program controls.
It also corrects two previous assertions. Distinct-self supplier read/write IDs
equal the original aliased receiver; the synthesized result and expression share
a different, unaliased intersection ID. Expecting `Both` from the result was
wrong. Likewise, a merged clone's containing type is the prepared apparent
intersection, not the raw receiver. The old receipt and failing tests remain
unchanged as historical evidence; the replay here explicitly corrects them.

Matching the native exact-optional options exposed a real writer gap. Rust's
binder never sets OPTIONAL, so construction recovers the existing declaration
fact before allocating a private symbol. Ordinary property writes remove missing
after instantiation, independently of accessor writes; the read adapter removes
missing only for its native `getNonMissingTypeOfSymbol` projection. The initial
optional control used different options and its pass did not establish this
agreement. Its corrected red failure and subsequent repair remain in the receipt.

The current library run has **192 passes and one ignored pending test**. That
test is also executed explicitly: all three natural circular-default orders
still miss native TS2310. The actual property-image trace shows one Foo worker
per order, target resets with an active frame in the first two, and no target
reset after checked-first preparation. None of these property states is relabeled
as native MembersResolved. A separate natural recursive-base control observes
the unresolved `[Root]` prefix and completed `[Root, Later]` list at the real
base re-entry; it pushes no synthetic frame and invokes no explicit reset.

Full structured-member fields, natural reset/outer-worker continuation, native
MapsThisOnly/isThisless and objectFlags admission, complete mapper metadata,
synthesized declaration/flag propagation, signatures/indexes, augmentation and
broader fidelity remain unqualified. Ordinary output preservation and focused
identity tests cannot authorize production reuse or establish a speed win.

Three final mutations compile and fail previously passing controls: skipping raw
reduction, replacing the original final `this` argument, and retaining missing in
an optional ordinary write. Each source edit restores exactly; a subsequent full
library run again passes **192 tests with one ignored**. Compiler setup failures
are preserved separately and are not counted as detected mutations.

The intermediate and final binaries each complete **162 ordinary CLI invocations**
on the same 27 fixture/variant inputs, in both modes. In each batch all **54**
baseline/prototype stdout, stderr, exit-status and ordered-input pairs match,
including already-WRONG cases; **32** pairs match native diagnostic payloads.
The same **11** existing fixture/variant gaps remain, with no changed diagnostic
pairs to triage. Recorded PIDs identify wrapper launches. The baseline binary is
reused after verifying 650 Rust files, 22 Cargo inputs and 108 libraries equal;
it is not a fresh baseline build. These bounded replays have no CPU/RSS or
equivalent complete-work timing qualification.
