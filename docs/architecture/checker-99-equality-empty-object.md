# Equality narrowing through filtered unknown and loose coercion

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline `8f8f4e1a`: 458,022/478,855 matching assertions (95.65%),
7,059/9,538 complete cases.
Unit: tsr-djf / tsr-6.34 flow narrowing and adjusted-fact consumers.

## Native mechanism

`narrowTypeByEquality` (`internal/checker/flow.go:557-610`) has two equality
rules that the existing comparable filter did not fully carry.

First, a strict assume-true comparison can narrow not only `unknown`, but any
type for which `someType(t, IsEmptyAnonymousObjectType)` is true. This is
observable after adjusted facts expand and filter `unknown`:

```ts
function SendBlob(encoding: unknown) {
    if (encoding !== undefined && encoding !== "utf8") throw 0;
    encoding; // "utf8" | undefined
}
```

The first inequality leaves `{} | null` on its true path. For the false path of
the second inequality, native sees the empty-object constituent and answers the
primitive comparand itself. The branch join then yields `"utf8" | undefined`.
The port instead declined with `{} | null`; joining that with `undefined`
recombined toward `unknown`. `unknownControlFlow.types` records this #50706
repro and the parallel `42` equality branch.

Second, loose equality's filter is
`areTypesComparable(t, valueType) || isCoercibleUnderDoubleEquals(t,
valueType)`. The latter is an exact flag rule (`flow.go:1928`): a source
number, string, or boolean literal is coercible when the target is number,
string, or boolean. It is applied to each source constituent against the whole
comparand type. The prior port admitted only a union wholly inside one literal
domain, which declined every broad primitive comparison in
`narrowByEquality.types` and optional-chain comparisons in three more cases.

## Port boundary

`narrow_type_by_equality` now:

1. detects an empty anonymous object in `t` itself or a union constituent using
   the existing semantic `is_empty_anonymous_object_type` predicate;
2. admits an empty anonymous comparand alongside native's primitive and
   non-primitive comparands; and
3. applies native's loose-coercion flag test constituent-wise before the
   comparability relation.

No printed spellings, global empty-object identity, type creation, or
intersection reduction changed. The template/intersection worker's sequencing
and redundant-template extraction remain disjoint.

The rejected alternative was to special-case the #50706 branch join. That
would fix one spelling while leaving `doSomething2`, `narrowByEquality`,
`controlFlowOptionalChain`, and `discriminatedUnionTypes1` wrong. The native
predicate and coercion rule explain all converted rows through one equality
worker.

## Controls and measurement

Focused tests distinguish:

- equal syntax from the false branch of unequal syntax (both normalize to
  assume-true equality);
- nullable comparands (`undefined`, `null`) from primitive literals;
- broad number/string/boolean loose comparands from unit comparands; and
- strict literal equality from loose coercion.

The primary regression fails under a source-revert mutation, yielding
`{} | undefined` instead of `"utf8" | undefined`. Pinned native controls are
`unknownControlFlow.types` (#50706 and `42`) and `narrowByEquality.types` (all
seven loose-equality rows).

Full scorepair in the same checkout against a freshly accepted baseline at
`8f8f4e1a` reports 458,038 RIGHT, 2,445 GAP, and 13,761 WRONG: **+16
WRONG-to-RIGHT, zero RIGHT losses, zero GAP-to-WRONG, and zero changed-WRONG
payloads**. Attribution: `narrowByEquality` 7, `controlFlowOptionalChain` 6,
`unknownControlFlow` 2, and `discriminatedUnionTypes1` 1.

## Remaining risk

Named-union origin preservation and general subtype reduction remain separate.
The equality worker still relies on the relater for non-coercible structural
comparisons; an undecidable relation continues to decline the whole narrowing,
which is the existing conservative boundary.
