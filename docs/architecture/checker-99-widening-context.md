# Object-literal widening contexts

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline bd41d1b0: 454,791/478,855 correct assertions (94.97%).
The active 99% target requires 474,067 correct assertions.

## Forcing mechanism

objectLiteralNormalization has 72 wrong assertions at the baseline. The first
array-element union lacks optional undefined properties for siblings' missing
members. The existing widening path only regularizes object freshness; it does
not carry a sibling context into nested properties. A separate historical
is_object_literal_type predicate checks Anonymous types even though the literal
builder emits Named types, so it misses the literal metadata required by native
relations and normalization.

Native getWidenedTypeWithContext (checker.go:18359) creates a sibling context for
a union. getWidenedTypeOfObjectLiteral (18400) widens each property in its child
context and adds missing properties from the parent's literal siblings.
getPropertiesOfContext excludes spread-containing siblings from supplying names;
getSiblingsOfContext still follows their nested member types. getUndefinedProperty
caches by name across the checker, retaining the original property's declaration.
compareSymbols then sorts widened members by those declaration origins. A missing
b can therefore precede a later object's own a; simple append order is wrong.

The port introduces explicit literal/spread metadata, independent root and child
context caches, semantic property images and the checker-wide undefined-property
cache. Contexts retain source symbols for native member ordering. Widened objects
keep semantic members and presentation metadata but lose the ObjectLiteral flag;
regularization retains it. Missing members are visible to property lookup and
member enumeration, not just to the printer.

Native unionObjectAndArrayLiteralCandidates (inference.go:1470) combines literal
candidates before getCovariantInference selects its common supertype. Merely
normalizing the chosen result cannot recover candidates already discarded. Array
literal references therefore receive separate cached identities, following
createArrayLiteralType (checker.go:8103), without marking the canonical annotated
array or tuple. Tuple metadata and reference arguments are copied to the literal
image; widening returns a non-literal image.

## Initial controls and pending verification

Native controls pass --strict true explicitly. The first implementation matches
flat unions, nested member reads, globally cached missing-property ordering and
spread exclusion; the inference control identifies the earlier combination step.
The probe also checks that readonly const tuples retain literal members. An
invalid spread.a access was removed from the control because native declaration
emit prints any while expression-baseline error recovery is a separate question;
the spread shape itself still proves a is not injected into the other branch.

This is an in-progress port. Full-corpus measurements, adverse-transition analysis,
mutation controls, workspace checks and committed isolated verification are still
required. A regression in a previously correct assertion, context leakage between
siblings, treating spread names as normalization candidates, corruption of shared
array identities, or lost optional/readonly member semantics falsifies the
corresponding claim. Work is tracked in tsr-6.39.

## Integration findings

The first full candidate gains 157 RIGHT assertions but loses 152, for +5 net.
The losses are concentrated in reconstruction of pre-existing methods, divergent
accessors and computed names; most generic-construct fixtures have a method named
"new", whose quoted method syntax was replaced by a function-valued property.
The fix retains the original member representation and replaces only widened
property values, then merges synthetic missing members by source-symbol order.
Existing unrepresented computed members are preserved rather than silently erased.
Spread overwrite order remains the original order when no normalization adds
members. This restores all prior RIGHT assertions.

The next full candidate gains 153 RIGHT assertions (141 WRONG-to-RIGHT and 12
GAP-to-RIGHT), with zero RIGHT losses and no GAP-to-WRONG or WRONG-to-GAP.
Its 454,944 matches cross 95% of the unchanged 478,855 denominator. Fifty-nine
conversions occur in objectLiteralNormalization; twenty in
conditionalTypeDoesntSpinForever and eighteen in destructuring defaults. Thirteen
changed wrong answers remain, including method ordering and optional spread
merging. Follow-up native probes cover method/accessor normalization, nested array
properties and indexed literals. Method ordering is corrected by merging methods
and accessor pairs with the same source-property ordering, keeping index members
first. Missing values use missingType under exactOptionalPropertyTypes.

The indexed-literal probe already collapses its element union before widening:
[{["a" as string]:1},{b:"x"}] becomes only {[x:string]:number} instead of preserving
the b:string constituent. This is a subtype/index-relation limit, not evidence
that the widening context should inject the missing constituent after reduction.
It remains a separate follow-up. Final measurement follows the method-order and
native empty-object reduction corrections.

## Frozen candidate verification

The final frozen candidate gains 155 matches: 143 WRONG-to-RIGHT and 12 GAP-to-RIGHT,
with zero RIGHT losses and no GAP-to-WRONG or WRONG-to-GAP. Twenty-one changed
wrong answers remain. Aligned counts are 454,946 RIGHT, 2,924 GAP and 16,373 WRONG
out of 474,243 rows; the scope-wide denominator remains 478,855 (95.007% RIGHT).
The 99% target still needs 19,121 matches. Release workspace tests and clippy pass;
3,340 upstream references resolve. An obsolete array-literal identity test now
asserts separate literal identity, identical reference target/arguments, and
bidirectional assignability, matching native createArrayLiteralType.

An exact-optional native control verifies that reads of normalized missing members
include undefined. Native declaration emit spells those optional members as never;
that emit spelling is not substituted for expression-baseline display expectations.
The separate indexed-literal and optional-spread issues are tracked in tsr-6.40.
The indexed target is omitted by signature_bearing's binder-only index inspection;
a separate relation change and corpus measurement are required to establish its fix.

The simplification pass retains separate root/context caches and preserves original
rendered members: combining those representations would repeat the measured method
and accessor losses. No behavior-preserving simplification was applied. Sequential
review checks native context construction, source-symbol ordering, literal identity,
optional/readonly metadata, spread exclusion and cache boundaries; it does not claim
independent or cross-model review. Committed verification and the mutation outcome
are recorded in the checkpoint evidence after the source commit.

## Committed checkpoint

At bdc82dd5 the isolated checkout passes all three focused tests and reproduces
the frozen verdict byte-for-byte: 454,946 RIGHT, 2,924 GAP, 16,373 WRONG among
474,243 aligned rows. Coverage reports 6,822/9,538 complete cases (71.52%), six
more than the baseline. The all-assertion result is 454,946/478,855 (95.01%);
19,121 remain to the active 99% target. The checker snapshot is copied from this
isolated run. A separate build directory prevents reuse of main-checkout binaries.

Checker sources plus trace_case SHA-256:
4d3cf1b1ab055d901af0bbb7485d38b4c958837dde9c58f90829527fca4ad728

Depend completes with 512 non-gapping roots, 214 cycles, zero depth-cap hits and
3,776 walked gaps. C3 balances. C1 and the historical C4 checkpoint remain stale
(tsr-6.29) and are not evidence for the current coverage result.

Evidence:
- /tmp/tsr-99-widening-verified.tsv
- /tmp/tsr-99-widening-v4-transitions.txt
- /tmp/tsr-99-widening-verified-tests.log
- /tmp/tsr-99-widening-workspace-v5.log
- /tmp/tsr-99-widening-clippy-v5.log
- /tmp/tsr-99-widening-anchors.log
- /tmp/tsr-99-widening-verified-coverage.log
- /tmp/tsr-99-widening-verified-depend.log
- /tmp/compound-engineering-501/ce-code-review/widening-context/review.json

The isolated mutation disables only the insertion of missing context properties.
All three focused tests fail: flat normalization loses optional members, method
normalization loses sibling members, and exact-optional reads become errors.
The original committed source is restored byte-for-byte before cleanup. The
mutation log is /tmp/tsr-99-widening-mutation.log. Main sources are unchanged.
