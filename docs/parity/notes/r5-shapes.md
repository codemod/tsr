# r5-shapes — the mixed type-print buckets, split by producer (`tsr-2zk`)

Round-5 cloud lane. Native source is `vendor/typescript-go` @ `5b1047d`,
`internal/checker/checker.go` unless noted. Measured at integration head
`22a5e1a`. Owned: this note and its diffs, `crates/tsr-checker/tests/*.rs`
for these items, and the producer files no active lane lists:
`destructure.rs`, `objects.rs`, `indexed.rs`, `printing.rs` (the
r5-typetriage part, that lane being finished), and new functions in the hub
`expressions.rs`.

## 1. What was split, and why it had to be

r5-typetriage's classifier (`r5-typetriage/classify.py`) names a line's
*symptom* by string equality. Six of its rows are mixed by construction: a
line lands there when no narrower rule fired. This note splits each row by
the function that actually produced the wrong text, using one probe per
witness (`examples/probefile.rs`, which runs the corpus pipeline on a single
file) and a reading of the pinned Go.

Regenerated at `22a5e1a` (the classifier and `report.py` unchanged):

| bucket | lines | cases touched | cases solely blocked |
|---|---:|---:|---:|
| `signature-differs` | 418 | 146 | 26 |
| `object-members-differ` | 251 | 62 | 15 |
| `partial-any` | 226 | 82 | 14 |
| `any:function-expression-error` | 107 | 59 | 8 |
| `union-members-differ` | 89 | 40 | 8 |
| `any:binding-element` | 57 | 19 | 5 |

`object-members-differ` reads 15 here against the brief's 17: two of its
cases moved between the triage head (`ccb48e7`) and this one.

**A measurement caveat.** `any_audit` aborts at this head on its own
control: 1,976 rows no longer mirror `types_producer::type_at_location`
(`any_audit.rs:968`). Its producer column, which names the two `any:*`
buckets, is therefore a hint here, not evidence. Every `any:*` witness
below was re-probed directly, and the producer named is the one the probe
and the code show. The control failure itself belongs to whoever owns
`any_audit` (the r5-errorsplit work moved the producer under it).

## 2. The split

"Cases" counts the cases a sub-cause solely blocks (every non-RIGHT line in
the case is this sub-cause). A case is named once, under the sub-cause that
owns all of its lines.

### 2.1 Cross-bucket producers

Two producers each span three buckets. They are the largest single roots in
this set.

