# r7-reports — implicit-any, JSDoc, JSX and relation reports (`tsr-2zk.1276`)

Round-7 lane notes. The lane owns `relater.rs`, `assignreport.rs`,
`implicit_any.rs`, `jsdoc_*.rs`, `jsx_*.rs`, `js_case_data.rs` and
`assignment_declarations.rs` ([round7.md](../round7.md)). Native source is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/checker.go` unless
noted.

## 0. Base

Frozen base `9020aa67` (origin/main at dispatch), `scripts/parity_gate.sh
freeze`:

- types: 556,357 aligned lines, 551,176 RIGHT / 4,508 WRONG / 673 GAP;
- diagnostics: 12,238 cases, 5,741 RIGHT / 5,610 EMPTY_RIGHT / 852 WRONG /
  35 EMPTY_WRONG.

The r6-triage case lists ([r6-triage-issues.json](r6-triage-issues.json),
cut at `e6eadf4`) were re-measured on this base before porting. Of the 20
`IMPLICIT-ANY-PARAMETER-REPORT` cases, seven were already RIGHT
(`contextualOverloadListFromUnionWithPrimitiveNoImplicitAny`,
`contextualSignatureInArrayElementLibEs5`/`Es2015`, `contextualTyping38`,
`jsxFragmentFactoryNoUnusedLocals`, `parserindenter`,
`typeSatisfaction_contextualTyping2`); the rest split into the root causes
below.

## 1. TS7006 in a declaration file

**Forcing fact.** `implicitAnyInAmbientDeclaration2.d.ts` expects TS7006 at
`declare function foo(x)`, the public `publicFunction(x)` and the public
constructor's `publicConsParam`, and nothing for the `private` members.
`check_implicit_any_parameters` skipped every parameter when
`file_is_ambient`.

**Upstream.** `widenTypeForVariableLikeDeclaration` (`checker.go:18242`)
reaches `reportImplicitAny` for a nil declared type unless
`declarationBelongsToPrivateAmbientMember` (`utilities.go:334`): the root
declaration's member (the parameter's parent) is `isPrivateWithinAmbient`.
There is no declaration-file exemption; a `.d.ts` is checked unless
`skipLibCheck` drops the whole file, which the program already does.

**Port.** The gate is `(ambient || file_is_ambient) && private_within_ambient`.
The TS7031 binding-pattern arm keeps its walk-threaded `!ambient` decline (no
corpus case distinguishes it, and the pattern road reports from a different
upstream site).

**Measured** (both dumps unfiltered against the base): WRONG → RIGHT
`implicitAnyInAmbientDeclaration2.d`; no other row changed.

**Falsifier.** A `.d.ts` case where TSR now reports a TS7006 the baseline
lacks would show a declaration-file arm upstream that this reading missed.

## 2. The retained `return`-position arm stays (routed)

*Superseded by §18:* the arm is deleted, and the premise below (a producer
defect) was wrong.

`retained_return_position_report` (implicit-any-widening.md §3) keeps TS7006
extra in four cases: `asyncFunctionContextuallyTypedReturns` (19,10)(28,10),
`contextualTypeOnYield2` (3,13),
`inferPropertyWithContextSensitiveReturnStatement` (10,28) and
`invalidThisEmitInContextualObjectLiteral` (10,22). Removing it converts all
four and loses `subtypeReductionWithAnyFunctionType` (measured, unfiltered).

The loss is a producer answer, not this rule: for `return x => x.length > 0`
inside `useMemo(() => { … })` the port's contextual signature is
`(x: any) => boolean`. The `any` is the context-sensitive function's own
implicit parameter type, fed back through the inference of `T` from the
outer arrow's return expressions; tsgo has no usable signature there and
reports TS7006. Applying `within_generic_call_argument`'s decline to the
too-short source as well did not move it (the signature is present and long
enough). Routed to the contextual/inference owner: the inference of a
generic call's return type from a context-sensitive function's return
expressions must not see that function's own unfixed parameter types. When
it stops, delete the arm (+4 cases).

## 3. TS7032 / TS7033 and the set accessor's TS7006 (`getTypeOfAccessors`)

**Forcing fact.** Six cases miss `Property '{0}' implicitly has type 'any',
because its set accessor lacks a parameter type annotation.` (TS7032), its
getter twin TS7033, and the TS7006 beside it on the setter's parameter
(`parserES3Accessors4`, `implicitAnyGetAndSetAccessorWithAnyReturnType`,
`noImplicitAnyMissingGetAccessor`, `noImplicitAnyMissingSetAccessor`,
`isolatedDeclarationErrorsClasses`, `isolatedDeclarationErrorsObjects`).
Neither had a port: the parameter rule's owner test answered `Other` for a
set accessor, and nothing reported TS7032/TS7033.

**Upstream.**

- `getTypeOfAccessors` (`checker.go:18511`) tries the getter's annotation,
  the setter's parameter annotation, the auto-accessor's annotation, then the
  getter body's inferred return type. With none it reports TS7032 on the
  setter, else TS7033 on the getter, skipping one that
  `isPrivateWithinAmbient`, and answers `any`. `checkAccessorDeclaration`
  (`checker.go:2974`) resolves it for every accessor.
- A set accessor's parameter is typed by `getTypeForVariableLikeDeclaration`
  (`checker.go:16716`): with `hasBindableName` and a getter on the symbol, the
  getter's return type. Otherwise `getContextuallyTypedParameterType`
  answers nil (an accessor is not `isContextSensitiveFunctionOrObjectLiteralMethod`)
  and `widenTypeForVariableLikeDeclaration` reports TS7006.

**Port** (`implicit_any.rs`, `check_implicit_any_accessor`, called from
`resolve_accessor_symbol_type`, `assignreport.rs`, which already is
`checkAccessorDeclaration`'s `getTypeOfAccessors` site). The accessor pair is
`GetDeclarationOfKind` over the merged symbol's declarations, and the
annotation test is the existing `accessor_annotation` (JSDoc included). Only
a bodiless getter can reach TS7033, because a body always infers. Upstream
reports once, at first resolution. The diagnostic's declaration is checked
exactly once, so the report is made from that declaration's own check. The
setter's parameter goes through `check_implicit_any_parameters`, with a
`SetAccessorDeclaration` owner arm that is `Other` when a getter exists and
`Uncontextual` otherwise.

**Declined: late-bindable computed names.** `get [k]()`/`set [k](v)` with
`const k = "m"` (or a unique-symbol `[Symbol.x]`) share one **late-bound**
symbol upstream (`getSymbolOfDeclaration` → `getLateBoundSymbol`,
`checker.go:14390`). This binder keeps each computed member's own symbol, and
the checker has no late-bound member table, so the getter lookup misses.
The first unfiltered run reported TS7006/TS7032 on ten RIGHT cases this way
(`esDecorators-classDeclaration-accessors-{static,nonStatic,nonStaticAbstract}`
×3 targets, `symbolDeclarationEmit4/10/11`). `accessor_has_late_bindable_name`
(`hasLateBindableName`: a dynamic name, an entity-name expression, a type
`isTypeUsableAsPropertyName`) declines both arms. A dynamic name that is
*not* late-bindable has its own anonymous `__computed` symbol upstream too
(`bindAnonymousDeclaration`), so it reports
(`isolatedDeclarationErrorsClasses` 48:9/48:39 needs this). The decline
waits on late-bound member symbols (proposed issue in §9).

**Not a new table.** No cache or side table is added. The accessor pair is
read from the binder's merged symbol on each call (two declarations at
most), once per accessor declaration.

**Measured** (unfiltered diagnostics, on top of §1): WRONG → RIGHT the six
cases above; no other row changed; type lines byte-identical.

**Falsifier.** A case with a TS7032 on an accessor whose sibling is in a
merged declaration TSR's `merged_symbol` does not reach (an interface and a
class, say) would show the pair lookup is narrower than
`GetDeclarationOfKind` over the merged symbol.

## 4. TS2820: `reportRelationError`'s string-literal suggestion

**Forcing fact.** `didYouMeanStringLiteral` and
`errorsForCallAndAssignmentAreSimilar` expect `Type '"strong"' is not
assignable to type 'T1'. Did you mean '"string"'?` (TS2820) where TSR
printed TS2322 at the same position.

**Upstream.** `reportRelationError` (`relater.go:4751`), with no head
message, after the comparable, same-name and exact-optional arms: a source
with `TypeFlagsStringLiteral` against a union target asks
`getSuggestedTypeForNonexistentStringLiteralType` (`checker.go:27252`), the
spelling suggestion over the union's string-literal members, at most 1,000
of them. A suggestion reports TS2820 and returns. It skips the
type-parameter explanation and the excess/missing-property suppressions
below it.

**Port** (`assignreport.rs`, `report_relation_head` and
`suggested_type_for_nonexistent_string_literal_type`). The arm runs only
where the head resolved to plain TS2322, which is upstream's default arm. A
caller head, or an exact-optional head, keeps its message, as upstream's
earlier arms do. The 1,000-candidate cap is checked before the search.
`tsr_core::spelling` has no count parameter, and native returns nil on the
1,001st candidate, which is the same answer. Ties keep the earlier member in
union order (`CompareTypes` order).

**Not ported:** the same-name arm (TS2719,
`incompatibleAssignmentOfIdenticallyNamedTypes`). It compares
`getTypeNamesForErrorDisplay`, which re-prints both sides with
`TypeFormatFlagsUseFullyQualifiedType` when the plain names collide, and
this printer has no fully-qualified mode. Equal plain names are not equal
qualified names (`A.T` vs `B.T`), so the arm waits on that printer flag.

**Measured** (unfiltered, on top of §3): WRONG → RIGHT the two cases above;
no other row changed. `thislessFunctionsNotContextSensitive1` (176,3) is
missing TS2820 because no relation is reported there at all (a call-site
argument check), not because of the head.

**Falsifier.** A TS2820 TSR emits where the baseline has TS2322 would show
the suggestion's candidate set or distance differs from
`GetSpellingSuggestionWithMaxCandidateCount`.

## 5. `reportUnmatchedProperty` heads: two over-wide declines in `unmatched_property_report`

**Forcing fact.** Five cases print TS2322 (or TS2345) where tsgo prints
TS2740/TS2741 at the same position: `assignmentToObjectAndFunction`
(`var errFun: Function = {}`), `templateStringsArrayTypeDefinedInES5Mode`,
`…NotDefinedES5Mode`, `…RedefinedInES6Mode` (`f({})` against
`TemplateStringsArray`) and `assignmentCompatWithEnumIndexer`
(`let foo: Record<E, any> = {}`). The relation answers NotRelated in each.
The certified-table road (`missing_required_property`) cannot name the
missing members because `Function`, `TemplateStringsArray` and
`Record<E, any>` have no certified table, and the relater's own road
(`unmatched_property_report`, `relater.rs`) declined for one of two reasons.

**Upstream.** `propertiesRelatedTo` (`relater.go:4100`) finds the first
`getUnmatchedProperty` and calls `reportUnmatchedProperty` (`:4345`). For
the top-level pair, `reportRelationError`'s suppression (`:4816`) then drops
the head when the chain's missing-property message names the same pair
(`chainArgsMatch`).

1. *A fresh object literal.* The decline exists because `hasExcessProperties`
   (`relater.go:2714`) runs first and could own the failure. An **empty**
   literal has no member for it to report (`isKnownProperty` is asked once
   per source property), so the failure is the property walk's. The decline
   now requires at least one source property.
2. *An alias-owned object type.* The decline exists because
   `reportErrorResults` (`relater.go:4705`) displays the aliased original
   while the missing-property message names the normalized structure. But
   `getNormalizedType` (`relater.go:2619`) rewrites only fresh literals,
   references, unions/intersections, substitutions and simplifiable types.
   An alias whose body is a type literal or mapped type denotes an
   anonymous/mapped object type that is its own normal form, so both
   messages print `Record<E, any>`. `alias_object_body_is_its_own_normal_form`
   admits exactly that: every declaration's body is a `TypeLiteralNode` or
   `MappedTypeNode`, and the instantiation is not an array or tuple
   (`instantiateMappedArrayType`'s result is a reference). An alias of a
   reference, union, conditional or anything else keeps the decline.

**Measured** (unfiltered diagnostics, on top of §4): WRONG → RIGHT the five
cases above, two rows fixed in `destructuringParameterDeclaration2`; no other
row changed.

**Still declined, each with its blocker.** `mappedTypeWithAny`
(`Objectish<any> -> any[]`): `signatures_of_type_kind` cannot enumerate a
mapped type's call signatures, and `shouldReportUnmatchedPropertyError`
needs them. `mappedTypeNotMistakenlyHomomorphic` (`Gen2<ABC.B> ->
Gen2<ABC.A>`): the source answers a type for `a`, which upstream's
`Gen2<ABC.B>` lacks. The mapped type's key set over
`keyof (… & ({ v: A, a } | { v: B, b }))` is the declared-type lane's
(`mapped.rs`).

## 6. `indexSignaturesRelatedTo`'s `sourceIsPrimitive`

**Forcing fact.** `assignmentCompat1` expects TS2322 for `y = "foo"` with
`y: { [index: string]: any }` and for `z = false` with
`z: { [index: number]: any }`. TSR reported nothing, because the relation
answered Unknown.

**Upstream.** `structuredTypeRelatedToWorker` (`relater.go:3814`) records
`sourceIsPrimitive` and then relates the apparent wrapper (`String`,
`Boolean`) through the structural arm (`:3864`): properties, call and
construct signatures, then `indexSignaturesRelatedTo(source, target,
sourceIsPrimitive, …)` (`:4578`). With `sourceIsPrimitive` the
`any`-valued string-index shortcut (`:4588`) is skipped, so each target
index goes to `typeRelatedToIndexInfo`. `String` has no applicable string
index (its index is numeric), and an interface has no inferable index
(`isObjectTypeWithInferableIndex`), so the answer is False. `Boolean` fails
the same way against a number index.

**Port** (`relater.rs`). The primitive-source arm of
`is_related_to_with_flags` declined everything against an indexed target
except a property rejection. It now runs the structural trio over the
apparent type, threading `source_is_primitive` into
`related_index_signatures_ex`. The trio is computed in place rather than
through `is_related_to(apparent, target)`, because that result would be
published under the wrapper's own pair, where the shortcut does apply
(`String` *is* assignable to `{ [k: string]: any }`). Upstream keys the
answer by the primitive source. The member relations inside the trio are
ordinary pairs and publish as before. A mapped or generic mapped target
keeps the old decline after the property conjunct: its own arms precede the
structural one upstream (`relater.go:3593`).

**Not a new cache.** No table is added. The only publication change is that
this arm no longer publishes a wrapper-keyed pair, which it never did
before either (it returned Unknown).

**Measured** (unfiltered, on top of §5): WRONG → RIGHT `assignmentCompat1`
and `indexTypeCheck`; one row fixed in `unknownType1`; no other row changed.
Types: +5 RIGHT lines (`deeplyNestedConstraints` 0:6–0:8,
`typeGuardConstructorNarrowPrimitivesInUnion` 0:14,
`typeGuardConstructorPrimitiveTypes` 0:6: the relation now rejects a
primitive against an indexed constituent during narrowing), none lost.

**Falsifier.** A primitive assigned to a type with an `any` string index
that TSR now rejects while tsgo accepts would show a target arm ahead of the
structural one that this arm skips.

## 7. A circular type-parameter constraint explores `unknown`

**Forcing fact.** `typeParameterHasSelfAsConstraint`: `function foo<T
extends T>(x: T): number { return x; }` is TS2313 and TS2322 at the
`return`. The relation `T -> number` answered Unknown, so the TS2322 was
missing.

**Upstream.** The type-variable arm of `structuredTypeRelatedToWorker`
(`relater.go:3665`) relates `getConstraintOfType(source)`, or `unknown` when
that is nil. `getConstraintOfTypeParameter` (`checker.go:17059`) is nil when
`!hasNonCircularBaseConstraint`, which is the case for a cycle of
type-parameter constraints (`T extends T`, `T extends U, U extends T`, or a
chain into such a cycle). `unknown -> number` is False.

**Port** (`relater.rs`, `type_variable_source_related_to`). The
type-parameter chain walk already detected the cycle, and answered Unknown
there. It now continues with `unknown`, upstream's nil-constraint answer.
Cycles through non-parameter types (`T extends keyof T`) are not detected
by this walk and keep their old road.

**Measured** (unfiltered, on top of §6): WRONG → RIGHT
`typeParameterHasSelfAsConstraint`; no other row changed.

**Refused in the same session: structural `has_members` for every captured
anonymous type.** `getRestType`'s result (`objectRestNegative`: `({ b,
...notAssignable } = o)` relating `{ a: number } -> { a: string }`) is an
ownerless minted object whose captured property list is complete, and the
relater's `has_members` admits no ownerless type but the import-attributes
mint, so the walk answered Unknown (`NoMembersTable`). Admitting every
ownerless type with a complete capture measured **+4 cases, −5**:
`restElementAssignable`, `excessPropertyCheckWithSpread`,
`deeplyNestedMappedTypes`,
`homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1`,
`mappedTypeInferenceToMappedType`. Every loss is a false TS2322/TS2345/TS2416
on a spread, rest or mapped-type mint, so those captures relate wrongly
through the general walk. A retry has to separate the rest mint
(`mint_rest_properties`) from the spread and mapped mints, and it must
answer `restElementAssignable`'s two false lines first, since that case is
itself a rest type.

## 8. ENUM-RELATION-SAME-NAME: `isEnumTypeRelatedTo` read the wrong table

**Forcing fact.** `enumAssignmentCompat3` (70,1): `abc = secondCd`, with
`First.E { a, b, c }` and `Cd.E { c, d }`, is TS2322 (`Each declaration of
'E.c' differs in its value, where '2' was expected but '0' was given.`).
TSR related the two.

**Root cause.** `is_enum_type_related_to` (`relater.rs`, the port of
`isEnumTypeRelatedTo`, `relater.go`) walked `members` of the two enum
symbols. This binder files an enum's members in its **exports**, as
upstream's does (`binder.go:436`, `GetExports(container.Symbol())`;
`binder.rs` routes `EnumMember` under an `EnumDeclaration` to `Exports`).
The walk therefore saw no members and answered true for any two same-named
regular enums. Only the per-literal value match in `is_simple_type_related_to`
(`Cd.E.c = 0` equals `First.E.a = 0`; `Cd.E.d = 1` equals `First.E.b = 1`)
decided anything. A source member absent from the target, or a same-named
member with another value, never rejected.

**Port.** Both tables are read from `exports`. No other change. The value
comparison and the string/unknown arms were already upstream's.

**Measured** (unfiltered, both dumps, against the round-7 batch-4 base
`660718af`): WRONG → RIGHT `enumAssignmentCompat3`; no other row changed.

**Falsifier.** Two same-named enums with identical members in different
containers that tsgo relates and TSR now rejects would show a member-table
difference (merged declarations across files read through
`merged_symbol`, not the declaration's own symbol).

## 9. TS7008 is not reported on a static `prototype` property

**Forcing fact.** `staticPrototypeProperty` reports only TS2699 for `class
C2 { static prototype; }`. TSR added TS7008 at (6,11).

**Upstream.** `bindClassLikeDeclaration` (`binder.go:962`) seeds every
class's exports with a `Property | Prototype` symbol named `prototype`
before the members are bound. A static property of that name merges into
it, because `PropertyExcludes` is none. The method form collides and is
TS2300, as the same case shows at (2,11). The merged symbol's type is
`getTypeOfPrototypeProperty` (`checker.go:16580`), the instance type, so
the implicit-`any` fallback never runs for it.

**Port** (`implicit_any.rs`, `check_implicit_any_member`). A static class
member named `prototype` is skipped. This binder does not create the
prototype symbol (`SymbolFlags::PROTOTYPE` is declared and never set), so
the test reads the declaration that would have merged into it.

**Measured** (unfiltered diagnostics against `660718af` plus §5–§8): WRONG
→ RIGHT `staticPrototypeProperty`; no other row changed.

## 10. `reportErrorsFromWidening` for initialized variables and parameters

**Forcing fact.** With `strictNullChecks` off, `var b = a = [undefined,
null]` is TS7005 `Variable 'b' implicitly has an '[any, any]' type.`
(`wideningTuples3`). `const bar = { p: null, s: null, ...f() }` is TS7018 on
`p` and `s` (`noImplicitAnyUnionNormalizedObjectLiteral1`). A JS
`function f(a = null, l = [])` is TS7006 on `a` (`any`) and `l` (`any[]`)
(`typeFromJSInitializer4`). The implicit-any rules only ever looked at
declarations without an initializer.

**Upstream.** `widenTypeForVariableLikeDeclaration` (`checker.go:18242`)
calls `reportErrorsFromWidening` (`:20452`) on the declaration's
pre-widening type. Under `noImplicitAny`, a type with
`ObjectFlagsContainsWideningType` reports inside the type where
`reportWideningErrorsInType` (`:20497`) can:

- a union through a member (an empty-object member counts as reported);
- an array or tuple through a type argument;
- an object literal on each widening property it cannot descend, as TS7018
  at that property's declaration in this literal.

Otherwise it reports on the declaration (`reportImplicitAny`). The flag is
set only on `createWideningType`'s nullable twins (`checker.go:25027`,
non-strict) and on `nonInferrableAnyType`. `ObjectFlagsPropagatingFlags`
carries it into unions, intersections, type references and object literals.

**Port** (`implicit_any.rs`). `type_contains_widening_type` mirrors the
propagation over this port's shapes: the widening nullables
(`is_widening_nullable`), union/intersection constituents, tuple elements,
reference type arguments, and object-literal property types.
`report_widening_errors_in_type` and `report_implicit_any_for_widening`
are the two reporters. The literal's own declaration is the owner symbol's
value declaration, or its first declaration, since the binder sets none for
`__object`. The type whose flag is read is the initializer's checked type.
`getTypeForVariableLikeDeclaration` answers that type for an unannotated
identifier declaration once its earlier arms decline: the for-in/of heads,
the JSDoc tags, the full signature, the contextual parameter type (asked
through `contextual_parameter_type_is_absent`), and the two auto arms
(`[]`, and `null`/`undefined` on a non-`const`, non-exported, non-ambient
variable), which are mirrored before the check. Literal widening does not
change the flag.

**Declined: an initializer holding a deferred body.** Upstream checks a
function expression's, arrow's, method's, accessor's or class expression's
body deferred (`checkNodeDeferred`). This port types such bodies eagerly
when the enclosing expression is first checked. Checking the initializer
from this rule (pre-order, before the walk reaches it) moved three RIGHT
cases on the first unfiltered run:

- `thisInObjectLiterals`: TS2339 under an object-literal `this`, reported
  twice;
- `checkingObjectWithThisInNamePositionNoCrash`: once forcing the symbol
  type first, the same TS2339 was not reported at all;
- `declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`: a TS7024
  the deferred order never reaches.

`subtree_holds_deferred_body` declines those initializers, which leaves
`usingDeclarationsWithObjectLiterals2` (a `[Symbol.dispose]()` method beside
`value: null`) WRONG. It waits for the deferred-check order (proposed issue
in §11).

**A consequence that is upstream's.** `checkVariableLikeDeclaration` always
checks an unannotated initializer (`getTypeOfSymbol` →
`checkDeclarationInitializer` → `checkExpressionCached`). This port's walk
did not, so `checkConstEnumAccess`'s TS2475 on `var x = E` (inside
`checkExpression`) never ran for one. The widening rule checks the
initializer and supplies it: `constEnumErrors` and
`constEnumPropertyAccess2` convert. `es2020IntlAPIs` loses four false
TS2322 for the same reason (its initializers are typed in declaration order
now). `keyofIsLiteralContexualType` fixes one row.

**Not a new cache.** Nothing is stored. The containment walk keeps an
active-set guard (and a depth bound of 32) against cyclic references.

**Measured** (unfiltered diagnostics, against batch-5 main `20501307` with
§5–§9): WRONG → RIGHT `wideningTuples3`,
`noImplicitAnyUnionNormalizedObjectLiteral1`, `typeFromJSInitializer4`,
`constEnumErrors`, `constEnumPropertyAccess2`, `es2020IntlAPIs`; one row in
`keyofIsLiteralContexualType`; no RIGHT or EMPTY_RIGHT case lost.

**Not reached:** `wideningTuples5` (binding elements of a variable pattern
take the parent tuple's element types, which needs
`getTypeForBindingElement`), `wideningTuples7` (the function-return arm,
`WideningKindFunctionReturn`; `check_implicit_any_return` is held for
r7-parser), and `destructuringWithLiteralInitializers2` (strict mode, so the
flag comes from `nonInferrableAnyType`, which is not ported).

## 11. Closed mapped instances relate structurally, and deep nesting follows native's creation order

Routed from r7-declared: `deeplyNestedMappedTypes` loses its TS2322 at
(69,5)/(77,5) once r7-declared's mapped stack (`tsr-2zk.1266`, held as
`r7-declared-HELD-mapped-stack.diff`) evaluates TypeBox's
`PropertiesReduce<…>` to object types.

**What the relater saw.** On batch-5 main plus the stack, the top-level
`Input → Output` pair was Unknown at the no-members-table fall-through, not
the cached overflow the routing note described. Main has moved since then.
Both sides are closed mapped instances (`Evaluate<…>`'s `{ [K in keyof O]:
O[K] }`): `Named { members: None }`, in `mapped_types`, with a complete
captured property list. The relater's `has_members` admitted no ownerless
type but the import-attributes mint, so the walk never started.

**Part 1: a closed mapped instance with a complete capture is structured.**
Native relates every mapped type through `resolveMappedTypeMembers`' table,
and the captured list is that table. Alone, this measured on main: types +8,
diagnostics +2, **−1** (`deeplyNestedMappedTypes` 45:7, a false TS2322 on
`NestedRecord<"x.y.z.a.b.c", number> → …<…, string>`). §7's refused
widening, which admitted every ownerless capture (spreads and rests too),
lost five cases. This is that list's mapped subset, and the one loss is
Part 2's.

**Part 2: `isDeeplyNestedType`'s id order is creation order.**
`isDeeplyNestedType` counts stack entries with the same recursion identity,
but only those "with a higher type id than the previous occurrence, since
higher type ids are an indicator of newer instantiations caused by
recursion". Native instantiates a mapped type's template and resolves its
members on first read, so `NestedRecord`'s nested `{ [P in K0]: … }`
instances are created during the walk, with rising ids. After three it is
deeply nested, the walk answers Maybe, and tsgo reports nothing. This port
evaluates the chain eagerly, inner first, so the same instances have
falling ids, the count never passes one, and the leaves `number`/`string`
are related.

Ids cannot be read in reverse either. Explicit nesting also has falling ids,
in native too, because type arguments are resolved first. Oracle-checked:
tsgo reports `Record<"a", Record<"a", Record<"a", Record<"a", number>>>>`
against its `string` twin, and four nested written mapped types, while it
accepts `NestedRecord`. A two-direction count measured −1 case
(`deeplyNestedCheck`) and −6 type lines. "Not one of the instance's mapper
arguments" was tried too: it fixed `NestedRecord` but broke TypeBox, whose
nested `Evaluate` instances already existed, built by the inner
`Type.Object` calls before the outer instance. Native also sees those as
older.

The exact emulation of native's order is a **watermark**. A nested instance
reached from mapped instance `M` is newer than `M` in native's lazy order
iff it was created while `M` was being evaluated. Native would have created
it on first read. A type that existed before `M`'s evaluation keeps its id
order, as native's would. `MappedTypeInfo.evaluation_start` records
`store.len()` when `mapped_type_info` starts (before the template is
evaluated) and when `instantiate_mapped_type_worker` starts.
`created_during_mapped_evaluation(last, previous)` reads it, and the count
admits `previous` when its id is higher *or* it was created during `last`'s
evaluation.

**Cache/table convention.** `evaluation_start` is a field of the existing
`MappedTypeInfo` side entry (owner `mapped.rs`, keyed by the instance
`TypeId`). It is written once when the info is built, before publication,
and never changed. It is read only by the relater's deep-nesting count. No
new table, and no new expensive work: one `usize` per mapped info.

**Measured** (unfiltered, both dumps, against batch-5 main `20501307`):

| tree | types RIGHT | diagnostics |
|---|---|---|
| this branch's tip + Parts 1–2, before Part 1's decline | +8, 0 lost | +2 (`mappedTypeAsStringTemplate`, `paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized`), 0 lost (a unit test failed, below) |
| r7-declared stack (`.1266` + `e291ff89` + members diff) alone | +75, 0 lost | −1 (`deeplyNestedMappedTypes`) |
| stack + Parts 1–2 | +83, 0 lost | +2 over the tip, **0 lost** |

On the stack, the file gives exactly tsgo's five TS2322s: 10:7, 18:7,
70:5, 74:5, 78:5.

**Part 1's decline: instances built by `instantiate_mapped_type`.** The
gate's `reverse_mapped_filters_use_captured_computed_name_origins` caught a
member defect that Part 1 would expose. `strings<T>(value: { [K in keyof T &
string]: Box<T[K]> })` over an inferred `T` resolves its parameter instance
to `{ 0: Box<{ value: string }>; 01: Box<number>; … }`: the wrong keys and
member types. tsgo relates the argument, and this port reported a false
TS2345 once the instance became structured. Before Part 1, the relater's
refusal hid it. The admission is therefore limited to node evaluations
(`instance: None`, the alias-evaluation mints TypeBox and `NestedRecord`
produce). An instance from `instantiate_mapped_type` keeps the old decline
until its member resolution is right (r7-declared, `mapped.rs`; proposed
issue in §13). With that limit: main types +7, 0 lost, and no case moves
(`mappedTypeAsStringTemplate` and
`paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized` were instance
gains and go back); stack + Parts 1–2: types +77, 0 lost, 0 cases lost.

The convention record for the watermark is in
[checker-relation-publication.md](../../architecture/checker-relation-publication.md#deep-nesting-order-for-eagerly-evaluated-mapped-instances-r7-reports-tsr-2zk1276).

**Ownership.** Part 2's field is in `mapped.rs` (r7-declared), granted by the
integrator for this commit. Its diff is
[`r7-reports-mapped-evaluation-start.diff`](r7-reports-mapped-evaluation-start.diff)
(11 lines). The relater half cannot compile without it, so the two land
together.

**Falsifier.** A recursive non-homomorphic mapped chain that tsgo reports,
where this count now finds it deeply nested, would show a nested instance
that native resolves before its outer instance even though this port
created it during the outer's evaluation (an instantiation-cache hit native
has and this port does not).
## 12. Two `has_no_contextual_type` arms: an uncontextual parameter's initializer and a `yield` operand (granted: `signatures.rs`)

**Forcing fact.** TS7057 (`'yield' expression implicitly results in an 'any'
type …`) was missing for `function*foo(a = yield) {}`
(`FunctionDeclaration6_es6`, `FunctionDeclaration7_es6`) and for the inner
`yield` of `yield yield` (`generatorTypeCheck50`).
`check_implicit_any_yield_expression` reports only where
`has_no_contextual_type` shows the yield's contextual type is nil, and that
walk had no arm for either parent.

**Upstream.**
- A parameter's initializer: `getContextualTypeForInitializerExpression` →
  `getContextualTypeForVariableLikeDeclaration` is the annotation, else
  `getContextuallyTypedParameterType`, which is nil outright for a function
  that is not `isContextSensitiveFunctionOrObjectLiteralMethod`. Then a
  non-empty binding-pattern name supplies the pattern's type.
- A `yield` operand: `getContextualTypeForYieldOperand` reads
  `getContextualReturnType(containing function)`: the return annotation,
  the contextual signature, and finally the IIFE arm (the call's own
  contextual type).

**Port** (`signatures.rs`, granted by the integrator for this commit). The
parameter arm shows absence for an unannotated, identifier-named parameter
of a declaration, class method, constructor or accessor. A
context-sensitive owner is left undecided. The yield arm mirrors the
`return` arm: no return annotation, then
`declaration_takes_no_contextual_return`. It declines two owners that
helper does not separate: an immediately invoked function (the IIFE arm)
and an object-literal method (its contextual signature comes from the
literal). The first unfiltered run without those two declines lost four
EMPTY_RIGHT cases (`generatorTypeCheck27/29/30/64`, a `yield x => …` in an
IIFE generator under an annotated `yield*`) and then one more
(`generatorTypeCheck28`, the object-literal method).

**Measured** (unfiltered, both dumps, against batch-5 main `20501307` plus
this branch): WRONG → RIGHT `FunctionDeclaration6_es6`,
`FunctionDeclaration7_es6`, `generatorTypeCheck50`, and
`duplicateIdentifierBindingElementInParameterDeclaration1/2` (their TS7006
on an uncontextual parameter's initializer callback); types +25 RIGHT, 0
lost. The walk is shared with the type producers, so their decline on these
positions answers now. No RIGHT/EMPTY_RIGHT case or type line lost.

`generatorImplicitAny` (26,7) `f(yield)` stays: the yield is an argument of
a generic call, whose contextual type is the instantiated parameter, not a
shown absence.

## 13. `hasCommonProperties` over an intersection reads every constituent's properties

**Forcing fact.** `tsxStatelessFunctionComponentsWithTypeArguments4` (12,15):
`<OverloadComponent {...arg1} ignore-prop />` with `T extends { b: number }`
is TS2769. Its first overload takes no props, so its target is the weak
`IntrinsicAttributes`. The attributes type `T & { "ignore-prop": true }`
shares no property with it, and the candidate fails. TSR chose that
candidate.

**Upstream.** `isRelatedTo`'s common-property check (`relater.go`) reads
`getPropertiesOfType(source)`: the reduced apparent type, here `{ b: number
} & { "ignore-prop": true }`. `getPropertiesOfUnionOrIntersectionType`
creates a property for every name any constituent declares. The
intersection is not `ObjectFlagsJsxAttributes` (only the minted object
part is), so `ignore-prop` is not known to `IntrinsicAttributes` either.

**Port** (`assignreport.rs`, `common_check_property_names`).
`get_property_names_of_type` answered None for that intersection, so
`fails_common_property_check` declined. An intersection's names are now the
union of each constituent's apparent names, and stay None if any
constituent's names are not enumerable.

**Measured** (unfiltered, against batch-11 main `86d9406e` plus §11–§12):
WRONG → RIGHT `tsxStatelessFunctionComponentsWithTypeArguments4`; no other
row changed; type lines identical.

## 14. `isWeakType`'s substitution arm

**Forcing fact.** `noInferCommonPropertyCheck1`: `test1(partialObj1,
someObj1)` with `b: NoInfer<T> & { prop?: unknown }` is TS2559 (`Type '{ x:
string; }' has no properties in common with type 'NoInfer<Partial<…>> & {
prop?: unknown; }'`), and the same holds for `NoInfer<T1> & NoInfer<T2>`.
TSR reported neither.

