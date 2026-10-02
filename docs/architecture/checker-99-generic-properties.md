# Semantic generic properties and receiver maps

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline b167f6ff: 454,757/478,855 correct assertions (94.97%).
The 99% target requires 474,067 correct assertions.

## Two historical safeguards against a missing type graph

The constrained-parameter reducer exposed a refusal for recursive<T extends
{next:T}> beside a separately constructed {next:T}. Its source constraint and
target both have the same semantic T member, but properties_related_to refused
any property type carrying TYPE_PARAMETER. That safeguard predated instantiated
member reads and native source-constraint relations. Native
isPropertySymbolTypeRelated (relater.go:4334) compares resolved member types with
the ordinary relation; it does not classify every generic member as unresolved.

The port now follows that comparison. Identity handles equal parameters, source
constraints handle related parameters, and unrelated parameters remain distinct.
Readonly, privacy and optionality checks still run before the comparison. Unknown
relations continue to report a gap. The unused GenericMember histogram slot is
retained with a historical comment so measurement indices remain stable.

A separate native control exposed Holder<number>.choose<T> returning "x" | T
instead of number | "x". instantiate_for_reference dropped the class's T mapping
because the method declared a same-named T. That was once needed by a textual
mentions-type-parameter scan. The current scan follows identities through
signatures and composite types; a method parameter and class parameter are
separate TypeIds. The native instantiateSymbol mapper includes every receiver
parameter. Keeping that map substitutes the outer T while retaining the inner T.

The name-based filtering and its carried shadow-name lists are removed from own,
tuple and inherited member paths. Heritage still composes the base map with each
enclosing reference and keeps its cycle guard. Polymorphic this keeps its explicit
receiver argument. Replacing the refusal with only an equal-ID exception was not
selected: native also relates distinct constrained parameters, and those paths
are already implemented by the shared relater.

## Native controls and measurement

The first focused test fails on the baseline and passes after property comparison
is enabled. Native probes pass --strict true explicitly and cover recursive
constraints, Box<T> beside {value:T}, constrained and unrelated generic boxes,
number versus string boxes, readonly members and optional members. Receiver
controls cover number/string instances, inherited generic methods, tuple returns
and a retained method-local constraint. Native declaration serialization still
uses T_1 for a shadowed outer parameter; this unit fixes semantic substitution,
not the remaining uninstantiated name-allocation problem.

The first full measurement gains 34 matches (18 WRONG-to-RIGHT, 16 GAP-to-RIGHT)
with zero RIGHT losses, no GAP-to-WRONG and no WRONG-to-GAP. Thirty-two gains are
in generic call/construct signature subtyping; two are in
conditionalTypeDoesntSpinForever. One answer in the latter changes while staying
wrong: its intersection still omits Record<"record", unknown>. Keeping the full
receiver map fixes the native boundary probes and leaves the corpus verdict
unchanged. This is useful fidelity outside the corpus, not a claimed coverage gain.

A collapse of unrelated parameters, substitution of a method's own shadowed
parameter, leaked outer parameter, failed readonly ordering, lost prior RIGHT
answer or isolated-source mismatch falsifies the corresponding claim. Legacy
metadata-free text fallback and protected-target relations retain their limits.
Review and simplification run sequentially in the primary thread under the user's
AGENTS tool map; no independent or cross-model review is claimed. Work is tracked
in tsr-6.37. Final frozen measurement and committed-checkout verification follow.

After removing obsolete shadow-name plumbing, the frozen verdict is byte-identical
to the previous candidate: 454,791 RIGHT, 2,936 GAP and 16,516 WRONG among 474,243
aligned assertions. This is 94.97% against all 478,855 expected assertions;
19,276 remain to 99%. Both new tests and both parameter-reduction controls pass.
An isolated mutation bypassing receiver instantiation fails the shadowed-member
test on numberChoice, which again leaks T instead of number. The first property
test's baseline failure separately establishes the need for semantic comparison.

Checker sources plus trace_case SHA-256:
13c04bee450d1199895160487e2005e3767e983ca67e5767a7cad89c509313ca

Release workspace tests pass 198 result blocks; clippy with warnings denied,
formatting, whitespace and 3,340 native references pass. The final cleanup updates
the instantiateSymbol anchor to its pinned location (checker.go:20753). Native
instantiateSignatureEx also clones retained parameters; this port's existing
signature representation and its serialization limits are unchanged by restoring
the complete receiver mapper. Remaining names/contextual/this and intersection
rendering are tracked in tsr-6.38.

## Committed checkpoint

At bd41d1b0, the isolated checkout's four focused tests pass and its checker
source hash matches. Its verdict is byte-identical to the frozen candidate:
454,791 RIGHT, 2,936 GAP and 16,516 WRONG among 474,243 aligned assertions.
The checker_types run reports 6,816/9,538 complete cases (71.46%), one more than
the baseline. Its snapshot is refreshed from the isolated checkout. Against all
478,855 expected assertions this is 94.97%; 19,276 remain to the 99% target.

Evidence:
- /tmp/tsr-99-generic-properties-verified.tsv
- /tmp/tsr-99-generic-properties-v2-transitions.txt
- /tmp/tsr-99-generic-properties-verified-tests.log
- /tmp/tsr-99-generic-properties-workspace.log
- /tmp/tsr-99-generic-properties-clippy-final.log
- /tmp/tsr-99-generic-properties-anchors-final.log
- /tmp/tsr-99-generic-properties-mutation.log
- /tmp/tsr-99-generic-properties-sourcehash.txt
- /tmp/tsr-99-generic-properties-coverage.log
- /tmp/compound-engineering-501/ce-code-review/generic-properties/review.json

The remaining names/contextual/this/intersection consumers move to tsr-6.38.
The next identified widening-context gap is tracked in tsr-6.39; its fixture's
72 wrong assertions are an investigation population, not promised conversions.

Depend completes with 521 non-gapping roots, 222 cycles, zero depth-cap hits and
3,797 walked gaps. C3 balances; C1 and the historical C4 checkpoint remain stale
(tsr-6.29). These controls are not used to validate current coverage.
