# Natural member publication: private progress, retention refused

The `tsr-1yb.33.1` experiment at TSR
`d417a8c954527930ab9f9d801b26874920482060` / native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` now reproduces natural member
publication, target reset and continuation in the circular-default fixture.
Production retention is **refused**: the final unfiltered comparison loses
**63 previously RIGHT type assertions and eight passing diagnostic cases**.
There is no main runtime change, coverage gain or measured speed improvement.

The [receipt](checker-natural-member-publication.json) preserves source payloads,
terminal runs, native controls, all changed corpus rows and ordinary diagnostics.
The [replay patch](checker-natural-member-publication.patch) applies to the frozen
revision above. It is a private experiment to continue, not a qualified cache to
copy into production. The [earlier preparation evidence](checker-private-member-publication.md#prepared-reference-progress-at-55ed1a2a)
remains unchanged.

## The native order and actual stored fields

Native `resolveObjectTypeMembers` (`checker.go:19106`) obtains base types before
publishing structured fields. A worker active before that read is not equivalent
to MembersResolved. A default can re-enter the worker, publish its own member
table and return; `getBaseTypes` (`checker.go:19167`) then clears the target's
MembersResolved state. A fresh worker, or an already active outer worker, later
publishes inherited members. Rejecting every active worker prevents this sequence.

The private store now holds members, call signatures, construct signatures and
index infos, with separate publication, inheritance, frame and completion state.
Own declaration fields precede inherited fields. A target reset withdraws actual
publication without flushing escaped symbols or positive composite history.
Inherited member insertion updates the stored map as the worker continues;
signature and index vectors are published again when inheritance finishes.

Fresh extension of the existing recursive-base observer runs its 30 parsed
controls without adding any checker query. In all three circular-default orders,
the stored native table has `someProp` before reset, then `bar`, `baz`, `someProp`
after completion; call/construct/index counts are zero. Each order reports exactly
one TS2310. The Rust control now reproduces these projections and transitions.
The expression-first order continues its actual outer worker. No synthetic frame
or manually invoked reset stands in for this natural control.

A separate parsed fixture inherits one call, one construct and one string index
through `Base<T>`, `Mid<U>`, `Leaf`. Fresh native cold, checked and warm queries
confirm numeric parameter/return/property/index values; the private stored-field
control passes. This qualifies that fixture, not full native signature ownership,
lazy mapper identity or all structured member categories.

## Two broader failures repaired at their writers

The typed named-property reader initially lost the original intersection `this`.
Concrete reference preparation repairs it. Fresh native enumeration confirms
`left`, `right`, `self` on both the container and inherited self in all three
orders. The old expected-None test was preserving an expired gap; its assertion
is explicitly corrected. Its first replacement also needed sorting because the
Rust helper preserves insertion order; that failed run remains in the receipt.

The first wider candidate introduced false TS2430 on shared-self inheritance.
Two declaration entry points can mint different raw TypeIds for the same owner;
their inequality is not proof of a reference. The writer now uses the actual
reference registry, and applies an inherited receiver mapper only to an
instantiated image. The native `resolveClassOrInterfaceMembers` entry
(`checker.go:19091`) and reference entry (`checker.go:19095`) remain distinct.
A focused diagnostic control and a compiling mutation qualify this correction.

The stored declaration table also omitted late-bound semantic keys. Native
`getMembersOfSymbol` (`checker.go:16124`) includes both early and late members.
The private writer now calls the existing canonical `late_bound_members_of` and
stores its declaration symbol as the supplier. Fresh replay of the existing
shared-property producer passes 42 controls, including three added computed-key
orders. No getter invents a substitute symbol after a read. Broader merge and
synthesized metadata remain unqualified.

## Verification and the remaining rejection

All **196 private library tests pass**, with no ignored tests. Six compiling
mutations fail previously passing controls: treating an active frame as published,
leaving publication set after reset, losing the explicit whole `this`, omitting
call mapping, misclassifying a raw declaration ID, and dropping late suppliers.
The exact sources restore after every mutation and the full library passes again.

Two ordinary CLI batches execute **336 terminal children**, native/baseline/
candidate in default and single modes. In the final batch, **46 of 56 complete
baseline/candidate output pairs are identical**. All ten changed pairs improve to
the native diagnostic text; there are no previous native agreements lost in this
bounded matrix. Native diagnostic agreements rise from 34 to 44 pairs. Remaining
existing gaps are retained with their full output, rather than hidden by code sets.

Fresh release binaries also run the unfiltered type and diagnostic producers.
The fixture pin is `4d4f005c8541e0255a9d8791205fdce326e462bc`.

| Comparison at frozen `d417a8c9` | Initial candidate | Final private candidate |
| --- | ---: | ---: |
| Aligned type rows | 477,970 | 477,970 |
| Previously RIGHT type losses | 611 | **63** |
| Type rows changed, including already-WRONG | 723 | 108 |
| Diagnostic cases | Not run | 10,570 |
| Previously passing diagnostic losses | Not run | **8** |

The final type transitions are 55 RIGHT-to-WRONG, eight RIGHT-to-GAP,
30 WRONG-to-RIGHT, eight GAP-to-RIGHT and seven changed already-WRONG rows.
The diagnostic comparison changes 14 cases: eight losses, five improvements and
one changed already-WRONG case. Type losses remain in collection inference,
contextual `this`, JSX, overload/contextual inference, overrides and conditional
types. Diagnostic losses also include arrays, weak intersections and super access.
The receipt contains the exact cases and complete before/after rows. These are
remaining regressions to repair, not exceptions to the no-RIGHT-loss gate.

The initial type postprocessor split printed multiline TypeScript literals at
newlines and failed after the compiler finished. Its raw output is preserved;
it supplies no recorded terminal code or elapsed-time qualification. Fresh
corrected runs identify keyed verdict starts, validate population against the
producer's TOTAL and record terminal return codes. The two fresh baseline type
streams and binaries are byte-identical. Native Go quoting and Rust Parameter API
setup failures are also preserved and never counted as semantic sensitivity.

## Delivery and continuation

Canonical runtime remains unchanged. The seven-source replay, new receipt and
status notes are the delivery. All 650 baseline Rust files are unchanged on main;
the private candidate has 651. Native restoration is limited to the checker and
two qualified helper files, with no new whole-archive restoration claim. Full row
streams remain in the private archive with exact byte hashes in the receipt.

`tsr-1yb.33.1` remains in progress. Its next work is the concrete remaining
63 type/eight diagnostic losses, preserving receiver `tsr-6.69.2` and signature
`.27`/`.28` ownership. Native MapsThisOnly/isThisless, object flags, full mapper
and synthesized metadata, unions, augmentation and anonymous constructor/static
fields also remain unqualified. The builder `tsr-1yb.4.2.1` still requires current
expensive-work attribution and measured ordinary benefit before production reuse.

The private run times overlap builds and other fidelity runs. They lack CPU/RSS
and equivalent performed-work qualification, so they establish no speed ratio.
The reported PR #5 slowdown remains unresolved; the separate investigation is
`tsr-1yb.34`. Equivalent complete-work TSR/native median wall <=0.50 is unmet.
