# Object-literal semantic construction and presentation

Preparation for `tsr-1yb.16.3.10`, at main `e744714c`. This records inspected
producer/consumer boundaries and a subsequent isolated runtime experiment. It
does not complete the ticket: general pending/refused rendering and transformed
presentation ownership remain unqualified. The source manifest is
[checker-object-rendering-boundary.json](checker-object-rendering-boundary.json).

## Why the boundary matters

Pinned native `5b1047d10d32e7d5b446be4de56b126ff42f82bb` separates semantic
construction from node building. `checkObjectLiteral` (`checker.go:13144`)
collects property symbols and semantic types, and defers accessor declarations
through `checkNodeDeferred` (`:13315`). `newAnonymousType` (`:25090`) publishes
structured members, signatures and indexes. It does not render a string for
every member or the whole object.

Native presentation lives in `serializeTypeForDeclaration`
(`nodebuilderimpl.go:2181`), `createTypeNodesFromResolvedType` (`:2627`) and
`createAnonymousTypeNodeEx` (`:2812`). These use the enclosing declaration,
source reuse, symbol accessibility and visited semantic identities. Printing
can request semantic types; this separation does not mean printing is pure.

Rust currently renders during semantic construction. A plain property executes
`member_text_at` at `objects.rs:2025`, unless written single-quote reuse supplies
text. The call precedes the unnamed-member early exit, so its result can also
be discarded. Named properties may copy the string into `PrintedSlot` (`:2076`)
and retain it in `Member`. Property-only ordering rebuilds those members with
`property_members` (`callable_expandos.rs:124`). The mint renders the whole object
(`objects.rs:2154`), stores that text in `TypeData::Named`, and clones the member
plan into `object_literal_members` (`:2197`). These are locating sites, not
executed call counts, copied bytes or a measured benefit ceiling.

The older CPU sample's 16.4% generated/2.0% API rendering share belongs to
`8817b95b` (runtime-equivalent to `5dd3bad8`). It is not a current-source sample
or proof of time recoverable by removing these strings. The frozen native
executable and raw `/tmp/tsr-native-gap-profile` archive are absent in this
workspace; new native execution requires a newly qualified pinned build.

## Existing owners and publication

| Value | Owner/key | Publication and consumer context |
| --- | --- | --- |
| Property semantic type | Private Checker; resolved `TypeId` or accessor `SymbolId` in `PropertySlot` | `property_type` is the canonical reader; an accessor follows the symbol's resolution stack and published type. |
| Property printed slot | Private anonymous image and surviving member | Baked `Some(String)` or on-demand `None`; `property_printed_type` prints a missing slot without a reference site. It cannot substitute for arbitrary site-aware property rendering. |
| Source provenance | `AnonymousProperty.origin` and `checked_declaration` | The surviving checked assignment differs from a merged symbol's first declaration. Regular/widened transfers preserve provenance while semantic slots change. |
| Semantic object image | Private minted `TypeId`; symbol, properties, indexes and signature metadata | The current mint publishes images after member construction. Reserved instantiation identities and temporary widening self-transfers are not completed images. |
| Member display plan | `object_literal_members[TypeId]` | Carries source ordering, modifier/name spelling and signature/member text. It is also copied for site rendering. |
| Whole-object text | `TypeData::Named.text` | Used by presentation and by existing semantic empty-object shortcuts. Equal text is not type identity. |
| Recursive presentation state | Private Checker, per print's semantic identity | `rendering_composites` is reset after success/refusal. Re-entry and source pseudo-type reuse have different native paths. |

An on-demand slot is already used for synthetic `any` properties in
`empty_object_type_from_string_literal` (`inference.rs:4293`). That site has no
original alias-bearing declaration and does not qualify general object-literal
reuse. Inferred reverse mappings also have placeholder slots during preparation
(`inference.rs:3646`), replaced by completed member text (`:3709`). Neither is a
license to treat placeholder or absent presentation as completed semantic work.

The presentation slot is transformed by semantic consumers: instantiation
replaces it after substituting the property and accessor write type
(`declared.rs:2101`); widening replaces changed property types and leaves lazy
accessors unread (`widening.rs:376`); spread collisions retain left provenance
but replace type/modifiers and printed text (`spreads.rs:207`). A future deferred
plan must follow these transformations rather than cache the original string
for every receiver or mapper image.

## Boundaries that prevent a direct string-removal change

The immutable fallback is another API boundary. `Checker::type_to_string`
(`checker.rs:1934`) takes `&self` and delegates to `printing::type_to_string`
(`printing.rs:699`), which reads stored text without semantic preparation.
`type_to_string_at` (`checker.rs:1984`) takes `&mut self` and can execute the
site-aware worker. Removing stored text therefore requires auditing callers
that currently rely on immutable baked presentation; making a slot absent does
not create a native mutable node-builder demand. A deferred representation must
provide that owner/demand path or retain a qualified prepared fallback. This API
prerequisite belongs to `.16.3.10.1`, alongside original reference ownership.

