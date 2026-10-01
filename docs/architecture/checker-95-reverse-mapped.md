# Homomorphic reverse mapped inference

Baseline `659f9303`:448,193/478,855 correct assertions. Pinned tsgo:
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

Mapped targets now retain semantic parameter,constraint,template and modifier
metadata. The homomorphic keyof T arm of inferToMappedType (inference.go:945-971)
constructs reverse inputs by treating T[P] as the inference variable for each
source property,index value,array or tuple element. The shared template collector
uses HomomorphicMappedType priority. Source optionality/readonly and tuple labels
survive unless the mapped declaration added the corresponding modifier. Generic
homomorphic mapped array contexts preserve positional tuples. Captured anonymous
template properties retain their mapped types after the alias mapper is popped;
deferred conditional templates infer through their true/false branches.

## Accepted measurement and limits

Checkpoint `MAPPED_CHECKPOINT`:448,323/478,855 correct assertions (93.62%),
6,568/9,538 complete cases (68.86%). Another 6,590 assertions are needed for95%.
Aligned verdicts:474,243 total;448,323 right;4,547 gap;21,373 wrong. Against659f9303:
82 WRONG→RIGHT,48 GAP→RIGHT,zero RIGHT losses;31 GAP→WRONG and2 WRONG→GAP.

The corpus exposed two identity bugs. Global interface `this` substitution
searched by printed name and could select another interface's identity; it now
uses identity alone. This fixes41 additional assertions. A generic object alias
cache used printed argument spellings,merging separate Tuple[Key] types. Removing
that duplicate mint routes literal aliases through the existing factory cached
by target and argument identities. The two-mapper control and corpus confirm
that both aliases infer their own [string,number] result.

Leading gains:looseThisTypeInFunctions14;isomorphicMappedTypeInference10;
genericFunctionInference2 and reverseMappedTypeInferenceWidening1 each9;
valueOfTypedArray8;strictOptionalProperties1 and thisTypeInFunctions each7;
reverseMappedTupleContext6. These are observed conversions,not family ceilings.
The31 newly wrong answers remain deficits:isomorphicMappedTypeInference8;
reverseMappedTupleContext5;reverseMappedTypeDeepDeclarationEmit5;
mappedTypesArraysTuples4;homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1,
reverseMappedTypePrimitiveConstraintProperty,reverseMappedUnionInference and
substitutionTypesInIndexedAccessTypes each2;genericFunctionsAndConditionalInference1.
Forward transformed mapped member reads,constrained key inference and context
sensitive reverse inference remain incomplete. No as-clause remapping is admitted.
Recursive expanding-source/target guards,partial-inferability priorities,full
reverse member widening and readonly index signature metadata still require their
upstream mechanisms. Direct-call raw contextual fallback does not cover every
nested/new/tagged expression context. tsr-6.9 tracks this continuation.

Pinned declaration controls cover boxed object/array/readonly tuple reversal,
anonymous and nested templates,Partial identity,explicit array context,readonly
object modifiers,deferred conditional templates and two separately scoped aliases
with equal printed arguments. Release workspace tests and clippy pass;3,379
upstream anchors resolve;snapshot refreshed;whitespace checks pass.
Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
Manual review checked mapper push/pop,canonical indexed access identity,alias
cache keys,priority restoration,modifier masks,cached cycles,tuple context and
source union traversal. This is not independent review.

Evidence:/tmp/tsr-95-mapped-identity-verdict.{tsv,log},
/tmp/tsr-95-mapped-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-mapped-controls.log,/tmp/tsr-95-oracle-reverse-mapped-final*,
/tmp/tsr-95-mapped-identity-trace.log (temporary tracing removed).
Checker sources plus trace_case.rs SHA256:
`61da241fee69615ed7d00151a79bfe76c47442047b9c7e1caa59d93bc1d5fccd`.
Goal remains active.
