# Lane `relate-report` (tsr-2zk.1) — relation error reporting and elaboration

Working notes for the parity box on epic `tsr-2zk`, lane issue `tsr-2zk.1`.
Case list: `docs/parity/lanes/relate-report.txt` (568 cases at `06f25e0`).
Upstream pinned at `vendor/typescript-go` @ `5b1047d`.

## 1. How the lane was bucketed

`crates/tsr-conformance/examples/relatelane.rs` lists every *missing* TS2322
line (or `CODE=<n>`) in the lane's cases with the
`report_assignability_failure` gate that decided it — `NEVER` (no
assignability report was asked at that position), `DECLINED` (the relation
did not answer `NotRelated`), `NOTREPORTABLE`, `OLUNION` — and the innermost
node kind at the position. `VERDICT=1` instead prints the lane's cases in
`diagverdictdump`'s format (a 7 s inner loop instead of the 3 min full dump).

Measured at `0d996e8` (before any change in this lane), 591 missing TS2322
lines:

| gate | lines |
|---|---|
| NEVER | 347 |
| DECLINED | 191 |
| NOTREPORTABLE | 32 |
| OLUNION | 16 |
| REPORTED (elsewhere) | 5 |

The NEVER bucket is dominated by elaboration positions: object-literal
property assignments (65), array-literal elements (~70), arrow-function
expression bodies (~48), JSX attributes (18). That confirms the brief's
hypothesis 1 *as a position* — but most of these are not missing
elaboration code. `elaborate_error` and its family exist in
`assignreport.rs`; the positions are NEVER because the **outer** check that
would hand the expression to `elaborate_error` is never made. The largest
families:

- **call arguments** whose parameter has no annotation, an overload set, or a
  generic signature (`destructuringParameterDeclaration1ES5`: 10 lines, every
  one an element of an array-literal argument to a function whose parameter
  type comes from an initializer). TS2345/TS2322-in-argument reporting lives
  in `call_arity.rs::check_argument_types`, which only handles one sole,
  non-generic, fully annotated signature. Not this lane's file; listed in the
  final report.
- **expression statements** (`x = y` assignments, 79 DECLINED + 47/32 NEVER),
  which split between relation completeness and property/element-access
  assignment targets that `check_assignment_operator` declines.

## 2. Ported: index-signature targets in `elaborateObjectLiteral`

`elaborateElement` (`relater.go:546`) reads the target member through
`getBestMatchIndexedAccessTypeOrUndefined` → `getIndexedAccessTypeOrUndefined`,
which falls back to the target's applicable index signature
(`getPropertyTypeForIndexType`) when the name is not a property. The port read
`get_type_of_property_of_type` only, so `var o: { [s: string]: number } = { p: "" }`
skipped the member, elaboration answered "nothing to say", and the outer
TS2322 was reported at `o` instead of at `p`. One MISSING plus one EXTRA per
case.

The fix asks `get_applicable_index_info` with the name's literal type
(`getLiteralTypeFromPropertyName`: a numeric-literal name is a number literal,
everything else a string literal), so numeric index signatures apply to
numeric names exactly as upstream. Computed names are still skipped (they were
before; `identifier_text` does not read them).

Converted: `compiler/contextualTypeAny`, `compiler/controlFlowForIndexSignatures`,
`compiler/objectLiteralIndexerErrors`.

Would be wrong if: a case reports TS2322 at an object-literal member against
an index signature that upstream reports at the outer node. None in the
corpus at this commit (zero-loss check empty).

## 3. Found, outside this lane's files

- **Qualified enum type references.** `let x: A.E = B.F.a` (both enums in
  namespaces) relates the source to a `Named { members: Some(<enum symbol>) }`
  OBJECT image for `A.E`, not to the enum's union of member literals, and the
  relater (correctly, for that image) answers `Related`. Unqualified
  `let x: E = F.a` reports TS2322 as upstream. The type-reference resolution
  for a qualified name naming an enum is `declared.rs` (type-refs box).
  Unlocks the enum families of `enumAssignmentCompat3` (12 lines) and
  `enumAssignmentCompat6` (6 lines).