| sub-cause | producer (function, file) | owner | cases |
|---|---|---|---:|
| **Members of distinct declarations collapse into one type.** Native keeps `(() => void) \| (() => void)` for `a.run` on `T extends Cat \| Dog`, and `A & B` for `window.setTimeout` (the `Window` method and the `typeof globalThis` function). Native's types are distinct because `getObjectTypeInstantiation` re-instantiates an unannotated method with a body for each `this` receiver: `isTypeParameterPossiblyReferenced` answers true for a method with an inferred return (`checker.go`, the `KindMethodDeclaration` arm), so `Cat.run` and `Dog.run` are different types. This port substitutes `this` only where the member's type mentions it (`members.rs` §164/§165), so both receivers share one type and the union deduplicates. | `get_type_of_property_with_this_argument` and the `this`-substitution block in `get_property_type` (`members.rs`) | main (`members.rs`) | 6: `sliceResultCast`, `typeParameterExtendingUnion1`, `typeParameterExtendingUnion2`, `bivariantInferences`, `multiExtendsSplitInterfaces1`, `arrowFunctionContexts(alwaysstrict=true)` |
| **A late-bound method's overload implementation is printed as a signature.** `[Symbol.iterator](x: string): string; [Symbol.iterator](x: number): number; [Symbol.iterator](x: any) {…}` prints a third `(x: any): any`. The binder gives each computed member its own `__computed` symbol, and the sibling merge in `symbols.rs` rebuilds the set by collapsing siblings with **identical printed text**. That is not upstream's rule: `getSignaturesOfSymbol` (`checker.go:19806`) drops a body-carrying declaration that immediately follows a same-kind sibling. The text rule happens to agree when the implementation is spelled like an overload (`overloadsWithComputedNames`) and disagrees otherwise. | the late-bound sibling merge in `get_type_of_func_class_enum_module`'s `'late` block (`symbols.rs:3795`), which needs `is_overload_implementation` (`signatures.rs:1324`) | main (`symbols.rs`); `signatures.rs` r5-funcdecl (a visibility change only) | 4: `symbolProperty39`, `symbolProperty40`, `symbolProperty41`, `symbolDeclarationEmit3` |

### 2.2 `signature-differs` (26 cases)

| sub-cause | producer (function, file) | owner | cases |
|---|---|---|---:|
| Pseudochecker / written-node reuse. Native prints the declaration's written type node where `serializeTypeForDeclaration` (`nodebuilderimpl.go:2181`) finds the pseudo type equivalent: a rest parameter with no annotation inside a function **type** prints `...rest: any`; a return inferred from `null! as typeof v` prints `typeof v`; `{ [n]: string }` keeps its computed names; a member of a written type literal keeps its written order. And it declines reuse for an inaccessible alias (`PrivateSpecialString` → `string`). | `node_reuse.rs` (the pseudochecker equivalence and `tryReuseExistingTypeNode`) | r5-mapped4 | 8: `collisionArgumentsInType(alwaysstrict=true)`, `collisionRestParameterInType`, `emitRestParametersFunctionProperty(target=es2015)`, `emitRestParametersFunctionPropertyES6`, `declarationEmitScopeConsistency3`, `declarationEmitPartialReuseComputedProperty`, `declarationEmitPartialNodeReuseTypeReferences`, `typeName1` |
| Identical-member collapse (§2.1). | `members.rs` | main | 2: `multiExtendsSplitInterfaces1`, `arrowFunctionContexts(alwaysstrict=true)` |
| Implicit return of a function expression whose contextual return type includes `undefined` is `undefined`, not `void`. This is `getReturnTypeFromBody`'s `functionHasImplicitReturn` arm (`checker.go:20126`ff.: a contextual return type containing `undefined` makes the implicit return `undefinedType`). | `get_return_type_from_body` (`signatures.rs:2608`) | r5-funcdecl | 2: `functionsMissingReturnStatementsAndExpressions(target=es2015)`, `functionsMissingReturnStatementsAndExpressionsStrictNullChecks` |
| `void e` answers plain `undefined`. `checkVoidExpression` (`checker.go:10840`) answers `undefinedWideningType`, so `{ c: void 4 }` widens to `c: any` in a non-strict return. | the `VoidExpression` arm of `check_expression` (`expressions.rs:1034`) | this lane (a hub edit: the arm calls a new function) | 2: `declInput`, `declInput3` |
| A nameless enum (`enum void {}`, `function f1(enum)`) prints `` where native prints `(Missing)`. `getNameOfSymbolAsWritten` → `scanner.DeclarationNameToString` (`scanner/utilities.go:76`) prints `(Missing)` for a zero-width name; this port's enum type takes its text from the binder name. | `get_declared_type_of_enum_values` (`declared.rs:6569`), the `name` read | r5-declared2 | 2: `reservedWords3`, `parserEnumDeclaration4` |
| Alias names: `IDestructuring<TFuncs1>` in a constraint (node reuse of a type-parameter constraint), `PartialUser` for `Partial<User>`, `O['prop']` kept where native resolves through the instantiated alias. | `declared.rs` alias attachment; constraint reuse in `node_reuse.rs` | r5-declared2 / r5-mapped4 | 3: `bindingPatternCannotBeOnlyInferenceSource`, `partialTypeNarrowedToByTypeGuard`, `declarationEmitAliasInlineing` |
| Mapped type with `as` clause over array keys: member order and the late-bound `[Symbol.iterator]`/`toString` members. | `mapped.rs` resolveMappedTypeMembers | r5-mapped4 | 2: `mappedTypeWithAsClauseAndLateBoundProperty`, `mappedTypeWithAsClauseAndLateBoundProperty2` |
| Spread of a namespace object: a function export prints in method form (`exportedDirectly(): void`) and a class export as `typeof stuff.klass`. The method form is `addPropertyToElementList`'s `SymbolFlagsFunction\|Method` arm; the qualifier is the symbol-chain printer. | `spreads.rs` (method form); symbol chain | r5-typetriage's item (finished, not landed); main `.39` | 1: `spreadExpressionContextualTypeWithNamespace` |
| Inferred type predicate from `#x in u` (`u is A`). | `getTypePredicateFromBody` road (`signatures.rs`) and private-name `in` narrowing (`flow.rs`) | r5-funcdecl / main | 1: `importHelpersES6` |
| Subtype reduction of `{ foo: M.MyEnum } \| { foo: N.MyEnum }`: two single-member enums from different namespaces relate. | `isEnumTypeRelatedTo` (`relater.rs`) | r5-relater6 | 1: `enumAssignmentCompat4` |
| `T & { … }` printed where native narrows `T` to its constraint union at the declaration. | flow / declared type of an annotated `const` | main (`flow.rs`) | 1: `intersectionSatisfiesConstraint` |
| Binding-pattern parameter in a function type: `({ a: string }: any)` where native implies `{ a: any; }`. | `binding_patterns.rs` (`getTypeFromBindingPattern` for parameters, `.16.47`) | unclaimed (`.16.47`) | 1: `renamingDestructuredPropertyInFunctionType2` |

