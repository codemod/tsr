# Lane notes: r5-index4 (tsr-2zk.1012)

Judgment calls made by the `r5-index4` parity box, continuing
[r4-index](r4-index.md), [r4-index2](r4-index2.md) and
[r4-index3](r4-index3.md). Baseline frozen at `f4ae684` (the integration
head at dispatch). Native behaviour is read from the pinned submodule
(`vendor/typescript-go` @ `5b1047d`) and its reference baselines.

## 1. TS7017 for a dotted name `typeof globalThis` does not have

**Forcing constraint.** `checkPropertyAccessExpressionOrQualifiedName`
(`checker.go:11337-11345`) has an arm for a receiver whose symbol is
`globalThisSymbol`. When the name is not a member and no index info applies
(`typeof globalThis` has none), it reports TS2339 for a block-scoped global
and otherwise, under `noImplicitAny`, TS7017 at the name: `Element
implicitly has an 'any' type because type 'typeof globalThis' has no index
signature.` The answer is `anyType` either way. This port ported only the
TS2339 half (`check_nonexistent_property`) and the `any` answer
(`members.rs`, the `global_this_type` arm). `nonexistent_property.rs`
records that TS7017 was left out deliberately: an implicit-any report does
not belong in the nonexistent-property rule.

Witnesses: script-level `this.x` (`this` is `typeof globalThis` at the top of
a script and in an arrow there) and `globalThis.x`. The cases are
`emitCapturingThisInTupleDestructuring1` (3 lines), `parserConditionalExpression1`
(2), `jsxReactTestSuite` (3) and `globalThisUnknownNoImplicitAny` (2 of its
5 missing lines; the other 3 are element accesses).

**Where it lives.** `check_global_this_property_access`
(`index_access_reports.rs`) is called from `check.rs`'s per-node walk next to
the element-access arm. The arm uses the same membership test as the member
road for this receiver: a global that is not block-scoped and is a value
(`resolveAnonymousTypeMembers` keeps the non-block-scoped exports, and
`getPropertyOfType` filters them with `symbolIsValue`). It reads
`check_expression(receiver)`, which is already cached, and the globals
table. No side table, cache or traversal is added.

**`globalThis` and `undefined` are members.** The checker files
`globalThisSymbol` (`checker.go:964`) and `undefinedSymbol`
(`addUndefinedToGlobalsOrErrorOnRedeclaration`) in `globals` itself, so
`globalThis.globalThis` and `globalThis.undefined` resolve natively. This
binder's globals table has neither, and without the exception the first
measurement put a false TS7017 on `globalThisReadonlyProperties:1`.

**Not covered.** `typeofThis:57` (`typeof this.no` in an arrow inside a
namespace). The port answers `any` for `this` there (the namespace arm of
`check_this_expression`), so the receiver is never `typeof globalThis`.
That is a `this`-typing question, outside this lane.

**Falsifier.** A baseline TS7017 that is missing where the receiver is
`typeof globalThis`, or an extra one on a name that some global declares.

## 2. r4-index3's two `get_index_infos_of_type` gaps, measured (not shipped)

Both are measured against the baseline frozen at `f4ae684`, with §1 applied.

