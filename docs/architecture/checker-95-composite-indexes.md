# Composite index signatures and intersection contexts

Baseline 87f2dc92: 450,628/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

## Native rules and the missing callers

getIndexInfosOfType previously returned no indexes for a union/intersection.
getUnionIndexInfos (checker.go) retains only exact key types found in every
constituent and unions their values. resolveIntersectionTypeMembers and
appendIndexInfo retain all constituent keys, intersecting values for repeated
keys. These shared collectors now serve indexed access, contextual typing,
relations and inference. The collector-only measurement gained 27 assertions,
with zero RIGHT losses and no new GAP→WRONG rows.

Class declarations already reached the instance collector, but static modifiers
were ignored and anonymous constructor types never reached a static collector.
Static/instance filtering now applies to class declarations and expressions.
Pinned tsgo rejects Derived[key] when only Base declares a static index:
resolveAnonymousTypeMembers inherits named base properties, excluding __index.
The prototype instance continues to inherit indexes by key. A tempting shared
base walk would incorrectly return number for the pinned absent-static control.
The instance recursion guard now pops successful paths, preserving diamond bases.
Generic instantiated heritage and index readonly metadata remain incomplete.

getTypeOfPropertyOfContextualTypeEx handles intersections in two stages: collect
concrete properties, then consult indexes only if no concrete property was found.
The first recursive-lookup draft lost 25 correct assertions; concrete-property
priority recovered 12. The remaining 13 contextualTypeShouldBeLiteral losses
identified two other native prerequisites. appendContextualPropertyTypeConstituent
replaces any with unknown so it cannot erase another intersection member's
context. The surrounding mapTypeEx uses noReductions=true, so contextual unions
must retain other constituents beside any/unknown. Implementing both recovered
all 13 and improved another six assertions. Keeping only the native index lookup
without these context rules would have regressed existing literal inference.

## Evidence and remaining work

Checkpoint 6837bbf9: **450,757/478,855 (94.13%)**;
6,634/9,538 complete cases (69.55%). The95% target needs4,156 more matches.
Aligned:474,243 total;450,757 RIGHT;3,885 GAP;19,601 WRONG.
Against87f2dc92:115 WRONG→RIGHT,14 GAP→RIGHT,zero RIGHT losses,
15 GAP→WRONG and14 changed wrong types. The129 matches comprise27 composite
collection,72 static class accesses and30 intersection contextual results.
Denominator and oracle remain unchanged.

All15 new GAP→WRONG and8 changed wrong results are
reverseMappedIntersectionInference1, where contextual callback types now reach
existing incomplete reverse inference. Four other changed wrong results are
assignment-flow narrowing in indexSignatures1; two concern unchecked destructuring.
These are incomplete prerequisites, not claimed conversions. tsr-6.25 andtsr-6.9
remain open. The inherited-static control declines locally to error while native
reports TS7053 and recovers to any; that existing recovery gap is explicit in the
test. No special-case recovery is added to make the control appear complete.

Pinned controls with explicit strict=true and target=es2020 verify union and
intersection values, numeric/string index precedence, separate class static and
instance types, class expressions, lack of static inheritance, pattern callback
context and named-property priority. The recursive literal-context regression is
also pinned locally against the upstream corpus expectation. Release workspace
tests,clippy with warnings denied,3,366 anchors and whitespace checks pass;
checker coverage snapshot refreshed. Binder retains its preceding100% measure.

Code review: manual sequential review; no independent agent review. Reviewed
key equality,union common-key selection,intersection value merging,base recursion
stack lifetime,static non-inheritance and both contextual precedence stages.

Evidence:/tmp/tsr-95-index-context-verdict.{tsv,log},
/tmp/tsr-95-index-context-transitions.txt,
/tmp/tsr-95-index-{controls,tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-composite-index.ts and its emitted declarations.
Checker sources plus trace_case.rs SHA256:
3074177bcf4167029f5d8d37953882f2271f1df4d31b742de15e803abe3f2d61.