An isolated receiver-only compile audit confirms the boundary: changing
`type_to_string(&self, ...)` to `&mut self` produces twelve checker-library
borrow errors. Index-info, mapped-type and predicate renderers need mutable
receivers; indexed-access and diagnostic arguments must be prepared before
borrowing their enclosing consumer; tuple printing cannot hold a side-table
borrow across a mutable print. The adapted ten-file prototype compiles every
workspace target, including the conformance `infergen` helper and union/intrinsic
test callers. The tuple loop copies one element identity and its presentation
metadata at a time, rather than cloning a whole production member plan.
The replay patch, original failure and final compile receipts are embedded in
the boundary record. It still delegates to the existing baked-text printer;
compilation alone does not establish deferred rendering or semantic preservation.

`relater.rs:1256` recognizes `Named.text == "{}"` for a primitive-to-empty-object
shortcut; `unions.rs:1615` also uses it while selecting subtype reduction. An
empty or placeholder string changes semantic answers. Native
`IsEmptyAnonymousObjectType` (`checker.go:26476`) instead requires anonymous
object identity and completed empty structured members, or an empty type-literal
member table. `isEmptyResolvedType` checks properties, signatures and indexes,
excluding `anyFunctionType`.

Rust has `is_empty_anonymous_object_type` (`intersections.rs:669`), including
type-literal provenance and represented property/signature/index facts. Its
applicability to every existing text shortcut still needs qualification; do not
replace those shortcuts merely because the helper has a matching name. An
uncomputed signature/index table is not established complete emptiness.

Union ordering is a separate semantic text consumer. `named_symbol_name`
(`unions.rs:398`) uses `Named.text.starts_with('{')` to distinguish structural
objects from named aliases, then reads the last dotted segment for name sorting;
`type_name` (`:434`) also reads stored text. Using the members-table symbol as a
replacement previously lost the passing `callWithSpread4` case: an alias's
synthetic member owner is not its native type-name symbol. A deferred plan must
preserve this alias/structural distinction before removing whole-object text;
repairing only the two empty-object shortcuts is insufficient.

`member_text_at` already declines forcing ordinary pending returns before object
publication (`objects.rs:1035`). `object_literal_text_at` additionally requires
a completed original image, qualified regular/widened transfers, source-plan
agreement and no active enclosing-variable resolution (`printing.rs:275`).
The actual display path may preserve annotation spelling, method/accessor
overrides, source arrow reuse, alias visibility and recursive elision. Reusing
the existing on-demand slot loses that site's explicit reference argument.

## Executable attribution handoff

The next isolated probe must distinguish three origins at the actual call sites:

1. Plain object-property rendering at `objects.rs:2025`, separating written
   spelling reuse, renderer executions, pending-return refusal, fallback and
   unnamed-member discard. Calls to the same helper from type literals are a
   different producer and must not be included by name alone.
2. Object-mint member-plan preparation through `property_members`, separating
   executed name/text copy payloads from retained final slots and overwritten
   intermediate plans. Other callers keep their own attribution.
3. Whole-object rendering at the mint and member-plan cloning, separating
   emitted string bytes, actual allocator requests, retained payload/capacity
   and discard. A logical clone payload is not an allocator request or RSS.

Use invocation/private-owner/local ordinal and source-node provenance for
observation; before the mint there is no final `TypeId` to use. Nested semantic
forcing belongs to one root origin, with inclusive/exclusive work reported
separately rather than added twice. Record unclassified allocation/copy traffic
and failed/refused roots. Probe-off/on/repeat must preserve complete output,
inputs and actual checked identities in default and single modes. The corrected
[pool-preserving tracer](tsr-work-trace-producer.md) provides covered owner/file
evidence; it still does not observe every semantic force.

No-emit semantic checks and actual display/declaration demands require separate
controls: imported/qualified and shadowed aliases, written annotations, spreads
and surviving order, `this`, optional/readonly/computed/accessor/signature
members, pending captured returns, recursive images and mapper query order.
Existing accessor fidelity `tsr-2zk.11.5` and regular-image completeness
`tsr-2zk.1.6` retain their scope and owners. The general mapper/member publication
contracts remain applicable. No production edit, completed handoff or speed gain
is claimed before those measurements and controls.

## Executed origin probe

The [probe record](checker-object-rendering-probe.json) qualifies three batches,
each with 12 fresh children: preflights and ordinary/off/on/repeat checks in
default and single modes, on the small object fixture, generated400 project,
and a natural captured-return/written-quote fixture.
Complete output and repeated covered owner-work agree; the strict reader accepts
all twelve observer receipts. Five allocator controls pass, including cross-thread
free/resize, alignment, rejected resize, nested ownership and unwind. This uses
the existing archived prefix allocator, with exclusive origins and actual
`type_to_string_at_worker` entry counters; canonical checker code is unchanged.

