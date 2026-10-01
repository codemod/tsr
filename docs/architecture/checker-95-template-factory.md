# Semantic template literal construction

Baseline2e3ef200:448,493/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

getTemplateLiteralType (checker.go:29145) now distributes union holes, concatenates
literal/null/undefined spans, flattens nested templates, retains generic/pattern
placeholders and interns normalized texts plus type identities. A hole-free result
is a regular string literal. Pure string placeholders normalize to string. Never
annihilates a template, including a product that otherwise has100,000 constituents.
The cross-product guard follows the pinned100,000 limit and detects integer overflow
in operand order. The checker has no wildcard intrinsic; that upstream branch is
not represented here.

Template literals now carry TEMPLATE_LITERAL flags and semantic parts instead of
an unresolved OBJECT spelling. instantiateType substitutes holes and reconstructs
the normalized template. The parameter identity walk visits semantic holes before
its primitive flag shortcut. Direct template-bodied generic aliases evaluate under
their parameter binding frames; an in-progress set protects recursive aliases.
Signature annotations reuse written template alias references when normalization
produced a union. This repaired eight written Pat<null|undefined> signature losses
from the first draft before acceptance; variable types retain their computed values.

## Verification and limits

Checkpoint b4e2483c:448,549/478,855 correct assertions (93.67%),
6,574/9,538 complete cases (68.92%). Another6,364 assertions are needed for95%.
Aligned verdicts:474,243 total;448,549 right;4,456 gap;21,238 wrong.
Against2e3ef200:49 WRONG→RIGHT,7 GAP→RIGHT,zero RIGHT losses;
three GAP→WRONG remain. templateLiteralIntersection has two MixE enum/template
answers instead of the literal "a";templateLiteralTypes8 still returns string
instead of "a"|"b". These remain counted in the deficit.
Observed gains:templateLiteralTypesPatterns18,relationComplexityError11,
templateLiteralTypes2 11,templateLiteralIntersection8,constAssertions4,
templateLiteralTypes3 2 and two singleton cases.

Ten pinned declaration controls cover two-hole union expansion, a generic return,
generic alias substitution, nested pattern flattening, string/never/boolean/nullish
holes, written union alias signatures and never in a100,000-member product. Release
workspace tests and clippy with warnings denied pass; upstream anchors resolve;
snapshot refreshed and whitespace checks pass.

Template matching/inference, semantic string mappings, enum literal value
concatenation and complete literal/intersection reduction remain incomplete.
Surrogate-pair and complete template escaping parity are not claimed. The alias
entry handles direct template bodies; arbitrary alias bodies containing template
literals still use their existing paths. The union factory's other limits remain
unchanged. tsr-6.11 stays active for continuation.

Code review: skipped (ce-code-review unavailable). The sequential main-thread rule
conflicts with independent review requirements. Manual review checked interning
identity, span order, nested flattening, mapper identity, never/product ordering,
regular string literal results and signature annotation reuse. This is not
independent review.

Evidence:/tmp/tsr-95-template-final-verdict.{tsv,log},
/tmp/tsr-95-template-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-template-factory{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
b7ada0b69f0ecb11caa6e2c8b2448ae14156f2102f4116646d81d579a8ea9d33.
Goal remains active.
