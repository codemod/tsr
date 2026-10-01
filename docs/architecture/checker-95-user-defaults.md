# Partial user defaults, annotation reuse and tuple expansion

Baseline bde0210e: 449,865/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

getTypeFromTypeReference accepts an argument count between minTypeArgumentCount
and the full parameter count. fillMissingTypeArguments substitutes each declared
default under the preceding arguments (checker.go:21954). The port already had
that substitution, but refused partial lists for user declarations. The old
library-only gate protected a display approximation rather than a semantic rule.

Valid partial user references now take the same default fill. Their computed
references carry the full argument list; written signature annotations retain
only the supplied arguments through qualified_written_text. Existing library
reference display handling stays on its older road. Globally shortening the new
reference would repeat the old error: genericDefaults wants i01<number,number>
from a computed assertion while a written function parameter keeps Pair<string>.
The pinned declaration probe distinguishes an explicitly asserted variable's
reused declaration node from a computed call result. The corpus types baseline
uses the semantic reference display at assertions; these are different print sites.

The first completed draft gained144 RIGHT but lost2 signature prints in
 deeplyNestedMappedTypes. Their previously unresolved Input/Output aliases became
references to Static<...>, and function signatures expanded that inner name.
serializeTypeForDeclaration reuses the outer written alias. Annotation reuse now
recognizes an alias whose resolved body retains a different generic alias, and
carries the same reuse through array element annotations. Primitive transparent
aliases do not take that reference-body branch. Final measurement restores both
lost answers and adds29 further RIGHT assertions.

## The tuple prerequisite

The initial unbounded full run was stopped after several minutes. An instrumented
corpus runner isolated excessivelyLargeTupleSpread: admitting BuildTuple<3>'s
partial default enabled its recursive tuple doubling. Syntax spread normalization
flattened concrete tuple operands without the limit already present in the semantic
normalizer. TupleNormalizer.normalize rejects an expansion of10,000 elements or
more before allocating it (checker.go:23378). The syntax path now applies that
same bound before flattening. This is the upstream limit, not a recursion guess
or a fixture exception. The completed corpus run has119/119 RIGHT in that case,
one more than bde0210e. The temporary timing runner was removed.

Pinned boundary controls show length9999 below the bound and TS2799 at exactly
10,000, plus TS2799 for the impossible recursive BuildTuple<3>. tsgo emits any
for the latter member reads during error recovery. The port still returns error
there; the local control pins that honest decline and termination. The rejection
is ported; general error-to-any member recovery remains unported.

## Verification and limits

Checkpoint CURRENT_USER_DEFAULTS_SHA: 450,038/478,855 matching assertions (93.98%).
Complete cases: 6,605/9,538 (69.25%). Another4,875 assertions are needed for95%.
Aligned verdicts:474,243 total;450,038 RIGHT;4,120 GAP;20,085 WRONG.
Against bde0210e:143 WRONG→RIGHT,30 GAP→RIGHT,zero RIGHT losses,2 GAP→WRONG,
1 WRONG→GAP and30 WRONG→WRONG type changes. Gains include55 genericDefaults,
49 tsxLibraryManagedAttributes,13 deeplyNestedMappedTypes and11
reverseMappedTypeInferenceSameSource1. Denominator and oracle are unchanged.

Two GAP→WRONG reads in declarationEmitReusesLambdaParameterNodes now retain
Props<Option,Whatever> where the oracle expands imported Omit/Partial types.
The WRONG→GAP is genericCallAtYieldExpressionInGenericCall3's Promise<void>.
Cyclic/forward defaults,broader constructor selection and imported/mapped alias
reductions remain in tsr-6.1/6.21/6.23. The95% goal remains unfinished.

Pinned user default controls compile with exit0; boundary controls exit2 with
exactly two TS2799 diagnostics. Local controls cover prior argument substitution,
regular/literal/explicit members, chained defaults, written signature spelling,
computed calls, alias arrays/constraints and tuple bounds. Release workspace
tests and clippy with warnings denied pass;all3,374 upstream anchors resolve.
Checker snapshot and whitespace checks pass. A test-only Clippy format_push_string
failure was repaired with writeln!;the control and clippy were rerun successfully.

Code review: skipped (ce-code-review unavailable). Independent dispatch conflicts
with the sequential main-thread instruction. Manual review checked arity/default
admission, preceding substitutions, annotation/computed display separation,
primitive alias transparency, array reuse, expansion before allocation and the
unported error recovery behavior.

Evidence:/tmp/tsr-95-user-partial-alias-verdict.{tsv,log},
/tmp/tsr-95-user-defaults-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-user-partial-alias-controls.log,
/tmp/tsr-95-user-defaults-tuple-control-final.log,
/tmp/tsr-95-oracle-user-partial-defaults{.ts,-out/},
/tmp/tsr-95-oracle-tuple-limit{.ts,.log,-out/}.
Earlier drafts:/tmp/tsr-95-user-partial-{defaults,limited}-verdict.{tsv,log};
timing evidence:/tmp/tsr-95-user-partial-timing.log.
Checker sources plus trace_case.rs SHA256:
28e2fbe61edef832df47489c6efcaed4f223a2fdbe46dcffa4cf67b11d3aaf29.