### 2.3 `object-members-differ` (15 cases)

| sub-cause | producer (function, file) | owner | cases |
|---|---|---|---:|
| Accessor form in a printed type literal: `{ get foo(): number; set foo(v: number \| string); }`. `addPropertyToElementList` (`nodebuilderimpl.go:2524`) prints a getter/setter pair when the write type differs, or for a class property declared only by accessors. | `printing.rs` object-literal member printer | this lane | 2: `circularObjectLiteralAccessors(target=es2015)`, `divergentAccessors1` |
| A parse-recovery property with a missing name (`{ m.x }` → `m` plus `"": any`). `checkObjectLiteral` puts the binder's `""`-named member into `propertiesTable`; §537 (`objects.rs:1821`) skips every empty-named member, on the evidence of `templateStringInPropertyName1`, whose own parse produces no member at all. | `check_object_literal` member loop (`objects.rs:1821`) | this lane | 2: `objectLiteralShorthandPropertiesErrorFromNotUsingIdentifier`, `objectLiteralShorthandPropertiesErrorWithModule` |
| Node reuse: computed names as written (`["a_b_c"]`, `[Enum.A]`), `typeof undefined`, `null \| 'string'`, `ISchema<T[K], any, any, any>`. | `node_reuse.rs` | r5-mapped4 | 5: `declarationEmitComputedPropertyName1`, `declarationEmitComputedPropertyNameEnum2`, `widenedTypes`, `intersectionIncludingPropFromGlobalAugmentation`, `mappedTypeTupleConstraintAssignability` |
| Overload implementation of a late-bound method (§2.1). | `symbols.rs` | main | 1: `symbolDeclarationEmit3` |
| `typeof b` for a variable whose initializer refers to itself (`var b = { foo: b }` printed through the self-reference). | `symbols.rs` circular variable type / printer `typeof` naming | main | 1: `assignmentCompatWithObjectMembers` |
| Numeric-like property name printed quoted (`"0"` vs `0`): `getPropertyNameNodeForSymbol` uses the first declaration's name kind. | `printing.rs` / `objects.rs` property-name text | this lane | 1: `numericStringNamedPropertyEquivalence` |
| Optional overloaded method printed without `?` (`func4?(x: number): number`). | `printing.rs` method-signature printer | this lane | 1: `methodSignaturesWithOverloads` |
| Alias kept (`id.A<1>`), mapped keyof constraint (`keyof T & "x"`). | `declared.rs`; `mapped.rs` | r5-declared2; r5-mapped4 | 2: `declarationEmitNoInvalidCommentReuse3`, `reverseMappedTypeLimitedConstraint` |

### 2.4 `partial-any` (14 cases)

| sub-cause | producer (function, file) | owner | cases |
|---|---|---|---:|
| Overload implementation of a late-bound method (§2.1). | `symbols.rs` | main | 3: `symbolProperty39`, `symbolProperty40`, `symbolProperty41` |
| `typeof x` in a type argument, before `x`'s declaration, answers `any`. | `getTypeFromTypeQueryNode` (`declared.rs`) | r5-declared2 | 1: `noUsedBeforeDefinedErrorInTypeContext` |
| Alias / constraint naming (`Target` vs `keyof Targets<any>`; import-type constraint). | `declared.rs` | r5-declared2 | 2: `genericFunctionsAndConditionalInference`, `spuriousCircularityOnTypeImport` |
| Getter `get x() { return 'boolean' as const }` in a generic argument object infers `any`. | contextual return of an object-literal accessor (`contextual.rs`) | main | 1: `accessorsOverrideProperty8` |
| Written default type arguments: `IterableIterator<number>` kept as written where this port prints `IterableIterator<number, any>`. | `node_reuse.rs` | r5-mapped4 | 2: `builtinIteratorReturn(strictbuiltiniteratorreturn=false)`, `builtinIteratorReturn(strictbuiltiniteratorreturn=true)` |
| `{ get #foo() {} }` (a grammar error) keeps a `get: any` member: parser recovery. | parser | main | 1: `privateNameInObjectLiteral-3(target=es2015)` |
| `typeof import("./types")` for a function merged with a module file. | `symbols.rs` merge | main | 1: `sourceFileMergeWithFunction` |
| Variance probing (13.6 GB case, `tsr-2zk.1041`). | `relater.rs`/`variances.rs` | r5-relater6 | 2: `varianceProblingAndZeroOrderIndexSignatureRelationsAlign`, `…Align2` |
| Unaligned expression text (`y } :`), harness. | `types_producer` | r5-align | 1: `maxConstraints` |