## 4. Ported: the generic-mapped-target arm of `structuredTypeRelatedToWorker`

`relater.go:3593`. A source `S` against `{ [P in Q]: T }` / `{ [P in Q as R]: T }`
(a mapped type whose constraint or name type is still generic) had no arm, so
`U -> { [P in keyof U]: U[keyof U] }` fell to the source type parameter's
`unknown` constraint and answered a confident `NotRelated` — a false TS2322
(`compiler/mappedTypeParameterConstraint`). `Relater::generic_mapped_target_related_to`
mirrors the arm: the `{ [P in Q]: S[P] }` identity shortcut, then (for a
non-generic-mapped source) `Q`/`R` related to `keyof S` (`?` targets need a
non-empty key intersection), then the `Obj[P]` fast path or `S[P] -> T`.

Deviations, each conservative (they leave a pair undecided rather than
deciding it differently):

- `keyof S` is `resolved_keyof_type`, which includes index-signature keys where
  upstream asks `IndexFlagsNoIndexSignatures`; a source with index signatures
  (or an unresolved index table) skips the arm.
- An `Unknown` key relation or an indexed access the port cannot build answers
  `Unknown`; upstream's arm would fall through to the remaining arms with a
  definite answer. `V -> { [P in keyof W]: W[P] }` therefore stays undecided
  (no report) where upstream reports TS2322.
- `isGenericMappedType` is the existing over-approximating
  `is_generic_mapped_target`, used for both sides.

Would be wrong if: a pair decided `Related` by the arm is reported by upstream.
Zero-loss check at this commit is empty on both suites.

## 5. Ported: widened object-literal types have a property table for the relation reporters

`var a = { x: 1, y: 2 }; a = { x: 1 }` is TS2741 upstream (`reportUnmatchedProperty`)
and `a = { x: 1, z: 3 }` is TS2353 (`hasExcessProperties`). The port reported
TS2322 for both, because `member_completeness` certified no table for a type
whose declaration is an `ObjectLiteralExpression` (the widened, regular
literal type — the fresh one already reads its captured list), so
`missing_required_property` and `check_excess_properties` declined.

`Checker::relation_property_table` / `relation_members_are_complete` add the
literal's own written member list (no spreads, no computed names, not JS) and
are read by **this lane's reporters only**. Measured alternatives:

| where the object-literal table is admitted | diagnostics | losses |
|---|---|---|
| `declared_property_table` **and** `declared_members_are_complete` | +8 | 1 (`nonPrimitiveAndEmptyObject`) |
| `declared_property_table` only | +1 | 0 |
| lane reporters only (chosen) | +1 | 0 |

