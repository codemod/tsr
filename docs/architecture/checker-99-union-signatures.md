# Union call and construct signatures

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline ced80f1f: 452,875/478,855 correct assertions (94.57%).
The active 99% target requires 474,067 correct assertions.

## Native paths

getUnionSignatures (checker.go:21108) now supplies the shared call and construct
lookup. Its first pass matches each candidate across every constituent, preferring
exact matches over partial subtype matches. Nongeneric candidates may differ in
returns and optional positions. Generic candidates come only from the first list
and require matching constraints, defaults and returns after alpha mapping.

If the first pass is empty and at most one constituent has overloads, the second
pass combines that master list with the other signatures. Effective tuple/rest
positions are intersected, required counts take the maximum, extra rest positions
are retained, and returns are unioned. Generic parameters are aligned by their
constraints; the first generic signature supplies defaults. Receiver types are
intersected. The old independent-constituent, agreed-return fallback is removed.

Composite predicates follow getUnionOrIntersectionTypePredicate (relater.go:2049):
parameter positions and kinds must match, false-only members do not prevent a
predicate, and assertion predicates are excluded. Intersection call lists use
resolveIntersectionTypeMembers / appendSignatures: concatenate and deduplicate
including returns. Contextual callbacks can then combine the overload contexts.
Unsupported type identities remain unsupported; identity is not approximated with
printed text or mutual assignability. Recursive callback identity has a 32-level
limit and does not implement general structural object identity.

getArrayMemberCallSignatures (checker.go:21070) supplies an empty union call list
from the same global Array/ReadonlyArray member on the union element type. Any
readonly member selects ReadonlyArray. Instantiated member types retain their
mapper, composing images across subsequent instantiation. Equal flat mapper
source lists also supply the native mapped-type ordering before the TypeId tie
break. Full native mapper-kind/composition ordering is not represented.

Predicate inference distinguishes no inference source from an unsupported source:
if ordinary callback returns are used instead of a target predicate, a parameter
mentioned only by that predicate has no source. Its constraint/default can apply.
Ordinary rest overloads use effective positions instead of raw tuple parameters.
Decidable nongeneric overload failure combines parameters and receivers with
subtype-reduced unions and intersects returns, following
createUnionOfSignaturesForOverloadFailure (checker.go:9581).

Union new expressions resolve the combined construct list, instantiate generics
and reject abstract constructors before selection. Native someSignature (:8710)
checks component flags, independently of the signature cloned for a union. The
port preserves that abstract-component fact separately from SignatureKind, so
constructor relations and printing retain the cloned signature's own kind.

Call nodes are passed explicitly through overload resolution. Recovering a call
only through its first argument cannot handle receiver overloads with no arguments.
The fallback lookup remains for callers that supply argument lists without nodes.

## Experiments and review

The initial lookup wiring alone changed no corpus assertions. Full construction
exposed an empty-candidate indexing panic; choose_overload now rejects empty sets.
The third candidate gained 88 matches but lost 14, chiefly predicate inference and
array method ordering. Native array-member fallback, effective rest applicability,
inference absence and union constructors produced 406 gains against 20 losses.
Mapper ordering and intersection call contexts raised that to 463 gains against
seven losses, all mixed abstract/concrete unions.

An instrumented native build made the last abstract detail explicit: the combined
signature itself was concrete, while someSignature inspected its abstract members.
The temporary Go overlay and diagnostic binary are outside the repository; pinned
vendor sources remain unchanged. Removing the return-agreement shortcut then
exposed two zero-argument receiver calls. Explicit call-node propagation recovered
both. An obsolete unit test requiring a union-parameter gap now asserts the native
combined result even when the argument is invalid.

Six focused union tests plus the seven binding tests cover generic mapping,
constraints/defaults, domains and overloads, tuple rests, composite predicates,
assertion exclusion, array fallback, intersection callback contexts, concrete and
abstract constructors, and zero-argument receivers. Native declaration controls
use explicit strict mode; invalid calls retain their expected diagnostics.
Sequential review covered correctness, standards, testing, maintainability,
adversarial cases and API changes. No independent or cross-model review is claimed.

## Remaining limits

General structural signature identity, recursive generic identity, full native
mapper representation, constructor intersection/mixin resolution and all spread
applicability remain incomplete. Existing strict-null callback narrowing, tuple
normalization/context, namespaced type printing and mixed call/construct anonymous
member lookup still produce mismatches. The typechecker remains a partial port;
this unit does not claim complete overload diagnostics or declaration serialization.
No denominator or expected native baseline changed. The depend instrument remains
non-authoritative while tsr-6.29's C1/C4 controls are stale.

## Verified checkpoint

At UNIT_CODE_CHECKPOINT the full aligned comparison records 453,337 RIGHT,
3,224 GAP and 17,682 WRONG among 474,243 aligned assertions. This is +462
versus ced80f1f: 405 WRONG-to-RIGHT, 57 GAP-to-RIGHT, zero RIGHT losses,
16 GAP-to-WRONG and 27 changed wrong answers.

Coverage is 453,337/478,855 assertions (94.67%) and 6,740/9,538 fully
matching cases (70.66%). The 99% goal still requires 20,730 matches.

Evidence:
- /tmp/tsr-99-union-signatures-final.tsv
- /tmp/tsr-99-union-signatures-final.log
- /tmp/tsr-99-union-signatures-final-transitions.txt
- /tmp/tsr-99-union-signatures-sourcehash.txt

Release workspace tests finish with exit 0 and 191 passing result blocks.
All six focused union tests and seven binding tests pass. Clippy with warnings
denied and 3,348 upstream anchors pass. The refreshed checker snapshot confirms
the counts above. Checker sources plus trace_case SHA-256:
2faafa9f4c424fe7a1c53aecb32f8790ea43167038e7bd945a476360139b446a.

Remaining work is tracked in tsr-6.28 and tsr-6.30. The goal remains active.

Formatting and whitespace checks pass. The depend instrument exits 0 but its
C1/C4 controls remain stale; C3 balances 4,191 walked gap lines. It is not used
to claim coverage.

```text
## Controls
  C1 construction: roots that do not gap 572 (expect 0)
  C2 construction: cycles 254, depth-cap hits 0
  C3 arithmetic: root buckets sum to 4191, gap lines walked 4191
  C4 frozen: STATUS.md publishes ~127,736 gap lines at 63.66%
```
