# Property lane notes (tsr-2zk.4)

Judgment calls made while porting property-access resolution and its
diagnostics toward pinned tsgo (`vendor/typescript-go` @ `5b1047d`). Numbers are
measured with `diagverdictdump` against the frozen baseline at `0d996e8`.

## 1. TS2339 on a flow-narrowed identifier receiver

**Forcing constraint.** `crate::nonexistent_property` reports TS2339 only when
it can certify the miss. One certificate,
`receiver_type_is_the_declared_one`, declined every identifier receiver whose
flow type differed from its symbol's declared type, because when it was drawn
this port's narrowing produced wrong types (`controlFlowInstanceof`,
`narrowByClauseExpressionInSwitchTrue7`, `typePredicateInLoop`). Upstream has
no such decline: `checkPropertyAccessExpressionOrQualifiedName`
(`checker.go`) reports on whatever `checkNonNullExpression` answered.

**Measurement.** With the decline removed outright, the whole corpus gained 37
baseline diagnostics and 7 false ones. All 7 came from the same road:
`getNarrowableTypeForReference` (`checker.go:31491`) substituting the base
constraint of an **indexed-access** or **conditional** declared type —

- `deeplyNestedConstraints`: `Extract<M[K], ArrayLike<any>>` reads as
  `string | number | boolean | number[]`, because `crate::constraints`' conditional
  arm unions both branches instead of `getDefaultConstraintOfConditionalType`'s
  substituted true branch (`M[K] & ArrayLike<any>`);
- `dependentDestructuredVariables` (6 lines): a contextually typed parameter
  whose declared type still carries the uninstantiated `ClientEvents[K]`.

**Decision.** Keep the decline only for that road: trust the flow type when
`narrowable_type_for_reference(declared) == declared` (pure narrowing), or when
every substituted part is a plain type parameter (its base constraint is its
declared constraint). Indexed-access, conditional, substitution and
intersection parts still decline. Result: 37 baseline lines, 0 false, 0
losses; `enumPropertyAccess`, `narrowExceptionVariableInCatchClause`,
`narrowFromAnyWithInstanceof`, `typeGuardsWithAny` convert.

**Rejected alternative.** Removing the decline entirely (+4 cases, 1 loss).
It wins once `crate::constraints` ports the conditional true-branch
substitution and contextual parameters are instantiated before their bodies are
checked; then both residual false reports disappear and this gate can go.

**Falsifier.** A new false TS2339 on an identifier receiver whose declared type
is a plain type parameter or a non-generic type would show the flow walk, not
the constraint road, is wrong.

## 2. TS2339 certified by a captured member image

**Forcing constraint.** `property_is_known_absent` certified a miss only
through `declared_members_are_complete` (class/interface/type-literal
declarations) or a lib interface. An object literal, spread result, JSON module
or contextually instantiated image is a `Named` type whose binder symbol is an
`ObjectLiteralExpression` or nothing, so the walk declined it — even though the
checker had already published the image's complete property list in
`anonymous_properties`, the list `property_names_of` treats as
`getPropertiesOfType` for the TS2551 suggestion.

**Decision.** `apparent_type_lacks` accepts that list as the certificate when
both roads miss (the image lacks the name *and* `get_property_of_type` answers
`None`), then asks index signatures per name as before. A name the image holds
but the lookup cannot reach declines: that is this port's gap.

Two prerequisites came with it, because the certificate exposed them:

- **`isJSLiteralType`** (`utilities.go:1753`) is upstream's arm in
  `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11344`) that
  answers `any` silently for a JS literal receiver when `noImplicitAny` is off.
  It was not ported because nothing reached it; without it `expandoOnAlias`,
  `exportNestedNamespaces2`, `amdLikeInputDeclarationEmit` and
  `propertyAssignmentOnImportedSymbol` lose. Ported for property access only;
  the element-access miss already has its own JS-literal handling in
  `crate::indexed`.
