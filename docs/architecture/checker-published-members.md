# Published member identity before reuse

`tsr-1yb.32` specifies the writer/reader boundary for the next compiling
consumer, `tsr-1yb.33`. The Rust inventory is frozen at
`abbaa1d909cdaa3425d1acea293294ec800b5e83`; native is
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, with fixtures at
`4d4f005c8541e0255a9d8791205fdce326e462bc`. This is a contract, not an
implemented member store, cache rollout or slowdown repair. No new throughput
or coverage result is claimed; equivalent complete-work TSR/native median
wall <=0.50 remains unverified.

The [source inventory and replay receipt](checker-published-members.json)
binds operations to exact bytes and records fresh replays of the existing
[shared-property producer](checker-native-shared-property.md) (39 controls)
and [recursive-base producer](checker-native-recursive-bases.md) (30 controls).
Both isolated archives are restored exactly. These tests deliberately execute
native queries; they are not passive ordinary-workload observations. The
[rejected Rust classifier](checker-member-consumer-prototype.md) remains rejected:
native scalar histories are Single/Merged/Single, while it reconstructed
Merged/Merged/Merged. Its patch is not applied here.

## What already exists, and what is missing

Line anchors in this section refer to the frozen Rust source, not later main.
All mutable type/link/member state belongs to one exclusive `Checker`.

| Existing writer or reader | Retained identity/state | Missing connection for published members |
| --- | --- | --- |
| `symbol_access.rs:32,284,330,338` — `SymbolRef`, `CheckerSymbols`, `bound`, `view` | Bound Program stamp or private Checker stamp plus opaque index; validates before access | Use this existing domain boundary. Member APIs still return binder `SymbolId`, not this handle. |
| `symbol_access.rs:117,415,500,545` — private record, allocation, cloning, merge redirect | Flags, declarations, parent, tables, origin and unresolved declared type | No member target/mapper/read/write/containing-type links. `clone_symbol` resets check flags and records a merged redirect; it is **not** native `createSymbolWithType` or `instantiateSymbol`. |
| `checker.rs:128,239,257,759,1003,1131` — symbol types, reference handles, anonymous properties, bases, retained mapper vectors | Separate private tables keyed by bound symbol, concrete type or ordered pairs | Existing reference/overlay publication does not establish a completed structured image or native mapper object identity. Keep owners distinct. |
| `symbols.rs:41,223,293,4666` — read dispatch, accessor read, accessor write, variable/property lookup | Bound-symbol read cache and resolution stack; divergent accessor write uses concrete annotation | No private-symbol dispatch. Setter type parameters currently decline the divergent-write road; read-only completion cannot certify original-symbol reuse. |
| `members.rs:1279,1837,1943` — property read, intersection read, reference substitution | Original receiver passed to type substitution; constituent read types recomputed | Correct read types cannot reconstruct the symbol that was actually published earlier. |
| `members.rs:2019,2049,2139,2171,2267` — composite lookup, readonly/write projections, public symbol lookup | Temporary first binder symbol plus synthetic facts; deduplicates by binder ID | No persistent composite symbol/cache identity. Two private instantiations can share one binder declaration but have different mapper/write/history. |
| `members.rs:2796,2908,3286,3358` — late-bound names, enumeration, own/base walks | Names and declarations; an empty late-bound vector is installed before computed-name forcing | Empty, active and completed are not distinguished by this sentinel. Name lists and binder walks cannot certify field-complete symbol/signature/index publication. |
| `base_types.rs:53,212` — target base links and `get_base_types` | Merged declaration-symbol key; ordered partial list with `resolved=false`, then completed list | Public getter returns only `Vec<TypeId>`. The port has no corresponding structured-member reset writer to receive native's base-resolution reset. |
| `objects.rs:55`; `checker.rs:759` — anonymous property overlays | Type, optional/readonly/method/write parameter, declaration provenance and display fields | Overlay boolean controls semantic overlay use, not `MembersResolved`. Provenance is not instantiated or merged-clone identity. |
| `relater.rs:3452` — `properties_related_to_with_optionals` | Enumerates names, obtains declaration metadata separately, reads types through original receiver | A migrated read must consume the published symbol without replacing it with declaration/read equality or silently changing heritage receiver context. |

