# r6-relater3 — single-code relation rows, the `||` type-parameter arm, MAIN diffs (`tsr-2zk.1148`)

Lane under epic `tsr-2zk`, round 6, successor to r6-relater2
([`r6-relater2.md`](r6-relater2.md) on its branch; §7 and §8 there are the
hand-off this lane works from). Owns `relater.rs`, `relation_cache.rs`,
`variances.rs`, `identity.rs`, `index_access_reports.rs`, `assignreport.rs`,
`binary.rs`, its tests under `crates/tsr-checker/tests/` and this file.
Native anchors are `vendor/typescript-go` @ `5b1047d`; every test
expectation was checked against a native tsgo built from that submodule
(`scripts/offline-cargo/build-tsgo.sh`).

## 0. Frozen base

Batch BR (r6-relater2) had not landed on `claude/beautiful-shannon-ar5gh0`
when this lane started; the base is that branch's tip, `5e4d21b` (after
batch BM).

- `diagverdictdump`: RIGHT 5614, EMPTY_RIGHT 5606, WRONG 979, EMPTY_WRONG 39;
- `verdictdump`: RIGHT 550280, WRONG 5266, GAP 757;
- callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,108,876; domain-model 1,091,067,960.

Every `Ir` below uses that command, and each run's complete CLI output is
compared with the base's (`cmp`). Loss checks are `box-protocol.md` §5's, on
`cut -f1,2`, unfiltered; slowcases runs on both dumps.

## 1. Item-5 triage: single-code TS2322 / TS2741 / TS2353

Rows: every WRONG/EMPTY_WRONG case on `5e4d21b` whose only differing code is
2322, 2741 or 2353 (87 TS2322, 16 TS2741, 9 TS2353; r6-relater2's count on
`fb29088` was 84/18/9). The rows without a recorded cause were triaged by
read-only probes against the native tsgo oracle and the port's own CLI
(each cause names the port function and the native counterpart; confidence
is in the probe notes, not repeated here). Columns: case, lines (miss unless
marked extra), cause, file, owner.