**Upstream.** `isWeakType` (`relater.go:681`): an object type by its
members, an intersection when every constituent is weak, and a
**substitution** type by its base (`t.AsSubstitutionType().baseType`).
`NoInfer<T>` is a substitution type.

**Port** (`assignreport.rs`, `is_weak_type`). A `NoInfer<T>` reference
(`no_infer_base_type`) answers its base's weakness. Before this it answered
None (undecided), so the intersection was never weak.

**Measured** (unfiltered, against `86d9406e` plus §11–§13): WRONG → RIGHT
`noInferCommonPropertyCheck1`; no other row changed; type lines identical.
`nestedExcessPropertyChecking` (30,22) stays: the elaboration's target for
the nested `overrides` property comes from an intersection with a
`Partial` instance, which `elaborate_object_literal_members` does not read.

## 15. TS7034 / TS7005 for an auto-typed variable read where its type cannot be determined

**Forcing fact.** `controlFlowNoImplicitAny`,
`implicitAnyDeclareVariablesWithoutTypeAndInit`, `narrowingPastLastAssignment`,
`tsxEmit1` and `tsxReactEmit1` miss `Variable 'x' implicitly has type 'any' in
some locations where its type cannot be determined.` at the declaration, and
`Variable 'x' implicitly has an 'any' type.` at a read. A typical read is a
captured `let x;` inside a nested function, before any assignment it can
see.