The minimum seam is therefore **writer publication → concrete member lookup →
private symbol read/write dispatch**. Adding only a new getter return struct
repeats `.31`'s failure. Adding only an enum beside `late_bound_member_names`
does not connect symbol identity, inherited mapping, signatures or indexes.
Current metadata remains useful, but it cannot stand in for the missing links.

## Required private representation

The following storage and operations are **proposed**, not built on main.
Use existing `SymbolRef` allocation/domain validation; do not mint binder IDs.
Bound symbol declarations and tables remain immutable. Link records for both
bound and private handles belong to the private Checker. Bound read adapters
must consult the existing `symbol_types` writer until migration establishes one
authority; two independently populated read caches would change query history.

| Fact | Authority and required behavior |
| --- | --- |
| Published symbol | Actual `SymbolRef` inserted into the concrete member table or native composite cache. Repeated lookup returns this handle. |
| Declaration provenance | Original declaration nodes and raw declaration parent, retained separately. Never a publication key or equality substitute. |
| Check flags | Preserve native constructor masks, especially `INSTANTIATED`. Transient ownership and instantiation are different flags. |
| Link target | Optional `SymbolRef` supplied by the writer. `getTargetSymbol(s)` returns that target **only when INSTANTIATED is set**; otherwise it returns `s`, including a merged clone with a nonempty link target. |
| Mapper | Checker-owned ordered/composed mapper handle with its own operation/context lifetime. Equal pair vectors or equal answers do not create native mapper identity. Follow the existing [mapper lifetime contract](rust-mapper-lifetimes.md). |
| Read/write links | Independently unresolved, active or published according to their native operation. Preserve a native completed error/recovery type distinctly from a port computation that cannot answer. Do not force either link while merely inspecting publication eligibility. |
| Container and receiver | `containingType` is the reduced apparent type used by the composite writer. Original receiver remains the `this` argument when preparing constituent/member references; neither replaces the other. |
| Parent/value declaration | Copy exact declaration sequence, value declaration and raw parent. For a merged clone, restore parent from its value declaration as native does. Do not assign the intersection as its symbol parent. |
| Composite result | Retain first supplier mapper/write, ordered unique published suppliers and native synthetic/deferred metadata. Do not sort suppliers or compare writes when native's merge comparison compares reads. |

Extend domain-checked operations to initialize value declarations and member
links before publishing a private symbol; existing `new_symbol`, `set_parent`
and `set_check_flags` alone cannot express the full record. Use distinct
constructors for native instantiation, merged clone and synthetic property.
Neither native member constructor records a declaration merge redirect, so do
not call the existing `clone_symbol` and then try to repair its redirect.
Keep declarations/parent/origin inspection short-lived: release the store borrow
before recursive type/member work, following ADR-0013.

Native `instantiateSymbol` (`checker.go:20753`) makes the decision **at member
construction time**, in this order:

1. Preserve the `MapsThisOnly`/`isThisless` early return.
2. Return the original symbol if its already-resolved read type cannot contain
   type variables; a setter also requires an already-resolved invariant write
   type. A generic declaration, scalar printed type, or eventual read answer
   does not establish that those links were resolved at this time.
3. If already instantiated, retain the original target and compose the old
   mapper with the new mapper in native order.
4. Allocate the private instantiated symbol, copy native declaration metadata
   and flag mask, set target/mapper/name-type links, then publish it in the
   reference's member table. Type forcing remains lazy (`16528`, `16536`).

Native composite creation (`21452`) compares actual supplier handles. Different
handles merge only when `getTargetSymbol` agrees and `compareProperties`
(`27672`) returns True: accessibility/origin or optionality, readonly and exact
nonmissing read identity matter. This comparison can force reads at its native
point. It does not retroactively replace previously published instantiations.
If that branch requests a merged clone, `createSymbolWithType` (`21703`) copies
only readonly check state, sets its own link target, and leaves INSTANTIATED
clear. The composite writer adds containing type, first mapper and first write.

## Publication, re-entry and reset

