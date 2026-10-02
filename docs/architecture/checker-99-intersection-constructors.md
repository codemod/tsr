# Constructor intersections and shared new-expression resolution

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 82ca5f80: 453,632/478,855 correct assertions (94.73%).
The active 99% target requires 474,067 correct assertions.

## Constraint and native behavior

The previous new-expression intersection arm intersected every constituent's
instance type. Native resolveIntersectionTypeMembers (checker.go:21302) instead
preserves ordinary constructors as overloads. Only mixin constructor signatures
are discarded, contributing their return types to each remaining constructor.
Thus `(new(string) => A) & (new(number) => B)` constructs A or B according to the
argument, while `(new(...any[]) => A) & (new(number) => B)` constructs A & B.
Nonconstructor properties do not prevent construction.

isMixinConstructorType (checker.go:17026) requires exactly one nongeneric
constructor with exactly one rest parameter of type any or an array of any.
getElementTypeOfArrayType recognizes both global Array and ReadonlyArray targets
(checker.go:23513). Tuple rests, unknown[] rests and generic constructors are not
mixins. findMixins retains the first constructor when every constructor is a
mixin; includeMixinType preserves the original constituent order in the result.
An abstract mixin is discarded before resolveNewExpression's abstract-signature
check, while an abstract ordinary constructor rejects construction.

Constructor candidate ordering follows reorderCandidates (checker.go:8957),
including declaration-parent groups and syntactically literal specialized
signatures. Default class constructors have no declaration upstream; this port
uses a class node for their source identity, so ordering treats that node as
absent. A native probe disproved the old regression test's claim that two
zero-argument ordinary class constructors produce A & B: the reordered candidate
set selects B. The test now checks B and its member, while focused mixin tests
retain the actual A & B behavior.

The shared signature query now uses apparent types, including type-variable
constraints. All supported new-expression targets use that query, overload
selection and generic inference. Resolved signatures are cached for contextual
argument checking. Class default constructors already expose inherited and
instantiated signatures; routing new expressions through them makes those
signatures available to inference. Existing fallback paths remain for signature
sets that the partial port cannot yet resolve.

## Alternatives and accepted limits

Retaining the old intersect-all-returns arm was ruled out by the ordinary
overload probe: the native answer depends on which constructor is selected.
Restricting the shared resolver to composite types gained only 14 matches in the
full corpus. Routing all constructor targets through the same native mechanism
gained 207 with zero RIGHT losses before the readonly-array correction.

The first candidate had six GAP-to-WRONG and nine changed wrong answers; the
second had nine GAP-to-WRONG and 21 changed wrong answers. These remain visible,
including generic callback contexts, union order, class references and namespace
serialization. Constructor accessibility, complete generic CheckMode state,
error-call recovery, expression-valued base members and generic mixin class
static intersections remain incomplete. Legacy recovery is retained until these
shared paths can replace it with measured parity; no full constructor-port claim
is made.

## Controls and review

Seven focused tests cover overload selection, mixin return order, all-mixin
intersections, inferred and explicit generic arguments, abstract mixins versus
abstract ordinary constructors, constrained constructor parameters, readonly and
non-mixin rest boundaries, inherited class inference, constructor overloads and
callback contexts and candidate ordering. Four existing test files replace obsolete
refusal expectations or the incorrect intersect-all-returns claim. Expected outcomes were checked against pinned native
compiler declarations. The abstract ordinary and abstract generic constructions
produce native TS2511 and any results. The expected corpus and denominator are
unchanged.

The ordering experiment exposed 19 RIGHT losses: six came from missing __new
member identities in the Rust binder, and thirteen from iterable inference.
Constructor ordering now compares merged owning-type identities for unallocated
signature members, and distinct node identities for standalone constructor types.
The inference investigation found that inferFromProperties used a property-name
list that omitted late-bound Symbol.iterator members. The semantic property
query supplies those members so ordinary structural inference can follow them.

The first full run with semantic properties was stopped after several minutes
without a verdict: repeated structural pairs made that traversal impractical.
The member walk now uses invokeOnce's pair-status memo (inference.go:335), scoped
to each inference entry and restored around nested inference. Active pairs record
circularity; completed pairs carry their observed priority. The existing depth
limit remains; native expanding recursion identities are not yet implemented.
Tuple element arguments now participate in couldContainTypeVariables, allowing
Iterable<readonly [K,V]> to reach structural inference. These changes recover the
WeakSet and readonly WeakMap witnesses without changing candidate order.

Native boundary probing found the missing ReadonlyArray recognition before final
verification. The shared element helper now follows native isArrayType for both
signature positions and mixin classification. The previous comment claiming
mutable-only recognition was incorrect.

Sequential primary-thread review checks native algorithm order, recursion-cache
cleanup, type argument inference, abstract flags, tests and project conventions.
No independent or cross-model review is claimed. An isolated scratch snapshot
was byte-checked against the source before changing mixin return construction to
never. The focused test failed on mixed: never instead of M & B. The main source
was unchanged. A previous classification mutation survived; it is not used as a
successful control. Sharing the target directory let a subsequent main test reuse
the scratch build, so the real checker was forced to rebuild before final checks.
The committed-source measurement uses an isolated worktree.

## Verification

The fifth full candidate records 453,951 RIGHT, 3,055 GAP and 17,237 WRONG
among 474,243 aligned assertions: +319 versus 82ca5f80, with 251 WRONG-to-RIGHT,
68 GAP-to-RIGHT and zero RIGHT losses. Nineteen GAP-to-WRONG, three WRONG-to-GAP
and 52 changed wrong answers remain. This is 453,951/478,855 assertions (94.80%);
20,116 additional matches are needed for the 99% target.

Final committed-source coverage and validation are pending. The previously
verified checkpoint remains 82ca5f80 until the evidence commit records the new
counts. Intermediate evidence is under /tmp/tsr-99-intersection-constructors-*.

Release workspace tests pass (193 result blocks), and all seven focused tests
pass after rebuilding the real checker following mutation. Clippy with warnings
denied, formatting and whitespace checks pass. Upstream anchors pass. Final
committed-source corpus and snapshot verification follow in the evidence commit.
