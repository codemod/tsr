# Semantic names for declared computed members

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code b51228ae, evidence 321cef73: 455,722/478,855 matching assertions.

`lateBindMember` and `getPropertyNameFromType` resolve literal computed keys to
raw string/number values. The port instead returned printable spellings, so a
string key containing spaces or digits was quoted in the lookup table, and a
negative numeric key had brackets. `get_property_of_declared_symbol` further
limited late lookup to names beginning with `[`. Normal literal keys therefore
failed both direct lookup and object spread. A real cross-file enum-key interface
spread returned error despite correctly resolving its source interface.

The shared late-member walk now reads semantic literal keys from
`property_name_from_index`; existing symbol keys retain their bracketed identity.
Lookup accepts every semantic key. Serialization continues to read the declaration
through `spread_property_name`, so removing quotes from a lookup key does not
remove required quotes from output. Accessors and overload grouping use the same
semantic names. This is shared declaration resolution, not an enum-only branch.

`getResolvedMembersOrExportsOfSymbol` separates instance members and static
exports. The late-member walk and its cache are keyed by owner and static side;
accessor and method grouping ask for their own side. The cache initially contains
no late members, leaving early-bound tables available on recursion, as the native
resolved-table sentinel does. It prevents repeated walks on ordinary failed
property queries and avoids re-entering the same owner's computed-name work.
A single cache per class was rejected because one side could hide the other's
computed declarations while resolving. No global AST mutation is introduced.

Native controls caught that `property_has_modifier` excluded getters and setters.
That omission combined same-named static and instance accessors and made an
instance read return string rather than number. The shared modifier reader now
includes both accessor kinds, matching declaration modifier semantics.

Three pipeline tests pin 18 native outcomes: string/numeric/negative/fractional/
symbol interface keys, inherited properties, static lookup, object spreads,
cross-file enum keys, accessor reads/writes and computed overloads. Native source
inputs: /tmp/tsr-99-declared-keys.ts, /tmp/tsr-99-declared-keys-accessors.ts, and
/tmp/tsr-99-declared-keys-nonlocal-native-input/{class,index}.ts. Native output
folders have the same prefixes ending in -native. Every invocation explicitly
sets --strict true. The cross-file control also uses --module commonjs.

Known negative probes are preserved: spreading a class constructor still prints
a synthetic prototype absent from native declaration emit. A computed class key
that refers to the class's own computed static property still admits a property
that native circular computed-name resolution rejects. The owner-table sentinel
is not a claim that all of checkComputedPropertyName's per-node resolution and
error recovery are ported. /tmp/tsr-99-declared-keys-cross.ts and its -native
output retain this evidence; tsr-8 tracks the remaining work.

Frozen comparison against the baseline gains 28 matches (20 WRONG→RIGHT,
8 GAP→RIGHT), with zero RIGHT losses and zero adverse transitions. Both the first
candidate and the accessor-modifier correction have that same corpus result;
the new native accessor control distinguishes them. This is why a passing corpus
alone does not establish complete fidelity. No corpus expectations, denominator,
or pinned upstream source changes. Full validation and committed measurements
are recorded below after they run.

All 208 release workspace result blocks pass; the three focused tests pass after
removing unnecessary raw-string delimiters, and clippy, formatting and 3,331
anchors pass. Main logs: /tmp/tsr-99-declared-keys-{workspace,focused,clippy2,anchors2}.log.
The simplification pass reused the existing semantic key and modifier readers,
kept side-specific caches, and moved inherited-walk documentation to its actual
function. Sequential primary-thread correctness, adversarial, standards and
coverage review found no additional required changes; independent or cross-model
review is not claimed under the user's no-delegation rule. Native negative
controls above remain explicit limitations of the ongoing port.

Frozen code SHA-256 (checker sources plus trace_case):
e3b8495f3dc19abf3863fdfe18e7afae4cf98914bf78a7bcac57522f7f2f455a.
Candidate verdict: /tmp/tsr-99-declared-keys-candidate2.tsv. Changes after that
run are comments and test literal delimiters only; committed verification follows.
Review receipt: /tmp/compound-engineering-501/ce-code-review/declared-computed-members/review.json.

Committed verification at 948a8f5b in /tmp/tsr-99-declared-keys-verify matches
the candidate verdict byte-for-byte and the frozen source hash. Coverage is
455,750/478,855 RIGHT (95.17%), 6,900/9,538 complete cases (72.34%), four more
complete cases. The 99% target still needs 18,317 matches. Aligned counts:
455,750 RIGHT, 2,787 GAP, 15,706 WRONG, 474,243 total. All 208 workspace result
blocks, clippy, formatting and 3,331 anchors pass in that checkout. Fresh depend:
505 non-gapping roots, 210 cycles, zero depth caps and 3,604 walked gaps; C3
balances, C1/C4 remain stale (tsr-6.29). The snapshot is copied from the isolated
checkout. Verdict: /tmp/tsr-99-declared-keys-verified.tsv; logs:
/tmp/tsr-99-declared-keys-verified-*.log.

Four isolated mutations compile and fail the intended assertions. Returning
printed keys fails quoted; restricting lookup to bracketed names fails bare;
sharing one cache between instance/static sides fails staticRead; omitting
accessor modifier kinds fails instanceRead. Every mutation restores its source
in a finally block; the final hash equals the committed source hash. Script:
/tmp/tsr-99-declared-keys-mutations.py. Logs:
/tmp/tsr-99-declared-keys-mutation-{keys,lookup,cache,accessors}.log.

All three focused tests pass after restoration:
/tmp/tsr-99-declared-keys-restored-tests.log.