**Upstream.** `checkIdentifier`'s auto arm (`checker.go:11182`): for an
auto-typed variable (`autoType`, from `getTypeForVariableLikeDeclaration`'s
auto arm), a read whose control-flow type is still `autoType` reports both
diagnostics under `noImplicitAny` and answers `convertAutoToAny`.

**Port.** r7-flow ported the flow test as
`identifier_flow_type_is_auto` (`flow.rs`, `docs/parity/notes/r7-flow.md`
§8). The reporter is `check_implicit_any_auto_reference` (`implicit_any.rs`),
dispatched from the identifier arm of the check walk beside
`check_used_before_assigned` (`check.rs`, granted). The walk is the site
because the expression cache would skip a re-check. It asks for a value
reference that is not a definite assignment target, which `checkIdentifier`
returns early for, and not an export assignment's expression, as the
prototype did. It also asks for a declaration with no JSDoc `@type`: in JS
the reparsed tag is the annotation, which the auto arm never reaches. The
flow query does not see it, and the first unfiltered run reported on
`typeFromJSInitializer4`'s `/** @type {number | undefined} */ var n;` (one
RIGHT case lost, fixed by this guard). Native's diagnostic collection drops
the identical TS7034 a second read would add; the reporter does too.

**Measured** (unfiltered, against batch-13 main `f960020e` plus §11–§14):
WRONG → RIGHT the five cases above; no other row changed; type lines
identical. Not covered: the auto-array track (`autoArrayType`,
`controlFlowArrayErrors`, `evolvingArrayResolvedAssert`), which the flow
query does not answer.

## 16. TS2719: `reportRelationError`'s same-name arm

