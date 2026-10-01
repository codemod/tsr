# Const callback return contexts

Baseline a973e8e8:448,945/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

Const callback bodies now read the selected call's uninstantiated parameter
identity from the active inference context. A callback checked during overload
selection can use cached signature metadata for const preflight; this query never
selects the overload again. Direct call literals retain the preceding guarded
callee lookup. The written parameter context still decides whether a body
expression is const; merely having a const parameter elsewhere is insufficient.

getReturnTypeFromBody and return/yield aggregation regularize const literals
before the contextual widening tail (checker.go:20141, :20292, :20329). Inline
array/object returns use the preceding AST source-view mapper, preserving deep
readonly literals, mutable array constraints and existing mutable variables.
Cached expression views are unchanged. Direct const callback return variables
bypass the legacy final readonly image, including deferred callback arguments.
The old call-level refusal for callback-shaped const signatures is removed.

checkTemplateExpression now constructs const patterns from semantic span types
following the existing constant-value fast path (checker.go:7997). Span types
outside the template constraint use string, as upstream does. Known generator
context references supply return and next slots; yield contexts already supplied
the yield slot. Yield expressions now read the contextual next slot where known.
Delegated arrays contribute ArrayIterator's unknown next type to the aggregate
(checker.go:20334 and :6384), preserving the unknown result when a generator has
an any contextual next slot.

## Verification and limits

Checkpoint 5fffc9af:449,141/478,855 correct assertions (93.79%).
Complete cases:6,578/9,538 (68.97%). Another5,772 assertions are needed for95%.
Aligned verdicts:474,243 total;449,141 right;4,338 gap;20,764 wrong.
Against a973e8e8:132 WRONG→RIGHT,64 GAP→RIGHT,zero RIGHT losses,
3 GAP→WRONG and40 WRONG→WRONG type changes. Denominator and oracle unchanged.

The first draft re-entered overload selection from a freshness query and lost
87 previously correct assertions. A partially guarded draft still lost98.
Reading active contexts or cached metadata without selecting callbacks repairs
all of them. The generator draft lost one correct next slot; the delegated array
next contribution repairs it. Fifteen pinned declaration controls cover concise
and block literals, deep object/tuple returns, primitive and union returns,
mutable constraints and variables, ordinary callbacks, templates and generator
return/yield inference. Pinned declaration compilation exits0. The old refusal
test now asserts the pinned readonly callback result.

The three new wrong template rows expose missing constant-variable evaluation:
compiler/templateLiteralConstantEvaluation wants literal "1 2 3" but receives
pattern `${string} 3`. The previous checker gapped these rows. tsr-6.18 tracks
checker.go evaluateEntity, including constant-variable identity, initializer
annotation and before-use/cycle checks. This is a recorded unported dependency,
not a reason to treat string patterns as constant values.

Release workspace tests and clippy with warnings denied pass;3,378 upstream
anchors resolve. The checker snapshot and whitespace checks are refreshed.

Async callback body contexts, async generator slot unwrapping, const overloaded
function types, nested mapped callback sources, composite/indirect source views,
constructors/no-call contexts and complete base-constraint normalization remain
incomplete (tsr-6.15 and related issues). This checkpoint does not complete const
inference or the95% goal.

Code review: skipped (ce-code-review unavailable). Independent review dispatch
conflicts with the sequential main-thread instruction. Manual review checked
parameter identity, inference-context lifetime, recursion guards, cached own-node
preservation, mutable source origins, pre-widening regularization, template span
constraints and generator next aggregation. This is not independent review.

Evidence:/tmp/tsr-95-const-callbacks-delegation-verdict.{tsv,log},
/tmp/tsr-95-const-callbacks-accepted-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-const-callbacks-expanded{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
fa97d807da0ca7fe613508e454ab60297117bbad554bf15faf0c61ca7da46eb6.
