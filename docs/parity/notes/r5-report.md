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

## 3. Alias and generic targets (root cause C)

### What the census called "alias declines" were three other things

Round 4 recorded `generic`, `chained2`, `importClause_namespaceImport`,
`mappedTypeNotMistakenlyHomomorphic` and `consistentAliasVsNonAliasRecordBehavior`
as declines on alias targets. Tracing each with the reporter instrumented:

1. **`a!: string` read as optional** (`chained2`, `importClause_namespaceImport`).
   The type-only imports are not the cause: `class L { a!: string }; const l:
   L = {}` declined the same way. `declaration_is_optional_member`
   (`member_completeness.rs`, not owned) treats any `postfix_token` as `?`,
   but a property's postfix token can be the definite-assignment `!`
   (`ast.HasQuestionToken` is `?` only). Fix: compare the token kind. Measured
   alone on top of §2: zero losses on both dumps, positions 5037/1309 →
   5032/1304 (both TS2741 positions in `chained2` and
   `importClause_namespaceImport`, plus one in
   `didYouMeanElaborationsForExpressionsWhichCouldBeCalled`); no verdict flips
   because the remaining positions in those cases are other codes (TS2339,
   TS2749). Shipped as
   [r5-report-optional-postfix.diff](r5-report-optional-postfix.diff), not
   applied (file not owned).
2. **Generic class/interface references decline the table** (`generic`:
   `A<boolean>`). `declared_property_table_worker` declines every
   `type_reference_targets` entry that is not an identity map, and
   `declaration_property_names_are_readable` declines any declaration with
   type parameters. `resolveTypeReferenceMembers` instantiates the target's
   members: names and `SymbolFlagsOptional` survive, so the (name, optional)
   table of `A<boolean>` is the target symbol's. Lifting both gates (tuple and
   alias targets still declined) on top of 1: zero losses, `generic` and
   `errorsWithInvokablesInUnions01` WRONG → RIGHT — but it exposed one position
   regression, `deepExcessPropertyCheckingWhenTargetIsIntersection` 21:33
   TS2353 → 21:24 TS2322, which §4's port removes. With §4 applied, 1 + 2 add
   `generic` and `errorsWithInvokablesInUnions01` with no loss and no case
   worse. Shipped as
   [r5-report-generic-reference-table.diff](r5-report-generic-reference-table.diff)
   (contains 1), not applied.
