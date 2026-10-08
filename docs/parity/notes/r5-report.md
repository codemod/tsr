# r5-report: reportRelationError's missing-property head (tsr-2zk.961)

Lane `tsr-2zk.961`, continuing `tsr-2zk.956` (round 4's census,
[r4-report.md](r4-report.md)), with `tsr-2zk.918` and `tsr-2zk.1.2`.
Vendor `5b1047d`. Frozen baseline: branch head `ac56208f` — diagnostics dump
5070 RIGHT, 1521 WRONG, 5551 EMPTY_RIGHT, 96 EMPTY_WRONG.

## 1. The head swap needs `chainArgsMatch` (root cause A)

### Forcing constraint

`reportRelationError` (`relater.go:4751`) drops its own head (TS2322, TS2345)
only when the chain's next message is the missing-property message **and**
`chainArgsMatch` (`relater.go:4816`) finds that message naming the same
*printed* source and target as the head. The two pairs are different objects:

- the missing-property message names the pair `propertiesRelatedTo` saw.
  `isRelatedTo` normalized it first (`getNormalizedType`, `checker.go:27865`),
  whose reference arm replaces a class or interface that declares no members
  with its single base (`getSingleBaseForNonAugmentingSubtype`,
  `checker.go:28087`), iterated. A type-parameter source was moved to its
  constraint by `structuredTypeRelatedTo`'s type-variable arm
  (`relater.go:3665`) before the properties were compared;
- the head names the pair `reportErrorResults` (`relater.go:4705`) displays:
  the *original* side when it has an alias or a single base.

TSR's `missing_required_property` swapped the head whenever a required
property was absent. Five positions in four cases printed TS2741 where native
keeps TS2322 and puts the TS2741 line under it:

| Case | Display | Message names |
|---|---|---|
| `inheritance1` 40:1, 46:1 | `ImageBase`, `Image1` | `Control` (single base) |
| `classImplementsClass4` 16:1 | target `C2` | `A` (single base) |
| `privateNamesUnique-4` 6:7 | `A2` (interface extending a class) | `A1` |
| `fuzzy` 21:34 | `this` | `C` (constraint) |

### What was ported

`missing_property_chain` (`assignreport.rs`) computes the message's pair —
type-parameter source to `getConstraintOfType`, then each side through
`single_base_normalized` — and compares printed strings with the head's
display pair, as `chainArgsMatch` does. Equal: the message stands alone, as
before. Different: the caller's head is reported with the missing-property
message (naming the normalized pair) as its chain child. Both report paths
(`report_relation_failure`, `report_argument_failure`) and both property
sources (`missing_required_property`, `unmatched_property_report`) go through
it. TSR now prints, as native does:

```
error TS2322: Type 'ImageBase' is not assignable to type 'SelectableControl'.
  Property 'select' is missing in type 'Control' but required in type 'SelectableControl'.
```

`single_base_for_non_augmenting_subtype` mirrors native's gates:

- `ObjectFlagsReference`: every class; an interface only when it is generic or
  not `isThislessInterface` (`checker.go:17356`, ported as
  `is_thisless_interface`, reading the binder's `CONTAINS_THIS` fact). A
  thisless interface is not a reference, so `interface I2 extends I1 {}` over
  thisless interfaces keeps the swap — native does too.
- `getMembersOfSymbol(t.symbol)` empty: the binder files a declaration's type
  parameters in that table, so a generic target never qualifies and the base
  needs no instantiation through type arguments. A `this` argument
  (`getTypeWithThisArgument`) does not change the printed base, so it is not
  applied.
- a class whose `extends` expression is not an identifier or property access
  has no single base.

### Alternatives

- **Port the recursive `errorChain`** so the relater itself produces the
  chain. The faithful end state, but the relater's error state is not ported
  (`relate_with_signature_diagnostic` carries a single signature message), and
  it lives in `relater.rs`, which this lane does not own. This port answers
  the same question at the one place the reporter needs it; it would be
  replaced, not extended, once the relater reports chains.
- **Decline (keep TS2322 without a child)**, as `unmatched_property_report`
  already does for single-base references. Right code, wrong text; the child
  costs one more printed type on the error path only.

### Accepted limits

- An interface that is a reference *only* through outer type parameters is
  read as thisless (no single base). Not seen in the corpus.
- A type-parameter source whose constraint this port cannot decide keeps the
  head with no child message.
- `unmatched_property_report` (`relater.rs`, not owned) still declines
  single-base and alias pairs outright; with this gate in place those declines
  could be narrowed (a `relater.rs` change, reported to the integrator).

### How we would know it is wrong

A case where TSR prints TS2322 + child but native swaps: the display and
normalized strings differ in TSR but not natively — e.g. a TSR members table
holding a symbol native's `getMembersOfSymbol` lacks, or the reverse.

### Measured

Converted `inheritance1`, `classImplementsClass4`, `fuzzy`,
`privateNamesUnique-4` (WRONG → RIGHT). Both loss checks empty; no WRONG case
gained a mismatch (missing/extra positions 5057/1329 → 5051/1323). Perf
(median child CPU, new/old): domain-model 1.020, generic-imports 1.002 at 41
samples (21-sample generic-imports read 1.056 and was re-run per protocol).

## 2. A JSX attributes source drops the outer head (root cause B, tsr-2zk.918)

### Forcing constraint

`reportErrorResults` (`relater.go:4722`) returns before `reportRelationError`
when the source carries `ObjectFlagsJsxAttributes` and the target is an
intersection holding `JSX.IntrinsicAttributes` or
`JSX.IntrinsicClassAttributes` (both `getJsxType`s must be non-error; the
generic `IntrinsicClassAttributes<T>`'s declared type is never itself a
constituent, so in practice the test is `IntrinsicAttributes`). The only
message left is the chain of the constituent `typeRelatedToEachType` failed
first — e.g. TS2741 against `IntrinsicAttributes` in
`tsxIntrinsicAttributeErrors`, against the props object elsewhere. TSR
reported TS2322 against the whole intersection.

TSR's types carry no `JsxAttributes` object flag (`jsx_component.rs`, the
hyphenated-name decline), so the reporter cannot see the fact. The JSX
caller states it instead: `check_jsx_attributes_assignable` now calls
`report_jsx_attributes_relation_failure` (`assignreport.rs`), whose source is
by construction the attributes type `createJsxAttributesTypeFromAttributesProperty`
built. (Native's spread of a *generic* object yields the spread type itself
or an intersection, neither flagged; TSR's attributes builder declines such a
spread before this point, via `spread_properties`.)

### What was ported

Each constituent is related in order, as `typeRelatedToEachType` does under
`IntersectionStateTarget`:

- **no excess-property check** — the source is read regular
  (`getRegularTypeOfObjectLiteral`); the caller already ran
  `hasExcessProperties` against the whole target;
- **no common-property check** — a weak constituent that shares no property
  with the source is counted related. That is exact, not a guess: a weak type
  (`isWeakType`) has no required property, no signature and no index, and
  the source has no property in common to compare, so the structural walk
  cannot fail.

The first `NotRelated` constituent is reported through
`report_relation_failure` with no head override, so the missing-property swap
of §1 applies to it. An undecided constituent declines; all constituents
relating means the failure came from the combined property pass
(`relater.go:3232`), whose chain this port does not build, and declines too.

### Alternatives

- **Add an `ObjectFlagsJsxAttributes` bit to TSR's types.** The faithful
  representation; it touches the type store and every attributes builder,
  none owned here, and the hyphenated-name decline needs the same bit in the
  relater (`isComparingJsxAttributes`). Worth doing when that lane runs; this
  entry point then becomes a check of the bit.
- **Relate per constituent with `relate_ternary` on the fresh source.** Wrong:
  the top-level excess and weak checks would reject `IntrinsicAttributes`
  itself for any attribute it does not declare.

### Measured

Converted `tsxIntrinsicAttributeErrors`, `tsxSpreadAttributesResolution2`,
`tsxSpreadAttributesResolution16`, `tsxReactComponentWithDefaultTypeParameter3`,
`checkJsxChildrenProperty2`, `checkJsxChildrenProperty5`. `tsxUnionElementType3`
and `tsxUnionElementType6` each convert one position; the other is an element
with a hyphenated attribute (`data-extra`), which the caller still declines
for want of the JSX-attributes relater flag. Loss checks empty; missing/extra
positions 5051/1323 → 5037/1309. Perf (21 samples, new/old): domain-model
1.027, generic-imports 1.027.