### 2.5 `any:function-expression-error` (8 cases)

All eight are one producer. `get_type_of_function_expression`
(`signatures.rs:6429`) refuses a function expression with an unannotated
parameter unless its contextual signature is **grounded**: no parameter type
of the contextual signature may mention a type parameter (§192/§137). That gate
is this port's own, not upstream's (upstream's `checkFunctionExpressionOrObjectLiteralMethod`
always answers the function's type, `checker.go:9077`). It was added because
an un-instantiated contextual signature typed arrows wrongly. Every witness
here has a contextual signature whose parameters mention an outer type
parameter that upstream's `instantiateContextualSignature` fixes:
`(x) => f(g(x))` under `(r: U) => S` wants `(x: U) => S`.

| sub-cause | producer | owner | cases |
|---|---|---|---:|
| GROUNDED gate refuses a contextual signature that mentions a type parameter. | `get_type_of_function_expression` (`signatures.rs:6429`), open root `.16.70` | r5-funcdecl (file); `tsr-2zk.14` (no lane) | 8: `contextualSignatureInstantiation2`, `genericCallAtYieldExpressionInGenericCall1`, `inferFromGenericFunctionReturnTypes1`, `mismatchedExplicitTypeParameterAndArgumentType`, `generatorTypeCheck62`, `generatorTypeCheck63`, `partiallyAnnotatedFunctionInferenceError`, `tsxInArrowFunction` |

Lifting the gate needs `instantiateContextualSignature`'s inference context
(the outer call's mapper) to be available at the arrow, which is `contextual.rs`
(main). It is reported, not attempted.

### 2.6 `union-members-differ` (8 cases)

| sub-cause | producer (function, file) | owner | cases |
|---|---|---|---:|
| Identical-member collapse (§2.1). | `members.rs` | main | 4: `sliceResultCast`, `typeParameterExtendingUnion1`, `typeParameterExtendingUnion2`, `bivariantInferences` |
| Union origin order of an alias (`(string \| false)[] \| (string \| false)`). | union origin (`unions.rs`) | r5-funcdecl (file) | 1: `literalFreshnessPropagationOnNarrowing` |
| `keyof O` kept as the declared origin in a narrowed reference. | flow narrowing keeps the declared type's origin | main (`flow.rs`) | 1: `exhaustiveSwitchStatements1` |
| Written predicate `value is ("foo" \| "bar")` reused. | `node_reuse.rs` | r5-mapped4 | 1: `typeGuardNarrowsToLiteralTypeUnion` |
| Enum literal union `X.Foo.A \| X.Foo.B` not recombined into `X.Foo` inside a larger union. | union printing of enum literal sets (`printing.rs`/`unions.rs`) | r5-funcdecl (file) | 1: `enumLiteralAssignableToEnumInsideUnion` |

### 2.7 `any:binding-element` (5 cases)

All five are `get_type_for_binding_element_impl` (`destructure.rs:98`),
which this lane owns.

| sub-cause | upstream | cases |
|---|---|---:|
| A computed key that is a symbol misses the string-index fallback. `getPropertyTypeForIndexType` (`checker.go:27075`ff.): "If no index signature is applicable, we default to the string index signature" — `{[sym]: p} = strIndexed` is `string`. This port answers error. | `getPropertyTypeForIndexType` | 1: `lateBoundDestructuringImplicitAnyError` |
| A unique-symbol computed key (`{ [Symbol.iterator]: d } = []`) names a late-bound member. This port names those members by their entity text (`indexed.rs` §381); the destructuring arm never looks them up. | `getPropertyNameFromIndex` → late-bound member | 1: `destructuredLateBoundNameHasCorrectTypes` |
| A union parent containing an empty tuple (`RegExpMatchArray \| []`) with a default: `AccessFlagsAllowMissing` reads the missing tuple element as `undefined`. | `getIndexedAccessTypeOrUndefined` with `AllowMissing` (`checker.go:17776`) | 1: `initializedDestructuringAssignmentTypes` |
| A `never` parent of an array pattern: `isArrayLikeType(never)` holds, and the element is `never`. | `getBindingElementTypeFromParentType` | 1: `arrayDestructuringInSwitch2` |
| Object rest of `object`: `getRestType` over a non-primitive answers `{}`. | `getRestType` | 1: `nonPrimitiveAccessProperty` |

## 3. What this lane takes, in order

By cases unlocked per owned file:

1. `destructure.rs` binding elements (§2.7), up to 5 cases.
2. `expressions.rs` `void` → `undefinedWideningType` (§2.2), 2 cases.
3. `objects.rs` missing-name members (§2.3), 2 cases, if the parse that
   §537 cited really differs.
4. `printing.rs` accessor form, quoted numeric names and optional method
   overloads (§2.3), up to 4 cases.
5. Measured diffs for other lanes' files: the late-bound overload
   implementation (`symbols.rs` + a `pub(crate)` in `signatures.rs`, 4
   cases), the `(Missing)` enum name (`declared.rs`, 2 cases).

Results are recorded in §4 as they land.

## 4. Results

The three owned fixes below were gated together on one unfiltered run
against the frozen base (`22a5e1a`). They touch disjoint code paths, and each
case named below flips on its own commit's change alone (probed per witness).
Both loss checks printed nothing on the combined run:

- types: RIGHT 544,668 → 544,693 (+25 lines, 0 lost);
- diagnostics: RIGHT 5,374 → 5,375 (`initializedDestructuringAssignmentTypes`:
  its TS2551 now names `string`), 0 lost;
- median child CPU, new/old, 21 samples: domain-model 0.974,
  generic-imports 1.000; diagnostics identical to the base binary.

### 4.1 Binding elements (`destructure.rs`, `indexed.rs`)

Five ports, all from `getBindingElementTypeFromParentType` and the
`getPropertyTypeForIndexType` it reaches:

- **String-index fallback for a computed key.** The applicable-signature
  choice with upstream's string fallback was already written inside
  `resolved_indexed_access_type`; it is now `index_info_for_property_key`
  (`indexed.rs`) and the destructuring arm calls it, rather than
  `get_applicable_index_info` alone.
- **Late-bound member for a symbol key.** `late_bound_entity_name`
  (`indexed.rs`) is §381's entity-text key, lifted out of the element-access
  arm so the destructuring arm spells it identically. Rejected: resolving
  the unique symbol to upstream's `__@iterator@N` name. This port does not
  name late-bound members that way anywhere, so the lookup would miss.
- **Union parent, array pattern.** `isArrayLikeType` is
  `isTypeAssignableTo(t, anyReadonlyArrayType)`. The relater answers
  Unknown for `RegExpMatchArray | []`. A union source is related exactly
  when each constituent is, so `union_parent_is_array_like` decides per
  constituent, and **only when the whole-type relation is undecided**. A
  relation that does decide is never second-guessed. A nullable
  constituent asks the relation, because `isArrayLikeType`'s `Nullable`
  gate is on the whole type and strictness belongs to the relation.
- **`never` object.** `getPropertyTypeForIndexType`'s index arm answers an
  `any` or `never` object read with a property-key type as itself. `any`
  already returned early; `never` now returns at upstream's position, after
  the property lookups.
- **Rest of `object`.** `getPropertiesOfType` reads the apparent type, which
  for `object` is `{}`.

Cases converted: `arrayDestructuringInSwitch2`,
`destructuredLateBoundNameHasCorrectTypes`,
`initializedDestructuringAssignmentTypes`,
`lateBoundDestructuringImplicitAnyError`, `nonPrimitiveAccessProperty`. Also
+2 lines each in `declarationEmitComputedNameCausesImportToBePainted` and
`indexingTypesWithNever`, and +6 in `checkJsdocSatisfiesTag15`.

**How this would be wrong.** The per-constituent array-like answer would be
wrong if the relater someday decided a union differently from its
constituents. That cannot happen for a union *source* under
`eachTypeRelatedToType`. The `never` arm would be wrong if a caller relied
on `None` for a `never` object to mean "decline"; none lost a line.

### 4.2 `void` answers `undefinedWideningType` (`expressions.rs`)

`checkVoidExpression` (`checker.go:10840`) is now `check_void_expression`.
The `check_expression` arm calls it, and the hub file gains only that
function. Outside strict mode, `{ c: void 4 }` widens to `c: any` in an
inferred return. In strict mode the two intrinsics are the same type, so
nothing else moves. Cases converted: `declInput`, `declInput3`.
