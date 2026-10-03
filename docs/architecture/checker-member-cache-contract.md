# Structured-member reuse contract

This audit pins native source to
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` and TSR to `f16fd811`.
It defines the prerequisite for member reuse in `tsr-1yb.4.1.1`; it adds no
production cache. The temporary-vector experiment in
[whole-project performance](whole-project-performance.md#structured-property-name-enumeration-experiment)
failed current-source timing. Repeated owner visits justify investigating
reuse, but do not prove that the repeated results are interchangeable.

## Native identity and publication

In `vendor/typescript-go/internal/checker/checker.go`,
`getPropertiesOfType` reduces the apparent type, then dispatches objects and
union/intersection types separately. `getPropertiesOfObjectType` returns
`resolveStructuredTypeMembers(t).properties`. `ObjectFlagsMembersResolved`
guards the table attached to that concrete `Type`, not its declaration symbol.

`resolveTypeReferenceMembers` uses the reference target and ordered type
arguments. When the list omits the target's polymorphic `this` parameter, it
appends the reference itself. `resolveObjectTypeMembers` instantiates symbols,
signatures and index information using that mapper, then layers inherited
members with the appropriate `this` argument. Equal property names alone
neither identify equal member types nor permit mapper reuse.

`setStructuredTypeMembers` sets `MembersResolved` and installs the current
member/property/signature/index views. For inheritance,
`resolveObjectTypeMembers` publishes the own-member view **before** visiting
bases, marks `ObjectFlagsUnresolvedMembers`, and replaces the views after base
resolution. Re-entry can therefore observe a provisional table. A set
`MembersResolved` bit does not universally mean final inheritance completion.
`getLiteralTypeFromProperties` includes the unresolved-members bit in its
`PropertiesTypesKey`, along with type identity, inclusion flags and origin
policy. Its provisional key union cannot masquerade as the completed union.

`cloneTypeReference` clears `MembersResolved`. A cloned reference does not
inherit a resolved-table state merely because it retains target and arguments.
Mapped, anonymous, reverse-mapped and composite builders have their own
publication rules; inheritance's flag sequence cannot be imposed on all of them.

## Current TSR storage

All entries below belong to one `Checker`. A `TypeId` is meaningful only with
that checker's `TypeStore`; a `SymbolId` belongs to the immutable binder domain.

| Storage or query | Identity and retained value | Reuse boundary |
|---|---|---|
| `type_reference_targets` | Concrete `TypeId` → owner and ordered argument identities | Describes a reference; does not retain resolved members |
| `qualified_generic_reference_types` | Written qualification, target symbol, ordered argument IDs → type | Preserves distinct argument identities; not a member cache |
| `instantiated_objects` | Source type and ordered substitution pairs → object image | Retains an instantiated image; completion of additional side metadata must be audited separately |
| `anonymous_properties` | Type ID → semantic property images and a boolean | The boolean controls semantic/synthetic overlay use; it is not `MembersResolved` |
| `late_bound_member_names` | Owner symbol and static/instance side → computed names/declarations | An empty entry is also installed during resolution; empty does not distinguish active from completed |
| `get_property_names_of_type` | Concrete type → freshly allocated names or unsupported result | No general resolved-table memo; mapped keys are argument-dependent, statics and instance members have separate paths |
| `declared_members_are_complete` | Conservative graph traversal for absent-property diagnostics | Not a publication state; computed names and cycles may decline despite successful enumeration |
| `declared_property_table` | Fresh traversal of names and optionality | A different eligibility policy, including index signatures; not interchangeable with the preceding predicate |

The anonymous-property boolean is set from synthetic spread/renamed-property
conditions in `objects.rs::check_object_literal`. Regularization and other
object-image builders may install an authoritative overlay. A cache must
respect those writers rather than relabel that boolean as general completion.

`members.rs::instantiate_for_reference_with_this` substitutes explicit type
arguments and a concrete receiver `this` argument. Reading a declaration's raw
`this` property type is a different operation from checking `left.self`.
Caching the raw declaration answer as the concrete access answer is invalid.

## Required implementation boundary

A resolved-member image must retain its concrete checker-local type identity,
static/instance domain, ordered mapper and receiver `this` identity. Preserve
alias/origin presentation separately from structural member sharing. Printed
type names and owner symbols are insufficient keys. Sharing equivalent images
is permitted only when those distinctions are proven preserved.

The builder must distinguish absent storage, an active/provisional view, a
completed view and an unsupported or failed port computation. Re-entry reads
the builder's native-supported provisional answer; it must not permanently
memoize that answer as a completed table. Do not turn an unfollowable base into
a successful empty table. Stable empty results are valid only after the owning
builder has established that they are final. Metadata installed after a query
must cause the appropriate builder to complete or re-enter before reuse.

Caller projections are separate: names, property symbols/types, signatures,
index information and literal-key unions have different inclusion and origin
rules. A cached name list cannot replace the concrete member image. Returning
`Vec<String>` clones from a name cache may still retain most copy cost; measure
expensive forcing, traversal and result copying separately before claiming a win.

The immutable Program/binder must contain merged declarations and augmentation
inputs before checker construction. No cache crosses private checker domains
or outlives its Program/options. Query and declaration-emit access must use the
same worker ownership contract described in [threading](threading.md).

## Executable controls and limits

`members.rs::property_name_tests` exercises distinct mapped arguments, equal
names with different instantiated value types, concrete inherited `this`
access, static/instance separation, synthetic spread overlays and derived
shadowing. Existing controls cover diamonds, computed keys, repeat ordering,
cycles and unfollowable bases. The active-entry control explicitly installs
the late-bound resolver's empty marker, reads the provisional names, removes
the marker and verifies that completed enumeration gains the computed name.

Four bounded CLI controls are recorded in
`/tmp/tsr-member-contract-controls/results.json`, with source and binary hashes.
They use the saved `4bf5fe96` TSR reference; `f16fd811` changed only tests and
documentation, so its production checker is identical.
Native and TSR reject the deliberately wrong generic value, static property
and concrete receiver assignments with identical complete diagnostics after
formatter-summary normalization. Correct assignments in those files produce
no additional errors. This does not establish whole-project diagnostic parity.

The mapped-key control resolves distinct names correctly, but native reports
TS2741 using aliases `One`/`Two`, while TSR reports TS2322 using expanded
`Keys<...>` names. `tsr-6.50` tracks that diagnostic/presentation gap.
Cross-file imported generic members and module augmentation remain explicitly
tracked by `tsr-6.48` and `tsr-6.49`; worker ownership controls currently mark
their positive native cases unsupported. These are not evidence that incomplete
member tables may be cached as native-complete tables.

The implementation handoff `tsr-1yb.4.2` must first count concrete-type queries, repeated
completed versus active queries, expensive member forcing and copied result
bytes on current source. Previous counts by owner symbol cannot estimate actual
cache hits. Only then implement the measured native builder slice, preserving
the controls, complete diagnostics and previously RIGHT corpus assertions.
Keep a change only if fresh-process whole-project timing exceeds the unchanged
measurement threshold. The overall native wall ratio target remains 0.50 and
is unverified.

The subsequent [concrete member-query observation](checker-member-query-profile.md)
separates repeat name answers, structured walks and returned payload bytes on
`074f60a7`. Stable names are an opportunity bound, not proof of completed
member-image equivalence or saved CPU. Mapper/instantiation identity and relation
publication now have separate audits, `tsr-1yb.4.1.2` and `tsr-1yb.4.1.3`.
