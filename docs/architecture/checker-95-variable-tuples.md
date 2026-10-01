# Variable source tuple inference

Baseline c438a8aa:448,134/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferFromObjectTypes (inference.go:714-809) matches tuple element structure,
then starting and ending fixed elements. A remaining source array/rest infers
against each target middle element. Adjacent variadic/rest target pairs split
using the variadic type parameter's fixed tuple constraint. The trailing
variadic slice is constructed directly to retain the fixed suffix after a
source rest. Single variadics retain optional suffix speculative priority.

Acceptance requires pinned declaration controls, fixed corpus comparison,
zero previously RIGHT losses and release workspace gates. Goal remains active.

## Accepted measurement and limits

Checkpoint `659f9303`:448,193/478,855 correct assertions (93.60%),
6,559/9,538 complete cases (68.77%). Another 6,720 assertions are needed for
95%. Aligned verdicts:474,243 total;448,193 right;4,624 gap;21,426 wrong.
Against c438a8aa:56 WRONG→RIGHT,3 GAP→RIGHT,zero RIGHT losses;one former gap
is now wrong. Gains:variadicTuples1 29;destructuringParameterProperties3 16;
inferTypesWithFixedTupleExtendsAtVariadicPosition 10;tupleTypeInference 3;
mapConstructorOnReadonlyTuple 1. All ten constrained tuple failures from the
conditional infer continuation are repaired.

The initial matcher lost MergedParams in genericRestParameters3. Source union
traversal exposed the underlying mistake: applyToParameterTypes wrapped a bare
non-tuple rest parameter P as [...P]. getEffectiveRestType must retain P itself,
so a union remains one contravariant candidate. The shared signature rest path
now preserves non-tuple target and corresponding source operands. Tuple-to-array
inference uses the tuple numeric index type. A workspace mismatch control caught
the omitted tupleTypesDefinitelyUnrelated arity gate; the final matcher includes
that gate. These repairs follow upstream mechanisms and the final verdict has
zero RIGHT losses. A failed nonempty slice projection contributes no candidate;
only an empty tuple numeric index supplies never.

Pinned declarations confirm leading/trailing constrained rest splits,labels,
fixed tuple sources,array-to-fixed-tuple recovery and optional suffix speculative
priority. The array recovery and incompatible-arity oracle controls intentionally
report TS2345 while still emitting their declarations. Existing MergedParams and
implied-arity controls pass. Release workspace tests and clippy pass;3,379 upstream
anchors resolve;snapshot refreshed;whitespace checks pass. The anchor count falls
by one because the superseded equal-length-only tuple block was removed.

iterableArrayPattern30 now infers Map<string | boolean,string | boolean> instead
of Map<string,boolean>. It was previously a gap and remains in the deficit;
argument contextual tuple inference is still incomplete. Generic conditional
return annotations and standard-library bind selection remain separate limits.
Mapped target metadata and homomorphic reverse inference are next in tsr-6.8.
Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
Manual review checked arity admission,source/target rest representation,optional
flags and labels,source union variance,fixed constraint chains and bounds,
speculative priority restoration and metadata reuse. This is not independent review.

Evidence:/tmp/tsr-95-variable-tuples-arity-verdict.{tsv,log},
/tmp/tsr-95-variable-tuples-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-variable-tuples-{controls,arity-controls}.log,
/tmp/tsr-95-oracle-variable-tuples*,/tmp/tsr-95-oracle-tuple-mismatch*.
Checker sources plus trace_case.rs SHA256:
`ee5e2d2be9c2ccf6fc458585fa63c58f56a69da59e5cefa57d5bc00b561ab81b`.
Goal remains active.
