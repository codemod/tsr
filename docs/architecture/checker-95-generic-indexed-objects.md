# Generic indexed objects and conditional identities

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 6786ef93: 451,768/478,855 correct assertions (94.34%).

## Native rules and representation

getGenericObjectFlags/isGenericMappedType and shouldDeferIndexedAccessType
(checker.go:24880,27370) defer higher-order indexed operations until their
operands can be instantiated. The port previously recognized type parameters
and generic indexes but eagerly read mapped, conditional and composite objects.
For example, Mapped<T>["a"] became its current member template, and an index
into a conditional alias could become an error instead of retaining its operands.

The object query now includes instantiable non-primitive types, generic mapped
types, generic variadic tuples, and generic union/intersection constituents.
Remapped names are checked after substituting the key constraint for the mapped
parameter. The index query includes conditional types and generic string-like
types, while concrete pattern literals stay eligible for ordinary lookup.
Fixed tuple indexes retain their existing eager path. Deferred composite objects
use the existing postfix-parenthesization helper. The native
TypeFlagsInstantiableNonPrimitive mask is shared across these consumers.

String-index-only objects normalize compatible keys before deferral, following
getIndexedAccessTypeOrUndefined and isStringIndexSignatureOnlyTypeWorker
(checker.go:26935,27363). Every union/intersection constituent must qualify.
Nullable keys are excluded. Number and string assignability checks are separate,
as in isTypeAssignableToKind; source-variable constraints are obtained explicitly
because the general assignability relation does not yet traverse all of them.

A deferred conditional alias now carries CONDITIONAL instead of OBJECT. Its
base constraint uses the existing branch capture, including distributive
re-instantiation at the check operand's base constraint. That operand can be a
keyof or indexed type, not only a type parameter. Keyof and homomorphic identity
maps preserve instantiable operands. Partial<Conditional<T>> therefore retains
the metadata needed to keep its callback context generic.

getAwaitedTypeNoAlias preserves an unresolved conditional, including an existing
Awaited<T> alias. The separate operation that introduces an Awaited wrapper is
still incomplete. Keeping conditional flags while retaining the old blanket
await rejection would discard inferred async return types; reverting the flags
would hide the underlying type identity again.

## Experiments, controls and review

The first full pair gained 22 assertions and lost 17 in awaitedType. Preserving
the conditional operand on the no-alias await path restored all 17. The second
pair gained 30 but lost three: two constrained-key assignments and one argument
to Partial<Conditional<T>>. General base-constraint lookup and mapped operand
deferral restored all three. The final pair has zero RIGHT losses.

Pinned native declarations and Rust controls cover mapped and conditional
indexes, generic union/intersection indexes, concrete instantiation, mapped
member constraints, fixed and variadic tuple positions, template patterns,
string-index-only normalization, ordinary Box<T> members, and generic async
conditional returns. Corpus expectations and the oracle were not changed.

Simplification and review ran sequentially in the main thread under the user's
tool mapping. The simplification pass consolidated repeated native flag masks
and removed the stale claim that mapped members cannot resolve. Correctness and
adversarial passes inspected normalization ordering, recursive constraint guards,
mapper preservation, eager tuple exceptions and async substitution. No independent
agent review is claimed. The review has no remaining actionable findings for this
increment; its receipt is /tmp/compound-engineering-501/ce-code-review/generic-indexed-fr7htq42/review.json.

## Measured checkpoint and remaining work

CODE_CHECKPOINT: 451,812/478,855 correct assertions (94.35%).
6,674/9,538 complete cases (69.97%).
Aligned verdicts: 474,243 total; 451,812 RIGHT; 3,485 GAP; 18,946 WRONG.
Relative to 6786ef93: 35 WRONG→RIGHT, 9 GAP→RIGHT, zero RIGHT losses,
1 GAP→WRONG and 20 changed wrong answers. The 95% threshold is 454,913;
3,101 additional correct assertions remain.

The new wrong row is generatorYieldContextualType union ordering. Changed wrong
answers include conditional constraint rendering, recursive conditionals, generic
return constraints, reverse-mapped tuple literal retention and function signatures.
All remain in the comparison rather than being counted as matches. Generic
reducible object unions, full Awaited wrapper handling, generic mapped-value alias
display, lazy recursive rendering and mapped callback inference remain in tsr-6.30.

Evidence: /tmp/tsr-95-generic-indexed-final-verdict.{tsv,log},
/tmp/tsr-95-generic-indexed-final-transitions.txt,
/tmp/tsr-95-generic-indexed-{controls,workspace-tests,clippy,anchors,coverage,depend}.log,
and /tmp/tsr-95-oracle-generic-indexed.ts with its pinned native declarations.
Checker sources plus trace_case.rs SHA256:
dda896ce7605f5610e963c19f19e1c8aa80395e52567151caec1333648df2d71.

Release workspace tests, clippy across all targets with warnings denied, format
and whitespace checks pass. All 3,364 upstream anchors resolve; the checker
snapshot is refreshed. A fresh depend run walks 4,491 gap lines with 578 C1 roots
no longer gapping, 287 cycles, zero depth-cap hits and balanced C3 arithmetic.
C4 still quotes a historical population, so this instrument cannot establish
reachability. Repair remains tsr-6.29; the full aligned comparison is authoritative.
