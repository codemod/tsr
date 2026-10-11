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
