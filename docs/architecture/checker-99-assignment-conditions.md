# Assignment and comma conditions in flow narrowing

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline: 5ca50fea, 457,525/478,855 matching checker assertions.
Issue: bd tsr-8.6.

## From the real-project failure to the missing protocol

The strict CLI reported TS18047 for `while ((match = next()) !== null)` when
next returns string | null. Pinned native reports nothing. The binder already
records the assignment and condition flow. The checker reached the equality
condition but compared the ParenthesizedExpression/BinaryExpression directly
to the later identifier reference. Its identifier matcher accepted neither
wrapper, so the assigned union survived the null guard.

Native getReferenceCandidate (internal/checker/flow.go:1861) unwraps parentheses,
the four assignment forms `=`, `||=`, `&&=` and `??=`, and a comma's right
operand. Equality normalizes both operands; typeof normalizes its argument;
instanceof and in normalize their reference operand. This also exposes
assigned discriminant properties to the existing union-member narrowing.

Native isMatchingReference (flow.go:1597) has a distinct target-side switch:
parentheses/non-null wrappers are transparent, all assignment operators match
the left reference when ast.IsLeftHandSideExpression permits it, and commas
match the right reference. The port applies that switch to both symbol-based
and structural-access matches. It does not conflate the two native protocols:
getReferenceCandidate leaves arithmetic compound assignments and non-null
wrappers intact.

Native narrowTypeByBinaryExpression (flow.go:469) also handles a direct
assignment condition by narrowing with the RHS condition first, then the
truthiness of the assigned reference. A comma condition inherits its RHS.
Extracting the existing narrowTypeByTruthiness body permits that exact ordering
without recursively inlining an alias as though it were the assignment target.
Identifier alias inlining precedes truthiness, as native narrowType requires.

## The two exposed dependencies

The first complete verdict run gained 64 WRONG-to-RIGHT and 2 GAP-to-RIGHT
assertions but lost nine RIGHT assertions. It was not accepted as a landing.

Three losses were parenthesizedJSDocCastDoesNotNarrow. Native reparses a JS
@type cast as an AsExpression inside parentheses (ast/utilities.go:759), so
unwrapping parentheses still encounters an assertion boundary. This parser
keeps the cast type in a side table. Unwrapping directly to its operand erased
that boundary. The flow helpers now stop at a JS parenthesis with a cast tag
on both source and target matching, candidate normalization and condition
recursion. Ordinary JS parentheses remain transparent. Side-table lookup
precedes the file-root walk so ordinary parentheses avoid that walk.

The other six losses were enclosing expressions in
typeGuardsInRightOperandOfOrOrOperator. Improved assignment narrowing correctly
makes an inner x.toString receiver never. The native checker reports TS2339
there, but an enclosing definitely-falsy `&&` still returns its left type.
The port propagated the RHS error before checking whether the RHS contributed
to the logical result. checker.go:12496 and the adjacent ||/?? arms gate that
contribution on left-side facts. Error propagation now follows that gate;
both operands are still checked. Reverting the assignment port to avoid this
path would preserve a different flow semantics, so the logical gate was fixed.

## Controls and falsifiers

Nine new narrowing tests cover operand reversal, nested parentheses, assigned
null branches, unrelated references, truthiness, RHS typeof guards, all three
logical assignments, discriminant assignment, property references, predicate
arguments, comma conditions, loop reads and JS cast boundaries. Four diagnostic
controls silence the valid loop while preserving errors in the null branch,
on another receiver and after reassignment to null. The seven earlier finally
controls also remain green.

A logical-result test uses invalid `1 + {}` RHS expressions behind false &&,
truthy || and non-nullish ??. These expose the premature error propagation;
pinned native still reports the RHS errors while declarations retain false,
"value" and 1. A first proposed control using missing() passed prematurely:
the call answered any rather than the internal error sentinel, so it did not
exercise the defect. That input was replaced before accepting the control.

The JS test harness now stamps JS roots and passes JSDoc entries to both binder
and checker, matching Program construction. Without the root flag, the cast
control ignored JS semantics; without binding JSDoc, the object cast read
answered error before reference matching. The final controls distinguish
ordinary-parenthesis narrowing from a cast switch that leaves the original
string type, and retain number | null for a property read through a cast.
Pinned native declarations confirm the latter boundary independently.

## Alternatives and limits

Suppressing TS18047 in loop bodies would hide the null-branch and reassignment
errors. Treating every binary expression as its left reference would narrow
unrelated operands. Sharing one normalization rule for both native helpers
would erase the deliberate differences in compound operators and non-null
wrappers. The two helpers retain native dispatch and symbol identity.

Correctness, testing, standards, maintainability, error handling and adversarial
review ran sequentially in the primary context under the user's AGENTS
instruction. No independent or cross-model review is claimed. Reuse, quality
and efficiency review consolidated the cast predicate and ordered its lookups;
the three normalization protocols remain separate because their semantics differ.

The full-project recursive conditional/mapped slowdown remains on tsr-6.3.
This port removes the reproduced assignment-condition diagnostic; it does not
establish that the entire Next.js project completes or satisfies the 99% goal.
