# Supplying-base mapper experiment on ca47f70a

The isolated supplier repair is rejected. It corrects the selected nongeneric and
generic member queries, but changes a native-correct inherited intersection's
printed type and introduces a false cyclic-heritage diagnostic. Production code
is unchanged. This is a correctness prerequisite for `tsr-1yb`, not a speed gain;
the verified comparable TSR/tsgo median wall target remains <=0.50 and is unmet.

[The receipt](checker-supplier-mapper.json) retains full inputs, options, outputs,
source/binary bindings, executed drivers, test/probe text and restoration proof.
The [rejected patch](checker-supplier-mapper-rejected.patch) and
[test-only probe](checker-supplier-mapper-probe.patch) both apply directly to
`ca47f70afea348df903abe6f0a95674018bfab76`; the probe includes the candidate.
Do not layer these patches or apply the rejected repair to production.

## Fresh source and the supplying mapper

This run follows the [C3 traces](checker-inherited-this.md), using a clean private
checkout of ca47f70a rather than relabeling the frozen 36c9bb5b observations.
Native is clean `5b1047d10d32e7d5b446be4de56b126ff42f82bb`, with TypeScript fixtures
`4d4f005c8541e0255a9d8791205fdce326e462bc`. Separate ordinary release builds have
source-diff guards and clean measurement/debug environment overrides. The copied
Cargo target is private; its old binaries are rebuilt and independently frozen
before use. This run is not a timing comparison.

Two tests added to the existing `members.rs:property_name_tests` first fail:
`A.self: this` read through `B extends A` returns A's formal this rather than B,
and `C<string>` through two generic bases returns an instantiated base rather
than C. The ordinary successful symbol lookup finds A's property, then
`instantiate_for_reference_with_this` chooses the receiving owner's mapper.
Its own-this identity cannot substitute the supplying A-this identity.

The candidate uses current native-aligned `get_base_types` links, applies the
outer receiver mapper to each base, and reads the supplying property's type with
the original this argument. A query-local owner path guards recursion and is
popped on return; it publishes no member image or absent-member cache. Native
`resolveObjectTypeMembers` (`checker.go:19138`) supplies the relevant base/mapper
boundary. The rejected C3 hidden-tail representation and signature-guard changes
are not applied.

Both new tests pass, including repeated receivers, explicit formal this and
string/number outer maps. In the public CLI matrix, generic self, cross-file class
self and own-member shadowing positives now match native in both modes. Real
negative assignments and the merged function/namespace TS2411 remain errors.
The original complex collection positive, array heritage and contravariant sink
controls agree with native. The simple interface-this positive still falsely
reports TS2430, so the broader heritage-context repair is unfinished.

## Shared intersection properties must retain their identity

The original composite test deliberately refused inherited Base-this because the
member was wrong. The candidate makes its names readable. Strengthening that test
to check the member before accepting the names exposes a new boundary:

| View | TypeId | Data |
| --- | --- | --- |
| Receiver | 27 | `Both`, intersection of 25 and 26, alias symbol 6 |
| Returned self | 30 | `Left & Right`, intersection of 25 and 26, no alias symbol |

A test-only trace records the actual values. The constituents are retained;
the shared property's alias view is lost. An ordinary CLI replay confirms the
externally visible regression:

```typescript
interface Base { self: this }
interface Left extends Base { left: number }
interface Right extends Base { right: string }
type Both = Left & Right;
declare const b: Both;
const bad: number = b.self;
```

Native and baseline report that `Both` is not assignable to number. The candidate
reports `Left & Right`. A countercontrol declaring `self: this` separately on Left
and Right makes all three print `Left & Right`. Both modes reproduce this
shared-versus-distinct property distinction; a blanket alias-preservation change
would break the countercontrol.

Rust `property_type_via_shape` collects each mapped constituent hit and calls
`get_intersection_type(&hits, None)`, flattening the repeated aliased intersection.
Native `createUnionOrIntersectionProperty` (`checker.go:21452`) retains `singleProp`
when a distinct property set is unnecessary. Different instantiated symbols can
also merge only after target-symbol and `compareProperties(..., compareTypesEqual)`
checks. This native branch is source evidence, not an executed private Go trace.
Its full mapper/read/write/metadata equivalence remains a characterization task;
a shared declaration SymbolId alone cannot establish it. The assumption that the
supplier mapper could be retained independently of composite property publication
is disproven by the ordinary CLI result.

## Cycle and remaining qualification

The candidate also adds TS2430 to:

```typescript
interface A<T> extends B<T> { a: T }
interface B<T> extends A<T> { b: T }
declare const x: A<string>;
x.a;
```

Native and baseline emit two TS2310 occurrences; the candidate emits those plus
TS2430 for A incorrectly extending B. Baseline's missing generic arguments in the
TS2310 display remain `tsr-6.70`; that existing gap does not permit the new false
error. The new cycle's runtime causal chain has not been traced. The next step
must inspect base publication/forcing and heritage diagnostic admission, rather
than suppressing TS2430 or treating partial base lists as completed absence.

All 47 filtered erased-signature rows are byte-identical to the fresh baseline.
There are 158 distinct completed compiler children across the baseline,
supplier/composite/heritage controls and that filtered pair; none times out.
Ordered loaded identities agree within every CLI matrix. Full native message
chains are retained, including the still-missing heritage chain `tsr-6.71`.

The selected member suite passes 21 of 22 tests; the strengthened inherited
intersection test fails. Two test versions and a test-only probe are archived,
with one unchanged semantic candidate. Full unfiltered corpora, whole checker
package/ownership tests and strict lint gates were not run after this early native
rejection. These results cannot establish no full-corpus RIGHT losses or a speed
benefit.

The private source is restored clean at ca47f70a, all four archived patches pass
apply-check there, and the three ordinary release CLI/conformance outputs match
the qualified baseline hashes. Frozen rejected binaries remain evidence files.
No owned process remains live. Existing `tsr-6.69.2`, receiver/key publication,
full diagnostic replay and concrete-member implementation claims remain open;
this observation does not unblock cache reuse or scheduling.
