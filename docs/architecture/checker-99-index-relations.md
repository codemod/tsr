# Structural index signature relations

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline bdc82dd5: 454,946/478,855 correct assertions (95.01%).

## Forcing mechanism

The widening controls expose [{["a" as string]:1},{b:"x"}] losing the second
constituent before widening. signature_bearing inspects binder declarations,
but literal index signatures live in semantic side tables. The structural
relation therefore compares no properties and accepts an incompatible subtype.
Native structuredTypeRelatedToWorker (relater.go:3864) compares properties,
call/construct signatures and indexes as independent conjuncts. The port must
follow semantic index infos for every structural pair, including targets that
also have call signatures. An empty target index set is a successful conjunct.

This index unit is tracked in tsr-6.41; remaining consumers are in tsr-6.40. Native index inference restrictions,
key applicability and optional member types require controls alongside dispatch.
Full-corpus regression checks and committed isolated verification are pending.

## Native rules and controls

The dispatch-only draft changes zero corpus assertions, but preserves the indexed
literal branch in the native control. Wider controls expose three earlier bugs:
interfaces/classes incorrectly infer indexes, numeric indexes compare unrelated
string properties, and a signature-only shortcut drops an index requirement from
a callable object. Native typeRelatedToIndexInfo (relater.go:4603) permits inferred
indexes only for eligible literal/enum/module shapes (or JS/rest/reverse-mapped
objects), and strict subtyping requires a fresh literal source. Applicable source
index signatures take precedence; inferred members are filtered by key applicability.
Optional string-index members exclude undefined under non-exact optional semantics,
while optional numeric members retain it. Missing types are removed separately.

The implementation reuses get_applicable_index_info and is_applicable_index_type,
and shares the semantic property-key helper with reverse mapped inference. Pure
signature dispatch now verifies the absence of properties and index infos first.
Readonly indexes retain native covariant value comparison. The any-valued string
index exception applies outside strict subtyping, as native does.

A pinned native declaration probe confirms thirteen conditional relation results,
including interface/class rejection, type-literal acceptance, numeric key filtering,
optional string/numeric differences, callable/index conjunction, any-valued indexes,
template patterns and symbol keys. Indexed string and numeric literal unions retain
the incompatible constituent and normalize missing members. An additional compatible
indexed-array probe still collapses {[x:string]:number}|{b:number} before widening,
where native retains both; the remaining cause is unresolved and tracked in tsr-6.40.
Generic mapped template and object-rest/reverse-mapped source flags also retain
limits of the current type representation. These are not declared fully ported.

## Measured candidate

The combined native relation rules gain 17 matches: 15 WRONG-to-RIGHT and two
GAP-to-RIGHT, zero RIGHT losses, no changed wrong answers and no GAP-to-WRONG or
WRONG-to-GAP. Eleven conversions are in narrowingMutualSubtypes and six in
controlFlowFavorAssertedTypeThroughTypePredicate. The aligned result is 454,963
RIGHT, 2,922 GAP, 16,358 WRONG among 474,243 rows. Against all 478,855 assertions
this is 95.01%; 19,104 additional matches are required for 99%.

A routing exception restricted to computed literals was rejected as structurally
incomplete: mixed callable/index and mapped targets must obey the same conjunction.
Using every source property for a numeric or pattern index was also rejected by
the native controls. A lost RIGHT assertion, interface index inference, ignored
mixed target index, rejected unrelated string property under a numeric index, or
corruption of exact optional missing/undefined semantics falsifies the corresponding
claim. Full workspace and committed isolated checks follow the frozen measurement.

A second native control under exactOptionalPropertyTypes distinguishes optional
number members from explicit number|undefined: string-index relations are true
and false respectively, while optional numeric members relate after removing
missingType. Unresolved source index infos remain Unknown, since their absence
cannot be inferred from a failed resolver. This guard precedes inferred-index
eligibility; empty target index sets return Related without resolving the source.

Review preserves the native any-valued string-index shortcut before source index
resolution. This exception has no source index requirement; resolving the source
first would turn a known acceptance into Unknown when that unrelated resolver
cannot complete. The earlier guard remains on paths that do require source infos.
The existing source/target signature checks remain conjunctive with indexes; a
shortcut may run only after both property and index sets are empty.

Simplification reuses the existing semantic key/applicability functions and removes
obsolete dispatch commentary describing signature limitations already ported. No
new representation or textual key comparison is introduced. Review runs sequentially
in the primary thread under the user tool map, without independent/cross-model claims.

The final frozen verdict is byte-identical to the combined-rule candidate.
Both focused tests pass, including sixteen native relation outcomes; clippy,
formatting, whitespace and 3,336 upstream references pass. The release workspace
run passes 200 result blocks before the final any-shortcut ordering adjustment;
the final focused tests and full corpus verify that adjustment.

Checker sources plus trace_case SHA-256:
8f95a4b2ca2a439cd01acf66c5f429141d3f891379d2cd463f67329904dad19a

## Committed checkpoint

At 4aad09e1 the fresh checkout passes both focused tests, reproduces the frozen
verdict byte-for-byte and matches its source hash. It uses a separate build target.
Coverage is 454,963/478,855 correct assertions (95.01%) and 6,823/9,538 complete
cases (71.53%), one more complete case than the baseline. The snapshot comes from
that checkout. The active 99% target still needs 19,104 matches.

Depend reports 512 non-gapping roots, 214 cycles, zero depth-cap hits and 3,774
walked gaps. C3 balances; C1 and C4 remain stale (tsr-6.29), not current coverage
proof. The test fixture's internal probe label is corrected in the evidence commit;
source contents and expected assertions are unchanged, and the tests are rerun.

Evidence:
- /tmp/tsr-99-index-verified.tsv
- /tmp/tsr-99-index-v2-transitions.txt
- /tmp/tsr-99-index-verified-tests.log
- /tmp/tsr-99-index-workspace-v3.log
- /tmp/tsr-99-index-clippy-final.log
- /tmp/tsr-99-index-anchors-final.log
- /tmp/tsr-99-index-verified-coverage.log
- /tmp/tsr-99-index-verified-depend.log
- /tmp/compound-engineering-501/ce-code-review/index-relations/review.json

The isolated mutation keeps index resolution but forces every resolved index
relation to Related. Both focused tests fail, including the explicit-undefined
negative control. The committed source is restored byte-for-byte before cleanup.
Evidence: /tmp/tsr-99-index-mutation.log.

Follow-up tracing resolves the compatible-array cause: native propertiesRelatedTo
(relater.go:4240) requires every source property to exist as an actual property
when the target carries ObjectLiteral. An index signature does not satisfy that
requirement. The port lacks this target-side branch. A native probe confirms that
an indexed literal preserves the compatible b:number branch, while an explicitly
annotated {[x:string]:number} target reduces it. This property-relation rule is the
next tracked part of tsr-6.40; it is separate from the semantic index conjunction.

The final release workspace rerun also passes all 200 result blocks after the
shortcut-order adjustment and probe-label correction. Its log is
/tmp/tsr-99-index-workspace-final.log. The prior timing qualification is superseded
by this exact-source final run.
