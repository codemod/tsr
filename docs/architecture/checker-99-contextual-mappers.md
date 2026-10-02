# Recursive contextual inference mappers

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 7ef88ba8, evidence 3f184ca8: 455,928/478,855 RIGHT.

A contextual callback can depend on a constraint whose parameters are inferred
from other arguments. Reading that constraint directly leaves stale variables in
the callback. The port now resolves each dependency through the same recursive
constraint/default mapper used for final inference. A provisional result breaks
cycles; fixed results remain fixed. NoDefault uses silentNever, while JavaScript's
AnyDefault remains distinct from the ordinary unknown fallback. These follow
getInferredType/getMapperFromContext in internal/checker/inference.go.

Applying that mapper to an entire object too early is also wrong. Native
instantiateInstantiableTypes preserves object templates, maps instantiable operands,
and rebuilds unions without reduction. Contextual properties read the template
first, and contextuallyCheckFunctionExpressionOrObjectLiteralMethod instantiates
the selected signature before assigning its parameters. The port now carries the
call's mapper to this signature read, including contextual this. Unfixed parameters
missing from a partial map retain their own identity, rather than becoming an
unsupported substitution. The existing uninstantiated-context query flag keeps
const inference from losing literal context.

The new table is keyed by call NodeId. Like intra_expression_member_maps, it
persists for deferred reads after call checking and is cleared when that call
starts a new owned inference pass. It contains a snapshot, not the complete live
native InferenceContext. Nested contexts take the nearest ancestor mapper. This
is a bounded implementation of native context state, not a claim that all mapper
lifetime and cache invalidation behavior is complete.

Preserving templates exposes two more native dependencies:

- getTypeOfPropertyOfContextualTypeEx collects both a generic mapped member and
  applicable index signatures. Such a mapped member does not suppress indexes;
  a concrete named property does. Object-literal intersections now reach the
  existing contextual collector. The intrinsic object type has an empty
  signature set, so object intersected with a callable remains callable.
- checkFunctionExpressionOrObjectLiteralMethod creates an early return-only
  signature with SignatureFlagsIsNonInferrable and marks its object type
  NonInferrableType. inferFromSignature must skip its absent parameters while
  still inferring returns. Otherwise an absent parameter list wrongly infers an
  empty tuple for a higher-order rest parameter. The signature flag now records
  that distinction. A definite callback arity failure also triggers the existing
  SkipContextSensitive overload-recovery pass, including fixed arguments before
  an outer rest parameter.

## Alternatives and measured failures

Recursive constraint mapping alone gained 30 matches but lost four in mapped5:
instantiating its entire intersection collapsed the mapped key domain too early.
Preserving object templates recovered those but required mapping at signature
read; the initial signature-mapping draft gained 54 and lost 57 RIGHT rows, plus
seven GAP-to-WRONG. Apparent contextual member reads and this recovered 23 of
those losses. Identity entries for unfixed parameters recovered another 27.
Const queries, filtered keys, intersection lookup and intrinsic object signatures
recovered the remaining seven. Before the return-only signature flag and callback
arity recovery, the full draft gained 70 but produced four GAP-to-WRONG rows.
Those final dependencies remove all four and increase the gain to 73.

Key-remapping object arguments keep the pre-existing serving-stage instantiation
from inferred preceding arguments. The new instantiateInstantiableTypes helper
itself preserves every object, including mapped objects. Moving the serving-stage
exception into a complete native contextual key-remapping resolver remains future
work: removing it now changes two correct excluded-key callback parameters from
any to number and the literal key. This limitation is explicit rather than hidden
in the helper's semantics.

## Evidence and bounds

The frozen candidate /tmp/tsr-99-context-arity.tsv has 474,243 aligned rows:
456,001 RIGHT, 2,743 GAP and 15,499 WRONG. It gains 73 exact matches (48 WRONG-to-
RIGHT, 25 GAP-to-RIGHT), with zero RIGHT losses and zero GAP-to-WRONG. Sixty-two
already-WRONG assertions change in partially inferred reverse mappings, nested
intra-expression inference and const mapped callbacks. They remain incorrect and
are not gains. The denominator and upstream oracle are unchanged.

