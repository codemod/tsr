# Template matching and constrained inference

Baseline b4e2483c:448,549/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferFromLiteralPartsToTemplateLiteral and inferTypesFromTemplateLiteralType
(relater.go:2332–2475) now match prefix/suffix spans, search interior delimiters
within known spans, and capture literal or semantic template segments. Empty
delimiters consume one Unicode code point, as in pinned tsgo. Matching equal span
arrays compares hole base constraints and retains string-like source identities.

isValidTypeForTemplateLiteralPlaceholder (relater.go:2476) validates numeric,
bigint, boolean and nullish captures, intersections and nested templates using the
current relation. Comparable patterns use templateLiteralTypesDefinitelyUnrelated
(relater.go:2321); overlapping patterns need not be mutually assignable. Templates
are disjoint from non-string primitive domains after the simple relation arms.

inferToTemplateLiteralType (inference.go:566) contributes captures to inference
parameters, with never candidates on failed all-placeholder matches. Constraint
choices prefer matching templates, string literals, number/bigint literals and
boolean/nullish literals in upstream order. Numeric coercion checks finite values
and canonical round trips separately. Bigint scanner validation rejects separators,
trivia and incomplete tokens; radix conversion retains arbitrary precision.
String mappings and enum constraint choices remain unported here.

Concrete conditional aliases now evaluate at the general reference factory, using
existing argument mappers and instantiation caching. Deferred check operands retain
their parameter dependencies through semantic graph traversal; generic function
signatures continue through conditional signature inference. Written conditional
alias annotations survive in signatures outside active alias evaluation frames.
Object branches retain property types captured under the alias/infer mapper, rather
than rereading uninstantiated property declarations after that mapper is popped.

## Verification and limits

Checkpoint 55510106:448,659/478,855 correct assertions (93.69%).
Complete cases: 6,574/9,538 (68.92%).
Another6,254 correct assertions are needed for95%.
Aligned verdicts:474,243 total;448,659 right;4,451 gap;21,133 wrong.
Against b4e2483c:105 WRONG→RIGHT,5 GAP→RIGHT,zero RIGHT losses.
One WRONG→GAP and one GAP→WRONG remain. The latter is jsxPartialSpread's written
Partial<Parameters<typeof Select>[0]> annotation, which prints an evaluated object
instead; it remains a failure in the fixed denominator.

Twenty-six pinned declaration controls cover first-delimiter splitting, code-point
splitting, empty/failed matches, symbolic template sources, canonical/noncanonical
numeric values, arbitrary-precision decimal/radix bigints, invalid numeric/boolean
captures, union distribution, overlapping/disjoint pattern flow and mapped object
branch properties. The disjoint pattern control emits expected TS2367 from tsgo;
its declaration output is checked. An existing checker regression now expects the
correct number property through an intermediate conditional alias.

Release workspace tests and clippy with warnings denied pass;3,377 upstream anchors
resolve; checker snapshot is refreshed and whitespace checks pass.

The base-constraint helper follows parameter constraints; complete indexed/base
constraint reduction is unfinished. Full getGenericObjectFlags/isDeferredType,
permissive/restrictive conditional instantiations and recursive conditional tail
handling remain incomplete. This port retains conservative deferral for unresolved
parameter-dependent check graphs. Lone-surrogate and complete escaping parity are
not claimed. String mappings, enum choice provenance, generic signature/union
matching and the inference walk's existing depth limit remain in follow-up issues.

Code review: skipped (ce-code-review unavailable). Independent review dispatch is
incompatible with the sequential main-thread instruction. Manual review checked
span boundaries, UTF-8 byte slicing for prefix comparability, current-relation
placeholder checks, canonical numeric conversion, bigint representation, mapped
property capture, signature annotation reuse and unchanged corpus alignment.
This is not independent review.

Evidence:/tmp/tsr-95-template-match-captured-verdict.{tsv,log},
/tmp/tsr-95-template-match-captured-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-template-match-expanded-controls.log,
/tmp/tsr-95-oracle-template-match-expanded{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
5933b8b09d932ee0ecf2db04d11b2d0b139c87d0af10b7a222fc937be84d77ce.
The95% goal is not complete.
