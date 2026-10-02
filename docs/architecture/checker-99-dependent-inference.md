# Dependent candidate constraints and const signature sources

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 0ba7ce80, evidence b8e51f2f: 455,838/478,855 matching assertions.

The preceding unit resolved constraints only for candidate-free parameters.
A supplied candidate skipped dependent constraints: backward(1, 'x') on
<A, B extends A>(a:A,b:B):[A,B] returned [number,string] instead of the native
error-recovery result [number,number]. Forward chains, structural constraints and
keyof constraints had the same omission. Native getInferredType
(internal/checker/inference.go) first installs the inferred candidate, then
instantiates its constraint through the non-fixing mapper, and replaces candidates
that definitely fail. Recursive constraints must observe that provisional
candidate rather than the unknown fallback used when no candidate exists.

Final call inference now collects unconstrained candidates and resolves every
parameter through the recursive mapper. Candidate-free defaults/backreferences
retain the previous behavior. Candidate constraints reuse the existing pure
return-type filter and alternate co/contravariant fallback, factored into one
helper. The existing contextual-fixing entry point still uses its closed-
constraint path; integrating the recursive mapper into all fixing contexts is
remaining work, not claimed here. Structural-source refusals also remain.

The first candidate gained 20 RIGHT rows with zero RIGHT losses and changed four
already-WRONG rows. Two changes were const callbacks: a mutable tuple constraint
became unknown[] after rejection. Investigation showed an older approximation,
readonly_tuple_image, recursively made arbitrary inferred tuples and object
members readonly after collection, dropping tuple labels. Native does not do
this in getWidenedType. applyToParameterTypes chooses readonly when constructing
the source parameter tuple, using isConstTypeVariable and the mutable-array-like
constraint test. It preserves parameter labels and each element's original type.
Existing array/rest sources keep their own mutability under getRestTypeAtPosition.

The port now applies that native rule where signature parameter tuples are
collected. The blanket readonly transform and its exemption bookkeeping are
removed. Existing literal source views continue to handle direct/rest arguments
and callback-return literals before inference; a previously declared object does
not acquire readonly properties merely by passing through a const parameter.
This is a replacement of the approximation, not an exception for the two corpus
rows. Native controls distinguish mutable versus readonly constraints, tuple
labels, optional and variadic elements, nested mutable tuples, and array rests.

Four pipeline tests pin 22 native outcomes. Inputs:
/tmp/tsr-99-dependent-inference.ts, /tmp/tsr-99-dependent-inference-cycles.ts,
/tmp/tsr-99-dependent-const.ts, /tmp/tsr-99-dependent-const-rest.ts. Native outputs
use the same prefixes ending in -native. Every invocation explicitly sets
--strict true --target esnext --declaration --emitDeclarationOnly. Invalid
arguments diagnose but still emit the expected recovery types. The optional
control's native declaration emit includes string | undefined in the tuple
spelling while the port's expression printer uses b?: string; the test pins the
independently verified optional element read as string | undefined, not a claim
that those two printing contexts are identical.

The combined frozen candidate gains 35 WRONG→RIGHT assertions, with zero RIGHT
losses and zero new WRONG rows. Only two already-WRONG rows change, both in
extractInferenceImprovement: any becomes string | number where contextual indexed
inference expects string. That remaining inference mechanism is tracked by tsr-8.
Candidate 2 removes duplicate closed-constraint checking before final mapping and
has the same verdict as candidate 1. Candidate 3 replaces blanket const readonly
images and adds 15 matches. No oracle, denominator, or corpus expectations change.

Verdicts: /tmp/tsr-99-dependent-inference-candidate{,2,3}.tsv.
Comparisons: /tmp/tsr-99-dependent-inference-delta{,2,3}.txt.
Final quality gates, review and committed verification follow below.

Main verification passes: 210 release workspace result blocks, clippy, formatting,
diff checks and 3,330 upstream anchors. Sequential primary-thread correctness,
adversarial, standards and coverage review checked provisional candidates, default
precedence, error propagation, variance fallback reuse and const source mutability.
The simplification pass removes the old readonly transform and the bookkeeping
used only to bypass it; the shared source-view and constraint helpers carry the
behavior. No independent or cross-model review is claimed under the user's
no-delegation rule. Contextual-fixing mapper integration and the two indexed
inference rows remain explicit limitations.

Frozen checker/trace_case SHA-256:
3bb89471563ab4503b4383d011b39c8dc1a9a6775a3cdaa8022c0979b68659e2.
Review receipt: /tmp/compound-engineering-501/ce-code-review/dependent-inference/review.json.