Keep three independent state owners: symbol read/write resolution, target base
resolution, and the concrete type's structured member image. A successful type
read, a completed base list, or `MembersResolved=true` alone cannot complete all
three. Native structured views are mutable backing objects; an immutable
snapshot is only a caller projection qualified at an eligible boundary.

For the prospective class/interface/reference builder, a private image record
must hold ordered member handles, named properties, call/construct signatures
and index information, its native flags, a publication revision, and **all active
builder frames for that concrete type**, including nested frames. Entry, setter,
base-reset and worker-exit events update this same record. Counting only one
`building` boolean loses the continuing outer worker in the natural reset case.

| Transition | Required result and reusable state |
| --- | --- |
| Unstarted → entered | Record owning operation/frame before recursive work. Missing storage is not completed emptiness. |
| Own-member setter inside active worker | Publish the actual handles and available signature/index fields for native re-entry. `MembersResolved` may be set before `UnresolvedMembers`; this is a readable active view, not a whole-builder reuse certificate. |
| Inherited work / active base re-entry | Preserve the currently valid partial prefix, shadowing and native lookup behavior. Return a result tagged with the actual active publication state; do not turn the prefix into a final cached table or suppress all heritage checks. |
| Final setter and worker exit | Update all fields/flags before declaring the relevant builder complete. A nested exit does not certify immutable reuse while another enclosing frame can replace that type's view. Native getter behavior still reads its published view. |
| `getBaseTypes` clears target MembersResolved (`19200`) | Advance that target image's revision and withdraw its completed-reuse certificate. Preserve the native current fields/escaped symbol identities and active frames. Clearing is not replacement of Program declarations or deletion of every reference/composite cache. |
| Completion after reset | Either a fresh builder or the existing outer builder can publish the new view. The continuing frame must receive the current revision at the reset event and complete that revision; requiring a fresh entry or rejecting every pre-reset frame is wrong. |
| `cloneTypeReference` (`25133`) | New reference identity starts without MembersResolved, despite retained target/arguments. Never inherit the source image's completed certificate. |
| Native error/recovery completion | Preserve native result/diagnostics, which may include completed empty or nonempty bases. Completed does not mean error-free. |
| Port failure or unsupported operation | Explicit unavailable/refusal outcome, separate from absent property and completed empty. Preserve fallback/reporting; no completed negative cache from a guard or unported base. |

A proposed owned query result can carry `Published(handle, revision, state)`,
`Absent(revision, state)`, `Unsupported(operation)` or `Failed(operation)`.
Active Published/Absent is an observation of that native boundary, never a
complete-image reuse token. A completed token validates the exact image
revision and fields it covers on its next access. Same-checker query-after-check
reuses actual current publication; foreign private handles fail validation.
No long-lived borrow spans recursion; callers reacquire a view after mutations.
The active-frame/revision protocol itself still needs compilation and mutation
qualification in `.33`; it is not established by this document.

Composite cache behavior remains separate (`21428`): native stores **positive**
results by containing type/name and augmentation mode, propagates a nonpartial
nonaugmented hit without overwriting an augmented entry, and does not cache a
nil result. Preserve those maps and already-published clone history. Do not
introduce an epoch-wide flush after unrelated read completion or a target reset:
such a flush would erase the expression-first scalar clone that native retains.
Signature admissions and anonymous/mapped publication have separate protocols;
the class/reference exit is not their completion boundary.

## Required observations at the compiling seam

These results are from the reused native producers, replayed for this contract.
Addresses and numeric TypeIds are compared only within one recorded Checker;
fresh runs need no matching addresses. The adapters deliberately publish the
composite **before** inspecting supplier read/write links. Moving that inspection
earlier would change the very history under test.