**Forcing fact.** `incompatibleAssignmentOfIdenticallyNamedTypes`: `this.x
= a` with `a: T` (an interface) and `x: T` (the class's type parameter) is
TS2719, `Type 'T' is not assignable to type 'T'. Two different types with
this name exist, but they are unrelated.`. TSR printed TS2322. §4 left the
arm unported because the printer had no fully-qualified mode.

**Upstream.** `reportRelationError` (`relater.go:4751`) takes its names from
`getTypeNamesForErrorDisplay` (`relater.go:1277`): when the plain prints
collide, both sides are re-printed with `getTypeNameForErrorDisplay`
(`TypeFormatFlagsUseFullyQualifiedType`). Then, with no head message,
`sourceType == targetType` selects TS2719 ahead of the exact-optional and
default arms. The re-printed names are also what any other head prints.

**Port** (`assignreport.rs`, `report_relation_head`). r7-printer's
`type_name_for_error_display` (`checker.rs`, `deda2594`) is the qualified
print; its `allow(dead_code)` is removed (granted). When the plain prints
collide, both sides take the qualified print, and equal qualified prints
select TS2719. That comes after a caller's head and TSR's TS2412 head arm
(native's `checkAssignmentOperator` head), and before the exact-optional
arm, in upstream's order. A literal source keeps its generalized display
text, as upstream's `generalizedSourceType` does.

**Measured** (unfiltered, against `f960020e` plus §11–§15): WRONG → RIGHT
`incompatibleAssignmentOfIdenticallyNamedTypes`; no other row changed; type
lines identical.

## 17. Indexed-access sources meet union and intersection targets through their constraint

Routed from r7-calls: `relate_ternary(T["kind"], SK)` with `T extends { kind:
SK }` answered NotRelated, which gave `h(k)` a false TS2345 and held r7-calls'
`createToken` fix.

**Upstream.** `structuredTypeRelatedToWorker` (`relater.go:3380`): after the
ordered union/intersection decomposition fails, an *instantiable* source
(type parameter, indexed access, conditional, …) falls through to its own
arms. The type-variable arm relates `getConstraintOfType(source)`
(`relater.go:3652`): for `T["kind"]` that is `Node0["kind"]` = `SK`.

**Port** (`relater.rs`). The union- and intersection-target arms already
fell through for a type parameter and a deferred conditional. They now do so
for an indexed-access source as well. r5-relater5 §3 held this half because
it lost 6 type lines in `quickinfoTypeAtReturnPositionsInaccurate`. Re-measured
on today's main, the same 6 lines are lost, and their cause is in this file
too:

- The call `isNumClass(entry)` now checks, so `entry` narrows by the
  predicate `Extract<Entries[EntryId], NumClass<any>>`.
- `getNarrowedType` keeps the candidate for the `NumClass` constituent of
  the receiver's constraint, and needs `Extract<…> → StrClass<string>` to be
  *false* to drop the other.
- That relation was Unknown. The conditional source's default constraint
  takes the true type through `getConditionalFlowTypeOfType`, a substitution
  type `E[I]` with implied constraint `NumClass<any>`, which a relation source
  reads as `getSubstitutionIntersection` = `NumClass<any> & E[I]`.
  `inferred_true_type_of_check_reference` answered None (undecided) for any
  type-variable check that is not restrictively assignable.
- It now answers the intersection, except for a root nested in another
  conditional's true branch. There native intersects every enclosing
  conditional's implied constraint, and only this one's is known here.
  `Extract2<T, Foo, Bar>`'s nested root made `conditionalTypes2` (69,12) a
  false TS2345 before this limit.

**Decline: a non-public member listed by a mapped part.** The unit test
`unsupported_mapped_source_intersection_shapes_retain_unknown` (`Req<T> & {
extra: number } → PrivateState`, `T extends PrivateState`) flipped from
Unknown to *Related*, while tsgo reports TS2741. Its `T["d"] → boolean` now
relates through the constraint (correctly), which unmasked a member-list
defect. Native's mapped members drop non-public keys
(`getLiteralTypeFromProperty`, `NonPublicAccessibility`), so `Req<T>` has
no `a`. This port's list carries the private `a`. The property walk now
declines (Unknown) a source member that only a mapped part supplies from a
private or protected origin. The producer fix is `mapped.rs`'s (proposed in
the report).

**Decline: an intersection whose combined-constraint retry reads a
conditional's constraint.** *Corrected:* the first draft of this section
recorded `controlFlowGenericTypes` (22,9) as a known exposed gap and said the
relation `NonNullable<Extract<T, string | undefined>> → string` "is now
false, as native's is". That was wrong. Checked against pinned tsgo
(`function h<T>(y: NonNullable<Extract<T, string | undefined>>): string {
return y }` has no error; `y`'s base constraint is not a union, so
`getNarrowableTypeForReference` does not substitute it and the relation
itself is decided true).

- Native's route is `structuredTypeRelatedTo`'s combined-constraint retry
  (`relater.go:3216`). `getEffectiveConstraintOfIntersection` takes the
  conditional's `getConstraintOfConditionalType`. That is the default
  constraint, the substitution `T` with implied constraint `string |
  undefined`. With the `{}` constituent, `getNormalizedType` gives `T &
  string`, which relates.
- This port's `constraint_of_type` leaves a conditional undecided, and
  `effective_constraint_of_intersection` then falls back to its base
  constraint. That is None here, so the retry was skipped as though native
  had no constraint either.
- The newly decided `Extract<…> → string` = false (correct for the
  conditional alone) thereby became a decided false for the intersection.
- The retry site now answers Unknown when the walk failed, the retry would
  run (union target or a disjoint-domain / empty-object constituent), and a
  conditional constituent has neither a decided constraint nor a base
  constraint. The producer fix is `getConstraintOfConditionalType` in
  `constraints.rs`, which can reuse this file's default-constraint arm.
  It is proposed in the report.

## 18. TS7006 is decided at the parameter's assignment, not re-read afterwards

Closes §2. The retained `return`-position arm is deleted.

**Upstream.** TS7006 for a context-sensitive function's parameter comes
from `assignParameterType` (via `assignContextualParameterTypes` or
`assignNonContextualParameterTypes`). With a nil contextual type it widens
with `reportErrors`, once, under the contextual signature the function has
*at that moment*.

r7-contextual traced `subtypeReductionWithAnyFunctionType` with an
instrumented pinned tsgo (`return x => x.length > 0` inside
`useMemo<T>(func: () => T)`'s argument):

- `x` is assigned in inference pass 2 under `() => boolean` (`T` from the
  sibling `return () => true`).
- `isAritySmaller` rejects that signature, so `x` is implicitly `any` and
  TS7006 fires.
- Later, `T` is inferred as `(x: any) => boolean` from `x`'s own `any`. A
  rule that re-reads the contextual signature after resolution sees a
  usable signature and stays silent.

§2's routing premise, that the producer's `(x: any)` was wrong, does not
hold. The types already match native. Only the moment of the decision
differed.

**Port** (`implicit_any.rs`; the assignment hook in `symbols.rs` and the
side table in `checker.rs` are a grant request).

- *The record.* `getTypeForVariableLikeDeclaration`'s contextual-parameter
  arm records whether `get_contextually_typed_parameter_type` answered nil
  as the parameter's type was computed. The last computation wins: an
  eviction followed by a recomputation rewrites the record, and the reader
  computes the type first.
- *What is recorded.* Only a nil this port can attribute to native's
  sources 2 and 3 is kept: an `Absent` contextual signature, or one too short
  for the parameter. A nil that is the contextual machinery declining is not
  kept. Recording every nil lost `contextuallyTypedByDiscriminableUnion2` and
  `checkJsdocSatisfiesTag2`, whose parameters were first typed before their
  generic-call or `@satisfies` context was available.
- *Too short within a generic call.* `within_generic_call_argument`'s
  decline (the inference mapper is only partly applied) now yields when the
  instantiated contextual type is a single callable type that
  `isAritySmaller` rejects for every signature. That answer reads only
  signature lengths. The decline exists for a union of both conditional
  branches, which is a different shape. useMemo's pass-2 `() => boolean` is
  this shape.
- *The reader.* `contextual_parameter_type_is_absent`, after the IIFE and
  no-contextual-type arms, reports a parameter whose record says nil while
  the context now answers a type.

**Measured** (diagnostics, unfiltered, against batch 17 with §17 on top):
+12 cases, 0 lost. Every target is RIGHT:
`asyncFunctionContextuallyTypedReturns`, `contextualTypeOnYield2`,
`inferPropertyWithContextSensitiveReturnStatement` and
`invalidThisEmitInContextualObjectLiteral`.
`subtypeReductionWithAnyFunctionType` stays RIGHT.

**Falsifier.** A case where a parameter is first typed under a decided nil
and then re-typed under a real context after an eviction that is never
recomputed. The reader recomputes first, so the record would have to come
from a computation that native does not repeat. A new TS7006 extra would
show it.

## 19. `distributeObjectOverIndexType` for every object, not only a mapped one

**Forcing case.** `deepComparisons` (4,9). `T[K1][K2] → Extract<T[K1][K2],
string>` (with `K2 extends keyof T[K1]`) is false natively. Its chain runs
through `T[K1][keyof T[K1]]` and `T[K1][string] | T[K1][number] |
T[K1][symbol]` down to `T[string][string]`, which has no constraint. This
port kept `T[K1][string | number | symbol]` whole. Its next constraint,
`T[string | number | symbol][string | number | symbol]`, answered Unknown,
so the line was missing.

**Upstream.** `getSimplifiedIndexedAccessTypeWorker` (`checker.go:27930`)
asks `distributeObjectOverIndexType` (`:27975`) first, for any object: a
union index gives `T[A] | T[B]` when reading and `T[A] & T[B]` when writing,
each access simplified in turn. Only after that come the object
distribution, the generic tuple and the mapped substitution.

**Port** (`relater.rs::simplified_indexed_access_worker`). The union-index
distribution moves out of the mapped-object arm and comes first, as native's
does. The mapped arm now sees only a non-union index, and its union branch
is gone. Over a mapped object the two orders give the same types: each
`{…}[A]` is substituted when it is simplified. The generic-tuple arm is
still not ported (stated in the rustdoc).

**Measured** (diagnostics and types, unfiltered, against batch 19 with
§17 and §18 on top): `deepComparisons` RIGHT, no case or type line lost,
types unchanged.

**Falsifier.** A pair whose union index distributes into a constituent this
port then answers differently from the whole access, so a RIGHT relation
turns. None was measured.

## 20. Three nil contexts the TS7013 / TS7006 rules missed

Three missing lines, each a nil that native reports on and this port either
skipped or could not prove.

- **`noImplicitAnyModule` (4,9), TS7013.** `checkSignatureDeclaration`
  (`checker.go:2749`) reports a construct or call signature with no return
  annotation under `noImplicitAny`, and nothing more. There is no ambient
  condition. `check_implicit_any_signature_return` returned early on the
  walk's `ambient` flag, which silenced `new ();` in an interface inside a
  `declare namespace`. The flag is no longer read. `check.rs` still passes
  it; the parameter is `_ambient` so the call site stays untouched.
- **`tsxInArrowFunction` (15,7), TS7006.** `getContextualTypeForChildJsxExpression`
  (`jsx.go:241`) is nil when `getJsxElementChildrenPropertyName` answers
  `Missing` or `""`, whatever the attributes type. Under the classic
  runtime that answer is decided: there is no `ElementChildrenAttribute`
  (or no JSX namespace), or a container whose complete member table does not
  hold exactly one name. The automatic runtime reads the import source's
  namespace, which `jsx_children_name` does not, so it is not decided.
- **`tsxAttributeResolution2` (13,18), TS7006.** `getContextualTypeForJsxAttribute`
  (`jsx.go:214`) is nil when `getTypeOfPropertyOfContextualType` finds
  neither a member nor an applicable index signature (`data-c1` on an
  intrinsic element whose props lack it). This is decided only for an
  intrinsic element whose props type is computed, not `any`, not a union,
  not instantiable and not mapped, with a complete member table.
  - The first draft also decided this for value tags. It lost
    `tsxInferenceShouldNotYieldAnyOnUnions` (26,53)(27,51): a generic
    component's attributes type is the candidate's mid-inference
    instantiation, not the declared props. That draft was refused.

The two JSX proofs are `jsx_intrinsic.rs::jsx_children_name_is_absent` and
`jsx_attribute_context_is_absent`. The implicit-any rule reaches them
through `jsx_context_is_absent`, after the no-contextual-type arms. It looks
through parentheses and refuses a spread child.

**Measured** (diagnostics, unfiltered, against batch 19 with §17–§19 on
top): +3 cases, 0 lost.

## 21. A union JSX attributes target's excess check, and discriminated intersections

**Forcing case.** `tsxSpreadAttributesResolution6` (15,10). `<TextComponent
editable={true} />` against `IntrinsicAttributes & IntrinsicClassAttributes<…>
& TextProps & { children?: ReactNode }` is false natively, with
`TextProps = { editable: false } | { editable: true, onEdit: … }`. Two
undecided steps hid the line.

1. **`jsx_excess_attribute` declined every union target.** `hasExcessProperties`
   (`relater.go:2714`) reduces a union target by `findMatchingDiscriminantType`,
   else by `filterPrimitivesIfContainsNonPrimitive`. Each written attribute
   must then be `isKnownProperty` in the reduced target, and its type must
   relate to `getTypeOfPropertyInTypes` over the reduced constituents
   (`checkTypes`). The JSX check now runs that arm. It reuses
   `assignreport.rs`'s `find_matching_discriminant_type`,
   `filter_primitives_if_contains_non_primitive` and
   `type_of_property_in_types`, which are now `pub(crate)`.
   - An unknown attribute is reported against the reduced target, as
     native's error target is.
   - A failed property relation is `JsxExcess::Incompatible`, and the
     relation fails with it (native's `hasExcessProperties` returning true).
2. **`type_related_to_discriminated_type` answered Unknown for an
   intersection match.** The combination `editable: true` matches the
   second constituent. Native then relates the remaining members of that
   intersection (`propertiesRelatedTo(source, type, excluded)`), and the
   missing `onEdit` fails it. This port could not enumerate the
   intersection's combined member list (`IntrinsicClassAttributes<T>`).
   Each property of an intersection is the intersection of its
   constituents' properties, so a constituent whose remaining members fail
   (a required member the source lacks, or one it mistypes) fails the
   whole. The arm now asks each constituent when the whole is undecided.

**Measured** (diagnostics and types, unfiltered, against batch 19 with
§17–§20 on top): +1 case, 0 lost, types unchanged.

## 22. A closed mapped source has no signatures for `shouldReportUnmatchedPropertyError`

**Forcing case.** `consistentAliasVsNonAliasRecordBehavior` (18,5)(22,5)(34,5)(38,5).
`Record<string, string> → Record2<"a", string>` is false natively with head
TS2741 (`Property 'a' is missing…`). This port kept TS2322:
`unmatched_property_report` asked `signatures_of_type_kind` of the source
for `shouldReportUnmatchedPropertyError` (`relater.go:959`), and for a mapped
alias image that reader answers None (undecided).

**Upstream.** `resolveMappedTypeMembers` publishes no call or construct
signatures, so a mapped source is never signature-only and the gate passes.

**Port.** A mapped source that is not `isGenericMappedType` reads as having
no signatures. A generic mapped source is left to the reader:
`mappedTypeUnionConstrainTupleTreatedAsArrayLike` (9,9) keeps native's
TS2322 head, because there the failure comes from the source's constraint,
not this arm. The first draft took every mapped source and lost that case
(TS2740).

**Decline: a missing late-bound name of a mapped source.** This port's
mapped member lists omit late-bound keys. `{ [K in keyof number[] as
Exclude<K, "length">]: … }` (`mappedTypeWithAsClauseAndLateBoundProperty`
(3,1)) and `Readonly<PathPathComponents>` (jsTyping's `resolutionCache.ts`
(460,44)(463,84)) list no `[Symbol.iterator]` or `[Symbol.unscopables]`.
Without the decline, the first draft moved the jsTyping lines from a false
TS2322 to a false TS2739 (`new_false=2`), and the corpus case from TS2322 to
TS2739 where native has TS2741. A mapped source whose missing list holds a
late-bound name (`[…]`) keeps the caller's head. The member-list producer
fix is `members.rs`/`mapped.rs` (reported).

**Measured** (diagnostics, unfiltered, against batch 19 with §17–§21 on
top): +1 case, 0 lost.

## 23. `elaborateObjectLiteral` speaks before `hasExcessProperties`

**Forcing case.** `objectLiteralFunctionArgContextualTyping2` (11,6)(12,6)(13,17).
`f2({ toString: (s) => s })` against `I2` (`value`, `doStuff`) is native
TS2322 at `toString`: `Type '(s: any) => any' is not assignable to type '()
=> string'`. This port reported TS2353 (`'toString' does not exist in type
'I2'`).

**Upstream.** `checkTypeRelatedToAndOptionallyElaborate` runs `elaborateError`
before `checkTypeRelatedTo`. `elaborateObjectLiteral` relates each member,
in source order, to `getBestMatchIndexedAccessTypeOrUndefined(source,
target, name)`. That is an indexed access through `getPropertyOfType`, so it
finds an apparent `Object` member such as `toString`, which `isKnownProperty`
(own members and index signatures only) does not know. Every failing member
is reported. Only when none is does the relation run, and
`hasExcessProperties` reports the first unknown member.

So `{ hello: 1, value: 2 }` against `I2` is TS2322 at `value` with no
TS2353, and `{ value: 2, hello: 1 }` is the TS2322 alone (both checked
against pinned tsgo). This port reported the first unknown member as soon
as it met it, before any later member was related, and treated an apparent
member as unknown.

**Port** (`assignreport.rs::check_excess_properties`). Two passes. First,
every member whose name is a known member, or for which
`get_type_of_property_of_type` finds a member (the apparent `Object`
members included), is related and reported (`check_object_literal_member`).
If that reported anything the check ends. Otherwise the first member
outside the known table is the excess report, as before.

**Measured** (diagnostics, unfiltered, against batch 22 with §17–§22 on
top): `objectLiteralFunctionArgContextualTyping2` RIGHT, 0 lost.

## 24. The excess check reads index signatures instead of declining them

**Forcing case.** `objectLitIndexerContextualType` (18,5). `y = { s: t => t
* t }` with `J { [s: number]: … }` is native TS2353: `isKnownProperty`
(`relater.go:719`) knows a declared property or a name that an applicable
index signature admits (`getApplicableIndexInfoForName`). A number index
signature does not admit `s`.

**Port** (`assignreport.rs::check_excess_properties`).
- This check declined every target with an index signature, through
  `relation_members_are_complete`, on the ground that "an index signature
  makes every name known". That holds only for a string index signature.
- It now reads the target's index infos when `relation_property_table`
  answers (the declared table is complete apart from index signatures). A
  name is known when it is declared or an index signature applies to it.
- The elaboration pass (§23) relates a member admitted only by an index
  signature to that signature's value, as `getIndexedAccessTypeOrUndefined`
  does.
- An index-signature-only target is not the empty target the TS2322 rule
  exempts.

**Measured** (diagnostics, unfiltered, against batch 23 with §17–§23 on
top): `objectLitIndexerContextualType` RIGHT, 0 lost.

## 25. Refused: relating a union-bodied alias image as its body

**Finding.** In `typeAssignabilityErrorMessage` (40,5), `Foo<string> →
Bar<number>` with `Bar<T> = { foo: { what: T } } | boolean` is Related here
and false natively. The target is an alias-reference image. Its body has
object constituents, so `non_object_alias_image_body` keeps the image (the
r5-relater4 §1 memory blow-up). The image's member table is the alias
symbol's, which lists none of the constituents' members, and the
structural walk relates vacuously. The same vacuity is behind §17's
`Schema1<…>` image, there with an intersection body.

**Tried.** Relate an image whose body is a union as that body, unless the
other side is an image of the same alias (which keeps the variance road).
- Unrestricted, it lost `assignmentTypeNarrowing` (33,7) and
  `recursiveReverseMappedType` (8,5). A literal source's own type was
  computed with the image as contextual type, which this port does not read
  as the union: `[{ x: "ok" }]` widens to `{ x: string }[]`, and `['marker',
  ...l]` is no tuple.
- With object-literal, array and tuple sources keeping the image, it gained
  `typeAssignabilityErrorMessage` and `jsxElementTypeLiteralWithGeneric`
  with no case lost. It still lost the type line
  `styledComponentsInstantiaionLimitNotReached` (0:82): `children` printed
  `React.ReactNode` expanded. Its run time also went from 50 to 75 ms.

**Refused** on that type line (gate: 0 lost). **What would change it:** an alias-reference producer whose image exposes the union body
to both the relater and the contextual machinery (declared.rs; routed). With
that, the vacuous structural walk goes away at its source, and the
literal-source restriction is no longer needed.

## 26. Refused: an arrow-body arm in `has_no_contextual_type`

**Tried.** `getContextualType` answers `KindArrowFunction` with the `return`
arm's `getContextualTypeForReturnExpression` (`checker.go:29358`). A
`has_no_contextual_type` arm mirroring the `return` arm fixed
`fatarrowfunctionsOptionalArgs` (88,31)(88,46), with a JSDoc-`@returns`
decline for `instantiateTemplateTagTypeParameterOnVariableStatement` (6,18).

**Refused.** It gained 43 type lines elsewhere but lost 12 in
`conditionalTypeDoesntSpinForever`:
`BuildPubSubRecordType<…>` printed `error`. The first measurement was
diagnostics-only and missed them, and a types dump found them.
`has_no_contextual_type` is also a type producer's proof (the written-tree
proof the reporter's JS variant shares), so a new arm there changes which
arrows' return types are inferred without context. The reporter needs the
proof but the producers do not. §27's reporter-side walk covers the same
lines (an owner with no contextual type at all is its first branch), so no
`signatures.rs` arm is needed. **What would change it:** a
`has_no_contextual_type` that a type producer does not read.

## 27. An arrow body under an owner with a decided-absent contextual signature

**Forcing lines.** `fatarrowfunctionsOptionalArgs` (129,13)(129,20).
`foo(..., (a) => (b) => (c) => 121)` with `foo(...arg: any[])`. The
argument's contextual type is `any`, which has no call signature. So the
outer arrow's `getContextualSignatureForFunctionLikeDeclaration` is nil,
`getContextualReturnType` gives its body no context, and `b` (then `c`) is
implicitly `any`.

**Port** (`implicit_any.rs::return_context_is_absent`).
- The function is an arrow's expression body (parentheses looked through).
  Its owner arrow has no return annotation (written or JSDoc `@returns`)
  and is not an IIFE.
- The owner's contextual signature must be decidedly nil, by the same
  decision its own parameters' TS7006 reads: `Absent` (outside a generic
  call, or arity-rejected there), or no contextual type at all.
- An owner whose signature is undecided is itself asked one level up, to
  depth 8. If that owner is the body of a context-free arrow, its type is
  nil too.

The owner with no contextual type at all (`has_no_reparsed_contextual_type`)
is the first branch, and it covers (88,31)(88,46), the lines §26's refused
`signatures.rs` arm aimed at. The rest is the semantic half, kept in the
reporter because it needs the contextual signature.

**Measured** (diagnostics and types, unfiltered, against batch 28 with
§22–§25 on top): `fatarrowfunctionsOptionalArgs` RIGHT (all four lines),
0 lost.

## 28. A tuple source's discriminant candidates

**Forcing case.** `jsxChildrenWrongType` (11,9)(12,9). The children
attributes `{ children: [unknown, string] }` against `children: [string,
number?] | Iterable<boolean>` are native-false and report TS2769. This port
answered Unknown. `type_related_to_discriminated_type` asks the source's
property names first, and a tuple reference's name list is undecided here,
so every tuple source's discriminated-union arm was Unknown.

**Port** (`relater.rs::tuple_discriminant_candidates`). For a fixed
(non-variadic) tuple source the arm reads the element names and `length`.
Native's `getPropertiesOfType` of a tuple also lists the global `Array`
members, but none of them can be a discriminant: `isDiscriminantProperty`
needs `CheckFlagsHasLiteralPart`, and they are methods or the
`[Symbol.unscopables]` object. The rest of the arm is unchanged: a target
constituent without one of the names is skipped, as native's
`getPropertyOfType` miss is, and a combination with no match is false.

**Measured** (diagnostics and types, unfiltered, against batch 28 with
§22–§27 on top): `jsxChildrenWrongType` RIGHT, 0 lost, types unchanged.

## 29. `anyFunctionType` against any member-less object target

**Forcing case.** `tsxStatelessFunctionComponentOverload5` (50,24). In the
`SkipContextSensitive` pass, `onClick={(e)=>{}}` is `anyFunctionType`
against `ButtonProps.onClick: React.MouseEventHandler<any>`. The pass's
relation was Unknown, so overload resolution declined and TS2769 was
missing.

**Upstream.** `signaturesRelatedTo` answers True outright when the source is
`anyFunctionType`. That type has no property and no index signature, so
against an object target with neither, nothing else remains to relate.

**Port** (`relater.rs`). The wildcard read only targets that
`is_pure_signature_type` accepts: a signature list from a function-like
declaration. `MouseEventHandler<T> = EventHandler<MouseEvent<T>>` is an
alias-reference image whose body does not evaluate here (r7-declared's
alias-image item), but its member list is decided-empty and it carries one
call signature. The wildcard now covers any non-composite object target
whose decided property list and index infos are empty.
- An alias image must also carry a call signature. An image's member list
  can be the alias symbol's own, empty whatever its body holds (§25); a
  signature-bearing image names a function type, whose literal declares no
  property.

**Measured** (diagnostics and types, unfiltered, against batch 28 with
§22–§28 on top): `tsxStatelessFunctionComponentOverload5` RIGHT, types +11,
0 lost.

## 30. An array-like JSX children part answers element positions through its number index

**Forcing case.** `checkJsxChildrenProperty4` (38,15)(41,15). The children
target is `((user: IUser) => Element) & ReactNode`. It distributes into
array-like parts such as `((user) => Element) & Array<ReactChild | any[] |
boolean>` and non-array parts such as `((user) => Element) & string`.

**Upstream.** `elaborateIterableOrArrayLikeTargetElementwise` (`jsx.go:419`)
relates each child to the non-array parts' iteration type (`string`) united
with `getBestMatchIndexedAccessTypeOrUndefined` over the array-like part at
the child's position. For an intersection with an array, that is the
number index signature's element: `boolean | any[] | ReactChild`. The
child, a function, fails it. `elaborateDidYouMeanToCallOrConstruct` then
reports at the arrow (column 15), because the function's return
`Element` does relate.

**Port** (`jsx_component.rs::elaborate_jsx_children_elementwise`).
`array_or_tuple_element_access` answers only for array and tuple
references, so the indexed part was dropped. The target was `string`, the
did-you-mean arm saw `Element → string` fail, and the report fell to the
`JsxExpression` (column 13). An array-like part that is no tuple reference
now answers through its applicable number index signature. A tuple's
out-of-range position still answers nothing, as native's indexed access
does.

**Measured** (diagnostics and types, unfiltered, the change alone on the
same tree): `checkJsxChildrenProperty4` RIGHT, 0 lost, types unchanged.

## 31. A missing required member decides a hyphen-sensitive JSX relation

**Forcing case.** `ignoredJsxAttributes` (20,11). `<Yadda bar="hello"
data-yadda={42}/>` against `Props { foo: string; [dataProp: string]:
string }` is native TS2741 (`foo` missing).
- `jsx_attributes_relation` declines any source with a hyphenated member
  against a target where one of the two hyphen rules could decide the
  answer: `membersRelatedToIndexInfo` skipping it, or `hasCommonProperties`
  counting it. This port's relater carries no `ObjectFlagsJsxAttributes`.
- `propertiesRelatedTo` fails on a required target member the source lacks,
  and neither rule can turn that around. A hyphenated source member neither
  supplies a non-hyphenated name nor is consulted for it.

**Port.** Before declining, a target (or an intersection constituent,
whose required member is the intersection's) with a complete member table
that declares a required, non-hyphenated member the source lacks decides
NotRelated. A union target still declines, since its constituent choice is
the relation's.

**Measured** (diagnostics, the 375 JSX-named cases, the change alone):
`ignoredJsxAttributes` RIGHT, nothing else moved. The full gate follows.

## 32. A weak JSX attributes intersection, and weakness of a class or interface instantiation

**Forcing case.** `tsxSpreadAttributesResolution5` (35,10). `<EmptyProp
{...o} />` with `o: { prop1: boolean }` against `IntrinsicAttributes &
IntrinsicClassAttributes<EmptyProp> & { children?: ReactNode }` is native
TS2559. Every constituent is weak and none knows `prop1`.

**Two gaps.**
1. *`isWeakType` of an instantiation.* `IntrinsicClassAttributes<EmptyProp>`
   is weak through its inherited `ref?`/`key?`. `relation_property_table`
   declines a reference with type arguments. `is_weak_type` now falls back,
   for a class or interface instantiation that is not mapped, to the
   complete name list and each name's declaration `?`. No type argument
   changes that `?`. A member inherited through a generic base is read
   through `generic_heritage_property_symbol`, as the relater's
   `property_flags` reads it.
2. *The JSX reporter decomposed first.*
   `report_jsx_attributes_relation_failure` reports the first constituent
   that fails under `IntersectionStateTarget`, and a weak-type failure fails
   none of them. `isRelatedTo` runs the common-property check on the whole
   target before the intersection is decomposed (`relater.go:2676`). The
   reporter now asks `report_weak_type_failure` of the whole intersection
   first.

**Measured** (diagnostics and types, unfiltered, against the same tree
without the change): `tsxSpreadAttributesResolution5` RIGHT, 0 lost, types
unchanged. Cost: the first draft built each instantiation's full table on
every weak test and raised domain-model's callgrind Ir by 1.37%. The test
now stops at the first required member, which nearly every instantiation
has. That brought it to +0.25% (gi +0.02%).

## 33. An alias-named reference keeps the caller's head

**Forcing case.** `jsxClassAttributeResolution` (2,19). Under `declare
namespace JSX { type IntrinsicClassAttributes<T> =
IntrinsicClassAttributesAlias<T> }`, `<App></App>` is native TS2322: `Type
'{}' is not assignable to type 'IntrinsicClassAttributes<T>'`, with the
chain `Property 'ref' is missing in type '{}' but required in type
'IntrinsicClassAttributesAlias<T>'`. The head names the alias and the
missing-property message names the reference, so `chainArgsMatch` fails
and the head stays. This port reported TS2741 as the head.

**Port** (`relater.rs::unmatched_property_report`). r7-declared mints a
class or interface reference under an alias's printed name and records
the alias in `alias_of`. Such a side is a type the head prints differently
from the reference the chain names, so the report keeps the caller's head,
as it already does for an alias image normalization may rewrite.

**Measured** (diagnostics and types, unfiltered, the change alone on batch
37 with §30–§32): `jsxClassAttributeResolution` RIGHT, 0 lost, types
unchanged.

## 34. The intersection property-check pass over a non-enumerable target

**Forcing case.** `nestedExcessPropertyChecking` (30,22). `{ variables: {
overrides: false } }` against `Partial<{ something: any }> & { variables: {
overrides?: OverridesInput } & Partial<{ overrides?: OverridesInput }> }` is
native TS2559 at `overrides`.

**Upstream.** After relating the source to each constituent under
`IntersectionStateTarget`, `structuredTypeRelatedTo` (`relater.go:3232`)
relates the source's members to the target's combined property types with
`IntersectionStateNone`. That pass is where the nested weak type `false →
OverridesInput` fails: under `IntersectionStateTarget` the
common-property check does not run.

**Port** (`relater.rs`). The pass declined, so the constituent walk's
verdict stood, wherever `properties_related_to` could not enumerate the
target's combined member list. A mapped constituent (`Partial<…>`) is
enough to cause that. The pass's only new question is still answerable,
though: each member the source and the target share, related to the
combined property type with the state clear.
- A target member the source lacks was the constituent walk's. A member any
  constituent requires is the intersection's.
- A source member the target lacks is `hasExcessProperties`'.

So when the full pass is Unknown, `shared_members_related_to` answers it.

**Measured** (diagnostics and types, unfiltered, the change alone on batch
37 with §30–§33): `nestedExcessPropertyChecking`,
`identicalTypesNoDifferByCheckOrder` and `reactDefaultPropsInferenceSuccess`
RIGHT, one more line of `excessPropertyCheckIntersectionWithRecursiveType`,
0 lost, types unchanged.

## 35. A name written twice in an object literal: the last occurrence is the member

**Forcing lines.** `lastPropertyInLiteralWins` (8,5)(9,5). In `test({
thunk: (str: string) => {}, thunk: (num: number) => {} })` the literal's
member is the last `thunk`. `elaborateObjectLiteral` relates that one
member at every written occurrence and reports TS2322 at both (the first
occurrence through `checkExpressionForMutableLocationWithContextualType`'s
fallback to the member type).

**Port** (`assignreport.rs`).
- `check_excess_properties`' elaboration pass related only the first
  occurrence, with its own value.
- `elaborate_object_literal` read the member type from this port's literal
  type, which for a duplicated name is not the last occurrence's (it
  prints `(str: string) => error`; the producer is `objects.rs`).

Both now take the last occurrence's value type for a duplicated name and
relate it at each occurrence.

**Measured** (diagnostics and types, unfiltered, the change alone): in the
already-WRONG case (8,5)(9,5) are fixed and (14,5)'s extra TS2322 is gone.
The second call still fails, now as (12,6) TS2345 instead: its literal
type's member is the producer's broken `(str: string) => error`. No other
case moved, and types are unchanged. **What would finish it:** the
literal's own member type taking the last declaration (objects.rs,
reported).

## 36. The relation error chain, retraced over decided answers

**Forcing class.** The exact oracle's largest relater/assignreport classes
are `diag:chain`: TSR printed the head (`Type 'S' is not assignable to type
'T'.`) with no chain under it. Native re-runs the failed relation with
`reportErrors` (`checkTypeRelatedToEx`, `relater.go:359`), and every arm on
the failing path pushes its message onto `errorChain`. The chain becomes the
diagnostic's message chain, with every level carrying the error node's file
and location (`createDiagnosticChainFromErrorChain`).

**Why not in the relater.** This port's relater is three-valued and keeps
no error state. Threading a chain through it would touch every arm, and the
Unknown answers would still have to keep a chain from being reported. The
chain is instead retraced afterwards, in `assignreport.rs`, from decided
`relate_ternary` answers. Each step mirrors the native arm it names, and a
step that cannot pin its failing member abandons the whole chain, so a
guessed or partial chain is never printed.

**Arms covered** (`explain_relation`):
- **Union source** (`eachTypeRelatedToType`). The first constituent that
  fails, in union order. A primitive union (`boolean`, an enum) is related
  with `reportErrors` off (`relater.go:2873`), so it reports nothing. The
  first draft missed this and lost 10 exact cases (`for-of47`'s doubled
  `Type 'boolean' …`).
- **Primitive source.** Nothing below the relation error. Against a union,
  `typeRelatedToSomeType` elaborates only through `getBestMatchingType`, and
  `findMostOverlappyType` skips a primitive.
- **Array references.** The covariant element (`typeArgumentsRelatedTo`).
- **Plain objects** (`propertiesRelatedTo`, then `signaturesRelatedTo(Call)`):
  - the first missing required property (`reportUnmatchedProperty`'s
    message, from `unmatched_property_report`);
  - else the first target member, in declaration order, whose types fail
    (`Types of property 'p' are incompatible.`), declining a private or
    protected member;
  - else a call signature: none in the source gives `Type 'S' provides no
    match for the signature …`, and one against one goes to
    `compareSignaturesRelated`. That covers the first failing parameter
    (`Types of parameters 'a' and 'b' are incompatible.`, related in
    reverse under strict variance) and else the return types, with the
    elided return-type marker pushed for the reductions below.
- **`reportError`'s reductions.** An excess-property message suppresses
  `Types of property`. A return marker turns it into `The types returned by
  'x(...)' are incompatible between these types.` Two property messages
  become `The types of 'x.y' are incompatible between these types.`
  (`addToDottedName`).
- **`reportRelationError` per level.** The display generalization, TS2719,
  the exactOptional variant and TS2820. A missing-property or excess message
  below it for the same pair suppresses it. A type-parameter or
  indexed-access target declines the chain (its explanations are not yet
  retraced).

Chain children built anywhere in `report_relation_head` now take the head's
file (`attach_chain_file`). The missing-property and signature chains it
already built were printed without one.

**Measured** (exact oracle, unfiltered, batch 45 base): 9813 → 9932 exact
(+119), 0 lost.

**Falsifier.** A retraced chain that names a different member or
constituent than native's: an exact loss in a case whose head was already
right.

## 37. More of the chain: construct and index signatures, privacy, best match, type parameters

Arms added to §36's retracing, each checked against pinned tsgo first:
- **Construct signatures** after call signatures (`signaturesRelatedTo`'s
  kind order). `Type 'S' provides no match for the signature '…'` prints
  `signatureToString`'s declaration form `(a: T): R`; this port's printer
  writes the arrow form, so it is converted after the parameter list.
- **`indexSignaturesRelatedTo`.** A target index signature with no
  applicable source signature, on a class or interface instance (no
  inferable index): `Index signature for type 'K' is missing in type 'S'.`
- **`propertyRelatedTo`'s private arm.** `Property 'p' is private in type
  'A' but not in type 'B'.`, or `Types have separate declarations of a
  private property 'p'.` when both sides are private with different
  declarations. The protected arms still decline.
- **Union target** (`typeRelatedToSomeType`). Only `getBestMatchingType`'s
  pick is elaborated, in native order: `findMatchingDiscriminantType`, a
  same-target reference, `findBestTypeForObjectLiteral`, invokables, then
  `findMostOverlappyType` (last non-primitive with the most shared literal
  keys). No pick means no elaboration.
- **Intersection source** against a plain object: its constituents failed
  silently, and the structural pass over the combined members reports.
- **Type-parameter target** (`reportRelationError`, `relater.go:4768`). The
  constraint explanation (TS5075) is pushed before the relation error, or
  TS5082 after dropping the chain below ("only report this error once").
- **Nullable stripping.** A definitely non-nullable source against `null |
  undefined | X` relates to `X`, and that is the pair the chain names. This
  is what lets `foo.bar` merge through a `{ … } | undefined` property.

**Measured** (exact oracle, unfiltered, batch 45 base): 9813 → 9975 (+162
including §36's +119), 0 lost.

## 38. Related information: `'p' is declared here.` and the expected-type origin

Two `diag:related-info` classes in this lane's reports.

- **TS2728 under a missing property.** `reportUnmatchedProperty` adds
  `'p' is declared here.` at the missing property's first declaration when
  one property is missing. `NewDiagnosticChain` copies a chain child's
  related information up, so it appears at every level from that message to
  the head. `missing_properties_diagnostic` now attaches it (reading an
  inherited generic member through `generic_heritage_property_symbol`).
  §36's retrace collects it into the chain, and `attach_chain_file` lifts
  the first child's list onto the head.
- **TS6500 / TS6501 under an object-literal member.** `elaborateElement`
  (`relater.go:575`) adds `The expected type comes from property 'p' which
  is declared here on type 'T'` to its report on the member itself (not to
  an inner elaboration), at the target member's first declaration. Without
  a member, it uses the applicable index signature's declaration (`The
  expected type comes from this index signature.`). Nothing is added for a
  declaration in a default library. `add_expected_type_related_info`
  attaches it when the elaboration's single new diagnostic sits on the
  member's span.

**Measured** (exact oracle, against batch 45, with §36–§37 and main's
batch 46 in the tree): 9813 → 10138, 0 lost. A clean batch-46 base is
measured with the gate.

## 39. Related information on the excess-property path and under arrow returns

- **TS6500 on `check_excess_properties`' member check.** Most object
  literals against a non-union target never reach
  `elaborate_object_literal_members`. `check_excess_properties` relates
  each known member first (§23, its stand-in for `elaborateObjectLiteral`),
  and its TS2322 is the one that lands. §38 attached the expected-type
  origin only on the other path, so `foo({ id: 1, name: false })` lacked
  it. Both of `check_object_literal_member`'s report sites, the single and
  the duplicated name, now call `add_expected_type_related_info`. The rule
  is unchanged: one new diagnostic, on the member's own span.
- **TS6502 / TS1356 under an arrow return** (`elaborateArrowFunction`,
  `relater.go:641`). Upstream first tries `elaborateError` on the returned
  expression; an inner elaboration gets no related information. Only when
  that declines does it relate the returns at the expression and add `The
  expected type comes from the return type of this signature.` at the
  target's first declaration. A union or intersection target has no
  symbol, and so gets none. For a non-async arrow whose return has no
  `then`, it also adds `Did you mean to mark this function as 'async'?`
  at the arrow when `Promise<return>` would have been assignable. The port
  follows that order. `target.symbol` is `Anonymous.symbol` or
  `Named.members`. `createPromiseType` is `Promise<Awaited<T>>`, which is
  `Promise<T>` for a non-generic, non-thenable `T`. The hint declines for a
  generic return rather than building the `Awaited` alias.
- **The member report is the relation's, wherever it lands.** §38
  required the single new diagnostic to sit on the member's span with no
  related information. Upstream adds the origin to `diags[0]` of
  `elaborateElement`'s own `checkTypeRelatedToEx(…, prop, …)` (`relater.go:584`).
  That diagnostic may already carry `'a' is declared here.` (TS2741 under
  a property: the origin follows it). It may also have moved to a direct
  member of the value literal: the relation's excess-property check reports
  there (TS2353/TS2561). `is_member_relation_report` accepts both. An inner
  elaboration's report is at neither place, so it gets none, as upstream.
- **Generic signature pairs in the chain.** `explain_signature` declined
  every generic signature. `compareSignaturesRelated` (`relater.go:1462`)
  instantiates a generic source whose type parameters are not the
  target's in the context of the target's canonical signature. The
  verdict already does this (`one_signature_related_to`), and the retrace
  now does the same through the same two functions. The callback test
  also read `signatures_of_type_kind(…).is_none_or(…)`: a type parameter
  has no answer there, so it was treated as callable and declined. It now
  reads `getSingleCallSignature` of each non-nullable side, as upstream.
- **Type-parameter source** (`structuredTypeRelatedToWorker`,
  `relater.go:3665`). The constraint is related with `reportErrors` only
  when it is not `unknown` and the target is not a type parameter too.
  An unconstrained `T` therefore reports nothing below its relation
  error, and a constrained one explains `constraint → target`. A
  polymorphic `this` would need `getTypeWithThisArgument` and declines.
- **`This type parameter might need an \`extends X\` constraint.`**
  (`reportErrorResults`, `relater.go:4741`). An unconstrained, declared
  type-parameter source adds this related information at every failing
  level, unless constraining a clone to the target would be circular.
  The port decides circularity only for a non-instantiable target (a
  union or intersection with the source as a constituent) or a
  type-parameter target (never circular). Any other instantiable target
  declines. The head pair's hint is appended after the inner levels',
  because the outermost `reportErrorResults` runs last.
- **Not this lane: optional members print without `| undefined`.** 22
  cases print `{ id: number; name?: string; }` where native prints
  `name?: string | undefined` under `strictNullChecks`. The difference is
  in the property-type printing arm of `printing.rs`
  (`reused_property_type_text`). It is proposed as an issue, not fixed
  here.
- **Not this lane: a fresh literal's duplicated member relates its first
  value.** `test({ thunk: (num: number) => {}, thunk: (str: string) => {} })`
  prints the literal as `{ thunk: (str: string) => void; }`, but the
  relation to `Thing` fails. A regular copy (`const o = {…}; const u: Thing
  = o`) relates. §35 made the member check read the last value, as
  upstream does, so the per-member TS2322 is gone. That exposes the
  relation's verdict as a TS2345 head (`lastPropertyInLiteralWins`, WRONG
  → WRONG, 3 lines fixed and 1 new). The member type the relater reads
  comes from `objects.rs` / `members.rs`, so it is proposed as an issue.

