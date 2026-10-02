# Dependent bindings through tuple positions and instantiated aliases

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline e8c208fc: 453,951/478,855 correct assertions (94.80%).
The active 99% goal requires 474,067 correct assertions.

## Constraint and native behavior

The existing dependent-binding path already narrowed a binding pattern as a
pseudo-reference, then projected a sibling element from the narrowed parent.
It could not identify array discriminants: get_accessed_property_name accepted
object binding names but declined array patterns. Native
getDestructuringPropertyName (flow.go:1792) uses the element's index, counting
omitted elements. The port now exposes that index to the existing flow walk.
The isolated array change converted 20 WRONG assertions to RIGHT with no other
transitions. The ordinary const-like, initializer and assignment guards remain.

A separate failure affected generic aliases such as
`Result<T> = { ok: false; data: undefined } | { ok: true; data: T }`.
The alias-body evaluator refused every structural type literal because older
member reads leaked the written T instead of its instantiation. That historical
refusal was measured at 19 GAP-to-WRONG assertions in controlFlowAliasedDiscriminants.
The current type-literal builder captures statically named property types under
alias binding frames, and member lookup reads those captures. The old exclusion
therefore blocked supported property-only unions as well as unsupported members.

The evaluator now admits literals containing only statically named property
signatures. Dependent binding resolution expands a named alias before mapping
base constraints, and expands aliases reached through those constraints too.
Native getNarrowedTypeOfSymbol (checker.go:13723) already receives semantic alias
bodies; these expansions bridge the port's separate named-reference representation.
The shared binding alias helper retains cycle detection and cached evaluation.
The printed source alias identity is unchanged.

## Alternatives, rejected candidates and limits

Keeping the blanket literal refusal would retain the 90 wrong assertions in
controlFlowAliasedDiscriminants even though property capture now supports them.
Removing the entire refusal gained 187 matches with zero RIGHT losses, but a
native boundary probe exposed a method-bearing alias leaking T after its alias
frame ended. Native returned readonly [number, number]; the candidate returned
readonly [T, T]. Methods, accessors, index signatures and call/construct members
therefore retain the existing refusal. This narrower candidate gained 183 matches
with zero RIGHT losses and three GAP-to-WRONG transitions.

Computed properties need the same exclusion: a unique-symbol member probe
returned T | T[] where native returned number. Numeric literal property names
are supported and a native probe confirms their instantiated number result.
The admission test follows the existing structural spine through unions,
intersections and parentheses. It is a port capability boundary, not a native
restriction. Completing captured non-property/computed members is the reopening
condition; suppressing the wrong printed T without instantiating it is not.

The remaining JSX callback-context differences are measured explicitly rather
than hidden: admitting the property union makes a callback available, but its
context still yields any instead of string. The full goal remains unfinished.
Other binding-flow limitations, including full root-initializer circularity
handling and every native synthetic-reference path, are outside this unit.

## Verification and falsifiers

Four focused tests use native declaration results and trace assertions. They
cover rest parameters, local tuples, holes, unequal tuple arities, exhausted
switches, mutable/defaulted bindings, assignments, separate alias arguments,
alias chains, recursive aliases, nested bindings and generic constraints.
Method-bearing and computed-member aliases retain explicit refusal controls;
these are documented gaps, not native parity claims.

The first two tests failed before implementation: the tuple returned string |
number instead of number, and the generic object alias returned any instead of
number. Native probes ran with explicit strict mode. A scratch mutation shifts
the array discriminant index by one, with a separate build directory to prevent
cross-checkout artifact reuse. Source hashes and a committed isolated checkout
provide the final attribution check. Full-corpus RIGHT losses, a leaked type
parameter in a newly supported member, or mismatched source hashes would falsify
the implementation's claims.

Simplification and correctness/adversarial review run sequentially in the primary
thread under the user's tool map. No independent or cross-model review is claimed.
The reuse pass keeps the existing alias helper and projection machinery; no new
flow engine or type representation is introduced. The quality pass preserves
capability guards and removes stale local documentation. The efficiency pass
checks cached alias evaluation and bounded cycle detection.

## Checkpoint

The final candidate adds 183 matches: 163 WRONG-to-RIGHT, 20 GAP-to-RIGHT,
zero RIGHT losses and three GAP-to-WRONG transitions. It has 454,134 RIGHT,
3,032 GAP and 17,077 WRONG among 474,243 aligned assertions. Removing computed
members leaves these corpus verdicts unchanged while fixing the probe's leak.
The frozen release workspace run passes 194 result blocks. The isolated mutation
fails the tuple test with string | number instead of number, as intended.
A test-only raw-string lint was corrected; it does not change the checker hash.

Checker sources plus trace_case SHA-256:
3ab8543205e377b057687de3abc9221f7cab5f45bbb5aeafd5ab0aad4838a264.
Final committed-source coverage and snapshot results follow isolated verification.