| Case | Lines | Cause | File | Owner |
|---|---|---|---|---|
| assignmentCompat1 | 8:1, 10:1 | held primitive-index diff, contextual.rs (`tsr-2zk.1121`) | contextual.rs | MAIN |
| conditionalTypeAssignabilityWhenDeferred | 39:3, 63:9 | r6-relater2 §6 (landed in batch BR) — re-check on `c3c42d0` | relater.rs/declared.rs | landed |
| conditionalTypesExcessProperties | 8:5, 9:5 | r6-relater2 §6 + r6-declared's `55cbd3f` | declared.rs | r6-declared2 |
| deepComparisons | 4:9 | `constraint_of_type` nil for `T[K][K]` (r6-relater §9) | constraints.rs | MAIN |
| divergentAccessorsTypes8, noUncheckedIndexedAccess, symbolProperty46, symbolProperty47 | | the write type of an element access (r5-relater6 §4) | members.rs/expressions.rs | MAIN |
| emptyObjectNotSubtypeOfIndexSignatureContainingObject1/2 | 41:3, 42:3 | `mapValues(foos, f => f.foo)` not inferred | inference.rs | MAIN |
| enumAssignmentCompat3 | 70:1 | r5-relater7 §1 | declared.rs | r6-declared2 |
| exactOptionalPropertyTypesIdentical | 2:12 | `canonical_signature` declines `<T>() => T extends … ? 0 : 1` | signatures.rs | r6-printer4 |
| excessPropertyCheckWithEmptyObject | 4:58 | **§3 diff** | calls.rs | r6-callreport |
| flatArrayNoExcessiveStackDepth | 20:5 | `evaluate_alias_body` under variance markers for an indexed-access body | declared.rs | r6-declared2 (routed, as briefed) |
| identicalTypesNoDifferByCheckOrder | 32:30, 36:30 | nested identity mapped alias keeps `Pick`'s optionality (r6-relater §8.3) | declared.rs/mapped.rs | r6-declared2 |
| indexedAccessRelation | 16:25 | **§3 diff** (with BR) | calls.rs | r6-callreport |
| inferFromNestedSameShapeTuple, unionTypeInference, undefinedAssignableToGenericMappedIntersection | | converted by r6-relater2 (batch BR) | | landed |
| invariantGenericErrorElaboration, thisTypeInFunctions | | resolveTypeReferenceMembers' `this` padding (item 4, not started) | members.rs | MAIN |
| lastPropertyInLiteralWins | 8:5, 9:5 / extra 13:5 | last-wins member types of a fresh literal | objects.rs | r6-printer4 (routed, as briefed) |
| logicalOrOperatorWithTypeParameters | 5:9, 14:9 | **§2 diff** | binary.rs + contextual.rs/checker.rs/jsx_intrinsic.rs | r6-relater3 + MAIN + r6-errorsplit2 |
| mappedTypeAsClauseRelationships | 12:9, 22:9 | per-instance mapped parameters (r6-relater §3) | mapped.rs | r6-declared2 |
| mappedTypeInferenceFromApparentType | 10:1 | inferFromObjectTypes' mapped-to-mapped arm (item 5, not started) | inference.rs | MAIN |
| mappedTypeWithAsClauseAndLateBoundProperty | 3:1 | `as`-clause mapped members over an array | mapped.rs | r6-declared2 (routed, as briefed) |
| narrowingGenericTypeFromInstanceof01 | 13:17 | no relation asked for the call argument | flow.rs/calls.rs | MAIN |
| objectFreezeLiteralsDontWiden | 7:1 | `const T` inference keeps widened members | inference.rs/objects.rs | MAIN |
| objectRestNegative | 6:10 | object-rest assignment relation (item 7, not started) | destructure.rs | MAIN |
| generatorReturnContextualType | 29:3, 34:3 | async-generator return operand context (item 6, not started) | contextual.rs | MAIN |
| typeParameterHasSelfAsConstraint | 2:5 | the check site declines first (r5-relater7 §1) | check.rs | MAIN |
| assignmentCompatWithGenericCallSignatures4 | 12:1 | r5-relater8 §4 | inference.rs | MAIN |
| arrowExpressionBodyJSDoc, importTag24 | | **converted, §4** | assignreport.rs | r6-relater3 |
| genericIndexedAccessVarianceComparisonResultCorrect | extra 26:1 | **converted, §5** | relater.rs | r6-relater3 |
| declarationsAndAssignments | 106:17 | **WIP diff, §6** | relater.rs/assignreport.rs | r6-relater3 |
| reverseMappedTypeLimitedConstraint | 5:13, 14:3 | **§3 diff** | calls.rs | r6-callreport |
| objectLiteralThisWidenedOnUse | 8:21 TS2741 | `check_call_expression_head` answers Unknown when `callee_reads_object_literal_this` (`this.m(...)` in an object-literal method); native resolveCallExpression (checker.go:8471) resolves it | calls.rs | MAIN |
| overloadresolutionWithConstraintCheckingDeferred | 14:37,16:38,19:32 TS2741 | `check_new_expression_diagnostics` never runs `check_single_generic_candidate_arguments` for a generic `new` (CallExpression only); native resolveNewExpression → resolveCall (checker.go:8575, :8843) | calls.rs | MAIN |
| promiseChaining1, promiseChaining2 | 7:55/7:84, 7:50/7:67 | `check_instantiated_candidate_arguments` declines a context-sensitive argument, so elaborateArrowFunction (relater.go:641) never reports at the arrow body | calls.rs | MAIN |
| reverseMappedTypeContextualTypeNotCircular | 10:3 | the call is the error type: reverse-mapped inference through a homomorphic mapped parameter whose template mentions a second inference parameter declines (inferToMappedType, inference.go:948); site not pinned | inference.rs | MAIN |
| staticAnonymousTypeNotReferencingTypeParameter | extra 10:5 | `instantiate_type`/`mentions_type_parameter` miss the outer `T` in a local class's static members; native getObjectTypeInstantiation by outer type parameters (checker.go:22304, :22403) | inference.rs | MAIN |
| strictNullNotNullIndexTypeNoLib, strictNullNotNullIndexTypeShouldWork | extra 23:9, 22:9 | binding-default strip uses `get_type_with_facts(NE_UNDEFINED)` without getNonUndefinedType's generic-constraint mapping (checker.go:31548) | destructure.rs | MAIN |
| targetTypeBaseCalls | 17:61 | `contextual_type_for_argument_resolving` reads only call signatures; `super(...)` needs the base construct signatures (resolveCallExpression's super arm, checker.go:8471) | contextual.rs | MAIN |
| lateBoundAssignmentCandidateJS3 | 5:9 | `assignment_target_type` declines every element-access target in a JS file (getContextualTypeForAssignmentDeclaration's element-access arm unported) | assignreport.rs | r6-relater3 |
| mappedTypeAsStringTemplate | 7:5 TS2741 | closed mapped target: relater's `has_members` false for its data → Unknown; native resolveMappedTypeMembers + propertiesRelatedTo. Also `as` template member `xy` missing (mapped.rs) | relater.rs + mapped.rs | r6-relater3 + r6-declared2 |
| mergeMultipleInterfacesReexported | extra 4:7 | binder `merge_symbol` exports arm inserts the raw source symbol, not getMergedSymbol (checker.go:14116) | binder.rs | MAIN |
| mergeSymbolReexportInterface | 2:7 TS2741 | binder alias-target arm declines the merge (native mergeSymbol returns source on conflict, checker.go:14153) | binder.rs | MAIN |
| mismatchedExplicitTypeParameterAndArgumentType | 10:34 | `check_instantiated_candidate_arguments` declines an array literal (elaborateArrayLiteral, relater.go:522) | calls.rs | MAIN |
| narrowCommaOperatorNestedWithinLHS | extra 10:11,15:11 | `references_match` lacks the source-side comma arm (isMatchingReference, flow.go:1645) | flow.rs | MAIN |
| objectFromEntries | extra 8:7 | overload walk does not re-check a non-empty array literal under each generic candidate (inferTypeArguments, checker.go:9390) | calls.rs | MAIN |
| objectGroupBy | 9:49 | `check_instantiated_candidate_arguments` declines a context-sensitive argument | calls.rs | MAIN |
| objectLitIndexerContextualType | 18:5 TS2353 | `contextual_type_for_object_literal_named_element` answers None for an absent name under a non-applicable index (native nil → implicit any); argument typed error | contextual.rs | MAIN |
| objectLiteralExcessProperties | 45:76 TS2353 | `is_discriminant_property_of_types` returns None on a gap member read through `T extends IFoo` (IFoo undeclared); native finds no property | relater.rs (gap from constraint) | r6-relater3 |
| es2020IntlAPIs | extra 18:50,26:50,32:62,33:78 | `get_contextual_type`'s NewExpression arm finds no context before the call is resolved; the diagnostic walk checks the argument context-free first (native getContextualTypeForArgumentAtIndex resolves on demand, checker.go:29772) | contextual.rs (+calls.rs) | MAIN |
| generatorTypeCheck31 | 2:11 | `has_no_contextual_type` (signatures.rs) lacks a YieldExpression arm, so `yield x => x` is the error type (getContextualTypeForYieldOperand nil, checker.go:29719) | signatures.rs | r6-printer4 |
| generatorTypeCheck7, generatorTypeCheck8 | 4:17, 2:17 | iteration types of a return annotation fail: members inherited through a generic heritage clause (`extends Iterator<number>`) are a gap in `base_symbols_of` (bd tsr-4sc.7) | members.rs (consumer iteration.rs) | MAIN |
| genericCallWithGenericSignatureArguments2 | 37:43 | `check_instantiated_candidate_arguments` declines a context-sensitive argument (elaborateArrowFunction) | calls.rs | MAIN |
| importAttributes9 | 7:27 | `check_import_attributes` omits the opening checkTypeAssignableTo against ImportAttributes (checker.go:5408) | import_attributes.rs | unowned (nearest r6-modules3) |
| importTag24 | 19:17 | `check_return_statement` returns in a JS file; `return_type_from_annotation` never reads JSDoc `@returns` (getEffectiveReturnTypeNode) | assignreport.rs | r6-relater3 |
| intraExpressionInferences | 131:5 | `check_instantiated_candidate_arguments` declines a context-sensitive object literal | calls.rs | MAIN |
| jsdocTemplateTagNameResolution | 10:7 | a generic JSDoc `@typedef` reference is never instantiated (`evaluate_alias_body` takes only TypeAliasDeclaration) | declared.rs | r6-declared2 |
| mappedTypeConstraints2 (10:11,16:11,59:57,90:9) | miss | `deferred_indexed_access` admits a generic key only against `keyof M`, not getIndexTypeForMappedType's remapped keys (checkIndexedAccessIndexType, checker.go:8220) | indexed.rs | r6-errorsplit2 |
| mappedTypeConstraints2 (42:7) | miss | `constraint_from_indexed_access` answers Undecided for a remapping mapped object instead of the index-constraint road (checker.go:17227) | constraints.rs | MAIN |
| mappedTypeConstraints2 (32:7,50:7) | extra | deferred-keyof source arm lacks isDeferredMappedIndex's nameType/constraint continuation (relater.go:3695-3720) | relater.rs | r6-relater3 |
| typeParameterFixingWithContextSensitiveArguments2, …3 | 7:30, 7:35 TS2741 | `check_instantiated_candidate_arguments` declines a context-sensitive argument | calls.rs | MAIN |
| widenedTypes | extra 12:5 | `get_type_from_type_query_node` widens `typeof undefined` only for `intrinsics.undefined`, not the non-strict widening undefined (getWidenedType, checker.go:24102) | declared.rs | r6-declared2 |
| chained | 3:7 TS2741 | type reference through an export-specifier chain uses one-step `resolve_alias` (native resolveAlias follows the chain, checker.go:16266) | declared.rs | r6-declared2 |
| checkJsdocSatisfiesTag10 | 6:5 TS2353 | members/index image of `Partial<Record<K,V>>` (mapped over a mapped modifiers type) not certified; excess and relation never NotRelated | mapped.rs | r6-declared2 |
| checkJsdocSatisfiesTag8 | 6:5 | `get_intended_type_from_jsdoc_type_reference` lacks the `Object.<K,V>` arm (checker.go:23020) | declared.rs | r6-declared2 |
| checkJsxChildrenProperty4 | 38:15,41:15 / extra 34:10 | children half of elaborateJsxComponents (jsx.go:295) answers false for the React intersection children target; branch not pinned | jsx_component.rs | r6-jsx2 |
| controlFlowAssignmentPatternOrder | extra ×12 | `get_initial_or_assigned_type` lacks getAssignedType's destructuring arms (flow.go:2288) | flow.rs | MAIN |
| controlFlowBindingPatternOrder | extra 15:11 | computed union-literal key in a binding element reads the number index instead of distributing (getIndexedAccessTypeEx) | destructure.rs | MAIN |
| ignoredJsxAttributes | 20:11 TS2741 | `check_jsx_attributes_assignable` stops on a hyphenated attribute vs an index-signature target because the relater lacks isIgnoredJsxProperty (relater.go:4645, :719) | relater.rs + jsx_component.rs | r6-relater3 + r6-jsx2 |
| inferTypePredicates | extra 252:7 | `infer_type_predicate_from_body` requires a single-statement body (getTypePredicateFromBody, checker.go:20535) | signatures.rs | r6-printer4 |
| inferenceFromIncompleteSource | 11:11 TS2741 | `type_mentions_literal` decline in `check_instantiated_candidate_arguments` | calls.rs | MAIN (§3) |
| instanceofNarrowReadonlyArray | extra 7:17 | `is_derived_from_decidable` lacks isTypeDerivedFrom's ReadonlyArray arm (relater.go:4989) | flow.rs | MAIN |
| intersectionsAndOptionalProperties | extra 25:7 | property "0" of an intersection with a rest tuple constituent is absent, so no tuple context (isTupleLikeType) | members.rs | MAIN |
| jsdocBracelessTypeTag1 | 3:3 / 20:16 | 3:3: `check_return_statement` returns in JS, no JSDoc full-signature return type; 20:16: JS `@type` union context widens a literal (not pinned) | assignreport.rs / jsdoc | r6-relater3 / r6-jsdoc |
| jsxCallElaborationCheckNoCrash1 | 10:29 | `resolve_jsx_attributes_context` union fallback lacks the string-literal intrinsic arm (jsx.go:898) | jsx_intrinsic.rs | r6-errorsplit2 |
| jsxClassAttributeResolution | 2:19 | IntrinsicClassAttributes as a generic alias instantiated wrongly (getJsxPropsTypeFromClassType, jsx.go:970) | jsx_intrinsic.rs | r6-errorsplit2 |
| jsxFragmentWrongType | 6:28 | fragments get no attributes/children check (jsx.go:129) | jsx_component.rs | r6-jsx2 |
| keyRemappingKeyofResult | 69:5 / extra 16:1 | mapped type over a unique-symbol key has no `[sym]` member; `as` alias conditional keyof is error | mapped.rs / declared.rs | r6-declared2 |
| aliasDoesNotDuplicateSignatures | 4:5 | `get_external_module_member` keeps an `export =` value member only for site-independent types (checker.go:14667) | symbols.rs | MAIN |
| argumentsBindsToFunctionScopeArgumentList | 3:5 | binder `resolve_name` lacks the `arguments` arm (nameresolver.go:229) | binder | MAIN |
| chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2 | 7:49, 10:35 | context-sensitive decline in `check_instantiated_candidate_arguments` | calls.rs | MAIN |
| circularResolvedSignature | 11:9 | same | calls.rs | MAIN |
| coAndContraVariantInferences5 | 9:9 | `infer_to_union` breaks ties by TypeId, not CompareTypes (inference.go:410) | inference.rs | MAIN |
| coAndContraVariantInferences6 | 34:42 | an instantiated generic function-type alias has no signatures (getObjectTypeInstantiation); then the literal gate | declared.rs (+calls.rs) | r6-declared2 |
| contextualTypeBasedOnIntersectionWithAnyInTheMix3 | extra 23:7,24:7 | `contextual_type_for_element_expression` does not drop `any` constituents (getTypeOfPropertyOfContextualType, checker.go:30555) | contextual.rs | MAIN |
| contextualTypeIterableUnions | extra 7:7 | `union_contextual_property_type` second guard widens an array-element literal under a non-discriminable union | contextual.rs | MAIN |
| correctOrderOfPromiseMethod | extra 20:11 | array literal not re-checked per overload candidate (chooseOverload) | calls.rs | MAIN |
| destructuringTypeGuardFlow | extra 15:9,18:9,31:9,34:9 | getFlowTypeOfDestructuring/getSyntheticElementAccess unported in `get_type_for_binding_element` (checker.go:17849) | destructure.rs | MAIN |
| divergentAccessorsTypes3(target=es2015) | extra ×5 | `composite_property_of_type` declines a union receiver, so no union write type (createUnionOrIntersectionProperty, checker.go:21452) | members.rs | MAIN |
| excessPropertyCheckIntersectionWithRecursiveType | 13:9,26:9,39:9 TS2353 | intersection-target property pass dropped when `properties_related_to` answers Unknown for a generic-alias instantiation image constituent (relater.go:3232) | relater.rs (+ member enumeration) | r6-relater3 |
| excessPropertyCheckWithMultipleDiscriminants | 131:5 TS2353 | `union_contextual_property_type` uses discrimination only when it narrows to one constituent (discriminateContextualTypeByObjectMembers, checker.go:30755) | contextual.rs | MAIN |
| excessiveStackDepthFlatArray | 35:13 | namespace-rooted generic qualified alias reference keeps the print-only mint, body never instantiated | declared.rs | r6-declared2 |
| expandoFunctionExpressionsWithDynamicNames2 | extra 6:7,14:7 | assignment-declaration element-access context takes only literal arguments (getContextualTypeForAssignmentExpression, checker.go:29864) | contextual.rs | MAIN |
| exportAssignmentExpressionIsExpressionNode | 7:7 | `get_type_from_import_type_node` declines every `typeof import(...)` (bd tsr-e2u) | declared.rs | r6-declared2 |
| fixingTypeParametersRepeatedly2 | 11:32 TS2741 | context-sensitive decline in `check_instantiated_candidate_arguments` | calls.rs | MAIN |
| genericFunctionInference1 | extra 25:7,29:7 | higher-order propagation ignores hasOverlappingInferences (inference.go:1669) | inference.rs | MAIN |
| mappedTypeInferenceErrors | 16:9 | context-sensitive (method using `this`) decline in `check_instantiated_candidate_arguments`; also `contextual_object_this_type` uses a non-fixing mapper | calls.rs (+contextual.rs) | MAIN |
| mappedTypeOverlappingStringEnumKeys | extra 30:7 | conditional alias over the mapped parameter declines in `get_instantiated_type_reference` → error member | declared.rs | r6-declared2 |
| recursiveTypeReferences1 (60:7,66:8,72:8) | miss | `check_instantiated_candidate_arguments` declines an array literal | calls.rs | MAIN |
| recursiveTypeReferences1 (47:7) | extra | self-referential alias argument is a memberless placeholder (createDeferredTypeReference) | declared.rs | r6-declared2 |
| recursiveTypeReferences2 | 25:7 | self-naming intersection alias stays memberless (`instantiate_intersection_alias`, tsr-2zk.1010) | declared.rs | r6-declared2 |
| renamed | 2:7 TS2741 | renamed type-only import resolves to error (`alias_road`) | declared.rs | r6-declared2 |
| symbolProperty21 | 10:5 TS2353 | `type_mentions_literal` decline precedes the excess check | calls.rs | MAIN (§3) |
| tsxLibraryManagedAttributes | ×11 | `property_name_from_index` lacks a unique-symbol arm; deferred conditional property over the owner's parameter becomes error | indexed.rs (+declared.rs) | r6-errorsplit2 (+r6-declared2) |
| tsxSpreadAttributesResolution6 | 15:10 | `jsx_excess_attribute` declines a union props target | jsx_component.rs | r6-jsx2 |
| typeSatisfaction_propNameConstraining | 6:5 TS2353 | excess check undecided for `Partial<Record<Keys,unknown>>` (`is_known_property_keyed` → index infos None) | index_signatures.rs / mapped image | unowned / r6-declared2 |
| usingDeclarationsWithIteratorObject | extra 14:17 | `globalThis.X` heritage unresolved (`heritage_entity_symbol_worker`) | members.rs | MAIN |

In this lane and not converted (with the reason):
- `lateBoundAssignmentCandidateJS3`: `assignment_target_type` declines an
  element-access target in JS because contextual.rs answers no context for
  most JS assignments (MAIN); lift the decline with that arm.
- `jsdocBracelessTypeTag1` (20:16 left after §4): r6-jsdoc.
- `mappedTypeAsStringTemplate`: a closed mapped target is not `has_members`
  (relater.rs) **and** the `as` template member is missing (mapped.rs);
  needs both.
- `objectLiteralExcessProperties`: `is_discriminant_property_of_types`
  answers `None` on a gap member read through `T extends IFoo` with `IFoo`
  undeclared; the faithful fix is the constraint producing native's error
  type, not the gap (constraints/declared).
- `excessPropertyCheckIntersectionWithRecursiveType`: the intersection
  property pass is dropped when `properties_related_to` answers `Unknown`
  for a generic-alias instantiation constituent (`Example<1>`); member
  enumeration of that image is the cause. Not attempted.
- `mappedTypeConstraints2` extras 32:7, 50:7: the deferred-keyof source arm
  lacks isDeferredMappedIndex's continuation (relater.go:3695-3720); the case
  also needs indexed.rs and constraints.rs rows. Not attempted.
- `ignoredJsxAttributes`: the relater lacks isIgnoredJsxProperty
  (relater.go:4645, :719); the stop is in jsx_component.rs (r6-jsx2). Needs
  both.

Clusters outside the lane, by size: `check_instantiated_candidate_arguments`'
context-sensitive/array-literal declines (calls.rs, now r6-callreport): 13
cases (`promiseChaining1/2`, `objectGroupBy`, `genericCallWithGenericSignatureArguments2`,
`intraExpressionInferences`, `fixingTypeParametersRepeatedly2`,
`typeParameterFixingWithContextSensitiveArguments2/3`,
`chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2`,
`circularResolvedSignature`, `mappedTypeInferenceErrors`,
`mismatchedExplicitTypeParameterAndArgumentType`, `recursiveTypeReferences1`);
declared.rs alias reach (r6-declared2): 9; contextual.rs: 6; members.rs: 4.

## 2. Diff: the `||`/`??` type-parameter arm (`logicalOrOperatorWithTypeParameters`)

[`r6-relater3-logical-or.diff`](r6-relater3-logical-or.diff), against
`5e4d21b`.

**Forcing constraint.** checkBinaryLikeExpression's `||` and `??` arms
(checker.go:12509, :12518) answer `getUnionTypeEx([filtered left, right],
UnionReductionSubtype)` for every operand pair. `check_logical_or_coalescing`
(`binary.rs`) answered the error type for any pair with a type-parameter or
`unknown` constituent (or a reference with such an argument), so `t || u`
was untyped and `var r4: {} = t || u` (lines 5 and 14) reported nothing.
Natively it is `U | NonNullable<T>`, and `U -> {}` fails.

**Ported (`binary.rs`).** The decline is gone: a pair that is neither
identical, `any`, nor a known refinement goes to getUnionType's subtype
reduction (`union_with_subtype_reduction`, removeSubtypes), whose own
undecidable answer stays the error type as before.

**Why the bare lift overflowed the stack.** `discriminatedUnionJsxElement`:
`const v = data.menuItemsVariant ?? ListItemVariant.OneLine` is now
`MenuItemVariant | ListItemVariant.OneLine`, a type generic with a union
constraint. In `<ListItem variant={v} />` the reference `v` then asks for
its contextual type (getNarrowableTypeForReference's
hasContextualTypeWithNoGenericTypes), which discriminates the attributes
(discriminateContextualTypeByJSXAttributes), which checks `v` again, which
asks for its contextual type, and so on. Native ends this cycle in
getContextFreeTypeOfExpression (checker.go:7542): it pushes `anyType` as the
initializer's contextual type before checking it, and getContextualType
answers a pushed node first (findContextualNode, :29350). The port had no
contextual-type stack, so nothing ended it. No depth guard: the cause is the
missing push.

**Ported (MAIN and r6-errorsplit2 files).**
- `Checker::contextual_infos` (`checker.rs`), native's `contextualInfos`:
  a stack of `(node, type)`, pushed and popped around one check;
- `get_contextual_type` (`contextual.rs`) answers a pushed node first
  (findContextualNode);
- `jsx_discriminant_value_type` (`jsx_intrinsic.rs`) pushes `any` for the
  attribute initializer around its context-free check, as
  getContextFreeTypeOfExpression does. Its template-expression decline,
  which waited for exactly this scoped any-context, is lifted.

**Convention record** (`contextual_infos`): native operation
pushContextualType/findContextualNode; key the expression `NodeId` (node
identity, as natively); owned by the checker; published by push and removed
by the matching pop, so no entry outlives the check that pushed it; no
receiver or alias context; no expensive work of its own. Only
getContextFreeTypeOfExpression's push is ported; native's other pushes
(checkExpressionWithContextualType, the inference caches) are not, and the
`isCache` flag is not needed until one of them is.

**Measured** against §0, both loss checks empty, slowcases clean:
- diagnostics: `logicalOrOperatorWithTypeParameters` WRONG → RIGHT (RIGHT
  5615, WRONG 978);
- types: +18 lines (RIGHT 550298, WRONG 5260, GAP 745):
  `logicalOrOperatorWithTypeParameters` 8, `nonNullableTypes1` 4,
  `discriminatedUnionJsxElement` 2, `neverType` 2,
  `nullishCoalescingOperator_not_strict` 2;
- `Ir`: generic-imports 343,108,876 → 343,043,689 (−0.02%); domain-model
  1,091,067,960 → 1,090,813,230 (−0.02%). CLI output identical.

**Falsifier.** A context-free JSX discriminant that natively keeps a
contextual type it does not get from the pushed `any` (the port pushes for
the initializer as written, natively too); or a `||` pair whose subtype
reduction the port decides differently from removeSubtypes, now typed where
it was the error type.

Tests (in the diff, `crates/tsr-checker/tests/r6_relater3_logical_or.rs`):
`a_type_parameter_operand_of_or_reduces_by_subtype`,
`a_context_free_jsx_discriminant_meets_its_pushed_any_context` (overflows
the stack without the push).

## 3. Diff (routed to r6-callreport): the generic-call object-literal argument gates (`excessPropertyCheckWithEmptyObject`, `reverseMappedTypeLimitedConstraint`, `indexedAccessRelation`)

[`r6-relater3-call-excess.diff`](r6-relater3-call-excess.diff), against
`5e4d21b`; independent of §2's diff (no shared file).

**Routed.** After this diff was measured, the integrator moved item 3 to
r6-callreport (calls.rs's whole reporting pass, `tsr-2zk.1153` and
siblings). The diff stays here, measured, for that lane.

**Forcing constraint.** `check_instantiated_candidate_arguments`
(`calls.rs`, MAIN) is the port's isSignatureApplicable with `reportErrors`
for a generic candidate (checker.go:9256). Natively each argument is
re-checked under the instantiated parameter (checkExpressionWithContextualType,
:7484); the port reuses the argument's cached type, so its object-literal
arm declined whenever the target mentions a literal type
(`type_mentions_literal`: a literal member may be preserved under the
instantiated context and widened in the cache) or either side could contain
type variables (`head_could_contain_type_variables`). Both gates stood in
front of the excess-property report too:
- `Object.defineProperty(window, "prop", { value, readonly: false })`: the
  target `PropertyDescriptor & ThisType<any>` mentions `boolean`
  (`true | false`), so TS2353 on `readonly` was never reported;
- `k({ x: 1, y: 2 })` with `k<T extends number>(p: { x: T })`: the target
  `{ x: 1 }` is a literal;
- `this.setState({ a: a })` (`indexedAccessRelation`): the target
  `Pick<S & State<T>, "a">` contains type variables.

**Ported.**
- hasExcessProperties (relater.go:2714) runs on a fresh literal before any
  structural comparison, and against a non-union target it reads only the
  literal's property *names*, which literal preservation does not change.
  The arm now asks `report_fresh_literal_excess_property` (`assignreport.rs`,
  the existing hasExcessProperties port, which handles intersection targets
  where the older `check_excess_properties` declined them) before the
  literal gate. A union target stays behind the gate: its discriminant
  reduction (findMatchingDiscriminantType) reads the members' *types*.
- The type-variable gates are lifted on this arm. The relation is asked
  first, and it answers `Unknown` for a pair it cannot decide; the gates
  were the arm's older stand-in for that. Measured: no verdict or line moves
  on the base, and `indexedAccessRelation` converts once r6-relater2 §5's
  relation (batch BR) is in.

**Alternative.** A real checkExpressionWithContextualType (re-checking the
literal under the instantiated parameter, with a contextual-type push like
§2's) would lift `type_mentions_literal` entirely. That is the faithful end
state, but it needs a per-context type for an expression the port caches
once per node (`node_types`); not attempted here.

**Measured** against §0, both loss checks empty, slowcases clean:
- diagnostics: `excessPropertyCheckWithEmptyObject`,
  `reverseMappedTypeLimitedConstraint` WRONG → RIGHT (RIGHT 5616, WRONG 977);
- with batch BR merged (`origin/…-r6-relater2` at `587bc84`), against BR
  with only the excess reorder: also `indexedAccessRelation` WRONG → RIGHT,
  nothing else moves in either dump. (Measured on the diff before the union
  restriction below, which does not touch that case's non-union target.)
- types unchanged;
- `Ir`: generic-imports 343,068,763 → 343,072,504 (+0.001%); domain-model
  1,090,870,214 → 1,090,114,812 (−0.07%), each against a re-run of the base
  binary (base runs vary by ~0.02%: 343,108,876 and 1,091,067,960 on the
  first). CLI output identical.
- The first draft also ran the early report for union targets: domain-model
  +0.43% `Ir` (160 `find_matching_discriminant_type` calls, 5.7M `Ir`) for no
  verdict change. That is the reason for the union restriction.

**Falsifier.** An excess property the cached literal has but the literal
re-checked under the instantiated parameter would not have (a computed
name whose type depends on context); or a pair with type variables that the
relation now decides NotRelated wrongly and the arm reports.

Test (in the diff, `crates/tsr-checker/tests/r6_relater3_call_excess.rs`):
`an_instantiated_parameter_reports_a_literal_excess_property_first`.

## 4. A JS function's return checks read its JSDoc return type (`arrowExpressionBodyJSDoc`, `importTag24`)

Item 1's triage rows in this lane (§1): three cases whose missing TS2322 is
`check_return_statement` / `check_arrow_expression_body` (`assignreport.rs`)
returning before anything in a JS file.

**Forcing constraint.** checkReturnStatement (checker.go:4086) and
checkFunctionExpressionOrObjectLiteralMethodDeferred's concise-body arm
(:10206) relate against getReturnTypeFromAnnotation (:20058), which has no
JS condition: `declaration.Type()` (in JS the `@returns` type reparseHosted
stores there), an unannotated getter's setter annotation, then
getReturnTypeOfFullSignature (:20091, a JS function's `@type` signature).
The port read only the written annotation and declined JS files outright,
so `/** @returns {number} */ function f() { return "s"; }` was silent.

**Ported.**
- `annotated_return_type`: getReturnTypeFromAnnotation's three arms, with
  the reparsed `@returns` from `jsdoc_reparsed_function` and the full
  signature from `jsdoc_full_signature_return_type` (which reads function
  and method declarations, as that helper already does for parameters).
  `return_type_from_annotation` and the concise-arrow check use it; both JS
  declines are gone.
- getEffectiveCheckNode's `OEKExcludeJSDocTypeAssertion` (checker.go:9381):
  in JS the parentheses of a `@type` cast are the error node, not the
  expression inside them (`arrowExpressionBodyJSDoc` reports at column 44,
  the `(`). `skip_outer_parentheses` stops there too, as SkipParentheses
  stops at the reparsed `AsExpression` natively.

**Kept declined.** The constructor arm (TS2409) stays declined in JS:
`extendsTag5`'s `return a` with `@template {Foo} T` and `@param {T} a`
relates an unconstrained `T` to `A<T>`, because the class's JSDoc
`@template` constraint is not read there (in a .ts file the same class
matches tsgo). Lifting it lost `extendsTag5` (RIGHT → WRONG, a TS2322 and a
TS2409 native does not report). Owner of the constraint: r6-jsdoc.

**Measured** against §0, both loss checks empty, slowcases clean, no other
case's output moved:
- diagnostics: `arrowExpressionBodyJSDoc`, `importTag24` WRONG → RIGHT
  (RIGHT 5616, WRONG 977); `jsdocBracelessTypeTag1` gains its 3:3 line (its
  20:16 is a JS `@type` union context widening a literal: r6-jsdoc);
- types unchanged;
- `Ir`: generic-imports 343,049,980 (−0.02% against 343,068,763),
  domain-model 1,090,346,740 (−0.05% against 1,090,870,214). CLI output
  identical.

**Falsifier.** A JS function whose `@returns` or `@type` the port reads
differently from the reparser (a tag on an outer host, a function
expression's full signature, which the helper does not read yet), now
reporting where native relates or vice versa.

Test: `tests/r6_relater3.rs`
`a_js_function_returns_against_its_jsdoc_return_type`.

## 0a. Re-frozen base

Batch BR (r6-relater2) landed on main as `c3c42d0` (with its
inline-conditional diff). The branch merged it (`4b01243`, clean), and every
measurement from §5 on is against `c3c42d0` alone:
- `diagverdictdump`: RIGHT 5642, EMPTY_RIGHT 5606, WRONG 951, EMPTY_WRONG 39;
- `verdictdump`: RIGHT 550360, WRONG 5191, GAP 752;
- `Ir`: generic-imports 343,093,181; domain-model 1,092,310,010.

§2's and §3's diffs still apply cleanly on `4b01243`. §4's two conversions
show as gains against this base too.

## 5. A new non-generic alias relates structurally, not by alias variance (`genericIndexedAccessVarianceComparisonResultCorrect`)

**Forcing constraint.** `c = d` with `type C = T<A>`, `type D = T<B>` and
`type T<X extends { x: any }> = Pick<X, 'x'>` is silent natively.
getTypeFromTypeAliasReference (checker.go:23609-23616) gives `T<A>` the new
alias `C` with no type arguments, and structuredTypeRelatedToWorker's
alias-variance arm (relater.go:3392) needs `source.alias.typeArguments` and
one alias symbol on both sides, so the pair relates structurally (`{ x:
string }` both ways). The port's image of `C` (`new_alias_instantiation`,
`declared.rs`) keeps `T`'s `(target, arguments)` for members and
inference, and the relater's variance arm read it as `T<A> -> T<B>`, where
`T`'s measured variance fails (`b = a` does fail natively, through the
alias variance).

**Ported.** The variance arm is skipped when either side carries a new
alias (`carries_a_new_alias`: an `alias_of` entry with no arguments over a
reference whose target is a generic type alias). An alias written as a
class or interface reference (`type FooArray = FooBase[]`) is excluded:
natively that is the interned reference with no alias, and its reference
variances still apply. The first draft did not exclude it and lost
`spreadBooleanRespectsFreshness`'s 7 type lines (`Array.isArray`'s
narrowing of a `FooArray` constituent went `any`).

**Measured** against §0a, both loss checks empty, slowcases clean:
- diagnostics: `genericIndexedAccessVarianceComparisonResultCorrect` WRONG →
  RIGHT; types: its line 14 WRONG → RIGHT (+1) (the dump also shows §4's
  two gains);
- `Ir`: generic-imports 343,093,181 → 343,087,647 (−0.002%); domain-model
  1,092,310,010 → 1,092,026,575 (−0.03%). CLI output identical.

**Falsifier.** A pair of new-alias images that natively relate through some
other variance road (the instantiation is itself a type reference with
variances, e.g. a generic alias whose body is a union of references),
now compared structurally and answered differently.

Test: `tests/r6_relater3.rs`
`a_new_non_generic_alias_relates_structurally_not_by_alias_variance`.

## 6. WIP diff: an object binding-pattern type is structured (`declarationsAndAssignments`) — UNMEASURED

[`r6-relater3-WIP-binding-pattern-image.diff`](r6-relater3-WIP-binding-pattern-image.diff),
against `c7d5927`. getTypeFromObjectBindingPattern's object is an ordinary
anonymous object natively; the port's image (`binding_pattern_object`) is an
owner-less `Named` whose members live only in `anonymous_properties`, so
`has_members` answered false and every relation against it was `Unknown`
(`g({ y: false })` and `g0("s")` silent). The diff adds
`is_complete_captured_object_image` (`assignreport.rs`: owner-less OBJECT
image, complete capture, index infos recorded), lets `has_members` accept
it, and gives `missing_required_property` its captured name/optionality
table, so the report is TS2741 as natively. `diagcase
conformance/declarationsAndAssignments` matches the baseline with it, and
the diff carries a test (tsgo-checked). The wrap-up stopped its gate before
the dumps finished: **no loss check, slowcases or Ir yet.** Resume by
applying it and running both dumps against `c3c42d0`.

## 7. Not started (wrap-up)

Items 4–8 of the brief were not started: resolveTypeReferenceMembers' `this`
padding (`members.rs`), inferFromObjectTypes' mapped-to-mapped arm
(`inference.rs`, inference.go:699), the async-generator return operand's
contextual type (`contextual.rs`), the object-rest assignment relation
(`destructure.rs`), and instantiateContextualType's first pass
(`contextual.rs`/`inference.rs`, which still blocks
`r6-relater-write-constraint.diff`). Their causes are as r6-relater2 §7
records them. Routed as briefed: `flatArrayNoExcessiveStackDepth`,
`mappedTypeWithAsClauseAndLateBoundProperty` (r6-declared2),
`lastPropertyInLiteralWins` (r6-printer4).

## 8. Round summary

Commits (each zero-loss on both dumps, slowcases clean, CLI output identical
on both bench projects):

| § | Commit | Diagnostics | Types | Base |
|---|---|---|---|---|
| 4 | `5b81302` JS return checks read `@returns`/full signature | +2 | 0 | `5e4d21b` |
| 5 | `c7d5927` new non-generic alias skips alias variance | +1 | +1 | `c3c42d0` |

Against `c3c42d0` with both: RIGHT 5642 → 5645, WRONG 951 → 948; types RIGHT
550,360 → 550,361 (the §5 gate's dump, which includes §4).

Diffs, in apply order (independent of each other):
1. [`r6-relater3-logical-or.diff`](r6-relater3-logical-or.diff) (§2;
   binary.rs + checker.rs/contextual.rs MAIN + jsx_intrinsic.rs
   r6-errorsplit2): +1 diagnostic, +18 type lines against `5e4d21b`, 0 lost,
   `Ir` −0.02%.
2. [`r6-relater3-call-excess.diff`](r6-relater3-call-excess.diff) (§3;
   calls.rs, routed to r6-callreport): +2 against `5e4d21b`, +1 more with BR
   (`indexedAccessRelation`), 0 lost, `Ir` flat.
3. [`r6-relater3-WIP-binding-pattern-image.diff`](r6-relater3-WIP-binding-pattern-image.diff)
   (§6; relater.rs + assignreport.rs, this lane): UNMEASURED.