## 40. Overloaded signature sets in the chain

`signaturesRelatedTo`' N×M arm (`relater.go:4495`) handles every pair that
is not 1:1 and not two instantiations of one anonymous type or reference
target. It walks the target signatures in order. For the first target
signature that no source signature matches (both erased), only the first
source signature's failure is elaborated (`shouldElaborateErrors` is then
cleared). So with any source signature present, no `provides no match`
follows. `explain_signature_sets` does the same: verdicts come from
`compare_signature_ternary` on erased signatures, and the explanation from
`explain_signature`. The pairwise arm declines rather than guess its erased
pairing. `getErasedSignature` is re-implemented here
(`chain_erased_signature`) rather than exposed from `check.rs`, which this
lane does not own.

A function declaration's own overload set (`typeof f`) still declines.
`get_property_names_of_type` has no answer for it, and the unmatched-
property step needs one.

## 41. Common properties, intersection targets and the elaborated source

- **The common-property check in the chain** (`isRelatedTo`,
  `relater.go:2731`). A weak target that shares no property with the
  source fails before any structural step. It reports TS2559, or TS2560
  when the source's first call/construct return relates, and there is no
  relation error under it. `explain_pair` checks it first, sharing
  `weak_type_message` with `report_weak_type_failure`.
- **Intersection target.** `typeRelatedToEachType` relates the source to
  each constituent under `IntersectionStateTarget`. Every nested pair
  inherits that state, and it skips the common-property and
  excess-property checks. When every constituent relates, `isRelatedTo`'s
  extra `propertiesRelatedTo` over the intersection as one object is what
  failed (`relater.go:2770`); under the target state that pass is skipped.
  The chain mirrors this with `ChainStack::intersection_target`.
  `chain_relate` reads verdicts through the new
  `relate_ternary_in_intersection_target`, which runs a verdict-only walk
  with the relater's existing `intersection_target` flag. Without the
  state, `C1 → A1` in `nestedExcessPropertyChecking` fails on a nested weak
  check that upstream never makes. The retrace then explains a constituent
  native relates.