| Native control | Published state the Rust seam must reproduce |
| --- | --- |
| Scalar `Base<T>.value:number`, receiver-first (`cold`) | Single original symbol with INSTANTIATED clear; reads were already invariant when the supplier members were built. |
| Same scalar, expression-first | Merged private clone, INSTANTIATED clear; link target is the first supplier, getTargetSymbol is the clone itself, containing type is the apparent intersection. Later repeated queries retain that clone. |
| Same scalar, checked-first | Single original symbol. Optional/readonly/accessibility facts alone cannot distinguish these three orders. Readonly and private scalar variants reproduce the same sequence. |
| Shared inherited `self:this` | One reused instantiated supplier symbol; read/expression keeps the original `Both` receiver. |
| Distinct `Left.self`/`Right.self` | Synthesized symbol despite equal apparent supplier reads; the expression keeps the distinct-origin `Left & Right` result. Do not use equal reads as identity. |
| Common generic `value:T`, equal/unequal arguments | Equal arguments reuse the native single supplier; unequal string/number reads synthesize `never`. Same declaration target is insufficient. |
| Getter:number, setter:T, Left<string> then Right<number> | Merged clone keeps first mapper and string write, despite equal number reads. Reversing constituents keeps number write. Write equality is required for invariant original-symbol reuse, **not** for native's read-based merge comparison. |
| Mutual cycle with A bases `Root, B<T>, Later` | Active A prefix is `[Root]` with bases unresolved; completed A is `[Root, Later]`, B retains A. No excluded B heritage comparison. |
| Circular-default member reset | Foo MembersResolved changes true→false→true in all three orders. Member-first reaches completion by an already-entered outer worker; other orders enter a new worker. Preserve TS2310 and the retained NextType base. |
| Completed empty versus invalid base | Both can complete empty, while the invalid base reports TS2312. Active empty remains a third state. |

The receipt extracts these facts from all 69 fresh parsed-program observations
and cross-checks warm identity, clone target/flag/parent/mapper/write and reset
continuation. Historical mutation sensitivity stays bound to the original
`.29`/`.30` receipts (five/three detected mutations); no new mutation or Rust
runtime execution is claimed here. Synthetic native active/empty controls in
[the completion audit](checker-native-member-completion.md) remain explicitly
synthetic; the natural recursive/default controls do not relabel them.

## Bounded implementation and cost handoff

`.33` owns an isolated compiling writer/read seam and focused tests. It must
first fail the scalar history/clone/reset assertions against the current API,
then connect publication to the actual store. Cover the three fresh query orders
and repeats, shared/distinct this, unequal generic reads, both setter orders,
active prefix and both reset completion paths. Reject mutations that collapse
private symbols to bound roots, pre-force scalar links, reconstruct clones after
lookup, equate invariant reads with invariant setter writes, reorder suppliers,
clear every composite on a reset, or accept an active prefix as complete.
Keep the ordinary current/native/prototype full diagnostic payloads, including
already-WRONG cases; a unit identity pass cannot authorize new false errors.

The candidate builder family for `.4.2.1` is the **class/interface/reference
object-member worker**, using the private reference's target, ordered arguments
and appended original `this` argument. `.29`/`.30` establish semantic hazards
within this family; they do not certify an avoidable current Rust worker cost.
[Member cost attribution](checker-member-cost-performance.md) on `359a2789`
counts name walks, and later allocation attribution counts name bytes. Neither
counts current executions of a corresponding completed native-style Rust
builder, which is not implemented yet. There is therefore **no cost-qualified
production cache candidate in this contract**. `.4.2.1` must bind current
expensive construction counts and a benefit ceiling to this exact family before
retention; if none exists it publishes a no-change decision rather than turning
stable names into invented cache hits. Do not repeat rejected name-copy/index
candidates as a substitute for that measurement.

Concrete remaining seams are private member link construction/read dispatch,
base-state exposure and reset notification, active structured-image publication,
mapper handle/context qualification, and one composite consumer that returns
the actual stored handle. These are migration work for `.33` and existing
owners, not an assertion that the proposed API now works. `tsr-6.69.2` retains
heritage receiver/admission repair; `.27`/`.28` retain signature admission;
the mapper lifetime/context tasks retain mapper ownership. This task claims no
runtime files or new task overlapping their responsibilities. Module
augmentation, mapped/deferred, anonymous, reverse-mapped and broad composite
families retain their native-supported behavior and tracked ports; a bounded
prototype must expose Unsupported instead of treating those families as empty
or excluding their diagnostics from acceptance.

Full production retention still requires the existing complete corpus/changed
diagnostic triage and independently confirmed ordinary whole-CLI wall/CPU/RSS
gates. Private-state correctness is a prerequisite, not a measured speed gain
or an explanation of the reported PR #5 regression (`tsr-1yb.34`).
