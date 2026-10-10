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
