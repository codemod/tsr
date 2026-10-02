# Complete spread batches and index components

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline be4e7f41 (evidence 23513c6d): 455,326/478,855 matching assertions.
The 99% target requires 474,067 matches.

## Why the literal collector needed semantic members

checkObjectLiteral (checker.go:13170–13356) checks each ordinary batch, then folds
its complete type through getSpreadType. The port captured ordinary properties
but retained methods/accessors only as rendered text. Mixed literals consequently
used the older flat merge, which could not distribute unions or retain generic
intersections faithfully. All spread literals now use the shared fold. Each batch
captures its checked methods and accessors beside ordinary properties; an
incomplete capture cannot fall back to the enclosing binder table, which also
contains other batches. The flat SpreadAssignment collector arm is removed.

Declaration origin is not semantic member identity. Computed literal names use
the same type_literal_member_key helper as type literals, so a quoted or negative
name replaces its earlier method/property rather than creating two members with
the same spelling. Methods retain their Method flag, and readonly methods render
as function-valued properties, matching nodebuilderimpl.go:2581.

Accessor symbols need both read and write types. A checked setter parameter is
retained with the semantic property and instantiated alongside its read type.
The anonymous-member renderer prints differing getter/setter types while the
accessor symbol is reused. getSpreadSymbol creates an ordinary property when
readonly changes or the source is set-only; optional collisions and partial
unions likewise clear accessor metadata. Readonly accessor pairs print properties.
This avoids reconstructing an accessor from a declaration origin after its native
symbol kind has changed.

## Computed symbol components are not named properties

The first complete-batch candidate gained four matches but lost two RIGHT rows
in symbolProperty1/2. Both losses came from treating repeated plain-symbol names
as one named member: property, method and getter components collapsed together.
Native propertiesArray and propertiesTable are distinct. getObjectLiteralIndexInfo
(checker.go:19721–19738) retains component declarations and unions their types;
plain-symbol components never enter the named property table.

IndexInfo now holds an optional typed handle to a checker-owned immutable list of
component NodeIds. This preserves Copy semantics without copying declaration
lists on every index lookup. Copies, reference instantiation and heritage retain
the handle; union/intersection synthesis clears it, just as native clears
IndexInfo.components. Symbol index construction includes unique-symbol members in
the value union when a plain symbol requires that index, while serialization
excludes those named members from its component list.

indexInfoToIndexSignatureDeclarationHelper (nodebuilderimpl.go:2088–2135) emits
serializable components as property signatures, including method components as
function values. An unprintable component falls back to the synthesized index
signature. A copied index preserves components; a merged common-key index prints
[x: symbol] and the merged value. The semantic value supplies element lookup.

A native generic control demonstrates why component provenance and index value
must remain separate: indexGeneric<T>(value: T) returning { [key]: value } prints
an instantiated variable as { [key]: T }, yet indexing it after a number argument
returns number. Native serialization reuses the component declaration, while
instantiateAnonymousType maps the semantic index value. The port now retains and
maps anonymous index infos through instantiation, instead of silently dropping
them. No printed string is used to compute the lookup type.

The existing mixed string/number index-construction limitations remain. This unit
adds symbol components and complete member capture; it does not claim complete
getObjectLiteralIndexInfo support for every computed key combination. Broader
spread alias resolution and optional-symbol serialization remain tracked in tsr-8.

## Verification

spread_batches contains four pipeline tests and 43 native outcomes. Controls cover
method/accessor batches before and after spreads, generic tails, union distribution,
readonly conversion, duplicate replacement, unique versus plain symbols, repeated
component copies, component merging and reads, divergent accessor copies and
collisions, quoted/negative keys, and generic accessor/index instantiation.
Native sources and emitted declarations are retained under /tmp/tsr-99-mixed-*
and /tmp/tsr-99-index-generic*. The one TS2783 duplicate-method diagnostic in the
first probe is expected; native still emits declarations.

The final committed-checkout evidence is appended below. No corpus expectations
or scoring rules are changed. Sequential primary-thread review follows the user
AGENTS.md override; no independent or cross-model review is claimed.

Frozen candidate /tmp/tsr-99-batches-final.tsv: 455,346 RIGHT, 2,869 GAP and
16,028 WRONG among 474,243 aligned rows. Relative to the baseline, 14 WRONG-to-RIGHT
and six GAP-to-RIGHT; zero RIGHT losses and no adverse transitions. The full
478,855 denominator is unchanged. All 205 release workspace result blocks pass.
Source SHA-256 (checker sources plus trace_case): 469a6977a575227ca494a11b12295358963de342a3186a5fa54bd646dc4db1be .

## Committed checkpoint

The isolated checkout at 4e964b97595284be50c104403d95168ee7c1be7c reproduces the
frozen candidate byte-for-byte and matches its production-source hash. Coverage
is 455,346/478,855 matching assertions (95.09%) and 6,861/9,538 complete cases
(71.93%), three more complete cases than the baseline. 18,721 matches remain
before 99%. The committed snapshot is copied from this checkout.

All 205 release workspace result blocks, clippy, formatting, whitespace checks
and 3,338 upstream references pass. Depend reports 517 non-gapping roots, 214
cycles, zero depth-cap hits and 3,706 walked gaps. C3 balances; C1/C4 remain stale
under tsr-6.29 and are not current coverage proof.

Evidence:
- /tmp/tsr-99-batches-final-delta.txt
- /tmp/tsr-99-batches-verified.tsv
- /tmp/tsr-99-batches-verified-workspace.log
- /tmp/tsr-99-batches-verified-clippy.log
- /tmp/tsr-99-batches-verified-anchors.log
- /tmp/tsr-99-batches-verified-coverage.log
- /tmp/tsr-99-batches-verified-depend.log
- /tmp/compound-engineering-501/ce-code-review/spread-batches/review.json

Four isolated mutations remove accessor-write capture, component serialization,
instantiated index storage and semantic computed-name identity. Each fails the
expected assertion: pairAfter, componentCopy, indexCopy and overwritten,
respectively. The index-storage mutation also changes indexRead to error; its
first assertion failure is the earlier copied object. These are assertion failures,
not compilation failures. Production sources are restored byte-for-byte to the
committed hash and all four focused tests pass afterward.

Mutation logs: /tmp/tsr-99-batches-mutation-accessor.log,
/tmp/tsr-99-batches-mutation-components.log,
/tmp/tsr-99-batches-mutation-index-instantiation.log,
/tmp/tsr-99-batches-mutation-computed-key.log.
Restored verification: /tmp/tsr-99-batches-restored-tests.log.
