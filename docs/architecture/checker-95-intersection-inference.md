# Intersection inference and contextual substitution

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline abff3df6: 450,819/478,855 matching assertions (94.15%).

## Missing paths

`inferToMultipleTypes` (internal/checker/inference.go:448–543) first infers
against structured intersection constituents, then supplies a lower-priority
candidate to a single naked variable. The port had a special one-variable
intersection subtraction path but no general constituent walk. Adding the
native ordering gains eight assertions with no RIGHT losses. Two previous gaps
now expose alias display differences in unionAndIntersectionInference3.
General exact intersection constituent matching remains incomplete.

The reverseMappedIntersectionInference1 case needs more than that walk. Native
`inferTypeArguments` checks arguments with SkipContextSensitive before fixing
contextual reads. `checkFunctionExpressionOrObjectLiteralMethod` supplies an
anyFunctionType wildcard for deferred callbacks. `checkObjectLiteral` retains
data members and propagates ObjectFlagsNonInferrableType. Without the early
source, T and E were fixed before reverse mapping could collect their candidates.
Temporary tracing confirmed the final source reached reverse inference with
both variables fixed; the early source reaches it while both remain open.

The implementation creates an inference-only image for plain object literals,
with captured semantic properties and function wildcards. It does not cache
these images as final expression types. Non-inferable images cannot directly
infer a naked variable, but `isPartiallyInferableType` allows their data fields
to supply reverse mapped candidates at PartialHomomorphicMappedType priority.
Property images use the captured-member read path, avoiding a binder read that
would check the deferred callback. Duplicate names retain the final value.
The partial image graph is built from nested syntax and is acyclic.

An alternative was to remove the fixed-inference guard. That would contradict
native inference ordering and allow later sites to change already consumed
parameters. It was not used. Threading a full CheckMode through every expression
checker would cover more forms, but this unit keeps a conservative image builder:
methods, spreads, shorthand/computed/non-identifier names, arrays and nested
return-only callback inference remain for tsr-6.9. Unsupported forms keep the
existing ordinary checking path. Object data still uses the existing mutable
location checker, including its known contextual literal-retention limits.

Finally, `instantiateTypeWorker` (checker.go:22225–22281) substituted unions but
not intersections. Even with correct T/E candidates, the nested mapped callback
context could not substitute its intersection. Mapping the constituents through
the existing intersection constructor fixes callback parameter types and many
other generic intersections. Alias references still take the earlier reference
path; direct intersection metadata retains its symbol. The focused reverse
mapped case rises from18/42 to38/42 correct. Its four remaining differences are
boolean literal retention, not call-result or callback-parameter inference.

## Verification and limits

Checkpoint CODE_CHECKPOINT: 450,978/478,855 correct assertions (94.18%).
The95% target requires3,935 additional matches. Aligned verdicts:
474,243 total;450,978 RIGHT;3,808 GAP;19,457 WRONG.
Against abff3df6:110 WRONG→RIGHT,49 GAP→RIGHT,zero RIGHT losses,
31 GAP→WRONG,28 changed WRONG answers and3 WRONG→GAP. The denominator
and pinned oracle are unchanged. The extra31 wrong answers are previously
unanswered paths, not newly incorrect answers that previously matched.

Those31 exposed mismatches occur in genericCallInferenceInConditionalTypes1(6),
silentNeverPropagation(5),typeVariableConstraintIntersections(5),
objectAssignLikeNonUnionResult(4),freshLiteralTypesInIntersections(3),
isomorphicMappedTypeInference(2),unionAndIntersectionInference3(2),
and one each in intersectionOfTypeVariableHasApparentSignatures,
jsxGenericComponentWithSpreadingResultOfGenericFunction,
intersectionsAndEmptyObjects and mixinAccessors5. They involve remaining alias,
conditional/optional display, literal widening, constraint and higher-order
inference prerequisites. Three formerly wrong constraint-intersection rows now
decline; none matched native before this change. These limitations stay tracked
in tsr-6.9/tsr-6.1/tsr-6.3 rather than suppressing the new native paths.

Pinned declaration controls verify two structured generic constituents, one naked
variable beside a structured constituent, repeated candidates and reverse mapped
call results. Callback parameter assertions also match the pinned corpus.
Additional probes exposed pre-existing gaps for an inline mapped parameter and a
naked generic object containing an implicit-any callback; those are not represented
as supported controls. Native emits TS7006 for the latter while still emitting
its declaration. The port still declines these shapes.

Manual sequential review checked native ordering and priorities, wildcard
exclusion, captured property identities, final-value duplicate handling,
intersection reconstruction and unmodified final expression caches. No independent
agent review was performed. A previously correct assertion changing, or a pinned
control changing type, would falsify this landing; aggregate net gains alone are
not the acceptance criterion.

Complete cases:6,644/9,538 (69.66%). Release workspace tests,clippy with warnings
denied,all3,366 upstream anchors and whitespace checks pass. Checker snapshot
refreshed; binder retains the earlier100% result. After the final runtime
measurement,clippy-only changes added rustdoc backticks and made the same hash-set
default constructor explicit; these do not alter checker behavior.

Evidence:/tmp/tsr-95-intersection-final-verdict.{tsv,log},
/tmp/tsr-95-intersection-final-transitions.txt,
/tmp/tsr-95-intersection-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-intersection-inference.ts and emitted declarations,
/tmp/tsr-95-reverse-partial-debug.log and focused substitute verdict.
Checker sources plus trace_case.rs SHA256: ea3a61c111235b2d21379884f02c64ac67e039628a078bdcd1300de851cfe270.