- **`specificSource`** (`elaborateElement`, `relater.go:565`). The member's
  report relates the expression's own type, checked against the source
  member's type as contextual type, and falls back to the member's type
  when that relates. For a literal, `false` against `boolean` stays `false`.
  So TS2559 names `'false'`, as upstream does. TS2322's display
  generalizes it back to `boolean` through `reportRelationError`'s literal
  generalization. Only a literal-typed expression is re-read; any other
  keeps the member type, since re-checking under a contextual type is not
  free here.
- **`This type parameter might need…` on every head.** The hint now
  attaches in `report_relation_head` (both of its report sites). Before,
  it attached only in the final branch, and a missing-property `Kept` head
  lacked it. `createDiagnosticChainFromErrorChain` gives every chain level
  the walk's related information, so the head pair's hint is appended at
  every level, not only the head.
- **Arrow-return related information appends.** §39 attached TS6502 /
  TS1356 only to a diagnostic with no related information, and replaced
  the list. `AddRelatedInfo` appends: a TS2741 under an arrow return reads
  `'z' is declared here.` and then `The expected type comes from the
  return type of this signature.`

**Not done: `mappedTypeAsClauseRelationships`' `Modify<T>`.** Native fails
`T → { [P in keyof T as P extends string ? \`bool${P}\` : P]: T[P] }`
because the name type's constraint `` `bool${P}` | P `` does not relate to
`keyof T`. The port's generic-mapped-target arm has no `keyof T` for a
type-parameter source (`keyof_without_index_signatures` reads index
infos). With that supplied, `` `bool${P}` → keyof T `` is still `Unknown`
in the port. That needs the index-target arm's `getSimplifiedTypeOrConstraint`
and the template-source arm against an instantiable target. It was tried
and reverted in this commit. The case's verdict stays as it was.

## 42. The argument head's chain, and reference variances

- **TS2345 carries the chain.** `report_argument_failure` reported its
  head through `report_argument_head`, which never retraced the relation:
  every argument mismatch printed the head alone. Its final branch now
  builds `relation_failure_chain` exactly as `report_relation_failure`
  does (when the relation is decided and there is no signature-arity
  elaboration). The head goes through `attach_chain_file` and the
  type-parameter hint like the TS2322 head.
- **References to one class or interface** (`relateVariances`,
  `relater.go:3266`; `typeArgumentsRelatedTo`, `:3903`). Each type
  argument is related under its measured variance, and the first failure
  explains the pair, with no `Types of property` above it:
  `Container<Ref<string>> → Container<Ref<number>>` reads `Type
  'Ref<string>' is not assignable to type 'Ref<number>'.` under it.
  The structural comparison decides instead in three cases, and the arm
  steps aside for each: a variance with `Unmeasurable`/`Unreliable`
  flags (`AllowsStructuralFallback`), a covariant `void` target argument,
  or an invariant parameter (upstream resets the variance errors to show
  the structural reason). Aliases relate through their own variance path
  in the relater and decline here.

## 43. `Did you mean to call this expression?`

`elaborateDidYouMeanToCallOrConstruct` (`relater.go:480`) reports the
relation at the expression itself and adds `Did you mean to call this
expression?` (TS6212) or `Did you mean to use 'new' with this expression?`
(TS6213) to the first diagnostic, after anything the relation attached
(`'x' is declared here.` under TS2741). The port's doc comment had said
"the suite does not compare" related information. The exact oracle does,
so the hint is now attached.

## 44. TS7010 at a method signature spans the signature

`GetErrorRangeForNode` (`scanner.go:2604`) narrows a function or method
declaration, accessors, and property declarations and signatures to the
name. `MethodSignature` is not on that list, so `foo(); // error` in an
interface reports across `foo();`, semicolon included. The rank has 10
`diag:span-only` cases (`implicitAnyAmbients`,
`implicitAnyDeclareMemberWithoutType`, …). The shared `error_span` names a
method signature by its name, and other report sites read it. So the fix
is local to TS7010's report in `implicit_any.rs`, not a change to
`error_span`. Whether `error_span` should drop `MethodSignature` for every
caller is a question for its owner; it is proposed rather than changed.

## 45. Heads that keep the missing-property message; TS2786's elaboration

- **`isConversionOrInterfaceImplementationMessage`** (`relater.go:4934`).
  `reportRelationError` suppresses its head when the message below is
  the pair's missing-property message, except under six heads: the two
  `Class … incorrectly implements …` heads, TS2352's conversion head, and
  the three `Its … type '{0}' is not a valid JSX element.` heads.
  `missing_property_chain` takes the caller's head and keeps it for those
  six. TS2420/TS2720 in `heritage_conformance.rs` still report the bare
  head ("That reporter does not make the exception"). That file's owner
  can now route them through `report_relation_failure`; this commit
  proposes that and leaves their file unchanged.
- **TS2786's chain** (`checkJsxReturnAssignableToAppropriateBound`,
  `jsx.go:169`). The bound check relates the tag's return or instance
  type under `Its return type…` (function), `Its instance type…` (class)
  or `Its element type…` (both), and wraps `diags[0]` in TS2786. The port
  reported a bare TS2786. It now calls `report_relation_failure` with the
  kind's head and moves the one diagnostic it produces under TS2786. The
  chain's file goes through `attach_chain_file`, and `new_chain` copies
  the child's related information up. When the reporter declines, or
  produces anything but one diagnostic, the bare TS2786 stands as before.

## 46. Index signatures in the chain; a type-parameter target

- **`indexSignaturesRelatedTo`** (`relater.go:4578`). Each target index
  signature is related in order. A `string` index of `any` beside a
  string key relates outright. An applicable source index relates its
  value (`indexInfoRelatedTo`): `'string' index signatures are
  incompatible.`, or `'K' and 'L' index signatures…` when the keys
  differ. A class or interface source with none misses it (TS2329, §37).
  An inferable-index source (object literal, type literal) relates each
  applicable member first (`membersRelatedToIndexInfo`): `Property 'one'
  is incompatible with index signature.` Then it relates its own
  applicable index infos. The arm declines for a generic mapped source,
  a key other than `string`/`number`, a symbol-named member, and an
  optional member. The optional member's `getNonMissingTypeOfSymbol` /
  `NEUndefined` choice is not modelled.
- **A type-parameter target** (`structuredTypeRelatedToWorker`,
  `relater.go:3462`). Only a mapped source (`{ [P in Q]: X }` against
  `T`) or a generic source has an arm there. Any other source fails with
  nothing below the relation error, whose TS5075/TS5082 explanation
  `chain_report_relation_error` already pushes. `Derived → T` under
  `'number' index signatures are incompatible.` needed this.

## 47. `reportErrorResults`' messages; an object source against a primitive

- **TS2692 / TS2696** (`reportErrorResults`, `relater.go:4717`). Between
  the structural report and the relation error, an object source against
  a primitive target gets `'number' is a primitive, but 'Number' is a
  wrapper object.…` when the source is that primitive's global wrapper
  interface (`String`, `Number`, `Boolean`, `Symbol`). The global `Object`
  source gets `The 'Object' type is assignable to very few other types.…`
  above whatever the structural pass reported (`provides no match for the
  signature`). `chain_error_results` runs at every chain level and at the
  head pair, before the relation error. The wrapper comparison reads the
  global interface symbol at arity 0. `global_type_symbol`'s default
  arity is 1 and finds none of them.
- **An object source against a primitive** has no structural arm, so
  nothing is reported below its relation error except that message.
