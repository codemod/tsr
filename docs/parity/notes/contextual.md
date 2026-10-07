# Contextual lane notes (`tsr-2zk.14`)

Judgment calls made while porting contextual typing of functions and object
literals. Upstream anchors are to `vendor/typescript-go` @ `5b1047d`.

## 1. A dynamic computed name goes straight to the index fallback

`getContextualTypeForObjectLiteralElement` (`checker.go:29920`) has three
steps: a bindable name looks up the property (whose own lookup falls back to an
index signature); a dynamic computed name whose type is usable as a property
name looks that property up; and any named element then falls back to
`mapTypeEx(t, findApplicableIndexInfo(getIndexInfosOfStructuredType(t),
nameType).valueType, noReductions)` (`:29946-29955`).

`contextual_type_for_object_literal_named_element` covered the first step and
answered `None` as soon as a computed name was not late-bound (`["" + 1]`,
`[+"foo"]`). Such a name is never usable as a property name — that is what
"not late-bound" means in `late_bound_symbol_member_name` — so step two cannot
answer either, and the port now goes directly to step three with the regular
type of the name expression (`getLiteralTypeFromPropertyName` for a computed
name). `contextual_index_value_for_name_type` is that `mapTypeEx`: one
`get_applicable_index_info` per union constituent, unioned without reduction.

`get_applicable_index_info` is `getApplicableIndexInfo`, not
`findApplicableIndexInfo` over the structured type: upstream avoids the former
here only for the cost of intersection reduction. Same answer on every
non-intersection constituent; an intersection context reads its index infos
through `get_index_infos_of_type` either way.

Not changed: a late-bound name that misses keeps
`contextual_property_type`'s string-literal-keyed index fallback, which is the
right key for string and numeric names and the wrong one for a unique symbol.
No corpus line was found depending on it.

Converted: `computedPropertyNamesContextualType{1,2,3}_ES6` (36 lines).

**Falsifier.** A dynamic computed member whose contextual type has a matching
*property* (not index) would mean a dynamic name was usable as a property name
after all — `late_bound_symbol_member_name` declining a usable name.

## 2. The rendered member list is keyed by the escaped name

`checkObjectLiteral` stores each member in `propertiesTable[member.Name]`
(`checker.go:13331`), so `{ 0b11010: "hi", 26: "Hello", "26": "world" }` and
`{ "1": 1, [+1]: 0 }` each have one property. The port already keyed
`typed_properties` by the semantic name but upserted the printed `members`
list by its *spelling*, so `26` and `"26"` survived as two rows.

The property arm now replaces the earlier spelling's row in place when the
semantic name repeats. The surviving spelling follows the node builder:

