# Inferable data beside deferred callbacks

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 3d546aa9, evidence 58a4e940: 456,001/478,855 RIGHT.

The recursive contextual mapper exposed a missing earlier input. Before fixing a
callback parameter, native inferTypeArguments checks its argument with
SkipContextSensitive. This pass keeps ordinary data and replaces context-sensitive
functions with anyFunctionType, retaining a return-only signature where supported.
Object and tuple types propagate NonInferrableType but remain partially inferable
if any member or element can supply data. createReverseMappedType then infers from
those data parts before callback parameter types are fixed.

The port's context-free object builder already did this for property assignments,
but rejected an entire object when it contained a method. It also rejected arrays
with contextual callbacks. Consequently `foo: { contents: "", contains(k) { ... } }`
discarded the string candidate, and `[3, arg => arg.key > 5]` discarded its number
candidate. The final contextual mapper could not recover data it never received.

The builder now handles methods and context-sensitive arrays. A method requiring
context uses the existing context-free function helper or the non-inferable
function placeholder; a non-contextual method is checked normally. Its captured
member retains the method flag and source symbol. Arrays preserve tuple positions
when the existing contextual tuple predicate requires them. Otherwise they use
subtype reduction to combine elements and build an Array reference, matching
checkArrayLiteral's choice. Object/tuple non-inferability propagates through the
existing side table. Top-level deferred arrays now enter this same early pass.

Ordinary arrays continue through the existing checker. Rebuilding them here would
duplicate empty-array, const, spread and literal-widening semantics without a need
to defer any callback. The first broader draft happened to produce the same corpus
verdict, but that is weaker evidence than sharing the established semantic path.
The context-sensitive array arm still declines spreads and omissions; those forms
need their existing checkArrayLiteral logic threaded with the check mode. Object
spread, accessors and non-identifier names likewise retain the earlier bounded
builder's decline. These are unsupported early-pass shapes, not fixture gates.

## Native anchors and limitations

The relevant native paths are checkArrayLiteral and checkObjectLiteral in
internal/checker/checker.go; checkFunctionExpressionOrObjectLiteralMethod's
SkipContextSensitive branch; and isPartiallyInferableType/createReverseMappedType
in internal/checker/inference.go. The existing non-inferable return-only signature
flag and recursive contextual mapper remain prerequisites.

This does not implement the complete live intra-expression site queue. Native
InferenceTypeMapper.Map first calls inferFromIntraExpressionSites, clears cached
inferences and then fixes the requested parameter. The port still harvests many
sites ahead of that event and does not fully model nested fixing order. The remaining
intraExpressionInferences failures and the candidate-free/conditional examples in
reverseMappedPartiallyInferableTypes remain on tsr-8. Their unchanged failures are
not claimed as coverage gains.

## Evidence and falsifiers

Two tests were added and observed failing on the baseline before implementation:
object-method sibling data and tuple callback sibling elements. They pass after
the change. Two further tests cover the independently improved nested method and
array subtype-reduction cases. The tuple test requires four occurrences, covering
both the mutable and readonly mapped contexts and their parameter references.
The tests load the pinned corpus at runtime and follow the missing-corpus skip
policy; an absent individual fixture fails.

Native controls /tmp/tsr-99-partial-native.ts emit declarations without diagnostics
under explicit --strict true. They retain Boxes<{ foo: string }>, the inferred
{ key: number } tuple source, unknown for the no-data source, and number for an
explicitly annotated non-contextual callback. The emitted declarations are under
/tmp/tsr-99-partial-native-out/.

The frozen full candidate /tmp/tsr-99-partial-final.tsv has 474,243 aligned rows:
456,048 RIGHT, 2,743 GAP and 15,452 WRONG. Against 3d546aa9 it gains 47 WRONG-to-
RIGHT, with zero RIGHT losses, zero GAP-to-WRONG and no changed already-WRONG rows.
The gain comprises reverseMappedPartiallyInferableTypes (26),
thislessFunctionsNotContextSensitive1 (13) and subtypeReductionWithAnyFunctionType
(8). The focused two-family probe gained 26; the extra 21 were found by the full
run, not predicted from the focused population.

Sequential primary-thread review covers correctness, callback re-entry, subtype
reduction, method identity and test attribution; no independent review is claimed
under the user's no-delegation instruction. Simplification reuses the existing
tuple-context predicate, function placeholder, return-only signature helper and
subtype reducer. Restricting the new array arm to context-sensitive arrays avoids
replicating ordinary-array semantics. Isolated committed verification and mutation
results will be recorded below after running.
