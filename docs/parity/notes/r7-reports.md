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