- a **computed** entry carries its own `nameType` (checkObjectLiteral sets it
  from the computed name's literal type), so `[+1]` prints `1` and `[-1]`
  prints `[-1]`, replacing the earlier `"1"` / `"-1"`;
- a **written** name prints from the binder-merged symbol, whose first
  declaration's spelling wins: `26`, not the later `"26"`.

Alternative rejected: keying `upsert_member` itself by a name normalised from
the printed text (strip quotes, unwrap `[n]`). That re-derives the escaped
name from display text the producer already has semantically, and it would
also have to know which bracketed names are symbols.

Not changed: the method and accessor arms still upsert by spelling. No corpus
case mixes a method with a differently spelled property of the same name.

Converted: `duplicateObjectLiteralProperty_computedName1`,
`binaryIntegerLiteralError`, `octalIntegerLiteralError` (8 lines).

## 3. A method name is classified, not copied

`classifyPropertyName` (`nodebuilderimpl.go:2384`) makes a **method** named
`new` a string literal (`{ new(x) {} }` would re-parse as a construct
signature) and any name that is not identifier text a string literal too. The
object-literal method arms copied the spelled name, with a `new` check only on
the identifier route, so `{ ["new"](x) {} }` printed `new(x: number)` and the
parser-recovery empty name of `{ *() {} }` printed as a call signature
`(): Generator<…>`.

`classified_method_name` applies the rule to the name both arms have already
spelled. That spelling has quoted every non-identifier string and normalised
every number, so the only non-identifier text left to classify is the empty
name. Not changed (not this lane's files): the type-literal method signature
in `declared.rs` (`vardecl`, `parser645484` print `new?(): any`), and the
empty-named method symbol's own type in `symbols.rs`
(EMPTY-NAMED-FUNCTION-SYMBOL-TYPE, the remaining line of
`FunctionPropertyAssignments{2,3,6}_es6`).

Converted: `emitMethodCalledNew`; 6 lines of `FunctionPropertyAssignments{2,3,6}_es6`.

## 4. Pseudo-type reuse: the single-quoted literal arms only

`serializeTypeForDeclaration` prints an object-literal property from the
pseudochecker's `typeFromExpression` (`pseudochecker/lookup.go:262`) when that
pseudo-type is equivalent to the property's type. The port already reused a
single-quoted string literal written directly in a const context. It now also
follows `typeFromTypeAssertion` (`lookup.go:510`): `'x' as const` recurses
into the operand in a const location (and parentheses unwrap), and `e as 'x'`
is the asserted type node itself, reused when `getTypeFromTypeNode` of it is
the member's type.

Not ported: general type-node reuse (`{} as IThing<typeof foo>` printing the
written node where the semantic type is `IThing<any>`,
`noUsedBeforeDefinedErrorInTypeContext`) and the accessor annotation
(`GetTypeOfAccessor`, `objectLiteralGettersAndSetters`). Both need a printer
for a reused type node, and the literal arms do not: a single-quoted string
literal prints the same way as a node or as text.

Converted: `discriminatedUnionWithIndexSignature`,
`objectLiteralComputedNameNoDeclarationError` (and 5 lines of
`narrowingNoInfer1`, still blocked on NOINFER-SUBSTITUTION).

## 5. An untyped or error call gives its arguments an `any` context

`getContextualTypeForArgumentAtIndex` (`checker.go:29772`) answers
`getTypeAtPosition` of the call's resolved signature. `resolveCallExpression`
sends an untyped call (`isUntypedFunctionCall`: an `any` callee, or a
signature-less `Function`-typed one) to `resolveUntypedCall`, which resolves
to `anySignature`, and an error callee to `resolveErrorCall`, which checks the
arguments while the call's signature is still `resolvingSignature`. Neither
signature has parameters, so every argument's contextual type is `any`.

`contextual_type_for_argument_resolving` now answers `any` for those callees,
asking `is_untyped_call_target` / `is_untyped_function_typed_callee` (the
calls lane's ports of `isUntypedFunctionCall`'s disjuncts) and `errorType`
identity. The error arm reads TSR's `error` as upstream's `errorType`; where a
TSR producer answers `error` for "could not compute", that producer is the
defect (`tsr-2zk.31`), not this arm. Measured: zero RIGHT lines lost.

Not reached from here: a function nested in an argument under that `any`
context (`commentsOnObjectLiteral2`'s object-literal property,
`fatarrowfunctionsOptionalArgsErrors4`'s curried arrows) still gaps at the
function-expression gate in `signatures.rs`, which accepts an `any` context
only for a direct argument of a single-signature callee
(`argument_context_parameter`). Upstream reaches them through the general
road: the property of an `any` context, and the return of a function with no
contextual signature, have no contextual type.

Converted: `superCallFromFunction1`, `superCallFromClassThatHasNoBaseType1`,
`parser509534`, and lines of `fatarrowfunctionsOptionalArgsErrors4`,
`fatarrowfunctionsErrors`, `parserindenter`, `parserRealSource9`,
`parserMissingLambdaOpenBrace1` and others (+29 lines).

## 6. `this` of an uncontextualised object literal — not landed

`getContextualThisParameterType`'s object-literal arm (`checker.go:12049`)
answers `getWidenedType(checkExpressionCached(literal))` when the containing
literal has no contextual type. Porting it in `contextual_object_this_type`
converts `contextualThisTypeInJavascript`, `jsPropertyAssignedAfterMethod
Declaration_nonError`, `declarationEmitThisPredicates02`,
`declarationEmitThisPredicatesWithPrivateName02` and `typeOfThisInAccessor`,
and costs RIGHT lines that only matched because `this` was `any` before.
Each loss is a property-access defect outside this lane:

| lost line | upstream answer | TSR answer | missing piece |
|---|---|---|---|
| `contextualThisTypeInJavascript` `this.unknown` | `any` | `error` | a missed property on a JS literal answers `any` (`isJSLiteralType`, `checker.go:11344`); TSR only skips the report |
| `constAssertions` `this.x = 20` on `as const` | `any` | `10` | an assignment to a readonly property returns `errorType` (`isAssignmentToReadonlyEntity` in `checkPropertyAccessExpressionOrQualifiedName`) |
| `constructorTagOnObjectLiteralMethod` `this.bar` | `any` | `string \| undefined` | `this.bar = …` inside a JS object-literal method is not an expando of the literal |

Separately, a TSR-only cycle: this port prints an object literal's method
signatures eagerly, so `this` inside a method whose return type is resolving
re-enters the uncached literal (`thislessFunctionsNotContextSensitive2`,
`accessorInferredReturnTypeErrorInReturnStatement`). The draft declines to the
old receiver while a member's return-type frame is on the resolution stack.
That covers methods; it does not yet cover a getter, whose cycle runs through
the accessor symbol's `Type` frame (`accessorInferredReturnTypeErrorInReturnStatement`
still lost five lines with the draft) — the draft needs that frame added too.

### Round 3: the losses are fixed; the patch waits on two other lanes' files

Re-measured on the round-3 integration head (`ebf1951`), the draft's three
diagnostics flips (`vueLikeDataAndPropsInference{,2}`,
`thisTypeInObjectLiterals2`) no longer reproduce — the cases print nothing
with the draft applied — so whatever drove them was fixed upstream of this
lane in round 2. The type losses each have a faithful fix, now in the patch
[`contextual-6-object-literal-this.diff`](contextual-6-object-literal-this.diff):

- **Getter cycle** (`accessorInferredReturnTypeErrorInReturnStatement`, own
  file): the decline also checks the accessor symbol's `Type` frame
  (`getTypeOfAccessors` resolves a getter's return there).
- **`constAssertions` `this.x = 20`** (`readonly_target.rs`):
  `checkObjectLiteral` gives every member of a const-context literal
  `CheckFlagsReadonly` (`checker.go:13175`); the port records it on the
  literal's captured member image (`AnonymousProperty::readonly`), which
  `is_assignment_to_readonly_property` now reads. `o9.x = 20` on an
  `as const` literal was `10`, upstream `any`, independently of `this`.
- **`contextualThisTypeInJavascript` `this.unknown`** (`members.rs`):
  `checkPropertyAccessExpressionOrQualifiedName`'s miss arm answers `anyType`
  for an `isJSLiteralType` receiver (`checker.go:11334`) — TSR only skipped
  the report.
- **`constructorTagOnObjectLiteralMethod` `this.bar`** (`members.rs`):
  `bindThisPropertyAssignment` (`binder.go:1115`) declares a JS
  `this.bar = …` inside an object-literal method on the *literal's* symbol, but
  `checkObjectLiteral` builds the literal's type from its elements only, so
  that member is not a property of the type. TSR's declared-owner lookup read
  the binder table and found it (`obj.bar : string | undefined`).

Measured with the whole patch against `ebf1951`: +37 types lines, both loss
checks empty, diagnostics `checkingObjectWithThisInNamePositionNoCrash`,
`jsPropertyAssignedAfterMethodDeclaration`, `thisInObjectLiterals`
WRONG→RIGHT, perf CPU self-ratio 0.999 / 0.991. The `contextual.rs` hunk alone
still loses the lines above, so it is not committed on its own.

## 7. The widening nullables exist (`tsr-2zk.16.7`)

`createWideningType` (`checker.go:25027`) gives non-strict mode two more
intrinsics, `undefinedWideningType` and `nullWideningType`: same flags and
name as `undefinedType`/`nullType`, plus `ObjectFlagsContainsWideningType`.
Under `strictNullChecks` they *are* the plain types. The port had only the
undefined twin, never handed out by the `undefined` symbol, so it could not
tell `{ foo: null }` (widens to `{ foo: any }`) from `{ foo: n }` with
`n: null` (stays `{ foo: null }`), and `objects.rs` refused every nullable
member under non-strict to avoid printing one of them wrong.

What landed, each mirroring one upstream site:

| upstream | port |
|---|---|
| `nullWideningType` (`checker.go:990`) | `Intrinsics::null_widening`, selected by `select_strict_null_checks` like the undefined twin |
| `ContainsWideningType` on those two | `Intrinsics::is_widening_nullable` (identity, not a flag: only the two intrinsics carry it at creation) |
| `addTypeToUnion`'s `IncludesNonWideningType` (`:25784`) and `getUnionTypeWorker`'s empty-set arm (`:25692`: null beats undefined, widening unless a plain constituent was seen) | `Includes::non_widening`, `union_type_worker` (was `errorType`) |

Committed in the lane's files: the two rows above. Alone they convert
`conditionalExpressions2` (`false ? null : undefined` is `null`, was
`errorType`) and three lines elsewhere (+5, no losses): with no producer yet
answering a twin, every dropped nullable is non-widening, so the arm answers
the plain types. Everything else is one
patch for the integrator,
[`contextual-7-null-widening.diff`](contextual-7-null-widening.diff), because
it only lands without losses as a whole. It holds the lane's own
`check_object_literal_members` hunk — `checkObjectLiteral` keeps member types
as checked, so the non-strict nullable refusal goes — and, outside the lane's
files, each again one upstream site:

- `checkExpression`'s `KindNullKeyword` arm (`checker.go:7742`) answers
  `nullWideningType` (`expressions.rs`);
- `getTypeFromLiteralTypeNode` answers `nullType` for a `null` literal type
  rather than checking the keyword as an expression (`declared.rs`);
- `valueSymbolLinks.Get(c.undefinedSymbol).resolvedType =
  undefinedWideningType` (`checker.go:1345`), re-seeded when the case's
  `strictNullChecks` is applied (`checker.rs`);
- `getWidenedTypeWithContext`'s first arm (`checker.go:18368`): a widening
  nullable becomes `any` (`widening.rs`);
- the identity checks that stood for "a widening null" accept the twin:
  `symbols.rs`' declaration widening and `signatures.rs`' `extends null`
  (upstream compares against `nullWideningType`, `checker.go:12284`);
- `getReturnTypeOfFullSignature` (`checker.go:20089`), the JS `@type`
  full-signature arm of `getReturnTypeFromAnnotation`
  (`jsdoc_full_signature.rs`, read from `signatures.rs`). Without it
  `typeFromJSInitializer3` lost six RIGHT lines: `/** @type {() => null} */
  function f2() { return null; }` printed `() => null` only because the body's
  `null` used to be the plain type; once it widens, the tag has to supply
  the return as upstream's does.

Measured: removing the refusal *without* the rest of the patch converts 59
lines and loses five RIGHT ones (`{inc,dec}rementOperatorWithAnyOtherType`
`:44`/`:69`, `propertyNameWithoutTypeAnnotation:26`) — a literal's `null`
member then never widens. The whole patch: +149 types lines, both loss checks
empty, no diagnostics verdict moved. Cases fully converted (21):
`conditionalExpressions2`, `declFileRegressionTests`, `null`,
`overloadResolutionOverNonCTObjectLit`, `typeParameterFixingWithConstraints`,
`arrayLiterals2ES5`, `computedPropertyNames5_ES6`,
`{de,in}crementOperatorWithAnyOtherType`, `objectLiteralWidened`,
`propertyNameWithoutTypeAnnotation`, `symbolProperty19`, `widenedTypes1`,
`noImplicitAnyUnionNormalizedObjectLiteral1`, `typeMatch2`,
`arrayLiteralWidened`, `callSignatureWithoutReturnTypeAnnotationInference`,
`functionImplementations`, `typeArgumentInferenceConstructSignatures`,
`wideningTuples2`, `wideningTuples7`; lines in 16 more.

Two patch files, both for the integrator:

- [`contextual-7-intrinsic-count.diff`](contextual-7-intrinsic-count.diff) —
  goes with this commit alone. The new loose twin is one more intrinsic
  allocation, and `tsr-conformance/tests/undefined_widening_modes.rs` pins
  the store's size (25 → 26); this lane may not edit that crate.
- [`contextual-7-null-widening.diff`](contextual-7-null-widening.diff) —
  self-contained on top of this commit (it includes the count change). Besides
  the code above it updates the tests that pinned the old answers: the
  `undefined` global's identity (`globals.rs`, `undefined_widening_modes.rs`,
  and the IIFE raw-context test in `contextual.rs`), and
  `a_null_candidate_is_refused_only_where_upstream_would_widen_it`
  (`inference.rs`), whose non-strict `f(null)` now answers upstream's `any`
  instead of the gap the test recorded.

**Rejected:** keeping `null` expressions on the plain type and widening every
non-strict `null` (the identity rule `symbols.rs` used for declarations). It
makes `{ foo: n }` with `n: null` print `{ foo: any }` — confidently wrong
where upstream keeps the declared `null`, and the reason the objects.rs
refusal existed.

**Falsifier.** A non-strict nullable reaching an object literal or a widening
site through a producer that still answers the plain type where upstream
answers the twin (or the reverse) would show up as `{ p: null }` vs
`{ p: any }` disagreement on a declaration line; the fix is that producer.

## 8. Where `ContextualSignature::Absent` still disagrees with tsgo (round 3, measured, not fixed)

The implicit-any lane does not trust `Absent` (implicit-any-widening.md §3:
trusting it lost twelve cases in round 2). Re-measured on `ad012b1` by
locally letting `contextual_parameter_type_is_absent` answer `true` for
`Absent` (not committed): **only three cases lose now**, and four convert
(`jsxFragmentFactoryNoUnusedLocals`, `uncalledFunctionChecksInConditional2`,
`tsxReactEmitNesting`, `typeSatisfaction_contextualTyping2` WRONG→RIGHT).
The three, with what the contextual type was when `Absent` was answered:

| case | TSR context | cause | owner |
|---|---|---|---|
| `contextuallyTypedByDiscriminableUnion` (29,12) | the undiscriminated `ADT` union, two different `method` signatures | the literal discriminates by a **shorthand** `kind`; `discriminateContextualTypeByObjectMembers` (`checker.go:30755`) admits `ShorthandPropertyAssignment` and any `isPossiblyDiscriminantValue` initializer, `discriminate_union_root` only literal initializers of `PropertyAssignment` | `symbols.rs` |
| `ipromise4` (14,58) | `any` | the second `.then` callback's context is `any`: the receiver chain `p.then(...)` is answered as untyped/error, where tsgo resolves `IPromise<string>.then` | calls lane (overload resolution / `tsr-2zk.31` producer) |
| `parserArgumentList1` (2,81…) | `string` | `replace(_classNameRegexp(…), function …)` with an unresolved first argument: TSR picks the `replaceValue: string` overload, tsgo the replacer-function overload | calls lane (`chooseOverload`) |

Not a `crate::contextual` defect in any of the three rows, so nothing is
changed here. When those land, admitting `Absent` in `implicit_any.rs`
should be +4 cases with no losses — the falsifier for this table.

## 9. Rest-bearing array binding patterns imply a tuple (tsr-2zk.16.63)

`getTypeFromArrayBindingPattern` (`checker.go:17957`) answers the
iterable/array of `any` only for an empty pattern or a lone rest element;
every other array pattern, rest-bearing included, is a tuple, so the
initializer is checked in tuple context (`var [x, ...a] = [1, "a"]` records
`[number, string]`). `array_binding_pattern_implies_tuple` is that predicate
for the binding arms of `destructuring_array_pattern_slot`; no new state.
Not ported: assignment targets with a spread (`[...a, x] = [1, 2, 3]`),
whose tuple-ness depends on the checked left side (`restElementMustBeLast`).

## 10. Nullable constituents do not count toward §927's unit guard (tsr-2zk.16.130)

`getTypeOfPropertyOfContextualType` (`checker.go:30555`) maps over a union and
a nullable constituent has no property, so `X | undefined` (an optional tuple
slot, an optional member) has one candidate and nothing to discriminate.
`union_contextual_property_type` now counts only non-nullable constituents
before declining a unit-leaf answer; multi-object unions keep the guard.
`const a: [number, { t: 1 | 2 }?] = [0, { t: 1 }]` no longer reports TS2322
(tsgo: none). No new state.

## 12. getWidenedLiteralType's union arm at mutable locations (tsr-2zk.16.131)

`checkExpressionForMutableLocation` -> `getWidenedLiteralLikeTypeForContextualType`
calls `getWidenedLiteralType` (`checker.go:25499`), whose union arm maps
member-wise: `{ w: b ? f() : 0 }` is `{ w: void | number }`.
`get_widened_literal_type_with_unions` (literals.rs) is that function;
`check_expression_for_mutable_location` uses it. The scalar
`get_widened_literal_type` keeps its other callers (signatures.rs,
symbols.rs, destructure.rs, flow.rs, inference.rs, contextual.rs): switching
them lost 139 RIGHT lines because upstream gates those sites with
`isUnitType`/`getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded`
before widening, which those ports do not; reported, not changed.

Prerequisite, same commit: §927's unit-leaf decline now applies only when
`discriminateContextualTypeByObjectMembers` (`checker.go:30755`) could
discriminate at all (`object_literal_may_discriminate`: a
possibly-discriminant initializer, a shorthand, a spread/computed name, or a
constituent member the literal does not write). Without discriminators
upstream walks the whole union, so `{ type: b ? 'x' : 'y' }` against
`{ type: 'x' } | { type: 'y' }` keeps `"x" | "y"`
(`assignmentCompatWithDiscriminatedUnion`).

## 13. getContextualReturnType reads getReturnTypeFromAnnotation (tsr-2zk.16.200)

`getReturnTypeFromAnnotation` (`checker.go:20058`) answers the declaration's
type node — in JS the reparsed `@returns` (`reparseHosted`) — and, for a get
accessor without one, the paired setter's value-parameter annotation
(`getEffectiveSetAccessorTypeAnnotationNode`, explicit `this` skipped,
reparsed `@param` included). `get_contextual_return_type` now reads both via
`jsdoc_return_annotation` and `paired_setter_value_annotation`; no state.
Native control (tsgo `.types`): getter with typed setter returns
`{ tag: "a"; }`, a lone getter `{ tag: string; }`; JS `@return {[string,
number]}` function returns the tuple, an undocumented one
`(string | number)[]`. Still open in `contextualTypeFromJSDoc`: the getter
symbol's own type from the setter's JSDoc `@param` (accessor type
resolution, not this lane).

## 14. Parameter initializer context: getContextuallyTypedParameterType (tsr-2zk.16.230)

`getContextualTypeForVariableLikeDeclaration`'s Parameter arm
(`checker.go:29438`/`:29458`) answers the type node (JS: reparsed `@param`),
else `getContextuallyTypedParameterType`. The existing
`get_contextually_typed_parameter_type` also folds in
`assignContextualParameterTypes`' initializer widening, which checks the
initializer; the initializer's own context uses the split
`contextually_typed_parameter_type(_, false)` that skips it. No state.
`var f5: (a: (s: string) => any) => void = function (a = s => <number>s) {}`
types `s : string` (tsgo `.types` identical).

## 15. Parenthesized initializers keep the implied binding-pattern context (tsr-2zk.16.63)

`getContextualType`'s ParenthesizedExpression arm passes its parent's
contextual type through, so `const { B = class {} } = ({ B: undefined })`
types the literal against the implied pattern (`{ B?: undefined; }`).
`contextual_binding_pattern` (§489) now recurses through a parenthesis; the
parameter/binding-element arms still require the literal itself. No state.

## 16. IIFE parameter defaults past the arguments use the implied pattern (tsr-2zk.16.63)

`getContextuallyTypedParameterType`'s IIFE arm (`checker.go:29466`) answers
the argument type for a rest parameter or a position with an argument, and
nil for a defaulted position past the arguments, so
`getContextualTypeForInitializerExpression` falls through to the implied
binding-pattern type: `(({ u = 22 } = { u: 23 }) => u)()` records
`{ u?: number; }`, while `({ r = 17 } = { r: 18 }) => r)({ r: 19 })` keeps
`{ r: number; }` (tsgo `.types` identical). `iife_supplies_parameter_context`
is that test for `contextual_binding_pattern`; no state.

## 17. Property-assignment symbol type follows the final literal check (tsr-2zk.16.84)

Upstream's `getTypeOfSymbol` for a `PropertyAssignment` runs
`checkPropertyAssignment` -> `checkExpressionForMutableLocation`
(`checker.go:13673`) lazily, so it sees the literal's final contextual type
(after the call resolved), not the context-free first inference pass.
`check_object_literal`'s §892 record into `symbol_types` (key: the member's
binder symbol, owner: this checker) now lets the value declaration's latest
check replace the entry; other declarations of a merged duplicate name still
only fill an empty slot, so `getTypeOfSymbol`'s first-declaration answer is
kept (`lastPropertyInLiteralWins`). Native control (tsgo `.types`):
`id({ test: true })` -> `test : true`; `{ plain: true }` -> `plain : boolean`;
`{ dup: 1, dup: "x" }` -> `dup : number` twice. No new state; no extra work.
