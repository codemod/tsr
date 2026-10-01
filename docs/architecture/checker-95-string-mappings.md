# Semantic intrinsic string mappings

Baseline55510106:448,659/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

getStringMappingType/applyTemplateStringMapping (checker.go:29223–29284) now
resolve intrinsic Uppercase,Lowercase,Capitalize and Uncapitalize declarations.
Their symbol identities and semantic targets are interned, substituted and followed
by the parameter identity walk. Unions distribute, never stays never, equal nested
mappings are idempotent, and generic string/any/index targets retain mapping types.
Pattern number/bigint placeholders are wrapped as templates before mapping.
Upper/lower transformations map all template spans and holes; capitalization maps
only the first nonempty span or first hole, following the pinned algorithm.

isMemberOfStringMapping/applyTargetStringMappingToSource (relater.go:2498–2524)
apply nested mappings from inside outward and require unchanged source identity
plus membership in the innermost domain. Same-symbol mapping relations and
inference recurse into targets; differing symbols do not infer targets. Pattern
mapping detection prevents redundant template wrappers. Non-string primitive
relations remain disjoint after the simple arms. Template constrained inference
now includes the string-mapping preference from inference.go:617.

Written intrinsic annotations, including nested template arguments, survive in
signatures outside active alias evaluation. Written mapping aliases that normalized
to unions retain their alias names. A qualified enum-member spelling now retains
its regular semantic origin without changing its display/flags. Template
concatenation and string mappings read enum values from that origin; they do not
parse enum member names from printed types.

## Pinned Unicode casing

js_case_data.rs mechanically converts the2,927 lower/upper mapping records and
Cased/Case_Ignorable range triples from pinned stringutil/js_case_generated.go.
The source is Unicode15.1.0, including full multi-codepoint mappings. Rust escapes
represent the same scalar values as Go's four/eight-digit escapes. Tables remain
ordered for binary lookup. js_case implements the pinned Final_Sigma context walk
using those ranges; it does not depend on Rust's evolving Unicode casing tables.
The source's only conditional mapping is sigma. Lone-surrogate strings remain a
limitation of this checker's String representation.

## Verification and limits

Checkpoint STRING_MAPPING_SHA:448,856/478,855 correct assertions (93.74%).
Complete cases:6,577/9,538 (68.96%).
Another6,057 correct assertions are needed for95%.
Aligned verdicts:474,243 total;448,856 right;4,417 gap;20,970 wrong.
Against55510106:174 WRONG→RIGHT,23 GAP→RIGHT,zero RIGHT losses;
eleven GAP→WRONG remain. Ten are stringMappingReduction alias/intersection
answers that still need semantic normalization;one is a recursive Trim conditional.
These remain failures in the fixed denominator.

The first draft lost nine RIGHT assertions. Same-symbol inference, pattern-mapping
normalization and written template/mapping alias annotation reuse repaired them
before acceptance. Twenty-six pinned declaration controls cover full Unicode
case conversions/final sigma/version skew, empty strings, union/never/any,
idempotence, template prefix/hole transformations, generic instantiation and
same-symbol inference, membership rejection, enum values and annotation reuse.
Release workspace tests and clippy with warnings denied pass;3,379 upstream
anchors resolve; checker snapshot is refreshed and whitespace checks pass.

Full generic index classification and base-constraint mapping reductions remain
incomplete. Single-member enum value provenance, enum constraint preference,
intersection/string-mapping reductions, mapped key remapping and arbitrary alias
body normalization remain follow-ups in tsr-6.11/tsr-6.9. NoInfer remains on its
existing implementation path. Complete surrogate/escaping parity is not claimed.

Code review: skipped (ce-code-review unavailable). Independent review dispatch
conflicts with the sequential main-thread instruction. Manual review checked
intrinsic declaration identity, interning/substitution, mapping order/idempotence,
Unicode table conversion and sigma context, enum origin retention, regular/fresh
literal comparison, pattern normalization and written annotation reuse. This is
not independent review.

Evidence:/tmp/tsr-95-string-mapping-final-verdict.{tsv,log},
/tmp/tsr-95-string-mapping-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-string-mapping{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
c4254bec06d9bc14c5f38283cd21768dba0d8e94d4092c5f51e6e30d3846f73f.
The95% goal is not complete.
