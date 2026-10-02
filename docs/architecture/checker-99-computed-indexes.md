# Computed object indexes and component visibility

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 4e964b97, evidence 44629658: 455,346/478,855 matching assertions.
The active 99% target requires 474,067 matches.

## Construction follows the checked property array

checkObjectLiteral (checker.go:13170–13356) maintains both propertiesArray and a
named property table. The old collector accepted only one primitive index kind
and refused many mixtures with ordinary properties. Its symbol-only component
path also used the named table where the array was required.

The collector now retains each checked declaration and its value independently
of named-member replacement. getObjectLiteralIndexInfo (checker.go:19721–19738)
constructs string, number and symbol indexes in that order. String indexes include
all non-symbol properties; number indexes include numeric property names and
number-assignable computed names; symbol indexes include symbol names. Each
computed declaration contributes component provenance to every applicable index.
A numeric component can therefore appear in both string and number displays.
No printed name determines an index lookup value.

Duplicate ordinary properties contribute both checked values to the index union,
while their named member keeps the last value. Methods need the same rule:
checkObjectLiteralMethod's checked declaration signature is captured independently
of the original binder symbol, whose cached type can refer to an earlier method.
Rendered methods now retain a name for replacement, instead of being anonymous
signature strings that always append. The existing semantic subtype-union helper
replaces the special callable/plain partition, allowing identical callable values
to reduce with the other values.

Computed-name classification now follows native assignability: literal/unique
names become named members; an accepted dynamic name selects number first, then
symbol, then string. Unconstrained generic keys create no index. A constrained
key uses its base constraint. Direct keyof alias references in this port carry
an OBJECT-shaped written identity; base-constraint resolution now evaluates that
alias body so the existing deferred-keyof path can supply string|number|symbol.

## Display and lookup have separate inputs

indexInfoToIndexSignatureDeclarationHelper (nodebuilderimpl.go:2088–2135) can
reuse component declarations even when the semantic index value contains other
named properties. For example, {[str]: 1, a: true} displays [str]: number but
indexing with str returns number|boolean. A copied index retains components;
synthesized merged indexes use the merged semantic value and parameter x.

The native serializer requires every component to be a trivially serializable
computed name before filtering literal/unique names. An inline expression or
literal computed name therefore makes that index display as an ordinary index
signature. Entity references also require isEntityNameVisible and
hasVisibleDeclarations (emitresolver.go:340–467). The first identifier must
resolve, and its declarations must be visible or eligible to be made visible.
Function-local variables fail this rule. Parameters inherit their containing
declaration's visibility; destructuring follows its root declaration. Unresolved
identifiers also fall back to the ordinary index display.

This is the type display path, without mutation of declaration-emitter alias
visibility. Declaration emit can first mark export-linked aliases visible; a
separately emitted .d.ts can consequently expose a parameter name that a type
baseline does not. The existing print-at-creation architecture also does not yet
rebuild arbitrary object component visibility at every later reference site.

## Measured alternatives and remaining limits

The first complete index candidate gained 55 matches but lost 31 RIGHT rows:
29 visibility regressions and two keyof-alias constraint regressions. Visibility
and constraint semantics are required parts of this port, not optional display
improvements. A later candidate applying body expansion to every alias in the
shared constraint resolver lost 56 RIGHT rows (45 WRONG, 11 GAP), including
mapped-type and narrowing paths. It was discarded. The retained keyof path
uses structural declaration metadata and the existing mapper/cache; it does not
expand already represented mapped/conditional types through a different path.

The corrected candidate gains 59 matches (50 WRONG→RIGHT, nine GAP→RIGHT), with
zero RIGHT losses. One GAP→WRONG in computedPropertyNamesContextualType6_ES6
exposes the existing generic contextual boolean-widening difference: native true
versus boolean. Twenty-eight already-WRONG rows change; most involve enum keys
whose stored types still lack native literal identity. This unit does not claim
that these rows are correct. Non-strict null widening, complete generic alias
transparency, declaration-emitter visibility mutation and reference-sensitive
component rebuilding also remain outside the completed representation.