- **Three roads whose image is not upstream's type** decline
  (`object_image_road_is_uncertified`): a literal symbol with `exports` (JS
  expando assignments, `ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge`);
  a JS `/** @type {T} */ (literal)` whose tag does not compute and falls through
  to the literal (`strictOptionalProperties4`); a binding/parameter default,
  whose union reduction with the declared property type keeps the fresh `{}`
  where upstream's strict-subtype rule keeps the declared type
  (`nonPrimitiveAndEmptyObject`). Each is owned elsewhere (js, js, destructure);
  when it is fixed its decline can go.

**Measured.** 49 baseline lines, 0 false, 0 losses; 9 cases convert
(`checkingObjectDefinePropertyOnFunctionNonexistentPropertyNoCrash1`,
`importWithTrailingSlash`, `lambdaParamTypes`, `requireOfJsonFileInJsFile`,
`requireOfJsonFileWithEmptyObjectWithErrors`, `checkJsdocSatisfiesTag6`,
`requireOfESWithPropertyAccess`, `spreadMethods`,
`typeSatisfaction_optionalMemberConformance`).

**Cost.** No new table: the image lookup is one hash probe after a miss that
already paid for `get_property_of_type`. `is_js_literal_type` runs only after a
certified miss.

**Falsifier.** A false TS2339 on a receiver whose image is published by a
producer that captures a partial list (an inserter that stores fewer properties
than `getPropertiesOfType` would) shows the list is not a certificate for that
producer; the fix is at the producer, or a decline naming it here.

## 3. TS2339 on a `never` receiver

**Forcing constraint.** `getApparentType(never)` is `never`, `getPropertyOfType`
misses on it, and no index info applies, so
`checkPropertyAccessExpressionOrQualifiedName` reports TS2339 on `never`. Only
`silentNeverType` is any-like there. `property_is_known_absent` had no arm for
it and declined every `never` receiver.

**Decision.** Certify a `never` receiver directly, except two shapes whose
`never` upstream would never have produced:

- `unreachableNeverType` — `getFlowTypeOfReference` (`flow.go:111`) answers the
  declared type instead;
- the operand of `x!` — the same line answers the declared type when narrowing
  left only `null`/`undefined`. This port's flow walk lacks that rule
  (`typeGuardsAsAssertions` reads `x!` as `never` after `x = undefined`, 3 false
  lines). The decline goes when `crate::flow` ports `flow.go:111`.

**Measured.** 9 baseline lines, 0 false, 0 losses; `instanceofWithStructurallyIdenticalTypes`,
`narrowByClauseExpressionInSwitchTrue3`, `typeGuardConstructorDerivedClass`,
`nonPrimitiveNarrow`, `typeGuardsInIfStatement`,
`typeGuardsInRightOperandOfOrOrOperator` convert.

**Falsifier.** A false TS2339 on `never` reached by flow narrowing means a
narrowing arm over-narrows; it is fixed in the flow walk, not declined here.

## 4. TS2339 on a function declaration's `typeof`, and inherited lib interfaces

**Forcing constraint.** `getPropertyOfTypeEx` (`checker.go:18899`) answers a
function declaration's `typeof` from the symbol's exports, then falls to the
global `CallableFunction`/`NewableFunction` (under `strictBindCallApply`),
`Function` and `Object` interfaces. `declared_members_are_complete` certifies
only module, enum and class `typeof` receivers, and the lib-interface
certificate in `apparent_type_lacks` refused any interface with an `extends`
clause — and `CallableFunction extends Function`.

**Decision.**

- `function_declaration_lacks`: a symbol that is exactly `FUNCTION`, declared
  only by TS `FunctionDeclaration`s, with no exports (a merged namespace or an
  expando assignment is bound there and declines), misses a name when each
  fallback interface lacks it by `apparent_type_lacks`.
- `apparent_type_lacks` follows an interface's `extends` bases through
  `base_symbols_of` (a base it cannot follow is a gap, as for the lookup), each
  base certified by the same rule, depth-bounded at 32 like the completeness
  walk. A base that *has* the name while the derived lookup missed declines:
  the miss is this port's.