The loss in the first row is TS2339 (`crate::nonexistent_property`, not this
lane's file) on `fooProps.barProp` where `fooProps: (BarProps & object) | {}`:
upstream's `createUnionOrIntersectionProperty` gives an object-literal
constituent that lacks the property an `undefined` member instead of failing
the lookup. Once that union rule is ported, admitting object literals in
`declared_members_are_complete` is worth the other seven cases
(`checkingObjectWithThisInNamePositionNoCrash`, `importWithTrailingSlash`,
`lambdaParamTypes`, `requireOfJsonFileInJsFile`,
`requireOfJsonFileWithEmptyObjectWithErrors`, `thisInObjectLiterals`,
`typeSatisfaction_optionalMemberConformance`). The middle row equals the chosen
one, so the narrower change was kept.

Converted: `compiler/typeMatch2`.

## 6. Ported: TS2561 in the excess-property check

`hasExcessProperties` (`relater.go:2714`) reports TS2561 ("Did you mean to
write …?") when the excess name is an identifier with a spelling suggestion
among the target's properties (`getSuggestionForNonexistentProperty`), else
TS2353. The port *returned* on a suggestion — leaving the outer TS2322 to be
reported at the declaration instead. Now it reports TS2561 at the name; a
string-literal name never gets a suggestion, as upstream. The suggestion is
`crate::check::spelling_suggestion` over the certified table's names (only the
code and position are compared by the suite).

Converted: `compiler/spellingSuggestionLeadingUnderscores01`.
`objectLiteralExcessProperties` gains its TS2561 lines but still needs union
and intersection excess checks (`findMatchingDiscriminantType`,
`isKnownProperty` over intersections).

## 7. Ported: `object` against a target requiring a property

`structuredTypeRelatedTo` relates the non-primitive `object` through its
apparent type, the empty object type; `propertiesRelatedTo` then fails on a
required target property the empty object (Object's members included) cannot
supply. The port answered `Unknown` (the empty object has no member table
here). The arm in `is_related_to_with_excess` takes only that definite
negative, from `relation_property_table(target)`; every other `object` pair
keeps its existing path. A lib target such as `Date` has no certified table
and stays undecided.

Converted: `conformance/nonPrimitiveAssignError`.

## 8. Ported: `mappedTypeRelatedTo`'s modifier gate, and lazily captured mapped types

Two generic mapped types relate only if
`getCombinedMappedTypeOptionality(source) <= getCombinedMappedTypeOptionality(target)`
(`relater.go:3972`, reached from the default branch at `relater.go:3805` after
the generic-mapped-target arm fails), so `Partial<T> -> Readonly<T>` and
`Readonly<Partial<T>> -> Readonly<T>` are TS2322. `Relater::mapped_modifiers_reject`
ports that gate as a definite negative; the remainder of `mappedTypeRelatedTo`
(constraint `T' -> S'` and template comparison under the parameter mapper)
is **not** ported — pairs passing the gate keep their previous answer.
Narrowed to mapped types with a type-variable constraint and no `as` clause on
both sides, since `is_generic_mapped_target` over-approximates genericity.
`combined_mapped_optionality` follows `modifiers_source` (the homomorphic
operand) as upstream follows `getModifiersTypeFromMappedType`.

A reference such as `Partial<T>` reaches the relater as a member-less `Named`
image whose mapped info is captured on first use (`ensure_mapped_type_info`),
so the structural gate never routed it to `structured_type_related_to_worker`
and both mapped arms (this one and §4's) were unreachable for it. They now
also run just before the undecided fallthrough of `is_related_to_with_excess`,
after ensuring the info.

Converted: `conformance/mappedTypes5`. `mappedTypes6` and
`mappedTypeRelationships` need the unported remainder (template comparison,
type-parameter targets, and the default branch's "non-mapped source against a
generic mapped target is false").

## 9. Ported: `elaborateDidYouMeanToCallOrConstruct`

`elaborateError` (`relater.go:440`) first asks, for construct then call
signatures of the source, whether some signature's return type (not
`any`/`never`) is related to the target; if so and the whole pair fails, the
failure is reported **at the expression** (`x = f` reports at `f`), with
"Did you mean to call this expression?" as related information. The port's
`elaborate_error` lacked the arm, so these TS2322s landed at the assignment
target or declaration name (one MISSING + one EXTRA each).

`report_assignability_failure_with(at, None, …)` is the unelaborated report
the arm needs (upstream's `checkTypeRelatedToEx` with the expression as error
node); it keeps every decline of `report_assignability_failure`, so a pair the
relation does not reject falls through to the remaining elaboration arms as
upstream's related result would.

Converted: `compiler/avoidListingPropertiesForTypesWithOnlyCallOrConstructSignatures`,
`compiler/functionSignatureAssignmentCompat1`, `compiler/optionalParamAssignmentCompat`,
`compiler/staticMemberOfClassAndPublicMemberOfAnotherClassAssignment`,
`compiler/typeMatch1`, `conformance/invalidAssignmentsToVoid`,
`conformance/invalidVoidValues`.

## 10. Refused for now: primitive source against an index-signature target

`is_related_to_with_excess` answers `Unknown` for a primitive source (through
its apparent type) against a target with index signatures under the
assignable relation, taking the `propertiesRelatedTo` rejection only for the
subtype relations. Upstream's `structuredTypeRelatedTo` runs
`propertiesRelatedTo` before the index-signature comparison under **every**
relation, so `number -> string[]` (no `length`, …) is a definite negative.

Lifting the relation restriction measured **+12 diagnostics cases** in this
lane (the eight `assignmentCompatability16/18/20/22/29/30/31/32`, plus
`arraySigChecking`, `typeParameterConstrainedToOuterTypeParameter`,
`enumAssignability`, `recursiveConditionalEvaluationNonInfinite`) and **+16
`checker_types` lines**, against **two losses**, both downstream of a
now-correct `NotRelated` in files this lane does not own:

- `compiler/couldNotSelectGenericOverload` (diagnostics): `makeArray2(1, "")`
  against `(items: any[])` gains a TS2345 because
  `call_arity.rs::check_call_arity` runs `check_argument_types` *before* the
  arity test; upstream's `chooseOverload` only checks argument types for a
  candidate that passed `hasCorrectArity`.
- `compiler/destructuringTuple:0:13` (types): `number -> ConcatArray<never>` now
  fails both `concat` overloads, as upstream (TS2769), and the port's overload
  fallback in `calls.rs` then types `reduce`'s result differently from
  upstream's.

Kept out until those two are fixed by their owners; the integrator has the
one-hunk change (delete `matches!(self.relation, Relation::Subtype |
Relation::StrictSubtype) &&` in the primitive-apparent arm).

## 11. Ported (narrow slice): `getBestMatchingType` for an object literal against `Object | primitives`

`report_assignability_failure` declined every object literal against a union
target (the discriminant/excess machinery is unported), so
`function foo(): Stuff | string { return { b: () => "hello", … } }` reported
nothing where upstream elaborates each member against `Stuff`
(`elaborateObjectLiteral` → `getBestMatchIndexedAccessTypeOrUndefined` →
`getBestMatchingType`). `best_matching_object_constituent` answers only where
that choice is certain without the unported parts: exactly one non-primitive
constituent, a plain object type that is not array-like, sharing a property
name with the literal. In that domain `findMatchingDiscriminantType` can only
return that constituent or nil, the type-reference and invokable steps do not
apply to a signature-less literal, `findBestTypeForObjectLiteral` needs an
array-like constituent, and `findMostOverlappyType` picks it on any key
overlap. The literal is elaborated against it only when the whole relation is
`NotRelated`; otherwise the old decline stands.

Converted: `compiler/errorOnUnionVsObjectShouldDeeplyDisambiguate`. Its
sibling `…Disambiguate2` (`Stuff | Date`, two object constituents) needs the
full `findMostOverlappyType` key-overlap count and the discriminant step.

## 12. Ported: `elaborateArrayLiteral` against non-array object targets

`elaborateArrayLiteral` (`relater.go:522`) reads each element's target through
`getBestMatchIndexedAccessTypeOrUndefined(source, target, i)`, which for any
object target is a property named `i` or the applicable index signature. The
port elaborated only tuple and array targets, so `var x3: I = [new Date(), 1]`
with `interface I { [x: number]: Date }` reported at `x3` rather than at `1`.
A non-array, non-tuple target now resolves each element through the property
or `get_applicable_index_info` with the index's number-literal type; an index
with neither is skipped, as upstream skips a `nil` target member. Variadic
tuple and union targets keep their declines.

Converted: `compiler/contextualTypingOfArrayLiterals1`, `conformance/arrayLiterals`.

## 13. State at the end of the first box session, and what is outside this lane

Lane diagnostics at `919ac31`: 25 RIGHT + 1 EMPTY_RIGHT of 568 (was 8 + 0
at `0d996e8`); whole-suite `diagnostics` 3444/5488 (was 3427). Missing TS2322
lines in the lane by gate: NEVER 326, DECLINED 185, NOTREPORTABLE 32,
OLUNION 16.

Needed outside this lane's files (each verified on a minimized probe):

- `call_arity.rs::check_call_arity` / `check_argument_types` — TS2345 and
  argument-position elaboration are checked only for one sole, non-generic,
  fully annotated signature, and *before* the arity test. Parameters typed
  from initializers or binding patterns (`destructuringParameterDeclaration1ES5`,
  10 lines), overload sets (`functionOverloads`) and method calls on generic
  instances (`genericOfACloduleType2`) are never checked; most of the lane's
  531 missing TS2345 lines sit here. Reordering arity before argument types
  also unblocks §10.
- `calls.rs` overload fallback — §10's `destructuringTuple:0:13`.
- `declared.rs` — qualified enum type references (§3).
- `nonexistent_property.rs` — union property lookup over object-literal
  constituents (§5).
- binder/symbol merging — a script file's `interface Number { … }`
  augmentation is not merged into lib's `Number` for `declare var a: Number`
  (`assignFromNumberInterface2`, `assignFromBooleanInterface2`: false TS2322 on
  `b = a`).

## 14. Round 2: TS2352 is checked in files with parse errors

`check_assertion_overlap` returned early when the file had parse errors.
`checkAssertionDeferred` (`checker.go:12317`) has no such gate: upstream
compares the assertion's operand and type in any file it checks, and the
corpus baselines carry TS2352 next to TS1005 (`typeAssertions`: four
class-to-class conversions in a file whose `<numOrStr is string>` lines do not
parse). The gate was a decline with no upstream counterpart; removing it is
narrowing toward upstream. The JS-file and ambient declines stay (JS type
assertions are JSDoc casts, a different path).

Measured at `15f1743`: TS2352 matched lines 88 → 92, extra lines unchanged
(10), no case changes verdict (`typeAssertions` still misses TS2558/TS2693/
TS2322 owned elsewhere). Both zero-loss checks empty.

## 15. Round 2: comparability between type parameters

`structuredTypeRelatedToWorker`'s type-parameter target arm (`relater.go:3423`)
carves comparability out of the source-constraint rule: under
`comparableRelation`, a type-parameter source against a type-parameter target
relates only through a source constraint that itself mentions a type
parameter (`someType(constraint, isTypeParameter)`), and is otherwise
**false** — "forbid comparing a type parameter with another type parameter
unless one extends the other". The port had no arm, so `U` against `T` fell to
the source's constraint (`unknown`, or `Date` for `<T extends Date, U extends
Date>`) and answered `Unknown`; TS2352 (`<T>u`), TS2367 (`t === u`) and TS2365
(`t < u`) were never reported.

`Relater::comparable_type_parameter_pair` ports the carve-out for declared
parameters on both sides (the synthetic polymorphic `this` keeps its existing
path). A source with no written constraint is not comparable; a written
constraint the port cannot read leaves the pair `Unknown` rather than guessing.
The arm sits after the union/intersection decomposition and before the
source-variable arms, as upstream's switch does.

Converted: `compiler/genericTypeAssertions6`,
`conformance/comparisonOperatorWithNoRelationshipTypeParameter`,
`conformance/comparisonOperatorWithTypeParameter` (requested by the
flow/operators lanes). Lines at `15f1743` + §14: TS2352 92 → 95, TS2365
355 → 375, TS2367 355 → 375, no extra lines. Zero-loss checks empty.

Would be wrong if: a pair of type parameters upstream relates under
comparability without one constraining the other. The rejected alternative —
relating through the constraint as assignability does — is what produced the
silent `Unknown`.

## 16. Round 2: a tuple target against a plain object source

`propertiesRelatedTo` (`relater.go:4100`) takes its arity arm only for an
array or tuple source; any other object source meets the tuple target's
properties one by one — its leading fixed elements (`"0"`, `"1"`, …, optional
where the element is), `length` (the literal union of the possible lengths for
a plain tuple), and every member inherited from `Array`/`ReadonlyArray`. The
port had no arm: `StrNum` (an interface extending `Array<string | number>`)
and `{ 0: string; 1: number; length: 2 }` against `[number, number, number]`
fell through every gate to `Unknown`.

`Relater::non_array_source_tuple_target` takes only the definite failures of
that walk: a required name absent from the source's complete name table and
not supplied by the Object augmentation (`getPropertyOfType`), a fixed
element's property type that is not related, or a `length` that is not
related. Method types are never compared, so the arm never answers `Related`;
a pair that passes stays `Unknown`. `Checker::tuple_target_properties` is the
shared property list, in upstream's order; symbol-named array members
(`[Symbol.iterator]`) are left out of it.

`unmatched_property_report` reads the same list for a tuple target and a
non-array source, so one missing element is TS2741 (`Property '2' is missing
in type 'StrNum'…`) and several are left to TS2322 —
`tryElaborateArrayLikeErrors` elaborates a tuple target only for an array
source.

Converted: `compiler/assigningFunctionToTupleIssuesError`,
`conformance/arityAndOrderCompatibility01`, `conformance/iterableArrayPattern10`,
`conformance/iterableArrayPattern13`; +11 `checker_types` lines. Two new
TS2345 lines in the already-wrong `destructuringParameterDeclaration3ES5/ES6`
(`a10([1, 2, 3, false, true])`: `3` against `[[any]]` is now decided, and the
argument path did not elaborate) are fixed by §17.

Would be wrong if: a plain object source upstream relates to a tuple target
while lacking one of the listed names. Zero-loss checks empty.

## 17. Round 2: argument failures are elaborated

`getSignatureApplicabilityError` checks each argument with
`checkTypeRelatedToAndOptionallyElaborate(argType, paramType, relation,
arg, arg, headMessage)`: a pair that is not related is first handed to
`elaborateError`, and the TS2345 head is issued only when the elaboration
says nothing. `report_argument_failure` (the TS2345 reporter every argument
check in `call_arity.rs`/`calls.rs` calls) went straight to TS2345, so an
array-literal or arrow argument reported at the argument instead of at the
offending element or returned expression. It now elaborates a `NotRelated`
pair after the port's reportability gate (kept in front, as for TS2322).

Two pieces the elaboration needed, ported with it:

- **The head message reaches the did-you-mean-to-call arm.**
  `elaborateDidYouMeanToCallOrConstruct` reports with
  `checkTypeRelatedTo(…, headMessage, …)`, so in an argument it is TS2345 at
  the argument, not TS2322 (`elaborationForPossiblyCallableTypeStillReferencesArgumentAtTopLevel`,
  `parser536727` were losses without this). `elaborate_error_with` carries
  the head; member elaborations still report without one, as upstream.
- **Variadic tuple targets in `elaborateArrayLiteral`.**
  `generateLimitedTupleElements` skips an index the tuple-like target has no
  property for; a variadic tuple's properties are its leading fixed elements,
  so `[1, 2, 3, false, true]` against `[any, any, [[any]], ...any[]]`
  elaborates `3` against `[[any]]` (TS2322 at the element). The port declined
  every variadic target.
- **`NoInfer<T>` in the missing-property messages.** `getNormalizedType`
  unwraps the substitution before the relation, so `() => new Animal()`
  against `() => NoInfer<Dog>` is TS2741 naming `Dog`. The missing-property
  helpers now read the unwrapped target (`noInfer`'s line 47 was a new TS2322
  extra without this).

Converted: `compiler/assignmentCompatBug5`, `compiler/contextualTyping30`,
`compiler/contextualTyping33`, `compiler/mapUpsert`,
`compiler/overloadResolutionOverCTLambda`,
`compiler/trailingCommaInHeterogenousArrayLiteral1`,
`conformance/destructuringParameterDeclaration3ES5`,
`conformance/destructuringParameterDeclaration3ES6`,
`conformance/destructuringParameterDeclaration4`,
`conformance/destructuringParameterProperties2`. TS2345 extra lines 64 → 39.
Three lines change from a wrong TS2345 at the argument to a wrong TS2322 at
the member (`es2020IntlAPIs` 32/33, `contextualTypeBasedOnIntersectionWithAnyInTheMix4`
43): the argument type reaching the reporter is widened (`{ type: string }`
for `{ type: 'region' }`) — the argument's contextual literal type is the
calls lane's (`checkExpressionWithContextualType`).

**Merge note (same session).** `main` landed the same argument elaboration
independently (`45236ee`, relate-4: `report_argument_failure` →
`elaborate_error(…, Some(TS2345 head))`). The merge keeps `main`'s
`assignreport.rs`, which carries the head as a message rather than this
section's boolean; the variadic-tuple and `NoInfer` pieces, which `main` does
not have, are re-applied on top of it in §18.

## 18. Round 2: variadic tuple targets in `elaborateArrayLiteral`, and `NoInfer` in the missing-property messages

Re-applied on `main`'s `assignreport.rs` after the §17 merge (the two pieces
`main`'s relate-4 port does not have):

- `generateLimitedTupleElements` skips an index the tuple-like target has no
  property for; a variadic tuple's properties are its leading fixed elements.
  `elaborate_array_literal` declined every variadic target, so
  `a10([1, 2, 3, false, true])` against `[any, any, [[any]], ...any[]]`
  reported TS2345 at the argument instead of TS2322 at `3`.
- `getNormalizedType` (`relater.go:2619`) unwraps `NoInfer<T>` before the
  relation reports; the missing-property helpers now read the unwrapped
  target in both the TS2322 and the TS2345 reporter (`noInfer`: TS2741 naming
  `Dog` for `() => new Animal()` against `() => NoInfer<Dog>`).

Converted at `2114e6a`: `conformance/destructuringParameterDeclaration3ES5`,
`conformance/destructuringParameterDeclaration3ES6`; TS2741 matched
149 → 150. Zero-loss checks empty.

## 19. Round 2: the JSX-attributes arm of `elaborateError` (for the jsx lane)

`elaborateError`'s `KindJsxAttributes` case (`relater.go:470`) calls
`elaborateJsxComponents` (`jsx.go:295`), and its `KindJsxExpression` case
unwraps like a parenthesized expression. `Checker::elaborate_jsx_attributes`
ports the attribute half: each non-spread attribute whose name is not
hyphenated (`isHyphenatedJsxName`) is an element, with the attribute name as
error node and its initializer (none for `<C flag />`) as the expression to
elaborate into; the target member is the props type's property or applicable
index signature, and `elaborate_element` reports TS2322/TS2741/excess at the
attribute exactly as for an object-literal member.

The JSX checker is expected to call
`report_assignability_failure(tag_name, attributes_node, attributes_type,
props_type)` — upstream's `checkTypeRelatedToAndOptionallyElaborate(…,
node.tagName, node.attributes, …)` — and gets the attribute-level report for
free. No caller exists at this commit, so the corpus is unchanged (zero-loss
checks empty, no line moves); the jsx lane measured ~45 cases waiting on it.

Not ported: the children half (`getJsxElementChildrenPropertyName`, the
TS2745/TS2746 arity messages, per-child elaboration and the TS2747 text-child
diagnostic) and union props targets (the object-literal union decline
applies). A failure only in `children` still reports at the tag name.

## 20. Held at the end of round 2, with the number that held each

Each was measured zero-loss except for the named case. The loss comes from a
file another lane owns, so the change is held until that file is fixed (see
the box's final report).

- **Weak-type check (TS2559, `isWeakType`/`hasCommonProperties`,
  `relater.go:2676`).** Gains +3 cases (`assignmentCompatWithObjectMembersOptionality2`,
  `intersectionAsWeakTypeSource`, `nestedFreshLiteral`), TS2559 matched
  0 → 17 lines. It also loses 2 cases: `overloadBindingAcrossDeclarationBoundaries`
  and `…2`. The relation is right in both (`Opt1 -> Opt3` is not related),
  and that exposes an overload-order bug. `signatures.rs::reorder_candidates`
  gives each `CallSignatureDeclaration` its own symbol, so call signatures
  merged from two interface declarations are not regrouped the way
  `reorderCandidates` groups them, and `a({})` picks `Opt1` rather than
  `Opt3`. A one-line change gives call signatures the owner that construct
  signatures already use. With that change the combined run has zero losses,
  +3 cases and +6 type lines. Three more TS2420 lines in `subtypingWithObjectMembers5`
  would also need `heritage_conformance.rs` to emit TS2559 when the weak
  check is what fails.
- **Indexed-access target constraint arm (`relater.go:3456`, `S -> T[K]` via
  the write constraint of `T[K]`).** Gains +1 case (`nonPrimitiveConstraintOfIndexAccessType`,
  10 lines). It loses `contextuallyTypedSymbolNamedProperties`:
  `mapped.rs::generic_mapped_contextual_property_type` mints the string
  literal `"[A]"` for a symbol-keyed property name, and the relation now
  relates it. It also loses `correlatedUnions`: `base_constraint_of_type(Funcs[K])`
  answers the simplified `Func<K>`, where upstream's `getConstraintOfIndexedAccess`
  substitutes the index constraint and gets `Funcs["a" | "b"]`.
- **§10's primitive/index-signature change** landed on `main` from the
  relate-3 stream (`bfb9ec2`).

Lane state at `d7d1d2b`: whole-suite `diagnostics` 3935/5488 and
`checker_types` 7609/9538.

## 21. Round 3: the weak-type check is an arm of the relation

`isRelatedToWorker`'s common-property check (`relater.go:2675`) is now an
arm of `Relater::is_related_to_with_excess`, after the excess-property check
and before the structural walk, as upstream orders it. It applies under every
relation except comparability for a non-unit source, and not to a constituent
of a target intersection (`IntersectionStateTarget`; this port's
`check_excess == false`, the only caller passing it). The decision is the
existing `fails_common_property_check` (§20's reporter half), which answers
`false` for any uncertified table, so the arm adds only definite negatives.
The §20 overload-order loss went away with `main`'s `e292d1a` (calls lane:
`reorder_candidates` groups call signatures by owner), so the
calls-lane patch is no longer needed.

`issueMemberSpecificError`'s broad fallback reaches the same check:
`isRelatedToEx` reports the weak-type failure itself with no head message, so
the diagnostic is TS2559 at the class name, not TS2420
(`heritage_conformance.rs`).

**Cache.** `Checker::weak_type_answers` (key: the target `TypeId`; owner:
`assignreport.rs::is_weak_type`) memoizes decided answers. Upstream's
`isWeakType` reads resolved members, which are cached; the port's
`relation_property_table` rebuilds a `Vec<(String, bool)>` per call, and the
arm asks it of every object target at every depth. Measured without the cache:
generic-imports median CPU ratio 1.038 (21 samples) and 1.046 (41). With the
cache: 0.983 / 0.974 (41). Publication: only `Some` answers are stored. A
table moves from uncertified to certified at most once (lazy mapped info), and
a certified table does not change, so a stored answer cannot go stale. If a
zero-loss run showed a case whose verdict depends on the order in which types
are asked about, that assumption would be wrong.

Converted: `compiler/incorrectNumberOfTypeArgumentsDuringErrorReporting`,
`compiler/weakType`, `conformance/subtypingWithObjectMembers5`; +4
`checker_types` lines. Both zero-loss checks are empty.

**Not done this round.** The indexed-access target arm (§20) needs a write-flavoured
`getIndexedAccessTypeOrUndefined` (`AccessFlagsWriting`: an intersection over
a union index, and `NoIndexSignatures` when the object was replaced by its
constraint). `indexed.rs::resolved_indexed_access_type` answers the read
union, so that arm would accept pairs upstream rejects. The two §20 losses
(`mapped.rs` symbol key, `base_constraint_of_type(Funcs[K])`) are also still
open. `identity.rs` → `Relation::Identity` (tsr-2zk.32) is not cheap: it is a
630-line separate walk, and folding it in touches the hot relater.
