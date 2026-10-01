# Construct signature relations and class constructor types

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline aab165d8: 452,054/478,855 correct assertions (94.40%).

## Native rules and implementation

signaturesRelatedTo (relater.go:4441) compares call and construct signature sets
independently, sharing parameter, return, generic erasure and overload matching.
A missing required signature is a rejection; failed resolution remains unknown.
Construct signatures additionally reject abstract sources for concrete targets
and apply constructorVisibilitiesAreCompatible (relater.go:4520). Private targets
accept every visibility, protected targets reject private sources, and public
targets require public sources. Declaration identity survives inheritance so
these checks still see the original constructor's accessibility.

resolveAnonymousTypeMembers (checker.go:20650) gives class values their own
constructors or getDefaultConstructSignatures (checker.go:20857). The port now
provides those signatures through the shared signatures_of_type_kind consumer.
Own overload implementations are omitted. Generic class returns use a type
reference over the class's own parameter identities, so later inference can
substitute them. Default constructors inherit base parameters after substituting
explicit arguments and defaults, then replace the return and type parameters
with the derived class's own identities. fillMissingTypeArguments
(checker.go:21954) initializes unfilled parameters to error before applying
defaults, preventing invalid forward references from escaping unbound.

A per-class cache holds the resolved list and uses None during recursive or
unsupported resolution. Empty lists are reserved for a known absence of
signatures, such as a TypeScript base with no applicable type-argument arity.
JavaScript generic base defaults still decline: their implicit-any and default
normalization rules are not yet shared here. This prevents a missing JS argument
from becoming an incorrect applicability rejection.

Class static types now participate in structural member comparison. Their
requirements include the synthetic prototype and inherited static exports.
Computed static members use the same lookup and enumeration path; a computed
instance member cannot satisfy a missing static member. Class expression member
names participate in the late-bound walk as well. Constructor types without
properties use the existing pure-signature fast path; class values retain the
member comparison.

## Experiments, controls and decisions

Construct type annotations alone gained 154 matches with zero RIGHT losses,
including all 82 remaining mismatches in subtypingWithConstructSignatures2.
Class constructor resolution raised the gain to 203, again with zero RIGHT
losses. Returning the raw declared generic class type instead of an instantiated
reference would prevent inference from substituting constructor returns; the
existing new-expression implementation and the native generic class identity
supply the reason for using the reference representation here.

Pinned native checks verify ten construct/call relation pairs and missing-call
rejection. Rust tests exercise both assignability and subtyping. Declaration
controls verify public/private/protected and abstract constructors, generic and
inherited constructors, default type arguments, overloaded constructors, static
requirements and visibility in both directions. Extending a private class
intentionally carries native TS2675; its declaration output still supplies the
expected inaccessible inherited-constructor result.

Review found a concrete false acceptance: a class with only an instance
[Symbol.iterator] could satisfy typeof a class requiring that static method.
The computed static lookup/enumeration fix restores the native fallback, with
positive static and negative missing-member controls alongside it. Review also
updated an obsolete test that expected unknown for a missing call signature.
The native conditional-type control establishes that the result is false.

A defaulted generic base control exposed a separate existing instance-member
gap: a Derived class extending a defaulted generic Base can retain the base's uninstantiated
T when Derived reads inherited members without explicit heritage arguments.
The constructor-default control declares its instance property on Derived to
isolate constructor parameters; the inherited-member problem remains tracked in
tsr-6.28. It is not counted as implemented.

Simplification and review run sequentially in the main thread under the user's
tool mapping. No independent agent or cross-model review is claimed. The review
covers empty versus unknown signature sets, abstract/accessibility checks,
overload conjunctions, recursive constructor caching, inherited substitutions,
static requirements, native controls and full-corpus transitions.

## Checkpoint and remaining work

42e7881a: 452,265/478,855 correct assertions (94.45%).
6,694/9,538 complete cases (70.18%).
Aligned verdicts: 474,243 total; 452,265 RIGHT; 3,402 GAP; 18,576 WRONG.
Relative to aab165d8: 171 WRONG->RIGHT, 40 GAP->RIGHT, zero RIGHT losses,
11 GAP->WRONG and 19 changed wrong answers. The 95% threshold is 454,913;
2,648 additional correct assertions remain.

Ten new wrong answers in contextualSignatureInstantiation4 expose contextual
constructor instantiation (Banana<string> versus Banana<any> or Banana<"foo">).
The other is heterogeneousArrayLiterals' ObjectConstructor/empty-object union.
Changed wrong answers include higher-order generic constructors, literal-return
widening, nested function parentheses, Date constructor display and mixin returns.
These are recorded transitions, not suppressed or counted as successes.

Final source-frozen comparison:
- /tmp/tsr-95-construct-static-final-verdict.tsv
- /tmp/tsr-95-construct-static-final.log
- /tmp/tsr-95-construct-static-final-transitions.txt
- Checker sources plus trace_case SHA-256: aad81af43612aebfc36aaa836da0022d64f3097d0afba004d49d28eee781b81f

Release workspace tests, clippy with warnings denied, format, whitespace and
all 3,362 upstream anchors pass. checker_types coverage refreshes the committed
snapshot. Native controls: /tmp/tsr-95-oracle-construct-pairs.ts,
/tmp/tsr-95-oracle-construct-relations.ts and
/tmp/tsr-95-oracle-static-construct.ts. The three computed-static cases add eight
corpus matches after review, bringing the final gain to 211.

depend walks 4,392 gap lines: C1 still reports 575 roots that no longer gap,
C2 reports 285 cycles and zero depth-cap hits, and C3 balances. C4 still cites
stale 127,736-gap history. tsr-6.29 owns instrument repair; the aligned corpus
comparison and checker_types snapshot are the coverage evidence.

The remaining representation work includes contextual constructor-signature
instantiation (contextualSignatureInstantiation4), higher-order generic
constructor propagation (genericFunctionInference1), and lazy method/function
rendering. Broader CheckMode, full relation normalization, mixed index/signature
requirements, implicit generic heritage instance members and JavaScript generic
constructor defaults remain separate work. Corpus expectations and the pinned
oracle are unchanged. The 95% goal remains active.

Review receipt: /tmp/compound-engineering-501/ce-code-review/construct-relations/review.json
(status complete; sequential main-thread review, no independent coverage).