**Measured.** 5 baseline lines, 0 false, 0 losses;
`contextualReturnTypeOfIIFE2` and
`modularizeLibrary_ErrorFromUsingES6FeaturesWithOnlyES5Lib` convert.

**Rejected.** Certifying arrow-function and function-expression `typeof`
receivers the same way: their expando members are recorded on the variable,
not the function symbol, so an empty `exports` does not prove absence.

## 5. TS2729 asked of the resolved property symbol

**Forcing constraint.** `checkPropertyNotUsedBeforeDeclaration`
(`checker.go:11709`) runs inside `checkPropertyAccessExpressionOrQualifiedName`
on the `prop` the lookup found, and asks `isBlockScopedNameDeclaredBeforeUse`
(`checker.go:1922`) of `prop.ValueDeclaration`. This port's
`check_property_used_before_initialization` (check.rs) was a syntactic slice:
`this.X`/`C.X` against the same class's member list. It could not see an enum
member, an object-literal property or a namespace export read from a static
initializer (`classStaticInitializersUsePropertiesBeforeDeclaration`), a
`#private` name (`privateNamesUseBeforeDef`), an `accessor` field written from a
static block (`classStaticBlockUseBeforeDef5`), an uninitialized field read
before the constructor assigns it (`initializerWithThisPropertyAccess`), and
its nested-access test read the *parent* (`this.a.b` declined) where upstream
reads the node's *expression* (`this.bar.prop` reports on `bar`).

**Decision.** Replaced by `check_property_not_used_before_declaration`
(`readonly_target.rs`): resolve `prop` as `getPropertyOfType(getApparentType(leftType))`
(a `#name` through the lexically declaring class), then upstream's conjuncts in
order, with `isBlockScopedNameDeclaredBeforeUse`,
`isUsedInFunctionOrInstanceProperty`, `isPropertyImmediatelyReferencedWithinDeclaration`,
`isImmediatelyUsedInInitializerOfBlockScopedVariable` and
`isInPropertyInitializerOrClassStaticBlock` transcribed whole beside it. The
check.rs helpers of the same names were not reused: they are slices for TS2448
(no static-initializer arm, an extra type-node stop, no decorator quit), owned
by another lane.

**Correction.** §975's note in check.rs said upstream does not report TS2729 on
a JSX tag name. It does, on the opening and self-closing tags; it is only the
*closing* tag that `isInPropertyInitializerOrClassStaticBlock` quits at
(`useBeforeDeclaration_jsx` wants `<C.z>` and `<C.z/>` reported). The earlier
line was right for its own wrong reason.

**Declines** (each answers silence, never a report):
- a union/intersection apparent receiver — upstream's synthetic property has a
  `valueDeclaration` only when its constituents agree, and this lookup answers
  one constituent's symbol;
- `isPropertyInitializedInStaticBlocks` with a static block in range — it asks
  the flow type at the block's end, which needs a synthesized reference;
- a class-like `valueDeclaration` (the computed-name/decorator arm, which is
  check.rs-private);
- `isPropertyDeclaredInAncestorClass` when the first base type does not resolve.
  An `extends` naming a **value** (`extends BaseFactory`) is resolved here
  through the entry's value symbol and its first arity-matching construct
  signature (`resolveBaseTypesOfClass`), because
  `first_base_type_of_class_symbol` resolves the entry in type meaning;
  `checkInheritedProperty` was this port's one loss without it.

**Known divergence.** Upstream uses `emitStandardClassFields` inside
`isBlockScopedNameDeclaredBeforeUse` and `GetUseDefineForClassFields` in the
TS2729 conjunct. The checker stores only the latter (`standard_class_fields`);
they differ only for `useDefineForClassFields: true` on a target below ES2022.
Needs a checker field (hub, not owned).

