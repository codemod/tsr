# Contextual this assignment and completed method slots

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code c0da5327, evidence fc4577a9: 456,322/478,855 RIGHT.

## The two missing routes

Native assignContextualParameterTypes copies a contextual signature's this
parameter whenever contextual parameter assignment runs. Context sensitivity can
come from untyped ordinary parameters, a this expression, or a contextual return
or yield. The old port only copied this when the body contained this. Thus a
method such as consume(data) {} lost its contextual this slot even though data
made the method context sensitive. The earlier restriction recorded in §928.1 was
too narrow: it correctly excluded non-context-sensitive functions but conflated
that condition with absence of a this expression.

The second route was getContextualThisParameterType's method branch. It manually
read a member from the containing object's contextual type, bypassing reference
instantiation and the live fixing mapper. Ordinary function expressions already
used contextual_signature. Object methods now use that same query, including its
computed-name, union, generic and index-signature paths. The pre-existing binary
assignment fallback remains. Lexical this in arrows is still handled by the
existing expression walk; copying a this slot into an arrow's contextual
signature does not change where a this expression in its body resolves.

consumed_contextual_parameter_types now includes the contextual this slot when
contextual assignment needs it, allowing the live mapper to fix the corresponding
owned type parameter. The signature builder uses the same context-sensitivity
predicate when copying the assigned slot.

## Completion state and query order

The broader query exposed two prerequisites:

1. Contextual object member lookup must establish the containing literal's
   apparent contextual type before evaluating a computed name. In
   computedPropertyNames22_ES6, [this.bar()]() {} has no contextual object type;
   evaluating its name first re-enters the enclosing bar signature and overflows
   the stack. The native getContextualTypeForObjectLiteralElement order avoids
   that evaluation entirely when the contextual type is absent. This changes
   evaluation order rather than adding a depth budget or name-specific exclusion.
2. Once a method has been checked, native assignParameterType's symbol links
   retain its assigned this slot, including resolved absence. The port rebuilt
   that slot when reading a method name after the active inference context had
   ended. That lost the concrete Context type, and extra lookups regressed one
   assertion each in inferObjectTypeFromStringLiteralToKeyof and
   inferentialTypingUsingApparentType2.

A node-keyed optional this-type table records the completed method's assigned
slot. A missing table entry means unchecked; an entry containing None means
checked with no this parameter. The signature builder reuses this parameter state;
each subtree eviction clears it with the existing expression and symbol caches.
This represents a resolved parameter, not an additional source of inference.

Caching the whole completed signature was tried and rejected. It gained 68 RIGHT
assertions but lost silentNeverPropagation's { foo(): true } by retaining an
older boolean return. Parameter assignment and later contextual return inference
have different lifetimes in this port. The final design therefore retains only
the assigned this slot and leaves return inference on its existing path.

## Native controls and regression evidence

A direct strict native program (/tmp/tsr-99-this-native.ts) emits a contextual this
slot for an ordinary method with an untyped data parameter even without a this
expression, for a method that uses this, and for an arrow's function signature.
The same program preserves source-order inference: producerFirst is
[{ value: number }, number], while consumerFirst is
[{ value: number }, unknown]. Its declarations are under
/tmp/tsr-99-this-native-out/.

The contextual_mappers tests add coverage for methods whose body lacks this,
completed method slots, consumer-before-producer ordering, lexical arrow this,
and the computed-name recursion witness. Native anchors are
assignContextualParameterTypes, assignParameterType, getContextualThisParameterType,
getContextualTypeForObjectLiteralElement and contextuallyCheckFunctionExpressionOrObjectLiteralMethod
in internal/checker/checker.go; context sensitivity also follows
ast.HasContextSensitiveParameters in internal/ast/utilities.go.

The first full attempt overflowed on computedPropertyNames22_ES6. Reordering the
member lookup completed the full run with 54 gains and two RIGHT losses. Caching
whole signatures removed those two losses and reached 68 gains but introduced the
silentNeverPropagation loss described above. These rejected results remain
falsifiers for future changes. A complete native signature-links/context-checked
state machine is still broader than this assigned-slot port; tsr-8 remains active.

## Frozen candidate

The final assigned-slot design measures 456,390 RIGHT, 2,697 GAP and 15,156
WRONG across all 474,243 aligned rows. Relative to fc4577a9 it adds 68
WRONG-to-RIGHT assertions, with zero RIGHT losses and zero GAP-to-WRONG.
Four already-WRONG rows in thisTypeInFunctions2 change from missing/any this
to a polymorphic this; resolving that this to IndexedWithThis remains unported.
The full verdict is /tmp/tsr-99-this-slots.tsv. The production source hash is
af22bfc3c1eb2d420d761775669b183e450f91e78c9277b628f4370abe93ca33
(sorted checker/src/*.rs plus conformance/src/trace_case.rs, paths and bytes).

All 216 release workspace result blocks pass. All 14 contextual_mappers tests
pass after adding the return-lifetime regression. Release clippy, formatting and
3,320 upstream anchors pass. Review is sequential in the primary thread under
the repository's user override; no independent reviewer is claimed.

## Committed verification

Commit 270ada6b in /tmp/tsr-99-this-verify reproduces the frozen verdict
byte-for-byte (/tmp/tsr-99-this-verified.tsv). Coverage reports
456,390/478,855 assertions (95.31%) and 6,946/9,538 complete cases (72.82%):
68 more matching assertions and one additional complete case. The 99% target
still needs 17,677 matches. All 216 release workspace result blocks, clippy,
formatting and 3,320 anchors pass in the isolated checkout.

Fresh depend reports 495 non-gapping roots, 209 cycles, zero depth caps and
3,500 walked gaps. C3 balances; C1/C4 remain stale under tsr-6.29.
The four changed already-WRONG polymorphic-this rows remain on tsr-8.

In the isolated checkout, restoring the old body-CONTAINS_THIS assignment gate
fails contextual_this_is_assigned_for_untyped_parameters_without_body_this.
Removing completed this-slot capture fails
completed_method_signature_keeps_its_fixed_context. These are behavioral
counterchecks through the ordinary conformance pipeline, with no production
entry point added for tests. Logs: /tmp/tsr-99-this-mutation-guard.log and
/tmp/tsr-99-this-mutation-slot.log.

Replacing all six changed production files with fc4577a9 fails the two new
assignment/completed-slot tests; the other 12 pass, including the computed-name,
ordering and return-lifetime preservation controls. The first overflowing draft
and rejected whole-signature draft independently exercise the latter regression
risks. Baseline log: /tmp/tsr-99-this-mutation-baseline.log.

After restoring the committed production hash, all 14 contextual_mappers tests
pass again. With the isolated vendor link temporarily absent, all 20 fixture
consults explicitly print the existing submodule-skip message; the test harness
therefore does not assert corpus behavior in that configuration. The vendor link
was restored. Review receipt:
/tmp/compound-engineering-501/ce-code-review/contextual-this/review.json.
