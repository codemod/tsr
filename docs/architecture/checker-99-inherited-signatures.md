# Instantiated inherited signatures and callable lookup

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 0d806f93: 453,337/478,855 correct assertions (94.67%).
The active 99% target requires 474,067 correct assertions.

## Native behavior and implementation

Named callable interfaces now expose their complete signature candidates to call
resolution. Each inherited signature receives its base reference mapper before
the derived receiver mapper. This follows resolveObjectTypeMembers: own signatures
precede inherited signatures. Type parameter shadowing remains handled by
instantiate_signature_for_reference. A visiting-symbol stack rejects recursive
heritage while allowing valid chains longer than the previous eight-level limit.

Heritage lookup resolves qualified namespace names and import-equals aliases.
Generic bases apply explicit arguments and sequential defaults for signatures,
properties and indexes. getTypeFromClassOrInterfaceReference / fillMissingTypeArguments
(checker.go:23169, :21954) supply the rules: missing required TypeScript arguments
reject the reference; JavaScript missing arguments use any, including unknown or
empty-object defaults. Default mapping starts with unresolved slots set to error,
so invalid forward references cannot escape as free type parameters. A raw symbol
member lookup declines generic bases even when no arguments are written, ensuring
that omitted defaults reach the same instantiated member path as explicit arguments.

The shared signature lookup also preserves union construct candidates when deciding
whether a new expression lacks a construct signature. Previously its named-only
query classified callable constructor unions as call-only and returned any.

Nongeneric overload candidates use the context-sensitive candidate walk. The first
applicability pass checks context-free argument images, including anyFunctionType
for callbacks, before assigning parameter types. The first checked callback image
is retained across later candidates and the subtype/assignable passes, matching
NodeCheckFlagsContextChecked (checker.go:10155). Generic inference retains its
existing speculative rechecking path; this unit does not claim full CheckMode parity.
Unsupported walks restore previous node, symbol and resolved-call caches.

getReducedType / isDiscriminantWithNeverType (checker.go:21819) provides an empty
signature view for an intersection with a required, nonuniform literal property
whose constituent types intersect to never. Optional properties, already-never
constituents and nonliteral conflicts do not establish that reduction. The written
intersection identity remains intact, avoiding the alias-printing regressions of
an eager global reduction. Pattern-literal and conflicting-private-property arms
remain unported.

checkSuperExpression reads only the containing class node's heritage. The instance
path follows resolveBaseTypesOfClass (checker.go:19220): class symbols instantiate
their reference, while class-like values use the return of the first constructor
with matching type-argument arity. This matters for Array, whose nongeneric
constructor returns any[]. The static/call path retains constructor-value lookup.

## Experiments and review

The first candidate gained 32 matches without RIGHT losses. Full named candidate
lookup and heritage defaults then gained 170 while losing 13: JavaScript defaults,
callback contexts and an impossible callable intersection. Fixing those native
boundaries produced 277 gains and two losses, both super.length on a generic base.
The class-versus-constructor base distinction raised this to 293 gains with no
RIGHT losses. Review then added the missing required-argument guard before the
verified measurement below.

Seven focused tests and two updated former-refusal tests cover call/construct inheritance, generic shadowing, dependent
defaults, qualified aliases, deep chains and cycles, forward and missing defaults,
JavaScript empty defaults, callback context persistence, never discriminants and
super instance types. Expected outcomes come from the pinned native compiler's
declarations and existing native corpus baselines. Invalid native inputs retain
expected diagnostics; the raw Rust checker keeps error rather than manufacturing
native error-any, with baseline normalization remaining unchanged.

Sequential review in the primary thread covered correctness, standards, testing,
maintainability and adversarial combinations. No independent or cross-model review
is claimed. Simplification consolidated duplicate super heritage selection and
removed stale comments claiming generic/qualified inheritance remained refused.

An isolated scratch snapshot verified byte-for-byte against the working sources
then disabled explicit heritage instantiation with a uniquely checked mutation.
The focused call/construct test failed on callResult: error instead of string[][].
The real checkout was unchanged; all seven focused tests passed again afterward.

## Remaining limits

Alias-valued and expression-valued heritage, captured outer type parameters,
constructor intersections/mixins, full generic CheckMode state, diagnostic parity
and declaration serialization remain incomplete. Existing namespace printing,
JSDoc augments and mapped/conditional inference mismatches remain visible. This
unit changes neither expected native baselines nor the denominator. The stale
C1/C4 depend controls remain tracked in tsr-6.29 and are not coverage evidence.

## Verified checkpoint

At UNIT_CODE_CHECKPOINT the aligned corpus records 453,632 RIGHT,
3,139 GAP and 17,472 WRONG among 474,243 aligned assertions. This is +295
versus 0d806f93: 229 WRONG-to-RIGHT, 66 GAP-to-RIGHT, zero RIGHT losses,
19 GAP-to-WRONG and 43 changed wrong answers.

Coverage is 453,632/478,855 assertions (94.73%) and 6,766/9,538 fully
matching cases (70.94%). The 99% goal still requires 20,435 matches.

Evidence:
- /tmp/tsr-99-inherited-signatures-verified.tsv
- /tmp/tsr-99-inherited-verdict.log
- /tmp/tsr-99-inherited-signatures-verified-transitions.txt
- /tmp/tsr-99-inherited-sourcehash.txt
- /tmp/tsr-99-inherited-mutation.log

Release workspace tests finish with exit 0 and 192 passing result blocks.
All seven focused tests pass. Clippy with warnings denied and 3,344 upstream
anchors pass. The refreshed checker snapshot confirms the counts above.
Checker sources plus trace_case SHA-256:
744f3402d4bad28ef23cb80dc988238f2b7568b690f95c06883cf9144c9e5f93.

Remaining work is tracked in tsr-6.28 and tsr-6.30. The goal remains active.


Formatting and whitespace checks pass. The depend instrument exits 0 but its
C1/C4 controls remain stale: C1 reports 556 nongapping roots, C2 reports 254 cycles
and zero depth-cap hits, and C3 balances 4,073 walked gap lines. It is not used to
claim coverage. Review receipt: /tmp/compound-engineering-501/ce-code-review/inherited-signatures/review.json.