- **A target with no properties** needs no source property list.
  `get_property_names_of_type` has no answer for a function declaration's
  overload set (`typeof f`, §40). Against a signature-only target the list
  is never read, so the N×M signature arm now runs:
  `assignmentCompatWithOverloads`' `g = f4` reads `Type 'string' is not
  assignable to type 'number'.`

## 48. Member relations inside the walk that is being explained

The retrace asks `relate_ternary` about each member pair. That is a
fresh walk, and the pairs native is still comparing are on its maybe
stack (`recursiveTypeRelatedTo`, `relater.go:3089`). `Uint8Array →
Int8Array` fails on `[Symbol.toStringTag]`. Its `copyWithin(): this`
members return `Uint8Array` and `Int8Array`, which is the outer pair
again, so native answers `Maybe` there and keeps looking. A fresh walk
finds the outer pair already published as failed, so `copyWithin` fails
and the retrace declined.

`relate_ternary_assuming` seeds a verdict-only walk with the pairs being
explained (`ChainStack::assumed`, pushed by `explain_pair` and the head),
in the walk's maybe keys and its source/target stacks. Two departures
from an ordinary walk make the seeding observable:

- a seeded key answers `Maybe` **before** the completed results are read,
  because the published failure of the outer pair is exactly what the
  assumption stands in for;
- a completed *failure* is not reused, since it may have been decided
  without the assumptions. Completed successes are still reused.

Nothing a seeded walk proves through an assumption is published. Its
stacks are never empty, so a `Maybe` frame retains its keys, and the
walk is dropped with them. A failure it publishes holds without the
assumptions too, as in any walk.

## 49. Callback parameters in the chain

`compareSignaturesRelated` (`relater.go:1550`) relates two parameters
that each have a single call signature (non-nullable, neither an
instantiated generic parameter, neither a type predicate, the same
nullability facts) as callbacks. It compares target against source in a
callback mode: `StrictCallback` under strict variance, else
`BivariantCallback`. In that mode parameters relate contravariantly
only, and a bivariant callback's return type tries the reverse relation
first. `explain_signature` declined every callback pair; it now takes
the mode, reads the verdict from `compare_signature_ternary_in_mode`
(the relater's own `one_signature_related_to` in that mode), and recurses
with the swapped pair. `(f: (x: Dog) => Dog) => void` against
`(f: (x: Animal) => Animal) => void` reads `Types of parameters 'f' and
'f' are incompatible.` → `Property 'dog' is missing…`. The callback's
return marker is elided, as upstream does.

## 50. The source's property list is not read; arrays against objects

- `propertiesRelatedTo` walks the **target's** properties and looks each
  one up on the source by name. `explain_object_relation` also asked for
  the source's full list and declined without one, which
  `get_property_names_of_type` does not have for an interface extending
  a type alias (`interface I1 extends T1 { a: string }`). The list was
  never read, so the guard is gone: TS2430's `Types of property 'a' are
  incompatible.` now appears under `I1`, `I12`, `I20` and `I21` in
  `interfaceExtendsObjectIntersectionErrors`.
- Array/tuple pairs keep their own arms (tuple elements,
  `tryElaborateArrayLikeErrors`) and decline. A plain object against an
  array or tuple, or the reverse, is the structural comparison and is
  retraced as one. In that file, `I2 extends T1 & { b: number }` still
  declines (the `I2 → T1` constituent verdict is `Unknown` under
  `IntersectionStateTarget`), and so does `I3 extends number[]`.

## 51. `Property 'x' is optional in type 'S' but required in type 'T'.`

`propertyRelatedTo` (`relater.go:4309`) relates the member types first.
When they relate, an optional source member against a required target
member is still a failure (TS2327), with no `Types of property` above
it. The target member must carry `SymbolFlagsClassMember`
(method, accessor or property, which is every property symbol). The
property loop now checks this after a related member pair, so `C` with
`x?` against `{ x: any }` reads it.

## 52. Fixed tuple targets in the chain

`propertiesRelatedTo`'s tuple-target arm (`relater.go:4103`) runs before
any property. For a target whose elements are all required, and a source
that is a fixed tuple or an array:

- an array source (all rest, minimum length 0) reads `Target requires N
  element(s) but source may have fewer.`, or `Target allows only 0…`
  against the empty tuple;
- a shorter tuple reads `Source has M element(s) but target requires N.`,
  and a longer one `…but target allows only N.`;
- otherwise the first element that fails is explained under `Type at
  position i in source is not compatible with type at position i in
  target.` (only when either side has more than one element).

Optional, rest and variadic elements on either side decline, since their
position mapping, `removeMissingType` and the variadic messages are not
retraced. A readonly source against a mutable target also declines: the
arm fails silently there, and `tryElaborateArrayLikeErrors` reports
instead.

## 53. `this` types in the chain

`compareSignaturesRelated` (`relater.go:1505`) compares the `this` types
before any parameter. A `void` source `this` accepts anything. Otherwise
the pair is related forward (unless strict variance), then in reverse.
The reverse failure is reported under `The 'this' types of each signature
are incompatible.` `explain_signature` declined every signature with a
`this` parameter; it now runs this arm. Only one side having `this` is
not compared, as upstream does.

## 54. Too few target arguments inside the chain; alias variances

- `compareSignaturesRelated`'s arity check (`relater.go:1470`) reports
  `Target signature provides too few arguments. Expected N or more, but
  got M.` at any depth outside strict arity. At the head this already
  came from the relater's own signature error. Under a property
  (`Types of property 'a' are incompatible.` in TS2430) the retrace
  declined; it now pushes the message.
- An alias instantiation relates its own arguments under the alias's
  variances (`relater.go:3392`) when the alias is not written as a type
  reference. `explain_reference_variances` now takes those aliases too.
  An alias of a class or interface reference relates as that reference
  in the relater, and still declines here. The motivating case,
  `Func<string, Object> → Func<Object, Object>` in
  `strictFunctionTypesErrors`, does not reach this arm yet: the port's
  type for the instantiation prints expanded (`(x: string) => Object`),
  with no alias to relate by.

## 55. Written property names; no retraced chain under a type-parameter head

- **`symbolToString` keeps a string-literal name as written.**
  `Property ''ry'' is incompatible with index signature.`,
  `Types of property ''a'' are incompatible.`, `Property '"e"'…`: a
  property declared with a quoted name prints with its own quotes. The
  rank has about 100 such lines. `chain_property_symbol_display` reads the
  property's first declaration (assignment, signature, declaration or
  method). A `StringLiteral` name keeps its quote style (`TokenFlags::
  SINGLE_QUOTE`); anything else is `chain_property_display` as before.
- **A type-parameter head target.** `reportRelationError` either resets
  the chain ("only report this error once", TS5082) or puts TS5075 above
  it. The head's report adds that message itself, so a retraced chain
  under it was wrong in both cases. `doNotElaborateAssignabilityToTypeParameters`
  showed `Yadda | Awaited<T> → T` with the union member's levels after
  TS5082. `relation_failure_chain` now declines for a type-parameter
  target.
- **Refused: an indexed-access source arm.** Relating `T[K]`'s constraint
  (`relater.go:3665`) needs `getConstraintOfIndexedAccess` to distribute
  over a union index: `T[keyof T]` → `T[string] | T[number] | T[symbol]`.
  The port's `constraint_of_type` answers `T[string | number | symbol]`,
  so the retraced level reads differently from native
  (`indexedAccessConstraints`). Through a conditional target
  (`deepComparisons`) it is undecided. Measured on those two files, then
  reverted; not in this commit.

## 56. A class's `prototype` is not compared

`propertiesRelatedTo` (`relater.go:4256`) skips every target property
with `SymbolFlagsPrototype`, so `typeof Ctor2 → typeof Ctor1` is explained
by the construct signatures' instance types. The dotted reduction then
reads `The types of '(new a.b.c.d.e.f()).g'…`. The port mints a class's
static-side `prototype` without a symbol (`members.rs`, §117 slice 2).
The property loop therefore skips `prototype` on a class's anonymous
static type, as well as any symbol carrying `PROTOTYPE`.

## 57. TS8030: a JS function's `@type` must fit its arity

`checkFunctionOrMethodDeclaration` (`checker.go:3441`) reports `A JSDoc
'@type' tag on a function must have a signature with the correct number
of arguments.` at the full-signature type node when
`getContextualCallSignature(type, node)` is nil. The call signatures not
smaller in arity than the function's leading required parameters
(`isAritySmaller`) must leave exactly one. Several are combined by
`getIntersectedSignatures` only under `noImplicitAny`, with identical
type-parameter lists; anything else is nil. `check_jsdoc_full_signature_arity`
(`jsdoc_full_signature.rs`) ports this for the hosts this port reads a
full signature from (function and method declarations). It declines on an
error type and on an undecided signature list. The JS per-function hook
(`check_jsdoc_reparsed_this_parameter`) calls it, because the dispatch in
`check.rs` is not this lane's. A quick oracle over `conformance/jsdoc`
against c49 read 3 gained and 0 lost (`checkJsdocTypeTag5`,
`checkJsdocTypeTagOnObjectProperty1`, `jsdocFunction_missingReturn`).

## 58. A protected source member; abstract constructor types

- `propertyRelatedTo`'s last modifier arm (`relater.go:4296`): a
  protected source member against a public target member reads `Property
  'x' is protected in type 'S' but public in type 'T'.`. A protected
  *target* needs `isValidOverrideOf`'s class derivation (`…is protected
  but type 'S' is not a class derived from 'T'`), which this retrace does
  not make, so it still declines. The private arms now print the target
  member's written name (§55).
- `signaturesRelatedTo(Construct)` (`relater.go:4455`): when the first
  source construct signature is abstract and the first target one is not,
  the pair fails with `Cannot assign an abstract constructor type to a
  non-abstract constructor type.` (`classAbstractAssignabilityConstructorFunction`).
  `constructorVisibilitiesAreCompatible` (private and protected
  constructors) is not retraced.

## 59. Correction to §51: an unreadable optionality does not decline

§51's optional-member check read both sides' optionality with `?`. For an
apparent member the source does not own (`constructor` on an interface
against `Object`), `chain_property_is_optional(source, …)` has no answer,
so every explanation that reached such a member *after* a related pair
was abandoned. `I → Object` lost `The types returned by 'toString()'…`.
The check now applies only when both answers are known (source optional,
target required). This is a fix to §51 as committed; the c50 measurement
(+18) included the defect.

The same applies to §46's member-to-index arm. A synthesized source
member with no symbol (an import-attributes literal, `{ field: 0 }`
against `ImportAttributes`) has no optionality answer. It is required,
and the arm now relates it instead of declining.

## 60. The chain under a type-parameter head (supersedes §55's decline)

§55 dropped the retraced chain for every type-parameter head target.
That is right after TS5082, because `reportRelationError` resets the error
chain ("only report this error once"). After TS5075 it is wrong: TS5075
is pushed *above* whatever the structural pass and `reportErrorResults`
reported, so `Object → T` reads TS5075 and then `The 'Object' type is
assignable to very few other types.…` beneath it
(`typeParametersShouldNotBeEqual3`). `add_relation_chain` now places the
chain by the head's own child: it is dropped under TS5082, nested under
TS5075, and appended otherwise. Both heads (TS2322 and TS2345) use it.

## 61. The expected-type origin under a union target

`elaborateElement` reads `getPropertyOfType(target, name)` on the union
itself. That is the synthesized union property, whose declarations are
its constituents' in union order (`createUnionOrIntersectionProperty`),
so the origin is the first constituent's declaration, `…declared here on
type 'Blah'`. The port's `get_property_of_type` has no union property, so
`add_expected_type_related_info` takes the first constituent that has the
member (`indirectDiscriminantAndExcessProperty`).

## 62. Rest parameters in the chain

`compareSignaturesRelated` (`relater.go:1532`) walks
`max(sourceCount, targetCount)` positions. When either side has a
non-array rest, it walks `min(…)` and reads the last position with
`getRestOrAnyTypeAtPosition`. The arity check is skipped when the target
has an effective rest. `explain_signature` declined every rest
parameter; it now follows the same walk through the port's
`signature_*_at_position` readers. Parameter names come from
`getParameterNameAtPosition`: a position past the fixed parameters is
named by the rest parameter. A tuple rest names its element by label,
which declines here. `(...x: number[]) => void` against `(x: number[], y:
string) => void` reads `Types of parameters 'x' and 'x' are
incompatible.`

## 63. Type predicates in the chain

`compareSignaturesRelated` (`relater.go:1606`) replaces the return-type
comparison with `compareTypePredicateRelatedTo` (`relater.go:1662`) when
the target has a type predicate. A source with none fails with `Signature
'…' must be a type predicate.`, unless the target only asserts. Otherwise
three failures each end with `Type predicate 'P' is not assignable to
'Q'.`:

- the kinds differ (this or parameter, asserting or not), with `A
  this-based type guard is not compatible with a parameter-based type
  guard.` below it;
- the predicates name different parameter positions, with `Parameter 'a'
  is not in the same position as parameter 'b'.` below it;
- the predicate types fail to relate, with their own explanation below
  it.

`explain_signature` declined every predicate; `explain_type_predicates`
ports the arm. On the `typeGuardFunction` subset, a quick oracle against
c51 read 1 gained and 0 lost (`typeGuardFunctionOfFormThisErrors`).
`typeGuardFunctionErrors` still differs on an unrelated TS2355.

## 64. A method's own type has no property list in the port

An interface method's overload set (`{ (x: string): any; (x: 'foo'):
string; }`, the type of `Base.addEventListener`) gets no answer from
`get_property_names_of_type`, so the retrace declined at the first step.
Natively that type has no properties: `resolveAnonymousTypeMembers` of a
function or method symbol reads only its exports, and a method has none.
`explain_object_relation` now takes the empty list for a callable symbol
without exports (`completed_callable_symbol`), and the N×M signature arm
explains it: `overloadOnConstInheritance2` reads `Types of parameters 'x'
and 'x' are incompatible.` → `Type '"foo"' is not assignable to type
'"bar"'.` (The signature text there still prints the written `'bar'`
quotes, which is the printer's.)

## 65. The expected-type origin falls back to the target's declaration

`elaborateElement` (`relater.go:587`) reads the target member's first
declaration, or else **the target's own** (`target.symbol.Declarations[0]`).
That covers a missing member with no applicable index signature, and a
member with no declarations of its own: a mapped member without a
modifiers property, as in `{[A in "foo"]: A}` and `MappedModel<"Foo">`.
The origin is then the mapped type node. `add_expected_type_related_info`
follows that order: index signature only for a missing member; then the
member's declaration, the mapped type's declaration (`mapped_types`),
and the target symbol's. `errorElaboration` and
`genericMappedTypeAsClause` read the native position.

## 66. exactOptionalPropertyTypes members in the chain

`propertiesRelatedTo` reads both members through
`getNonMissingTypeOfSymbol` (`relater.go:4334`). Under
`exactOptionalPropertyTypes` an optional member's missing type is
removed, so an explicit `undefined` source member fails against `number`.
The relater's walk already does this (`remove_missing_type`). The
retrace read the members with the missing type left in, found them
related, and declined. It now removes it the same way, so TS2375 reads
`Types of property 'value' are incompatible.` → `Type 'undefined' is not
assignable to type 'number'.` (`strictOptionalProperties1/3`).

## 67. Record: batch 55 refused on four chain losses a stale base hid

Batch 55 merged 3e0052dfb (§46–§57). The integration exact oracle
refused it on 4 EXACT losses, all `diag:chain` TS2322:
`assignmentToObject`, `doNotElaborateAssignabilityToTypeParameters`
(es2015), `typeParametersShouldNotBeEqual` and
`typeParametersShouldNotBeEqual2`. This lane's gates had compared every
candidate against base16 (batch 46). Those four were not EXACT there:
other lanes' later batches made them EXACT on main, so "0 lost against
base16" could not see a regression in them.

The causes were §51's `?` on an unreadable optionality and §55's blanket
decline under a type-parameter head. §59 and §60 fixed both before the
refusal arrived. A filtered oracle run of the corrected branch reads
every variant of the four EXACT.

**The rule this costs:** the exact leg's base is rebuilt at current main
whenever main moves, before the next candidate is gated. A base more
than one batch old measures gains, not losses.

## 68. Correction to §61: a union target's member must be in every constituent

§61 took the first constituent that has the member. Natively,
`getPropertyOfType(union, name)` answers none when any constituent lacks
the member: `createUnionOrIntersectionProperty` marks it partial, and
`undefined` in `T | undefined` lacks every member. The union has no
symbol for the fallback either. Gated against main (base17 = 81e030785),
§61 lost 8 EXACT cases on an extra TS6500 (`optionalBindingParameters2`,
`unionErrorMessageOnMatchingDiscriminant`, `nestedFreshLiteral`,
`coAndContraVariantInferences6`, `errorsOnUnionsOfOverlappingObjects01`,
`objectLiteralsAgainstUnionsOfArrays01`, `typeArgInference2`,
`optionalBindingParametersInOverloads2`). The origin is now taken only
when every constituent has the member. A filtered oracle reads all eight
EXACT, and `indirectDiscriminantAndExcessProperty` (§61's case) stays
EXACT.

## 69. Refused again: the indexed-access source arm, with simplification

§55's refusal named the missing distribution. This round added it locally
(`getSimplifiedIndexedAccessType`'s union-index arm: `T[A | B]` →
`T[A] | T[B]`, built with `resolved_indexed_access_type`). The chain then
reads the right levels, but the distributed union prints `T[number] |
T[string] | T[symbol]` where native prints `T[string] | T[number] |
T[symbol]`: `unions.rs`' `compare_types` has no `CompareTypes` arm for two
indexed accesses and falls back to creation order. Measured on
`indexedAccessConstraints`, then reverted. Reopen when union ordering
compares indexed accesses by object and then index (`CompareTypes`,
`utilities.go`); that is not this lane's file.

## 70. JSX overload failures carry TS2769's chain; children over several array-likes

- **TS2769 on a JSX tag** (`reportCallResolutionErrors`,
  `checker.go:9649`). With several argument-error candidates, each report
  of the last candidate is wrapped as `No overload matches this call.` →
  `The last overload gave the following error.` → the report, with `The
  last overload is declared here.`. The JSX road rewrote the head's
  message in place and dropped the rest. It now calls the call road's
  `chain_overload_failure` (made `pub(crate)` in `calls.rs`, a visibility
  change only). The tuple element there is read as the tuple's property
  `"0"`. Under an optional element its `undefined` is removed
  (`removeMissingType`), so `jsxChildrenWrongType` reads `Type 'string' is
  not assignable to type 'number | boolean'.`.
- **Children against a union of array-likes.**
  `getBestMatchIndexedAccessTypeOrUndefined` first tries
  `getIndexedAccessTypeOrUndefined` on the union itself. That indexes every
  constituent and answers their union when all have the element; only
  otherwise does it need `getBestMatchingType`. The elementwise
  elaboration declined for any second array-like part. It now unions the
  parts' elements, and declines only when one has none
  (`jsx_children_element_at`).
- **Not reached:** `jsxChildrenArrayWrongType` (ES2015) reports nothing,
  because the JSX overload resolution finds a candidate applicable before
  any elaboration runs. That is a verdict on `{ children: [unknown,
  string] }` against `[string, number] | boolean[]`, not a report.

## 71. Type parameters against unions, tuples against arrays, intersection sources

- **A type-parameter source against a union.** `typeRelatedToSomeType`
  fails first, but its best-match elaboration has no candidate: every
  `getBestMatchingType` arm reads an object source. The type-variable arm
  then relates the constraint with errors, as it does against an object.
  The arm declined every union target. (`typeVariableConstraintedToAliasNotAssignableToUnion`
  now reads the right levels. Its constraint prints the alias `Table`
  where native prints `TableClass<any>`, an alias of a reference that
  carries no alias natively. That is the printer's, not this arm's.)
- **A tuple against an array** (`relater.go:3780`). A readonly array
  target accepts any array or tuple, and a mutable one a mutable tuple; the
  two number-index types are related with errors:
  `Type 'readonly [1]' is not assignable to type 'readonly boolean[]'.` →
  `Type 'number' is not assignable to type 'boolean'.`
  (`readonlyTupleAndArrayElaboration`).
- **An intersection source's missing members.** `unmatched_property_report`
  answers only for plain object pairs. For an intersection of plain
  objects, the retrace lists the target's required members that the
  combined members lack, in target order, and reports them through
  `missing_properties_diagnostic` (TS2741 + TS2728, or TS2739/TS2740). An
  intersection source also enters the union-target best-match arm
  (`findMostOverlappyType` reads intersections). `intersectionAndUnionTypes`
  is EXACT.

## 72. A member symbol the port cannot name has no modifiers to read

`chain_property_privacy` declined when `get_property_of_type` had no
symbol for the member on either side. That happens for an inherited
member of an instantiated derived class (`B<number>`'s `x` from `A<{…}>`),
even though `get_type_of_property_of_type` answers its type. Such a member
carries no `private`/`protected` this port could read, and the relation
that failed did not fail on one either. The retrace now treats it as
public and relates the types. `genericDerivedTypeWithSpecializedBase` and
`…2` are EXACT.

## 73. A protected target member: `isValidOverrideOf`

`propertyRelatedTo`'s protected-target arm (`relater.go:4285`) passes
when the source member's declaring class is the target member's declaring
class or derives from it (`isPropertyInClassDerivedFrom`, `hasBaseType`).
Otherwise it fails with `Property 'x' is protected but type 'S' is not a
class derived from 'T'.`, naming the two declaring classes. §58 left this
arm declined. `chain_declaring_class` reads the class off the member's
value declaration, and `chain_class_derives_from` walks
`base_symbols_of_ex`; an unreadable base list declines. `protectedMembers`
(`b1 = a1` / `a1 = b1`) and `derivedClassTransitivity4` now read native's
levels; the latter still differs on `(x?: string)`'s printed `| undefined`.

## 74. TS7031 under an array literal; TS7006's `(Missing)`

- **An array binding pattern over an array literal.** The pattern's
  declared type is the literal's type padded by the pattern
  (`getTypeFromBindingPattern` with the initializer), and
  `widenTypeForVariableLikeDeclaration` reports each element that widens to
  `any`. An element past the literal's end, with no initializer of its
  own, is padded `any`: `function f([x, y] = [1])` reads TS7031 at `y`
  (`destructuringWithLiteralInitializers2`). Outside `strictNullChecks`, a
  `null` or `undefined` element widens to `any` too: `var [a, b] =
  [undefined, null]` (`wideningTuples5`). The parameter arm in
  `check_implicit_any_parameters` and the variable arm in
  `check_widening_of_variable_initializer` now walk the pattern; a spread
  in the literal, an empty literal, a rest element and a nested pattern
  are left alone.
- **`declarationNameToString`** prints a missing parameter name as
  `(Missing)` (`templateStringInFunctionParameterType`).

A quick oracle over `destructuring`, `idening` and `wideningTuples`
against base20 read 2 gained and 0 lost.

## 75. Regression fix: a duplicated literal member relates its last value

Routed by the integrator as a duplicate-property regression since batch 47.
§35 made the per-member check read the last occurrence's value, as
`elaborateObjectLiteral` does. That removed the per-member TS2322 and
exposed the relation's own verdict: `test({ thunk: (num: number) => {},
thunk: (str: string) => {} })` became a false TS2345 (with a chain naming
`(num: number) => void`), and `lastPropertyInLiteralWins` stayed WRONG.

**Cause.** checkObjectLiteral gives each member its own transient symbol,
and the last one wins `propertiesTable[name]` (`checker.go:13331`). The
port's literal type records the last member's resolved type, but its
property list is marked incomplete while a context-sensitive member is
pending. A read then takes the symbol road through the binder's merged
symbol, whose `getTypeOfSymbol` answers the *first* declaration
(contextual.md §17 keeps that answer for `.types`, as native does).

**Fix** (`members.rs`, a grant request; one condition): a property-list
entry whose binder symbol has more than one declaration, and whose type is
already resolved from its own initializer, is read from the list even when
the list is incomplete. `lastPropertyInLiteralWins` is EXACT. A quick
oracle over every `uplicate` case reads 0 lost against base20.

## 76. Non-strict `void` against an object; a missing-property chain suppresses its head

Routed from r7-calls (TS2684 silenced in non-strict mode).

- **`void` against an object** (`isSimpleTypeRelatedTo`). Outside strict
  null checks, `undefined` and `null` relate to every object, but `void`
  has no such arm, so it fails against an object in either mode. The
  relater's decided-failure rule covered the nullable flags only under
  `strictNullChecks`, and non-strict `void → C` was `Unknown`, which kept
  TS2684 (`The 'this' context of type 'void'…`) from reporting
  (`thisTypeInFunctionsNegative`, strict false). `relater.rs` now decides
  `void` the same way in both modes.
- **A primitive source against an intersection target** enters the
  intersection arm: `typeRelatedToEachType` explains the first failing
  constituent (`Type 'void' is not assignable to type 'A'.` under TS2684,
  `unionTypeCallSignatures6`).
- **The chain's missing-property message suppresses the head.**
  `reportRelationError` (`relater.go:4816`) pushes no head when the
  message below it is the pair's own TS2741/TS2739/TS2740 (`chainArgsMatch`),
  unless the head is a conversion or implementation message. The
  `missing_property_chain` path did this already. A chain retraced through
  §71's intersection-source arm did not: TS2684 over `A & C & {…}` → `B`
  kept its head. Both the TS2322-family and TS2345 sites now report the
  missing-property message alone in that case. `unionTypeCallSignatures6`
  is EXACT.

## 77. A binding-pattern parameter whose contextual type is `unknown`

Routed from r7-calls (arrow arguments with binding-pattern parameters).
Repro: `fallbackToBindingPatternForTypeInference`
(`trans(({a}) => a)` against `trans<T>(f: (x: T) => string)`).

- **The forcing constraint.** `assignParameterType` (`checker.go:10404–10425`)
  assigns the contextual type and then, when that type is `unknown`,
  replaces it with `getTypeFromBindingPattern(name, false, false)`. When
  inference finds no candidate for `T`, the arrow's parameter therefore
  has the type `{ a: any; }`, not `unknown`. The relation back against
  `(x: unknown) => string` fails (`unknown → { a: any; }`), which gives
  TS2345. The port kept `unknown`, so the arrow related and nothing was
  reported.
- **Where it went.** The fallback is the
  `binding_pattern_fallback_for_unknown` helper (implicit_any.rs). It reuses
  `binding_pattern_implied_type` (binding_patterns.rs), the port's
  `getTypeFromBindingPattern`. It has one call at each of the two sites
  where the port applies a contextual parameter type: symbols.rs
  (`get_contextually_typed_parameter_type` → `add_optionality_for_declaration`)
  and signatures.rs (the `Parameter::new` contextual arm). Those are
  one-line hooks outside this lane's files, in the same shape as the granted
  useMemo hook. Any other contextual type passes through unchanged, and so
  does a pattern the port cannot type.
- **The chain.** `parameter_name_at_position` now prints the binder's
  symbol name, `__<index>` (with `this` counted), for a binding-pattern
  parameter instead of the pattern text. The explain walk takes an
  `unknown` source as a leaf (no simple arm relates it below the pair), so
  `Type 'unknown' is not assignable to type '{ a: any; }'.` is the last line.
- **How it would be wrong.** The fallback could fire where tsgo's does not,
  for example where the port computes `unknown` but tsgo would have produced
  an inference candidate. That would show up as new TS2345s on
  `destructuring`/`binding` cases. The quick oracles over those filters are
  the check.

## 78. Indexed-access sources through their constraint; constraint descents push their source only

From the base22 rank (batch 63): the largest relater-rooted chain bucket is
TS2322 with no chain (111 rows across 40 files). Its biggest family is a
generic indexed-access source: `T[K] → object` explains as `T[keyof T]`, then
`T[string] | T[number] | T[symbol]`, then `T[string]`
(`keyofAndIndexedAccess`, `indexedAccessConstraints`, `deepComparisons`,
`mappedTypes6`).

- **The arm.** `structuredTypeRelatedToWorker`'s type-variable case
  (`relater.go:3665`) relates `getConstraintOfType(source)` and reports when
  that constraint is not `unknown`. The explain walk now covers an indexed
  access the same way: a nil constraint reports nothing; a constraint is
  related and explained. `isRelatedTo` normalizes the pair before it names
  it, so `T[string | number | symbol]` is printed as its distribution
  (`chain_simplified_indexed_access` is the relater's
  `simplified_indexed_access`, reading). The arm declines in three cases:
  a mapped object or alias reference (`isMappedTypeGenericIndexedAccess`
  explores a second constraint), an undecided constraint, and an
  indexed-access or intersection target.
- **Constraint descents push their source only.** Native relates the
  constraint with `RecursionFlagsSource`, so the target is not pushed on
  `targetStack`. The retrace's assumed pairs had always pushed both sides.
  At the third level `object` sat on the target stack three times,
  `isDeeplyNestedType` called the target expanding, and `T[string] → object`
  answered Maybe, which the walk took as related. The chain then stopped
  after its first line. The assumed pairs now record whether they were
  entered as a constraint descent (`relate_ternary_assuming`'s
  `source_only`). The type-parameter arm uses the same
  `explain_constraint_descent`.
- **Not yet EXACT: union order.** The port prints `T[number] | T[string] |
  T[symbol]`; native prints `T[string] | …`. `CompareTypes`
  (`utilities.go:544`) orders two indexed accesses by object, then by index.
  `unions.rs`'s `compare_types` has no such arm and falls back to type-id
  creation order. That file is outside this lane, so the fix is proposed to
  the integrator, not made here. Until it lands, the indexed-access cases
  above stay non-exact with the right chain shape.
- **A §76 loss, fixed.** In the base22 gate, `intersectionAsWeakTypeSource`
  went RIGHT→WRONG. Its source `Brand<…>` is `number & { __brand: T }`.
  Native's structural pass reads `getApparentType` of the intersection, so
  its TS2739 names `Number & { __brand: … }`, and §76's head suppression
  does not apply. The intersection-source arm named `Brand<…>`, which
  matched the head, so the head was dropped. That arm now declines for an
  intersection with a primitive constituent, which restores base22's
  output.
- **Refused here: conditional alias variances.** `Stuff<U> → Stuff<T>`
  (`conditionalTypeVarianceBigArrayConstraintsPerformance`) relates the
  alias arguments first (`relater.go:3392`). `inference_variances` answers
  `None` for an alias whose body is a conditional (variances.rs,
  `measurable_alias_body`), so the walk has nothing to explain from. That
  file is outside this lane.
- **Measured.** Quick oracles against base22 (`ntersection`,
  `ndexedAccess`, `keyof`, `onstrain`, `ypeParameter`, `eakType`) gained
  `errorMessageOnIntersectionsWithDiscriminants01`, `intersectionAndUnionTypes`
  and `typeParameterArgumentEquivalence3`, with 0 lost.

## 79. Unconstrained type parameters against intersections; the `T & {}` hint

- **`T → T & U`** (`intersectionTypeInference`). The intersection-target
  arm took only non-instantiable sources and only targets without
  instantiable constituents. Native's `unionOrIntersectionRelatedTo` runs
  `typeRelatedToEachType` (reporting) for an instantiable source too. Its
  fallthrough to the type-variable arm then relates `getConstraintOfType`,
  which for an unconstrained parameter is `unknown`, related without
  reporting (`relater.go:3672`). So the each-type report is the whole chain:
  `Type 'T' is not assignable to type 'U'.` plus the arbitrary-type line.
  The arm now admits an unconstrained (or `unknown`-constrained) type
  parameter source, and type-parameter constituents in the target. A
  constrained source still declines, because native then also reports the
  constraint's relation, and the two report sets interleave in an order
  this walk does not rebuild (`conditionalTypes1`'s line-17 chain ends with
  the each-type line). Other instantiable or union target constituents
  still decline, because `isRelatedTo` normalizes them first.
- **The hint for `T → NonNullable<T>`.** `reportErrorResults` constrains a
  clone of `T` to the target and asks `hasNonCircularBaseConstraint`. The
  port called a target with the source as a constituent circular. Native
  instantiates `T & {}` through `getIntersectionType`, and its two-member
  constraint reduction (`checker.go:26131`: a type variable with a
  primitive, `object` or `{}`) asks `getBaseConstraintOfType(clone)` before
  the clone's constraint is assigned. That caches "no constraint", so the
  later question answers non-circular, and native prints *This type
  parameter might need an \`extends NonNullable<T>\` constraint.* That pair
  shape is therefore not circular now. A generic string-like partner
  (template literal, string mapping) is undecided and declines. A union
  with the source as a member is still circular, because `getUnionType`
  asks no constraint.
- **Measured.** Quick oracles against base22 (`ntersection`,
  `ypeParameter`, `onditional`, `eneric`) gained
  `intersectionTypeInference`, `genericDerivedTypeWithSpecializedBase` and
  `genericDerivedTypeWithSpecializedBase2`, in addition to §78's three,
  with 0 lost. In `conditionalTypes1`, the line-12 chain and its two hints
  match native now; the file is still non-exact on other lines.

## 80. JSX attributes types order their members by declaration

`<OneThing {...obj} {...{yy: true}} />` with `obj = { yy: 10, yy1: "hello" }`
gives native `{ yy1: string; yy: boolean; }` and gave the port
`{ yy: boolean; yy1: string; }` (`tsxStatelessFunctionComponentOverload4`,
TS2769's inner line).

- **The forcing constraint.** `createJsxAttributesTypeFromAttributesProperty`
  mints every chunk through `newAnonymousType`: the written attributes
  through `createJsxAttributesType`, and each spread through
  `getSpreadType`. `getNamedMembers` (`checker.go:22049`) then sorts the
  members with `compareSymbols`, by first declaration. A spread member
  keeps its own declaration's place: `yy1` is declared in `obj`, before the
  later literal's `yy`. The port replaced a spread member in place, which
  kept the first writer's position.
- **The change.** `mint_jsx_attributes_chunk` sorts its properties by origin
  symbol, as `spreads.rs`'s `mint_anonymous_properties` does for object
  spreads. A property without an origin (the synthesized `children`) sorts
  last, which is where `compareSymbols` puts a symbol without declarations.
  Written attributes come out in source order, as before.
- **Measured.** Quick oracles over `tsx` and `jsx`, against the same tree
  without the sort: 0 gained and 0 lost as whole cases. The
  `tsxStatelessFunctionComponentOverload4` line now matches. The file's
  other differences are the `| undefined` printing of optional members,
  which belongs to another lane.

## 81. Two indexed accesses; indexed-access targets explained by their object

- **`S[K] → T[J]`** (`structuredTypeRelatedToWorker`'s indexed-access target
  arm, `relater.go:3542`). Native relates the object types and then the
  index types, both reporting. With a generic target index, the
  write-constraint fallback that follows is skipped, so the failing
  component is the whole chain: `T[K] → U[K]` explains as `Type 'T' is not
  assignable to type 'U'.`, and `T[K] → T[J]` as `Type 'K' is not
  assignable to type 'J'.` (`keyofAndIndexedAccessErrors`,
  `mappedTypeRelationships`). The arm declines in two cases: a non-generic
  target index, where the fallback relates the source to the write
  constraint, and a mapped, alias or nested indexed-access object, which
  `isRelatedTo` would normalize first.
- **The relation error for an indexed-access target.** `reportRelationError`
  (`relater.go:4760`) reads the flags of the target's object type when the
  source is not an indexed access. `T[K]` with a type-parameter `T`
  therefore gets the type-parameter explanation over
  `getBaseConstraintOfType(T[K])`: *'number' is assignable to the constraint
  of type 'T[K]', but 'T[K]' could be instantiated with a different subtype
  of constraint 'any'.* (`test1<T extends Record<string, any>, K extends
  keyof T>`). Both the head's `relation_diagnostic` and the chain's
  `chain_report_relation_error` now do this. The chain previously declined
  every indexed-access target. It still declines when the base constraint
  is undecided, or the object is one native normalizes first
  (`indexed_access_target_explained`).
- **Measured.** Quick oracles over `keyof`, `ndexedAccess`, `appedType`,
  `onstrain` and `eneric`: 0 lost. `keyofAndIndexedAccessErrors` fell from 41
  differing rows to 23. Its remainder includes `keyof T | keyof U → keyof T &
  keyof U`, whose native chain interleaves two reports of the same pair,
  and the head's normalized source (`Partial<T>[keyof T]` prints as `T[keyof
  T] | undefined` in `reportErrorResults` when the original has no alias),
  which the port's head printer does not do yet.

## 82. Mapped-type instances have no signatures in the retrace

`Dictionary<string> → Record<string, Bar | null>` reported its head with no
chain. Native's chain is `'string' index signatures are incompatible.` /
`Type 'string' is not assignable to type 'Bar'.`
(`emptyObjectNotSubtypeOfIndexSignatureContainingObject1`, whose own
return statement is a separate relation miss).

- **The cause.** `explain_object_relation` asks for the target's call and
  construct signatures before it reaches `indexSignaturesRelatedTo`. For an
  instance of a mapped type (`Record<string, X>`) the port's
  `signatures_of_type_kind` answers `None`, so the walk declined.
  `resolveMappedTypeMembers` sets no signatures on any mapped type.
- **The change.** `chain_signatures_of_kind` answers an empty list for a
  non-generic mapped type when the general reader is undecided. A generic
  mapped type relates through its own arms and still declines.
- **Measured.** Quick oracles over `appedType`, `ecord` and `ndexSignature`:
  0 lost.

## 83. Heads name the normalized indexed-access pair

`x[k] = y[k]` with `y: Partial<T>` reported `Type 'Partial<T>[keyof T]' is
not assignable to type 'T[keyof T]'.`. Native reports `Type 'T[keyof T] |
undefined' is not assignable to type 'T[keyof T]'.`, followed by `'T[keyof T]'
could be instantiated with an arbitrary type which could be unrelated to
'T[keyof T] | undefined'.` (`mappedTypeRelationships`, eight heads;
`conditionalTypes1`).

- **The forcing constraint.** `isRelatedTo` normalizes both sides before
  relating them: the source for reading, the target for writing.
  `reportErrorResults` (`relater.go:4700`) names the normalized pair unless
  the original has an alias. An indexed access over a mapped type has no
  alias of its own; `Partial<T>` is the alias of its object, not of the
  access.
- **The change.** `normalized_head_pair` runs before the head is reported
  and before its chain is retraced. A deferred indexed access that is not
  an alias instantiation (no reference target) is replaced by
  `simplified_indexed_access`: reading for the source, writing for the
  target, where `Readonly<U>[K]` is `U[K]`. The target is replaced only
  where the caller had not already chosen a different display target. The
  missing-property paths run earlier and are unchanged.
- **The head's own explanation.** For an indexed-access target over an
  unconstrained type parameter, `computeBaseConstraint` has no constraint
  for the object, so the access has none. The head therefore takes the
  arbitrary-type line. §81 had declined whenever the base constraint was
  undecided; an unconstrained type-parameter object now decides it
  (`indexed_access_object_unconstrained`).
- **Still blocked on union order.** The `T[string] | T[number] | T[symbol]`
  chains of §78 reach these heads too. Until `unions.rs` orders two indexed
  accesses (`CompareTypes`, `utilities.go:544`), they print `T[number]`
  first.
- **Measured.** Quick oracles over `appedType`, `onditional`, `keyof`,
  `ndexedAccess` and `eneric`: 0 lost against the previous build.

## 84. (Held: unions.rs) Indexed accesses sort by object and index

Held on `box/r7-reports-held` until `unions.rs` is granted or routed.
`CompareTypes` (`utilities.go:544`) orders two indexed accesses by object
type, then by index type. `getTypeNameSymbol` has no symbol for either, so
the name step leaves them equal. The port compared their printed text
(`T[number]` before `T[string]`) and then fell back to type ids. With the
arm, §78's distribution prints as native does: `T[string] | T[number] |
T[symbol]`. Measured with and without the commit on the same tree: quick
oracles over `keyof`, `ndexed`, `appedType`, `onditional` and `nion` gained
`indexedAccessConstraints` and lost nothing.
