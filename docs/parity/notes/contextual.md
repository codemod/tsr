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

## 5. `this` of an uncontextualised object literal — not landed

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
`accessorInferredReturnTypeErrorInReturnStatement`). The draft declined to the
old receiver while a member's return type (or an accessor's type) is on the
resolution stack.

The draft is not committed (a loss is fixed, never accepted); it waits on the
three property-access fixes above, which are reported to the integrator.
