# Member-consumer prototype qualification

The `tsr-1yb.31` prototype compiles and passes seven focused controls plus the
20 existing member/name controls at TSR
`f5db6e7898b2afdc51bc974512dd8b9351299864`. Four incorrect mutations are detected.
It is **rejected**, because its property identity model disagrees with pinned
native scalar-symbol completion. No runtime implementation is retained, and
this work supplies no throughput claim or proof of the required comparable
TSR/tsgo median wall ratio <=0.50.

The [receipt](checker-member-consumer-prototype.json) retains the source and
binary bindings, full test/build outputs, fixtures, ordinary CLI outputs,
executed drivers, mutation sources and restoration evidence. The
[rejected patch](checker-member-consumer-prototype-rejected.patch) applies to
the exact TSR revision above. It includes the failing scalar qualification
test; an expected failure is the reproduction of rejection, not a passing
implementation to copy into main.

## The compiled boundary

Native is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`. The prototype consumes the
[shared-property contract](checker-native-shared-property.md) and
[recursive-base contract](checker-native-recursive-bases.md), corresponding to
`getPropertyOfTypeEx`, `resolveTypeReferenceMembers`,
`createUnionOrIntersectionProperty`, `compareProperties` and `getBaseTypes`.

It changes one intersection read consumer in `members.rs::property_type_via_shape`.
A query-local result in `member_completeness.rs` distinguishes a complete read
view, an active base prefix, failure and unsupported work. The view carries a
declaration root, supplying reference, original receiver, read/write types,
optionality, readonly/accessibility metadata and a proposed single/merged/
synthesized identity classification. Every ID belongs to the executing private
checker and its binder/TypeStore; none is transferred between checkers.

The supplier walk reads filtered `get_base_types`, composes each base under the
receiving reference, and types the actual supplying declaration with the original
receiver. Equal declaration origins require exact read and metadata equality
before merging. Writes are deliberately not compared: the qualified native
accessor branch keeps the first supplier's mapper and write. Reversing the
intersection changes the prototype's retained write from string to number.

No member view is stored. Each request reconstructs it after base forcing. An
active base prefix can be observed but cannot be a completed view. Unsupported
results cannot certify general absence or a reusable empty member table. The
work boundary is base forcing, supplier traversal and instantiation; these tests
do not count saved worker executions or copy cost.

This is a read-view experiment, not the native symbol builder. In particular,
its `root` is a declaration origin, not the result of native `getTargetSymbol`
for a newly merged clone. A merged native clone preserves its declaration
parent and first mapper/write, has a containing intersection and a link target,
but lacks the instantiated check flag: its target identity is the clone itself.
The prototype's classification and supplying reference cannot replace that
published symbol state.

## What passed, and what rejected the model

The initial baseline test returns `this` for the cold shared-inherited query
where native returns `Both`; the distinct-declaration control already passes.
The prototype passes both. The expanded controls also preserve equal versus
unequal generic reads, distinct origins with equal reads, accessor write order,
and the cycle's retained `Root`/`Later` bases without re-admitting `B`.

Repeated queries use the same private checker; generic and accessor controls
also use fresh parsed Programs in receiver-first, expression-first and checked-
first orders. Assertions compare IDs only inside a checker and compare semantic
expectations between independent Programs. The active-prefix countercontrol
explicitly pushes a resolution frame; it is not presented as a naturally nested
Rust callback. Natural native re-entry and member reset remain qualified by
the recursive-base producer.

| Incorrect mutation | Observed failure |
| --- | --- |
| Unconditional synthesis | Shared `Both` becomes `Left & Right` |
| Merge on declaration symbol without read equality | String/number intersection returns `string` instead of `never` |
| Apply the receiving owner's mapper to a supplying declaration | Shared concrete receiver becomes `this` |
| Treat active base resolution as completed | The active-prefix control receives a complete own-member view |

The additional scalar qualification executes all three query orders for
`Base<T>.value: number`, inherited through different concrete generic arguments.
The prototype returns `[Merged, Merged, Merged]`. Native's qualified and replayed
answer is `[Single, Merged, Single]`: receiver-first and checked-first reuse the
resolved invariant original symbol; expression-first retains its earlier merged
clone. Read type `number`, declaration origin and supplier arguments are the
same relevant facts throughout. The missing input is the actual symbol
instantiation/publication history. Repeated getter reconstruction cannot infer it.

Do not fix this by equating all scalar reads or rejecting this native-supported
family in production. The next member builder must preserve the published
symbol view, instantiated flag/link-target distinction, completion-dependent
invariant reuse and clone context. Native's original-symbol reuse also depends
on resolved writes for setters; equal reads alone cannot establish eligibility.
The receiver repair remains `tsr-6.69.2`, and signature admission remains
`tsr-1yb.27`/`.28`.

The follow-up is `tsr-1yb.32` (published-member identity and ownership contract)
then `tsr-1yb.33` (one compiling consumer). The concrete builder
`tsr-1yb.4.2.1` depends on that consumer. These tasks preserve existing owners
and production fidelity/benefit gates; neither authorizes copying this rejected
classifier into runtime code.

## Ordinary output controls and limits

The replay executes 27 public fixtures/assignment variants from the two native
producers, with native, current-source baseline TSR and prototype TSR in default
and single-thread modes: 162 distinct completed compiler children. Full stdout,
stderr, exit status and ordered loaded-file lists agree between modes for each
tool. All 54 baseline/prototype output pairs are byte-identical. No changed
diagnostic case or user-visible improvement is claimed from this matrix.

Native and TSR still disagree in existing accessor-write, optional-read,
private-access, generic-cycle display, invalid-base, override-chain and circular-
default controls. Those complete outputs remain in the receipt. Mode agreement
and an unchanged baseline cannot establish native parity, performed-work
equivalence or whole-project speed.

The prototype does not qualify general union/index/mapped/static surfaces,
mixins, augmentation, signatures, more than two composite suppliers, or
cross-worker reuse. Its unresolved symbol identity already prevents retention;
these additional limits are not acceptance waivers. Full unfiltered Rust corpora,
whole-package/lint gates and paired timing were not run for a rejected private
prototype. Future production retention still requires the existing complete
fidelity, changed-diagnostic replay and measured-benefit gates.

The private archive restores all 643 baseline Rust files byte-identically,
without added Rust files. Its 108 pinned bundled library inputs are intentionally
present for CLI builds; the first build's missing-library setup failure remains
in the history. The original existing member controls are rerun after restoration.
Canonical runtime source is unchanged. Replay patches and driver payloads supply
the next implementation with concrete red controls rather than an accepted cache
key or a narrowed performance target.
