# Semantic index keys and contextual lookup

Baseline 594ac488: 450,110/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

The declared-index collector accepted only primitive string/number keys, and
object-literal property contexts stopped at named members. Pattern callbacks
therefore received any, while symbol and overlapping pattern accesses declined.

getIndexInfosOfIndexSymbol/isValidIndexKeyType (checker.go) split union keys,
retain the first declaration per key identity, and admit primitive, pattern and
nongeneric intersection keys. The existing generic signature-position predicate
supplies the supported instantiable/union/intersection/variadic tuple guard;
the old bounded parameter-mention helper did not traverse intersections.
findApplicableIndexInfo/isApplicableIndexType now use the existing assignability
relation, the numeric-name/numeric-template exceptions, string fallback, and
intersected values for overlapping non-string signatures. Unknown relations do
not prove applicability. getContextualTypeForObjectLiteralElement supplies an
applicable index when a named property is absent, including each union context.

The non-strict draft gained188 but lost6 formerly correct null/undefined reads.
getPropertyTypeForIndexType excludes nullable keys before lookup upstream.
Restoring that boundary recovers all6. A structural key-shape shortcut would
retain demonstrated wrong pattern/symbol answers; duplicating a separate callback
index lookup would split the same upstream operation across two implementations.

## Verification and limits

Checkpoint CHECKPOINT_SHA: 450,304/478,855 assertions (94.04%).
Complete cases: 6,615/9,538 (69.35%);4,609 assertions remain for95%.
Aligned verdicts:474,243 total;450,304 RIGHT;4,046 GAP;19,893 WRONG.
Against594ac488:151 WRONG→RIGHT,43 GAP→RIGHT,zero RIGHT losses,
11 GAP→WRONG and16 WRONG→WRONG changes. Denominator/oracle unchanged.
Gains include46 contextualTypeWithUnionTypeIndexSignatures,33 indexSignatures1,
21 objectLitIndexerContextualType and13 contextualTypeShouldBeLiteral.

Ten changed former gaps in contextualTypingOfOptionalMembers now expose missing
recursive generic callback parameter inference. One in generatorYieldContextualType
still resolves a deferred enum/generic index to string. Changed wrong answers retain
unknown callback parameters, Modules["foo"] and Promise<T> return reductions.
Composite contextual intersections, full generic mapped-key detection, union-key
printing, class/static index collection and broader index recovery remain outside
this slice. Nullable controls pin decline: tsgo's invalid-key recovery to any is
still incomplete for unannotated variable declarations, so local controls assert
error explicitly rather than claiming that recovery is ported.

Pinned declarations and local controls agree for string/number pattern callback
parameters, overlapping values reduced to"b", symbol/union keys, tagged string
keys and numeric-string templates. Release workspace tests,clippy with warnings
denied,all3,367 anchors,snapshot and whitespace checks pass.

Code review: skipped (ce-code-review unavailable). Sequential user instructions
prevent independent dispatch. Manual review checked key identity/deduplication,
relation direction,string fallback,nullable exclusion,unknown relations,union
context recursion and unsupported generic/composite boundaries.

Evidence:/tmp/tsr-95-semantic-index-final-verdict.{tsv,log},
/tmp/tsr-95-semantic-index-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-semantic-index-controls.log,
/tmp/tsr-95-oracle-semantic-index{.ts,-out/},
/tmp/tsr-95-semantic-index-probe.log.
Checker sources plus trace_case.rs SHA256:
3151c5bb79c5f215cdc6334bdab0b601446ee0aed84d2912359028363ede1cca.
