# Receiver flow and assignment contexts

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 08ea67e3, evidence 3de17368: 456,431/478,855 matching assertions.

## Receiver resolution precedes flow

Native tryGetThisTypeAtEx (checker.go:12146) first obtains the explicit or
contextual function receiver, or the instance/static class receiver. Each passes
through getFlowTypeOfReference. The port previously returned these types directly.
The existing flow engine now receives those same references with no variable
symbol; implicit-any, module and globalThis fallbacks retain their native direct
returns. This enables discriminants and assertion predicates on bare this.

The first flow run exposed two prerequisites. narrowType's dispatch includes
ThisKeyword and SuperKeyword (flow.go:399), so truthiness must reach the shared
fact filter. The derivation worker also confused a polymorphic this parameter
with the class symbol carried for its constraint. Native isTypeDerivedFrom
(relater.go:4976) follows a source constraint; a type parameter target retains
its own identity. getNarrowedTypeWorker then intersects an otherwise-unrelated
generic source with a candidate related by constraint (flow.go:933). These
branches preserve this & Derived and T & Derived instead of erasing this/T to
Derived. Existing undecidable relation fallbacks remain; this is not a complete
replacement of the flow relater.

## Assignment receivers and explicit slots

getContextualThisParameterType (checker.go:12021) gives an assigned function the
widened receiver of a property or element assignment. WalkUpParenthesizedExpressions
applies to the function, and IsAssignmentExpression with excludeCompoundAssignment
false includes logical and other compound operators. Explicit contextual this
parameters still win. The branch is gated by noImplicitThis or JavaScript, skips
arrows, and excludes CommonJS MODULE_EXPORTS pseudo-locals by symbol identity.
A local variable named exports is ordinary data.

Parameter initializers do not consume contextual function-this unless the
function explicitly declares a this parameter. The existing diagnostic helper
is_in_parameter_initializer_before_containing_function already implements the
native walk, including binding initializers; this port reuses it for typing.
An unannotated explicit this parameter uses its contextual slot when one exists,
and implicit any otherwise. This shadows class-this. Returning any before looking
for a contextual slot lost four RIGHT rows; that rejected draft is retained in
the measurement record.

## JSDoc precedence and host identity

The compound-assignment run exposed one new wrong answer:
thisPrototypeMethodCompoundAssignmentJs wanted Node | undefined but obtained
Element | undefined. Its second assignment explicitly writes @this Node. Native
reparseHosted inserts this as a synthetic first parameter (reparser.go:487), so
it must precede assignment-based inference.

The port already read @this when constructing a complete signature, but a receiver
query cannot construct that signature while its return body is being inferred.
A parameter-only JSDoc read uses the same parsed type nodes without entering
return inference. getFunctionLikeHost (reparser.go:653) identifies the exact
function receiving the comment: first variable initializer, property initializer,
return/export expression, or the rightmost assigned expression of a statement.
Only satisfies expressions are unwrapped. An experiment walking arbitrary
parentheses disagreed with a native control: a tag above a parenthesized function
does not annotate that function. Both the receiver read and existing signature
@this scan now require the native host identity. Broader JSDoc parameter/return
attachment and full signature-links lifecycle are unchanged.

## Alternatives measured

The assignment-only branch added one match with zero adverse transitions. Adding
receiver flow produced nine; truthiness and generic constraint prerequisites
raised this to 21, still without adverse transitions. The unannotated-this draft
lost four previously correct rows, fixed by reading contextual slots first.
Recognizing every assignment operator raised the gain to 39, with one
GAP-to-WRONG and three changed already-WRONG rows. Honoring explicit JSDoc receivers
removed that adverse transition and raised the gain to 49.

A separate context-free object receiver experiment directly called
checkExpressionCached on its containing literal, as native does. The focused
run overflowed its stack: this port eagerly computes object member returns,
where native defers them. That experiment was reverted. The next faithful step
requires deferred member/signature completion; adding a recursion fallback would
conceal the missing lifecycle. Existing literal-self handling remains limited.

## Verification scope

Nine added tests bring contextual_this_objects to 19. They cover property/element
and compound assignments, parentheses, explicit/lexical receivers, loose mode,
JavaScript/local exports, discriminants, truthiness, assertion predicates,
polymorphic/generic intersections, parameter initializers, unannotated slots and
JSDoc host precedence. Native programs are in
/tmp/tsr-99-assigned-this-native-tests-fixed and the jsdoc-host-native control.
The negative controls intentionally report TS7041 (global arrow this), TS2683
(parameter-initializer or unannotated JS this) and TS7006 (implicit this parameter).
These errors do not change the receiver types under test. The first native
extraction script mistakenly matched assertion strings; its parser-error outputs
were discarded and all 11 extracted programs were re-run from actual source
blocks before adding the final JSDoc control.

The frozen candidate has 456,480 RIGHT, 2,693 GAP and 15,070 WRONG across 474,243
aligned rows: 45 WRONG-to-RIGHT and four GAP-to-RIGHT, zero RIGHT losses or
GAP-to-WRONG, and one changed already-WRONG row. Full verdict:
/tmp/tsr-99-assigned-this-host-final.tsv. Source hash (checker/src/*.rs, then
conformance/src/trace_case.rs, sorted paths and bytes):
a40a96dbc34a345d3d83dd1c54d92de6e7c47a54ee5564c06d3a7026c54fa80a.

Review is sequential in the primary thread under the user's AGENTS override;
no independent or external review is claimed. Reuse review kept the existing
initializer, widening, constraint and flow machinery; the JSDoc host helper is
shared by its two consumers. Correctness/adversarial review traced precedence,
compound operators, parameter-default boundaries, generic identity and annotation
attachment. Remaining class-computed-name and context-free-object limitations
stay on tsr-8; the 99% goal is not complete.

The remaining changed WRONG row is assertionTypePredicates1: native retains
this & Test2, while this port returns Test2. The predicate narrowing worker has
its own subtype/intersection sequence; it remains incomplete separately from
this unit's instanceof derivation worker. The CommonJS negative test is also
bounded evidence: exports currently has any type, so the exclusion's fidelity
is established by native/binder symbol tracing in addition to that control.

Main validation passes all 217 release workspace result blocks (including all
19 contextual_this_objects tests), clippy, formatting and 3,320 upstream anchors.
The exact source is frozen for isolated committed verification.

## Isolated committed verification

Code commit 2671c87c reproduces the frozen full verdict byte-for-byte:
456,480/478,855 matching assertions (95.33%), 6,953/9,538 complete cases (72.90%).
That is 49 more assertions and three more cases than 08ea67e3/3de17368. The target
of 474,067 correct assertions leaves 17,587 to 99%. The production source hash
also matches the main checkout. All 217 isolated release workspace result blocks,
clippy, formatting and 3,320 upstream anchors pass. Fresh depend reports 493
non-gapping roots, 209 cycles, zero depth caps and 3,494 walked gaps. C3 balances;
C1/C4 remain stale under tsr-6.29. Logs use the
/tmp/tsr-99-assigned-this-verified prefix.

## Mutation checks

Replacing the five changed production files with their 3de17368 baseline while
retaining the new tests produces eight failures and eleven passing controls.
Two narrower mutations each fail their targeted test: removing bare-this
truthiness narrowing leaves the present branch possibly undefined; disabling
the JavaScript JSDoc receiver read selects the assignment receiver instead of
the explicit annotation. These are assertion failures, not build failures.
Restoring 2671c87c restores all 19 passing tests and the identical production
source hash in both checkouts. Logs use the
/tmp/tsr-99-assigned-this-mutation and -restored-tests prefixes.
