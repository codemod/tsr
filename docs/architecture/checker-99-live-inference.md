# Live intra-expression inference

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 5b72e201, evidence 660653be: 456,048/478,855 RIGHT.

## Why completion order matters

Native InferenceTypeMapper.Map (internal/checker/mapper.go) consumes completed
intra-expression inference sites before fixing a requested type parameter.
checkObjectLiteral and checkArrayLiteral register a context-sensitive member only
after checking it. inferFromIntraExpressionSites (inference.go) reads its raw
context without constraints, infers from the completed source, then clears the
queue. A callback that consumes T before a producer has completed must therefore
fix T without the later producer's contribution.

The preceding port harvested top-level members and walked nested functions ahead
of checking the argument. That could supply future information too early and miss
information inside nested objects and tuples. The replacement stores completed
sites on the active inference context. Object initializers, methods and contextual
tuple elements register their checked type; contextual parameter assignment
consumes the queue before fixing. Non-fixing contextual reads resolve current
candidates afresh. Fixed results survive later candidates. The existing
SkipContextSensitive early pass still supplies ordinary data and non-inferrable
function placeholders before the live pass starts.

This replaces the eager harvesting and serving-map block rather than adding a
second path beside it. The old recursive callback collector and unused mapper
helper are removed. The existing recursive constraint/default resolver, inference
priorities, parent return-inference context and signature instantiation remain
shared.

## Prerequisites exposed by removing the eager walk

Several ordinary checker paths had depended on that walk's cached answers:

* getApparentTypeOfContextualType must preserve mapped templates, distribute over
  unions and resolve instantiable constraints. An unconstrained variable has no
  contextual call signature; assuming an unresolved signature lost 78 previously
  correct promisePermutations assertions in an early draft.
* inferFromAnnotatedParametersAndReturn runs before contextual parameter fixing.
  Written parameter and return annotations contribute candidates. Rest parameters
  are excluded, and optionality here means a question token, not a default value.
  Contextual generic rest parameter assignment uses the non-fixing mapper.
* A contextual signature's return type remains a template while parameters are
  assigned. Instantiating that return eagerly loses the context for nested
  returned callbacks. A non-fixing result of any/unknown falls back to the parent
  return mapper before retaining the raw type.
* Contextual array elements need the existing iterable-element query after tuple
  and array handling. The narrower Array-only fallback failed boxed string and
  Iterable contexts, including contextualTypingOfOptionalMembers.
* Object generator methods have contextual return types, unlike class methods.
  Their NEXT slot can use the no-context path when absence of context is proven.
  For async empty returns, use functionHasImplicitReturn's existing bound-flow
  query. Rechecking the body with the older syntactic helper regressed 21 RIGHT
  assertions through mutually recursive async signatures and loop inference.
* Generic rest inference can expand the effective required argument count. A
  missing argument must enter SkipContextSensitive recovery, as Promise.try's
  missing-parameter example shows. Fixed callback parameter types survive that
  recovery; provisional rest inference does not. Clearing the entire callback
  mapper lost five previously correct invalid-enum assertions.
* Context-free method sources retain method syntax when they have one signature.
  anyFunctionType placeholders have no signatures and retain the placeholder path.

Native anchors are InferenceTypeMapper.Map; addIntraExpressionInferenceSite and
inferFromIntraExpressionSites in inference.go; and checkArrayLiteral,
checkObjectLiteral, contextuallyCheckFunctionExpressionOrObjectLiteralMethod,
inferFromAnnotatedParametersAndReturn, instantiateContextualType,
getApparentTypeOfContextualType and checkAndAggregateReturnExpressionTypes in
checker.go at the pinned revision.

## Bounded differences and falsifiers

This remains a port in progress under tsr-8. The fixing mapper identifies consumed
owned type parameters at signature assignment and resolves them in declaration
order; native lazy instantiation can encounter them individually in a different
order. Deeply interdependent constraints remain an important next falsifier.
The regular-element const-array branch does not yet register sites. The early
context-free builder still declines spread/omitted array elements and object
spread, accessors and non-identifier names. There is no general check-mode
parameter threaded through every checker entry point yet.

The mapped-key-remapping serving exception is retained: its contextual object
needs complete instantiation to expose filtered keys. This is an existing bounded
port behavior, not a claim that native instantiateContextualType eagerly maps all
objects. Promise.try's recovered outer result is correct; its unbound U[0]
callback display remains an existing gap/wrong pair.

Eleven regression tests use the real corpus pipeline and native expected types.
They cover completion order, nested objects, tuple siblings, annotations, absence
of context, method identity, generic rest recovery, generator methods, iterable
contexts and async reachability. The consumer-before-producer control requires
unknown, guarding against future eager harvesting. Fixture tests explicitly skip
when the whole corpus is absent and fail for a missing individual fixture.

A direct strict native control emits nested: [number[], string], tupleResult:
number[], fixedBeforeProducer: unknown and noContext: (x: any) => any. The Rust
probefile pipeline reproduces all four. Source and outputs are
/tmp/tsr-99-live-native.ts, /tmp/tsr-99-live-native-out/ and
/tmp/tsr-99-live-control.log.

## Frozen candidate evidence

/tmp/tsr-99-live-final.tsv gains 274 RIGHT assertions against the baseline:
228 WRONG-to-RIGHT and 46 GAP-to-RIGHT. There are zero RIGHT losses, zero
GAP-to-WRONG and 38 changed already-WRONG rows. The biggest gains are
intraExpressionInferences (45), genericFunctionInference1 (25),
partiallyAnnotatedFunctionInferenceWithTypeParameter (19), restTuplesFromContextualTypes
(14), inferFromAnnotatedReturn1 (13) and setMethods (13). These counts come from
aligned full-corpus comparison, not a selected fixture population.

Rejected drafts are preserved as evidence: the initial live draft gained 115 but
lost 169 RIGHT assertions; apparent contextual types reduced the losses to 34;
raw contextual returns and the parent return mapper reduced them to 14. Removing
the remaining eager serving block was verdict-identical at that point. Preserving
fixed contexts through recovery and restoring native generator/iterable paths
removed those regressions. Extending async never inference with the syntactic
body walk then lost 21 RIGHT assertions; the bound-flow query removes all 21.

Review is sequential in the primary thread under the user's delegation override.
It covers state restoration, source-order queue consumption, native annotation
and rest semantics, test attribution and project standards. No independent or
cross-model review is claimed. Committed verification and mutation evidence follow
in the next checkpoint record.
