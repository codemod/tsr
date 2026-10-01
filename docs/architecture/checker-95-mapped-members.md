# Forward mapped member resolution

Baseline502930b6:448,323/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

resolveMappedTypeMembers (checker.go:20894) now enumerates known source keys
or literal/primitive constraint keys and substitutes each key into the captured
template. Property reads use the resulting semantic types;primitive key domains
produce index signatures. Source optionality/readonly modifiers are retained in
the captured member table,and -? strips outer undefined on originally optional
properties. Nested and reference templates share instantiateType. The identity
walk now follows deferred keyof and indexed access operands,so an indexed T[P]
actually substitutes its P rather than remaining a written deferred access.

Recursive constraints exposed a prerequisite. S extends {[K in keyof S]:string}
was rebuilt on every lookup,creating fresh mapped identities until stack overflow.
getConstraintOfTypeParameter now retains resolved constraint identities keyed by
parameter and effective alias mapper bindings;cycles see an unresolved sentinel.
Mapped member resolution has an in-progress guard and publishes an empty member
table before substituting values. The full corpus finishes with normal stack size.
Temporary case and resolver tracing was removed.

## Accepted measurement and limits

CheckpointFORWARD_CHECKPOINT:448,415/478,855 correct assertions (93.64%),
6,570/9,538 complete cases (68.88%). Another6,498 assertions are needed for95%.
Aligned verdicts:474,243 total;448,415 right;4,487 gap;21,341 wrong.
Against502930b6:57 WRONG→RIGHT,35 GAP→RIGHT,zero RIGHT losses;
27 GAP→WRONG and2 WRONG→GAP remain in the deficit.

Observed gains:correlatedUnions14;mappedTypeModifiers14;
contextualComputedNonBindablePropertyType10;mappedTypes3 10;
definiteAssignmentOfDestructuredVariable7;specialIntersectionsInMappedTypes6.
The27 newly wrong answers:mappedTypeGenericIndexedAccess5;
reverseMappedTypeInferenceWidening1 4;mappedTypes2 4;
strictNullNotNullIndexTypeShouldWork3;destructuringWithConstraint,
typeVariableTypeGuards,voidReturnIndexUnionInference,
dependentDestructuredVariablesFromNestedPatterns and mappedTypesArraysTuples
 each2;substitutionTypeNoMergeOfAssignableType1.

Homomorphic array/tuple transformation and union distribution remain separate
from member reads. Generic key constraint inference,as-clause remapping,symbol
keys and complete lower-bound intersection/union key projection remain incomplete.
The captured table avoids inventing binder symbols;full synthetic property parity
for relations,optional/readonly assignment checks and declaration origins remains
unfinished. IndexInfo still lacks readonly metadata. These are continuation work
in tsr-6.9,which remains in progress.

Six pinned declaration controls cover transformed property reads,optional and
required modifiers,literal key substitution,string index signatures and nested
templates. Two older tests expected transforming maps to remain gaps;they now
assert the resolved Box<string>,including a predicate retaining that member type.
The independent as-clause control still declines. Release workspace tests and
clippy pass;3,379 upstream anchors resolve;snapshot refreshed;whitespace checks
pass. Code review: skipped (ce-code-review unavailable). The sequential main-thread
rule conflicts with the skill's independent review requirement. Manual review
checked canonical indexed access substitution,cache identity and mapper shadowing,
cycle sentinels,table publication,optional stripping and index/property routing.
This is not independent review.

Evidence:/tmp/tsr-95-mapped-members-index-verdict.{tsv,log},
/tmp/tsr-95-forward-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-forward-controls.log,/tmp/tsr-95-oracle-forward-mapped*,
/tmp/tsr-95-oracle-forward-alias*,/tmp/tsr-95-forward-recursive-trace.log.
Checker sources plus trace_case.rs SHA256:
174abfc4130958405df7f394506ec8d70b60d14f20d362319dfe2e72a36c2f09.
Goal remains active.