3. **Mapped alias instances** (`mappedTypeNotMistakenlyHomomorphic`'s
   `Gen2<ABC.A>`, `consistentAliasVsNonAliasRecordBehavior`'s `Record`) are
   genuine table gaps for non-identity mapped types — the mapped-type lane's
   member resolution, not the reporter. Not investigated further.

## 4. hasExcessProperties before the structural relation, for every fresh literal

### Forcing constraint

`checkTypeRelatedToAndOptionallyElaborate` runs `elaborateError` first; when it
stays silent, `checkTypeRelatedToEx` relates the source, and for a fresh object
literal `isRelatedTo` runs `hasExcessProperties` (`relater.go:2714`) before
anything structural. Its report moves the error node to the excess member's
declaration (TS2353, or TS2561 with a suggestion). `report_relation_failure`
ran that order only for a *union* target (`union_object_literal_failure`);
against any other target a nested literal with a foreign key — reached through
`check_object_literal_member` or an elaboration — fell through to TS2322 at the
member name.

### What was ported

After `elaborate_error` and the reportability gate, a fresh object-literal
source (its symbol's single `ObjectLiteralExpression` declaration) is run
through the existing `excess_properties_verdict`; an `Excess` answer is
reported with `report_excess_property` and ends the report. `None` (undecided)
and the other answers fall through to the code that ran before, so no existing
report is withdrawn.

A literal with a spread is left out: `shouldCheckAsExcessProperty` reads each
*final* property's declaration parent, so a written key a later spread
overrides belongs to the spread and is not excess, and
`excess_properties_verdict`'s written-member walk cannot see that. The
`mapped_property_relations` guard test (`need = { foreign: 1, ...inherited }`
is TS2739 natively) caught the first version, which had no such limit.

### Measured

On top of §2: `excessPropertyCheckIntersectionWithIndexSignature`,
`excessPropertyChecksWithNestedIntersections`, `logicalOrExpressionIsContextuallyTyped`,
`namespaceImportTypeQuery`, `namespaceImportTypeQuery4`,
`nonPrimitiveUnionIntersection`, `objectLiteralShorthandPropertiesAssignmentError`,
`objectLiteralShorthandPropertiesAssignmentErrorFromMissingIdentifier`,
`propertyAccess` WRONG → RIGHT; identical with and without the spread
limit. Loss checks empty; positions 5037/1309 → 5014/1286.
Perf (41 samples, new/old): domain-model 0.977, generic-imports 1.015.

## 5. Excess properties through optional members and spreads (tsr-2zk.974)

Routed by the integrator from the r5-triage2322 census (bucket X2/B1).

### Forcing constraints

- **An optional member's `T | undefined`.** `hasExcessProperties` on a union
  target reads each constituent's member through `getTypeOfPropertyInTypes`
  (`relater.go:2796`). For `undefined`, `getApparentType` leaves the type as
  it is, and `getPropertyOfObjectType` plus the index lookup find nothing, so
  the constituent contributes `undefined`. TSR's
  `type_of_property_or_index_signature` asked `certified_property_names` of
  `undefined`, got no answer, and declined the whole excess check. Every
  literal nested under an optional property then fell back to a TS2322 head
  at the outer declaration (`nestedFreshLiteral`: native TS2561 three levels
  down). Fix: `undefined`, `null` and `void` answer nil there.
- **A spread literal.** §4 excluded spreads, because
  `excess_properties_verdict` walks written members and
  `shouldCheckAsExcessProperty` reads final properties.
  `spread_literal_excess_property` now walks the fresh type's final
  properties (`anonymous_properties`, in property order). It admits one only
  when its origin's value declaration has the literal as its parent, so a
  spread's keys and an overridden written key are never excess. The first
  property `isKnownProperty` rejects is reported at its declaration's name.
  Non-union targets only: the union arm's discriminant reduction reads
  written members.

### Measured (on top of §4)

- **Converted (+7):** `nestedFreshLiteral`, `objectLiteralFreshnessWithSpread`,
  `deepExcessPropertyCheckingWhenTargetIsIntersection`, `typeArgInference2`,
  `intlNumberFormatES5UseGrouping(target=es2015)`, `optionalBindingParameters2`,
  `optionalBindingParametersInOverloads2`.
- **Losses:** none on either dump. Positions went 5014/1286 → 5004/1287.
- **Accepted: two already-WRONG cases gained extra lines.** Elaboration now
  reaches members behind `| undefined`, which exposes two existing defects
  in files I don't own:
  - `es2020IntlAPIs`, +2 TS2322 (`new Intl.RelativeTimeFormat('en',
    { style: 'narrow' })`). TSR types the argument's `'narrow'` as `string`
    in a `new` call. This is the same false positive the case already showed
    twice at 32:62/33:78 (`DisplayNames`). The fix belongs in the
    contextual/calls lane; the reporter only stopped hiding it.
  - `importAttributes9`, +1 TS2322 at the nested `type`. `import_call.rs`
    hands its options object to `report_relation_failure` as a source node,
    which elaborates. Native's `checkTypeAssignableTo` (`checker.go:8291`)
    passes no expression. Passing `None`
    ([r5-report-import-call-no-elaboration.diff](r5-report-import-call-no-elaboration.diff))
    removes the extra and also produces the expected 11:25. On every
    import-named case: positions 1567/230 → 1566/229, no verdict change.

### Still open in X2/B1

These need machinery outside the reporter:
- computed names (`usingDeclarationsWithObjectLiterals1`'s `[Symbol.dispose]`
  makes `excess_properties_verdict` decline);
- generic conditional/mapped alias targets
  (`excessPropertyCheckIntersectionWithRecursiveType`);
- template-literal and symbol index signatures (`indexSignatures1`);
- `ThisType<any>` intersections and `Object.defineProperty`'s descriptor
  (`excessPropertyCheckWithEmptyObject`);
- the union-of-array elaboration in `objectLiteralExcessProperties`, which is
  bucket B3 (tsr-2zk.975).
Perf (21 samples, new/old): domain-model 0.959, generic-imports 0.997.