Nine tests in contextual_mappers.rs pin dependent constraints, defaulted callback
parameters, seven mapped contexts, intersection callability, unfixed identities,
return-only rest inference, invalid-call recovery, contextual this, excluded keys
and const literal queries. Counting all seven repeated mapped assertions prevents
one already-correct occurrence from hiding regressions in mapped4/mapped5. Tests
load the pinned corpus at runtime, skip only an absent corpus, and fail on a missing
individual fixture. Existing fresh_signatures tests protect sequential generic
callback inference.

Direct native declaration controls are /tmp/tsr-99-contextual-mappers.ts and
/tmp/tsr-99-contextual-mappers-native/, run with explicit --strict true. Native
emits the inferred dependent callbacks, initialized methods, string mapped
callback, nested never tuple and age rest parameter; the invalid Promise.try call
diagnoses TS2554 and emits Promise<unknown>. Native source anchors are
checker.go's instantiateContextualType/instantiateInstantiableTypes,
getTypeOfPropertyOfContextualTypeEx and contextuallyCheckFunctionExpressionOrObjectLiteralMethod,
and inference.go's getInferredType/inferFromSignature.

The full native context engine, nested reverse-mapping fix order and invalid-call
callback printing remain incomplete (tsr-8). Existing mixed-unit union and
unresolved type-parameter intersection guards are unchanged. No fixture-name
conditions or baseline changes were introduced.

Sequential primary-thread simplify and code-review passes cover reuse, efficiency,
correctness, adversarial re-entry, maintainability, standards and tests under the
user's no-delegation instruction. No independent review is claimed. The test review
strengthened repeated mapped assertions. The implementation reuses the existing
constraint resolver, contextual intersection collector and overload-recovery path;
no behavior-preserving production simplification was needed. Committed-checkout
verification and isolated mutation results will be recorded below after they run.

Committed verification at 3d546aa9 in /tmp/tsr-99-context-verify matches the full
candidate verdict byte-for-byte. Source hash:
1a3165837c4736d25892fe613231d463c9e6292db62f13304830a2fea8da9180.
Coverage is 456,001/478,855 RIGHT (95.23%) and 6,923/9,538 complete cases (72.58%),
leaving 18,066 matches to 99%. All 214 release workspace result blocks, clippy
and 3,329 anchors pass in both checkouts. Fresh depend has 499 non-gapping roots,
210 cycles, zero depth caps and 3,554 walked gaps; C3 balances and C1/C4 remain
stale (tsr-6.29). The committed coverage snapshot comes from this isolated run.
The primary-thread review receipt is
/tmp/compound-engineering-501/ce-code-review/contextual-mappers/review.json.

Nine isolated mutations compile and fail their intended assertions: signature
mapping (contextual this), missing partial-map identities (nested never callback),
object-template preservation (all seven mapped contexts), const query guards,
intrinsic-object empty signatures, return-only signature parameter exclusion,
callback arity recovery, this dispatch and filtered-key serving. Each mutation
restores its file; final source hashes equal the committed hash. The const guards
were rerun with separate uniquely anchored replacements for the signature wrapper
and contextual member helper. Scripts/logs use /tmp/tsr-99-context-mutations.py,
/tmp/tsr-99-context-mutations-tail.log and
/tmp/tsr-99-context-mutations-signature.py (with .log sibling); individual logs use
/tmp/tsr-99-context-mutation-{name}.log.

Two narrower recursive-constraint mutations survived the dependent-constraint
test: restoring the old inferred_type_from_info helper, and bypassing constraint
instantiation inside the resolver. Other mapper paths still supply these contexts.
Neither is claimed as an isolated falsifier of recursion. Likewise, bypassing
signature mapping survived the initializer test; contextual this is the assertion
that actually fails. These results qualify the test attribution, not the full
73-match committed corpus delta.

A complete baseline counterfactual replaces all six changed production files with
3f184ca8 while keeping the new tests: five tests fail and four pass. The failures
cover dependent constraints, contextual initializers, mapped contexts, higher-order
rest inference and invalid-call recovery. The script restores the committed hash
in a finally block. This verifies that the tests distinguish the complete change
from the old compiler even where narrower mutations can be compensated by other
mapper paths. Script and logs: /tmp/tsr-99-context-baseline-tests.py and
/tmp/tsr-99-context-baseline-tests-summary.log.

The restored committed compiler passes all nine tests again. Running that test
binary with the corpus absent produces explicit skip notices and nine passing
tests; no compile-time fixture inclusion is required. Logs use
/tmp/tsr-99-context-restored-tests.log and /tmp/tsr-99-context-absent-corpus.log.