**A bare primitive at the top level.** Native `getIndexInfosOfType(t)` reads
`getReducedApparentType(t)`, so `string` answers `String`'s
`readonly [index: number]: string`. Variant measured: map a `PRIMITIVE`
receiver through `apparent_type` at the head of `get_index_infos_of_type`.
Types: 2 gained (`objectRest:0:24`, `:0:25`), 12 lost (`nestedTypeVariableInfersLiteral`
×6, `recursiveTypeReferences1` ×6). Diagnostics: 1 lost
(`nestedTypeVariableInfersLiteral`). Root cause of the loss: the inference
code (`inference.rs`, main's file) consumes this function for primitive
sources. With the change, `direct("z")` against `A | A[]` sees `String`'s
number index while inferring to `A[]`, and `A` becomes `string`
(`Record<string, string>`; native `Record<"z", string>`). Native's
inference reads the apparent type only after the naked-type-variable and
priority handling of `inferToMultipleTypes`, which this port's inference
does not mirror. The faithful change waits on that caller. Refused, with
these numbers.

**The union arm.** `union_index_infos` already maps *primitive* constituents
through `apparent_type`. Variant measured: map every constituent, as
`getReducedApparentType` does (type parameters to their constraint,
`object` to `{}`). Both dumps were verdict-for-verdict identical to the
baseline: 0 gained, 0 lost. Not shipped. A neutral change still costs a
lookup on every union receiver, and no case asks for it.

## 3. TS2536 (`checkIndexedAccessIndexType`), measured and refused

This port has no TS2536 reporter: 20 baseline lines in 10 cases are missing,
0 are present. Variant measured: the type-node check site
(`check_indexed_access_type_index_type`) took each deferred `Object[Index]`
the port mints (`deferred_indexed_access_types`) and related every index
constituent to `resolved_keyof_type(Object)`. A number index on the
apparent object admitted numeric keys. `relate_ternary` answering `Unknown`
declined the report.

Result: 1 line gained (`mappedTypeErrors2:13`), 14 false TS2536s added, and
5 diagnostics cases lost (`conditionalTypeVarianceBigArrayConstraintsPerformance`,
`neverAsDiscriminantType` ×2 configurations, `stringMappingReduction`,
`templateLiteralTypes6`). The false reports all come from the relater
answering a confident `NotRelated` where native proves the key assignable
to `keyof`:

- template-literal and string-mapping keys (`templateLiteralTypes5/6`,
  `stringMappingReduction`);
- `keyof (T & {})`-style operands (`unknownControlFlow:420`);
- deep conditional and mapped keys (`ramdaToolsNoInfinite2`,
  `mappedTypeInferenceFromApparentType`);
- a private member of a generic constraint
  (`indexedAccessPrivateMemberOfGenericConstraint`, native TS4105).

The reporter shape is correct. Its precondition is a relater that does not
answer `NotRelated` on generic-key-to-`keyof` pairs it cannot prove, and
that is `relater.rs` (r5-relater4). Falsifier for re-trying it: rerun this
variant after the relater's generic `keyof` arms land. Zero extra TS2536 on
the diagnostics dump is the bar.

## 4. Wide binary/octal/hex literals are `Infinity`, not their source text (diff)

**Forcing constraint.** The scanner keeps a radix literal's token value
(`"0b" + digits`). `jsnum.FromString` reads it through `tryParseInt`
(`internal/jsnum/string.go`). When the digits overflow `int64`, that falls
back to `big.Int.SetString(s, 0)` then `Float64()`: round to nearest, ties to
even, `+Inf` past the `f64` range. This port parses with
`u128::from_str_radix`. A wider literal fails, so `tsr_core::jsnum::numeric_value`
answers `NaN` and `printing::normalise_number` keeps the source spelling. The
1000-digit binary literal in `binaryIntegerLiteral*` is native's `Infinity`.
The port printed its digits as both the member name and the literal type.
With the member named wrongly, `obj1["0b11010"]` found no certified
receiver and its TS7053 was missing.

**The change.** `wide_radix_value` (`tsr-core/src/jsnum.rs`) rounds a
power-of-two-radix digit string to `f64` exactly as `big.Float64` does. It
keeps the top 53 significant bits, rounds on the guard bit with a sticky
bit (ties to even), and answers `Infinity` past exponent 1023. Both
`numeric_value` and `normalise_number` use it only when `u128` overflows,
so every literal that parsed before keeps its value.

Neither file is owned by this lane. The change is in
[r5-index4-wide-radix-literal.diff](r5-index4-wide-radix-literal.diff).
Measured on top of §1 against the frozen baseline:

- **Diagnostics:** +4 cases (`binaryIntegerLiteral(target=es2015)`,
  `binaryIntegerLiteralES6`, `octalIntegerLiteral(target=es2015)`,
  `octalIntegerLiteralES6`).
- **Types:** +78 RIGHT lines.
- **Losses:** none in either dump.
- **Checks:** `tsr-core` tests pass and `tsr-core` clippy is clean.

**Falsifier.** A radix literal wider than 128 bits whose baseline value is
not the correctly rounded `f64`.