## Verification

Five pipeline tests in computed_indexes cover 40 native outcomes: mixed index
kinds, numeric-name filtering, symbol exclusion, duplicate properties/methods,
callables and accessors, component copies/merges, readonly display, inline
fallbacks, unconstrained/constrained keys, keyof aliases and declaration visibility.
Inputs and native declarations are retained at /tmp/tsr-99-computed-*. Native
runs explicitly set --strict true and --target esnext. Duplicate-property and
unresolved-name diagnostics in the controls are expected; tsgo still emits their
declarations. The exported-after-declaration visibility probe is excluded from
these assertions because native declaration emission marks that alias visible
before serialization, unlike the type-baseline path.

Corpus expectations, denominator, pinned oracle and scoring rules are unchanged.
Primary-thread correctness, testing, maintainability, project-standards and
adversarial review run sequentially under the user's no-delegation instruction;
no independent or cross-model review is claimed. Committed-checkout measurements
and mutation evidence are appended after verification.

Frozen candidate /tmp/tsr-99-indexes-final2.tsv: 455,405 RIGHT, 2,859 GAP and
15,979 WRONG among 474,243 aligned rows; 95.10% of the full 478,855 denominator.
The candidate is byte-identical to /tmp/tsr-99-indexes4.tsv. Release workspace
checks pass all 206 result blocks. Four older checker unit tests were corrected
against /tmp/tsr-99-index-unit-controls.ts native output: visible components now
print individually, and a numeric component in mixed string/number indexes
appears twice. These are local regression tests, not corpus oracle edits.
Production-source SHA-256 (checker sources plus trace_case):
eb76b97e142e1622a946bcc9b1dd65d95b3e168b34013f5c5596336c58e73087.

## Committed checkpoint

Isolated checkout 056e7e87fac0e27599378e0f039d1687a1f2534d reproduces the candidate
verdict byte-for-byte and has the same source hash. Coverage is 455,405/478,855
matching assertions (95.10%) and 6,871/9,538 complete cases (72.04%), ten more
complete cases than the baseline. The snapshot is copied from that checkout.
18,662 matching assertions remain before 99%.

All 206 release workspace result blocks, clippy and 3,335 upstream references
pass in the isolated checkout. Main formatting and whitespace checks pass.
Depend reports 517 non-gapping roots, 214 cycles, zero depth-cap hits and 3,696
walked gaps. C3 balances; C1/C4 remain stale under tsr-6.29.

Evidence:
- /tmp/tsr-99-indexes-final-delta.txt
- /tmp/tsr-99-indexes-verified.tsv
- /tmp/tsr-99-indexes-verified-coverage.log
- /tmp/tsr-99-indexes-verified-workspace.log
- /tmp/tsr-99-indexes-verified-clippy.log
- /tmp/tsr-99-indexes-verified-anchors.log
- /tmp/tsr-99-indexes-verified-depend.log

Four isolated mutations fail their intended assertions: omitting ordinary members
from string-index values fails mixedReadString; disabling visibility fails local;
reusing the binder method type fails repeatedMethodsRead; disabling keyof-alias
constraint evaluation fails aliasKey. These are assertion failures, not compile
errors. Restored sources match the committed hash byte-for-byte, and all five
focused pipeline tests pass afterward.

Mutation logs: /tmp/tsr-99-indexes-mutation-filter.log,
/tmp/tsr-99-indexes-mutation-visibility.log,
/tmp/tsr-99-indexes-mutation-method.log,
/tmp/tsr-99-indexes-mutation-constraint.log.
Restored check: /tmp/tsr-99-indexes-restored-tests.log.
Review receipt: /tmp/compound-engineering-501/ce-code-review/computed-indexes/review.json.