| Generated400 boundary, default | Executions | Allocator request bytes | Logical string payload bytes |
| --- | ---: | ---: | ---: |
| Plain property render request | 9,197 | 158,807,373 | 89,082 |
| Property-plan preparation | 2,805 | 866,807 | 131,047 |
| Whole-object rendering | 2,805 | 545,482 | 176,247 |
| Member-plan clone | 2,805 | 866,807 | 131,047 |
| Printed-slot clone | 9,197 | 89,082 | 89,082 |

Those 9,197 requests execute **11,990 actual site-renderer workers**, including
nested rendering. All requests return site text in this workload. Other object
work requests 122,561,565 bytes; unclassified traffic and post-command live/peak
requested storage are retained in the record. Origins are exclusive, but a
renderer origin includes semantic preparation it calls. These bytes are neither
RSS nor a string-only allocation total, and the final post-command snapshot is
not retained object storage. Single mode repeats the same request/worker/payload
counts with 158,807,289 render request bytes. Written reuse, pending refusal,
site decline and unnamed discard are zero observations on generated400.

The natural captured-return fixture, taken from existing lazy-return test shapes,
separately executes seven helper requests: four pending-return refusals and
three site-renderer workers, plus one written single-quote reuse. Both modes
repeat those counts and preserve off/on/repeat output; fresh native and ordinary
release both check clean. This proves that request counts cannot substitute for
renderer executions. No-site fallback, renderer decline and unnamed discard
remain unobserved; original/transformed reference-site and private-root
publication still need qualification.

The result changes the next action: copying member plans is a small part of the
observed request traffic; removing those clones alone does not address the much
larger work beneath eager property rendering. Native avoids that rendering while
constructing semantic members. A safe deferred plan must retain the original
reference and survive the transformations above. `tsr-1yb.16.3.10.1` owns the
unobserved branches, private-root attribution and transformed presentation
boundary before a runtime change.

Fresh pinned native controls match the diagnostic on an object fixture covering
imported/qualified types, annotations, spreads, computed keys, readonly text,
accessors, `this`, captured returns, recursion and generics. Native declaration
mode also writes two `.d.ts` files; TSR writes none and returns outputs-skipped
on the intentional diagnostic. The current CLI does not emit outputs, as
`compile.rs` documents. This is a presentation limitation, not a successful
declaration-display comparison. Existing cold/warm reordered direct checker
tests remain required; this probe does not replace them or certify the corpus.

Five ordinary paired observations report default medians TSR **1.192s** versus
native **0.329s**, and single **2.913s** versus **0.539s**. Configuration, loaded
scope and complete diagnostics agree; complete cross-tool inputs/performed work
remain unverified. Workspace verification was running, and the default batch
also overlapped the observer. These observations establish neither controlled
throughput nor the <=0.50 release target. Probe resource timings remain separate.

Replay the [isolated patch](checker-object-rendering-probe.patch) on the recorded
source plus qualified tracer overlay, build with `tsr-execute/work-trace`, and
freeze the executable before running the [control driver](checker-object-rendering-probe.py)
with `--project`, `--probe`, `--ordinary` and `--output`. Do not rebuild its
executable during a batch. A v1 generated batch was correctly rejected when the
host violated that rule; its failure remains recorded. Cold first launches also
timed out at 300s for the probe and 60s for native before output, while unchanged
repeat version launches completed in 14/43ms. No security/signature change or
deadline increase was used; these rows stay outside checker comparisons.

## Original and spread presentation demands

An isolated follow-up at the same frozen `e744714c` base compiles the mutable
receiver caller adaptation and executes12 fixtures across36 private checkers,
three cold entry orders and three warm/reversed rounds. All432 explicit site
reads and216 baked reads preserve their answers and completed symbol identities
under these orders. The unchanged adapted sources also pass225 existing focused
object, lazy-return, type and union tests. These are bounded preservation checks,
not native display equivalence or a deferred-rendering implementation.

Fresh pinned native declaration controls succeed for all12 fixtures and retain
actual declaration files. The natural recursive fixture
`const value = { next() { return value; } }; const copy = { ...value };` exposes
a stronger limit: the private checker answers `any` and reports two false
circularity errors under every observed order. A frozen ordinary CLI reproduces
TS7022 and TS7023, exit1, while native with the same strict/noEmit/ES2020/skipLibCheck
arguments exits0. Native declaration demand writes the recursive method shape
with visited-type elision. Thus stable reads can preserve an incorrect eager
publication boundary. `tsr-1yb.16.3.10.2` owns attribution and repair of the actual
method-return demand; the plain-property rendering probe does not cover it.

The first follow-up harness build fails with10 compile errors (unavailable
test-only JSON dependency and tuple diagnostic access). A separate corrected
source copy uses literal fixtures and the existing diagnostic tuple API; its
Linux test completes with one passing characterization. The original source and
still-live macOS build are preserved. The lossless records and replay source are
embedded in the boundary JSON. No canonical checker implementation changes.