**Measured.** Every TS2729 line in the corpus now matches (10 cases had missing
lines, 0 false lines before or after); 7 cases convert, 0 losses.

**Falsifier.** A false TS2729 whose `valueDeclaration` sits after the use in
the same file but is reached through a type this port's lookup answers with
the wrong symbol (a merged or instantiated member) would show the lookup, not
the predicate, is at fault.

## 6. TS2803 from the lexical private symbol

**Port.** `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11280`)
reports TS2803 for an assignment target `x.#m` whose
`lookupSymbolForPrivateIdentifierDeclaration` symbol has a method
`valueDeclaration`. It asks the lexical symbol only, not the receiver's type,
so `b.#m = …` with `b: any` reports. `check_private_method_assignment`
(`readonly_target.rs`) reads the lexically declaring class
(`lexical_private_declaring_class`) and the first declaration of that name in
member order, which is the binder's `valueDeclaration`.

**Measured.** 11 baseline lines, 0 false, 0 losses;
`privateNameMethodAssignment`, `privateNameStaticMethodAssignment`,
`privateNameReadonly` convert.

**Perf note.** The first build measured a domain-model CPU-median ratio of
1.048 (21 samples) and 1.052 (41) against the previous commit's binary, while
two copies of one binary measured 1.008 and a rebuild of the previous commit
was bit-identical. The function returns on the first `match` for every
non-`#name` access, so the cost was code placement, not work:
`#[inline(never)]` on it measured 0.943 / 0.983 / 0.956. Kept, with this
record, so the next reader does not remove it as noise.

## 7. TS2576 by `typeHasStaticProperty`, and a clodule's instance side

**`typeHasStaticProperty`** (`checker.go:27215`) asks
`getPropertyOfType(getTypeOfSymbol(containingType.symbol), name)` and tests the
found property's `valueDeclaration` for `static`. This port answered TS2576
from the receiver symbol's own `exports` table (`other_side_of_class_has`), so
an **inherited** static (`c2.bar()` with `class C2 extends A`, `static bar` on
`A`) fell to TS2339 (`classSideInheritance1`, `classImplementsClass6`). Ported
as `type_has_static_property` (`nonexistent_property.rs`), which reads the
`typeof C` lookup and so its inherited statics.

**Removed decline.** When the other side held the name but not as a static
(a namespace export merged onto the class, `$.sammy.x`), the rule answered
silence. Upstream has no such arm: `typeHasStaticProperty` is false and the
plain TS2339 follows (`staticMemberExportAccess`, `cloduleTest2`,
`staticPropertyNotInClassType`).

**A clodule's instance members are complete.** `declared_members_are_complete`
(`member_completeness.rs`, the one function this lane may edit there) refused
every symbol with a `ModuleDeclaration`, because the shared walk reads one as an
unreadable declaration. A namespace merged onto a class files its declarations
in `exports` only — the `typeof C` side — never in `members`, which is what
`getPropertyOfType` reads for the instance type. So for a class symbol the
module declarations are skipped and the class/interface declarations and bases
walked as usual.

**The decline that rode on it.** Measured without one, the arm lost 6 cases
(`moduleAugmentation{DeclarationEmit,ExtendAmbientModule,ExtendFileModule}{1,2}`):
the binder does not run `mergeModuleAugmentation` (`checker.go:1407`), so an
augmented class's members lack what `declare module "./m" { interface C {…} }`
adds, and the old `ModuleDeclaration` refusal had been hiding that by
coincidence. The clodule arm therefore declines a class declared in an external
module or an ambient `declare module "name"` body — the two places an
augmentation can reach. Waits on tsr-2zk.38; when augmentations merge, the
decline goes and the falsifier is those 6 cases staying RIGHT.

**Measured.** 15 baseline lines, 0 false, 0 losses; `classImplementsClass6`,
`classSideInheritance1`, `cloduleTest2`, `mergedClassNamespaceRecordCast`,
`staticMemberExportAccess`, `staticPropertyNotInClassType` convert.
