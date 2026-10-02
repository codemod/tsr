# Semantic spread members and index provenance

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 39082049: 455,237/478,855 matching assertions (95.07%).
The 99% target requires 474,067 matches.

## The metadata the fold needs

getSpreadType (checker.go:13387) combines complete property symbols and index infos.
The earlier fold retained ordinary typed properties, but rejected indexes and
used an extraction path that read uninstantiated declaration types. The native
property lookup boundary requires resolved reference/heritage member types.

Spread extraction now reads captured semantic properties when available, otherwise
resolves names and values through get_property_names_of_type and
get_type_of_property_of_type. It preserves method flags separately from declaration
origins. getSpreadSymbol (13587) reuses a method only when readonly agrees; changing
readonly creates an ordinary property. Optional collisions and partial unions also
create ordinary property symbols, even when their origin was a method. Inferring
the Method flag from that origin would turn repeated spreads back into methods.
Instantiated method signatures are rendered from the semantic value type.

The shared isSpreadableProperty rule (13581) excludes class methods/accessors and
private identifier class members. Private/protected names are collected before
copying right properties so they suppress same-named left properties, as in native
skippedPrivateMembers. Constructor parameter visibility and accessor visibility use
the existing declaration-modifier reader. Rest filtering retains its separate
private/protected test and shares the native spreadability predicate. Set-only
accessors contribute undefined; getter copies retain their readable value type.

IndexInfo now retains its declaration. Native getIndexInfoWithReadonly copies that
provenance, including through reference/heritage instantiation. Union/intersection
index merges create a new info with no declaration (getUnionIndexInfos:13516 and
appendIndexInfo:21346). Reusing an index therefore preserves its parameter name;
a merged index prints the native fallback x. The fold uses the existing common-key
index merge and applies the requested readonly flag. Values remain TypeIds so
subsequent indexed access sees the same union printed in the spread result.
Partial-union objects retain index infos. Computed property batches can now use
the shared fold because their complete captured property list is independent of
those index infos. Methods/accessors written directly in a mixed batch still need
complete batch capture before the old collector can be removed.

## Name and alias boundaries

The first full candidate gained 85 matching assertions without RIGHT losses, but
quoted computed symbol names and treated unevaluated alias member tables as empty.
Computed names now use late_bound_symbol_member_name from their declarations.
Alias sources resolve through evaluate_alias_body with cycle detection, while the
generic intersection path retains the original alias identity. An alias body that
cannot be resolved remains incomplete; its empty binder table is never evidence
of an empty value type. Omit<T,K> in generic rest remains a known unresolved case.
Generic string/index classification reuses the existing native pattern-literal
predicate instead of a second approximation of pattern placeholders.

## Native controls and frozen evidence

The pipeline test spread_members covers eighteen native outcomes: instantiated
methods, readonly conversion, literal and optional methods, optional collisions,
set-only/getter values, inherited class properties, class/private-parameter
suppression, copied/merged/dropped indexes, index reads, readonly/partial indexes,
and a unique-symbol name. Native inputs and emitted declarations are in
/tmp/tsr-99-spread-members.ts and /tmp/tsr-99-spread-members-native.
The three TS2783 duplicate-property diagnostics are expected; native emits the
verified declarations. Existing exact-optional, ordering, generic and union spread
tests remain controls. No corpus expectations are edited.

The frozen candidate /tmp/tsr-99-spread-members2.tsv has 455,326 RIGHT, 2,875 GAP
and 16,042 WRONG among 474,243 aligned rows. Relative to 39082049: 88 WRONG-to-RIGHT,
one GAP-to-RIGHT, zero RIGHT losses, three GAP-to-WRONG and two changed-wrong rows.
The three exposed wrong answers are JS contextual named properties represented as
an index object. The two changed-wrong answers in spreadInvalidArgumentType now
resolve array members but retain optional-parameter/mapped-member display gaps.
The full denominator remains 478,855; 18,741 matches remain before 99%.

All 204 release workspace result blocks pass. Clippy caught an exhaustive-match
style issue and a string assignment allocation; both are corrected without changing
semantics. The committed-checkout results below verify the final source against
the frozen candidate and supply fresh gates, the snapshot and mutation evidence.
Sequential primary-thread review covers native control flow, provenance copies,
visibility suppression, semantic type lookup, tests and project standards; no
independent or cross-model review is claimed. No broader completeness claim is made
for mixed literal batches, unresolved aliases or symbol-index construction.

Checker sources plus trace_case SHA-256: 58040dac941c386561152c9ad71bad9f5166079600c18b06ce9dfad06be4323c

## Committed checkpoint

The isolated checkout at be4e7f41 reproduces the frozen verdict byte-for-byte and
matches the source hash above. Coverage is 455,326/478,855 matching assertions
(95.09%) and 6,858/9,538 complete cases (71.90%), six more complete cases than the
baseline. The snapshot comes from this checkout; 18,741 matches remain before 99%.

All 204 release workspace result blocks, clippy, formatting, whitespace checks and
3,341 upstream references pass. Depend reports 512 non-gapping roots, 214 cycles,
zero depth-cap hits and 3,712 walked gaps. C3 balances; C1/C4 remain stale under
tsr-6.29 and are not current coverage proof.

Three isolated mutations erase method rendering, discard the index declaration
name, and disable private-name suppression respectively. Each fails the new test's
expected assertion: instance, one and privateCollision. These are assertion
failures, not compilation failures. All mutated production sources are restored
byte-for-byte to the committed hash before the final focused run.

Evidence:
- /tmp/tsr-99-members-verified.tsv
- /tmp/tsr-99-spread-members2-delta.txt
- /tmp/tsr-99-members-verified-workspace.log
- /tmp/tsr-99-members-verified-clippy.log
- /tmp/tsr-99-members-verified-anchors.log
- /tmp/tsr-99-members-verified-coverage.log
- /tmp/tsr-99-members-verified-depend.log
- /tmp/tsr-99-members-mutation-method.log
- /tmp/tsr-99-members-mutation-index.log
- /tmp/tsr-99-members-mutation-private.log
- /tmp/tsr-99-members-restored-tests.log
- /tmp/compound-engineering-501/ce-code-review/spread-members/review.json

The broader tsr-8 work and 99% goal remain active. No other suite is remeasured.
