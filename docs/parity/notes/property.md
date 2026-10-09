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

## 8. Accessibility: a contextual `this`, and destructuring assignment targets

**`getEnclosingClassFromThisParameter` arm 3** (`checker.go:12002`): with no
annotated `this` parameter, the enclosing function's contextual `this`
parameter (`getContextualThisParameterType`) names the class a protected
access is judged from. `enclosing_class_from_this_parameter` read only the
syntactic parameter, so `const f: (this: Foo) => void = function () {
this.protec }` reported a false TS2445 (`protectedAccessThroughContextualThis`).
It now asks `contextual_this_parameter_type` (`expressions.rs`, the
contextual-signature arm). That helper does not port the object-literal and
`obj.m = function` arms, which apply under `noImplicitThis` or in JS; a function
in those positions with no signature answer declines (`Unsupported`) rather
than reading "no class".

**`checkObjectLiteralDestructuringPropertyAssignment`'s accessibility call**
(`checker.go:12608`): each `name: target` or shorthand of an object assignment
target is checked as a **write** with the source type, reported at the name.
Not ported before. `check_object_assignment_accessibility`
(`index_access_reports.rs`) uses the same name-literal rule
(`getLiteralTypeFromPropertyName`, so `[nameX]` with `const nameX = "x"`
counts) and `property_accessibility_error`. The source of a nested `{ a: { x } }`
target is the outer source's `a` property type; a nested target with a default,
or a union/intersection property type, declines (upstream's indexed access type
there is a union with the default or with `undefined`, whose property lookup
this port answers differently).

**Measured.** `protectedAccessThroughContextualThis` (1 false line gone) and
`destructuringAssignment_private` (4 lines) convert; 0 losses.

## 9. TS2542 on a union receiver: constituents through their apparent type

**Forcing constraint.** `getUnionIndexInfos` (`checker.go:13510`) asks
`getIndexInfosOfType` of each constituent, and that reads the constituent
through `getReducedApparentType`, so a `string` constituent contributes
`String`'s `readonly [index: number]: string` and the union's number index is
readonly. `union_index_infos` (`index_signatures.rs`) asked the bare primitive
(no infos), so the union had none; and `check_readonly_index_signature_write`
declined every union receiver. `x[0] = ""` with `x: string | Collection`
(the declared type, which TS2454 returns) is TS2542 upstream
(`classDoesNotDependOnBaseTypes`).

**Decision.** Primitive constituents are mapped through `apparent_type` in
`union_index_infos` (an object constituent is its own apparent type). The
TS2542 rule keeps its union decline for a **dotted** access only, whose
`getPropertyOfType(union)` is `createUnionOrIntersectionProperty`; an element
access asks `getPropertyTypeForIndexType` of the union's own index infos,
which are now upstream's.

**Measured.** 2 baseline lines, 0 false; `classDoesNotDependOnBaseTypes`
converts; 2 checker_types lines gain; 0 losses.

**Remaining TS2542** (not taken): readonly tuples have no index infos in
`get_index_infos_of_type` (`readonlyArraysAndTuples`, 5 lines; upstream's tuple
members come from `ReadonlyArray<union of elements>` — tuple subsystem); an
unresolved key (`ENUM1[A]--`, 1 line) is upstream's `errorType`, which is
any-flagged and applies the enum's reverse index, but this port's error key is
not known to be the deliberate one (§3a); mapped receivers
(`mappedTypeRelationships`, 4 lines) belong to the mapped-type cluster.

## 10. Private names: `checkPrivateIdentifierPropertyAccess` and its fall-through

**Forcing constraint.** The private-name arm of
`checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11268`) has four
outcomes, and this port had one: TS18013 whenever no enclosing class declared
the name and the receiver was not `any`. Upstream reports TS18013 only when the
receiver's **type** has a private-named property of that spelling
(`checkPrivateIdentifierPropertyAccess`, `checker.go:11494`); with none, the
access falls to `reportNonexistentProperty` — TS2339, which this port never
reported for a `#name` (`nonexistent_property` declines private names). An
any-like receiver outside every class body is the grammar error TS18016.

**Decision.** `check_private_identifier_access` (`readonly_target.rs`) ports
the arm in order:
- the lexical lookup is `lookupSymbolForPrivateIdentifierDeclaration`'s, which
  starts at `getContainingClassExcludingClassDecorators` — a `#x` in
  `@dec(x => x.#x) class A { #x }` is not scoped by `A`
  (`esDecorators-privateFieldAccess`). The type road's
  `lexical_private_declaring_class` (`members.rs`, SS190) still omits that
  exclusion; changing it changes types and was not measured here;
- any-like (`any`, `unknown` under `strictNullChecks`, which
  `checkNonNullExpression` makes the error type): silent with a lexical
  declaration, TS18016 outside class bodies;
- TS18013 when the receiver's property of that spelling is private-named and
  declared in a class that does not lexically enclose the lexical one (that
  case is TS18014; `check_private_name_shadowing` reports it, and now requires
  the enclosing relation too — it reported a sibling subclass,
  `privateNamesAndStaticFields`);
- otherwise TS2339, when the miss is certified: `never`, any-like, or
  `private_names_are_complete`, a narrower certificate than
  `declared_members_are_complete` because a `#name` is never answered by an
  index signature, never computed, and never inherited on the static side
  (`addInheritedMembers` skips `isStaticPrivateIdentifierProperty`).

**Text-keyed members.** Upstream files each class's `#x` under its own mangled
name, so `new Child().#foo` inside `Parent` finds `Parent`'s member even when
`Child` redeclares `#foo`; this port's table answers `Child`'s. An instance
receiver whose class inherits from the lexical class is therefore treated as
holding the lexical member (`privateNamesConstructorChain-1/-2` were the 2
false TS18013 lines without it).

**Declines.** Union/intersection receivers; an error-typed receiver (a gap)
except for the TS18016 position test; a `this` receiver inside a decorator
(`check_this_expression` types it from the decorated class, so `this.#foo` in
`@dec(() => this.#foo) class D {}` inside `C` was a false TS2339).

**Measured.** 27 baseline lines; `esDecorators-privateFieldAccess`,
`privateNameBadAssignment`, `privateNameStaticAccessorssDerivedClasses`,
`privateNameStaticFieldAccess`, `privateNamesUnique-2` convert; 0 losses.
**2 false lines, kept and reported:** `privateNameStaticMethodClassExpression`
types `C.getClass()` (and `D` inside its own static initializer) as `any` where
upstream infers `typeof D`, so the faithful any-like arm reports TS18016 where
upstream reports TS18013. The producer is the class-expression self-reference /
return-type inference, not this rule (§3a); guessing that this `any` is not
upstream's would be the rejected pattern.

## 11. TS7053/TS2339 on an element access: the captured image certifies too

**Forcing constraint.** `getPropertyTypeForIndexType` (`checker.go:27002`) asks
the same `getPropertyOfType` and index infos as a dotted access, but this
port's element-access miss accepted only `declared_members_are_complete`, so
an object-literal receiver (`var obj1 = { … }; obj1["0b11010"]`) never reached
the TS7053 family. §2's captured-image certificate (`apparent_type_lacks`) now
applies to an **object** element-access receiver with a published image;
primitives and unions keep their own roads.

**Exposed and ported.** `checkElementAccess` widens the receiver for an
assignment target or a method access for a call, and
`getWidenedTypeOfObjectLiteral` does not carry `ObjectFlagsObjectLiteral`, so a
write to a fresh literal skips the object-literal TS2339 arm and reaches the
TS7052 family (`noImplicitAnyStringIndexerOnObject`: 6 false TS2339 lines on
writes without this). TS7052 itself ("did you mean to call 'set'") is still
declined.

**Exposed and declined.** `tsr_core::jsnum::numeric_value` answers `NaN` for a
radix literal past `u128` (`0B111…1` with ~2,000 digits), where JavaScript
answers a huge double or `Infinity`, so the binder files that member under
`"NaN"` and the image is not upstream's (`obj1["Infinity"]` became a false
TS7053). An image with a numeric-literal name whose value is `NaN` is
uncertified (`object_image_road_is_uncertified`); a literal's value is never
`NaN` upstream. The fix is `numeric_value` accumulating overflowing digits in
`f64` (tsr-core, not owned); then `binaryIntegerLiteralES6` and
`octalIntegerLiteralES6` convert and this decline goes.

**Measured.** 6 baseline lines (`binaryIntegerLiteralES6` 2,
`noImplicitAnyStringIndexerOnObject` 3, `noImplicitAnyIndexing` 1), 0 false,
0 losses; no case flips yet.

## 12. Union property reads honour nonpublic origins (tsr-2zk.16.279)

`createUnionOrIntersectionProperty` (`checker.go:21554`) rejects a union
property whose constituents supply distinct private/protected declarations
without a common one. The union value loop in `members.rs` projected values
without that guard; it now records whether any constituent origin is
nonpublic and, only then, asks the existing canonical
`get_property_of_union_or_intersection_type` supplier, which owns the
privacy/common-declaration decision. No new cache; the per-constituent origin
lookup is query-local and publishes no image. Control: distinct protected
`A`/`B` rejects; a shared inherited `Root` declaration keeps `number`.

## 13. JS `@augments` arguments on inherited member reads (tsr-2zk.16.342)

Native `reparseHosted` copies JSDoc `@augments Base<T>` arguments onto the
heritage reference before `getBaseTypes`/`resolveObjectTypeMembers`, so an
inherited read sees `Base<number>`. `generic_heritage_member` used only the
written arguments; for a JS entry with none it now asks the existing
`jsdoc_augments_type_arguments` supplier (the base-type worker already does).
No new cache or image; the extra lookup runs only for argument-less JS
heritage entries. Converts `jsdocAugments_withTypeParameter` (3 type rows).

## 14. Own member tables in native declaration order (tsr-2zk.4.5)

`getNamedMembers` returns a symbol table's members sorted by `compareSymbols`
(`checker.go:22049`). `collect_structured_property_names` and
`collect_static_property_names` iterated the binder map and appended late-bound
members; they now gather each own partition (instance members or exports, each
with its late-bound declarations) and sort it with the existing
`compare_symbols`. Value filtering, static/instance separation and inherited
traversal order are unchanged; no member values are forced and no cache is
added (the name vector is query-local).

## 15. Receiver widening for method calls and assignment targets (tsr-2zk.16.208)

`checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11262`) widens the
receiver with `getWidenedType` when the access is an assignment target or
`isMethodAccessForCall` (callee of a call/new through parentheses). TSR's
property access never widened, so `[].values()` kept `undefined[]`. The access
now applies the existing `widen_object_literal_freshness` (getWidenedType) in
exactly those two contexts; detached reads keep the unwidened receiver. Control:
`[].values()` → `ArrayIterator<any>`, detached `[].values` →
`() => ArrayIterator<undefined>`, matching tsgo. No cache added.

## 16. Object-literal late-bound accessors (tsr-2zk.16.351)

Native `SymbolFlagsLateBindingContainer` includes object literals
(`getResolvedMembersOrExportsOfSymbol` / `lateBindMember`,
`checker.go:15930/16005`). `late_bound_members_of` walked only class,
interface and type-literal members, so `{ get [k]() {}, set [k](v) {} }` lost
its computed accessors. Object-literal properties now feed the same worker;
its existing (owner `SymbolId`, static) cache and publication are unchanged
and the existing accessor worker merges same-name getter/setter declarations.

## 17. Union-key indexed writes intersect per-key write types (tsr-2zk.16.213)

`getIndexedAccessTypeOrUndefined` (`checker.go:26993`) with a union index and
`AccessFlagsWriting` reads each key's write type and returns their
intersection (reads take the union). The definite-write dispatcher in
`indexed.rs` resolved only a single literal key; a union key now walks its
constituents through the existing `write_type_of_property_of_type` (or the
ordinary indexed access) and intersects them; a failed constituent fails the
access as natively. Query-local vector, no cache. Control: divergent setters
`{set a(v: boolean|string)}`/`{set b(v: boolean|number)}` written through
`'a'|'b'` require `boolean`; the read stays a union. Residual: a computed
symbol getter/setter pair splits into two symbols, so `write_type_of_accessors`
(`symbols.rs`, unowned) misses the setter.

## 18. Setter types instantiate through the receiver reference (tsr-2zk.16.282)

`getWriteTypeOfSymbol` on an instantiated property goes through
`getWriteTypeOfInstantiatedSymbol`, mapping the setter type with the same
reference mapper as the read. `write_type_of_property_of_type` returned the
raw declared setter type, so `T | undefined` escaped through `Box<string>`.
It now applies the existing `instantiate_for_reference` to the setter type;
no write-only mapper or cache. Control: `Box<string>`/`Box<number>` setters
keep `string | undefined` vs `number | undefined`.

## 19. Writes through a namespace import answer `errorType` (tsr-2zk.4.4)

`isAssignmentToReadonlyEntity` (`checker.go:27314`) treats an access whose
parenthesis-stripped receiver resolves to an alias declared by a
`NamespaceImport` as readonly, so `checkPropertyAccessExpressionOrQualifiedName`
and the element-access twin return `errorType` (printed `any`) for a found
member written through `import * as ns`. The existing
`receiver_alias_is_namespace_import` (readonly_target.rs, now crate-visible)
answers the alias question in both property and element assignment typing; the
TS2540 reporter is unchanged. Local namespaces stay writable (control).

## 20. An `object` receiver reads as the empty object (tsr-2zk.4)

`getApparentType` maps `TypeFlagsNonPrimitive` to `emptyObjectType`, so
`checkPropertyAccessExpressionOrQualifiedName` misses `a.nonExist` on
`a: object` against `{}`'s table and reports TS2339 printing the receiver
(`'object'`). `property_is_known_absent` answered `object` itself, which no
completeness certificate covers, so the miss declined. It now takes the
canonical empty object, the same road the destructuring twin already used.
No cache. Control: `a.toString()` (the `Object` augment) stays silent.
Converts `nonPrimitiveAccessProperty`.

## 21. Heritage members relate with the class's `this`; merged interfaces conform (tsr-2zk.4)

`checkClassLikeDeclaration` relates `typeWithThis` to
`getTypeWithThisArgument(baseType, type.thisType)`, and
`issueMemberSpecificError` reads both members off those, so a base member's
`this` is the derived class's `this` type. `issue_member_specific_error` read
each side with itself as the this argument: `sort(): this` in
`MyArray<T> implements Array<T>` became `MyArray<T>` against `T[]` and
reported a false TS2416. Both members are now read through
`get_type_of_property_with_this_argument` with the class's polymorphic `this`
(`class_instance_this_type`, the existing `this_types` identity keyed by
class symbol, minted once; no new cache).

With that fixed, the merged-declaration decline on implemented interfaces and
interface bases goes: `resolveDeclaredMembers` gathers one table from every
declaration, which this port's declared type already does. The class `extends`
arm keeps its lib-merged base decline (`class B extends Uint8Array`): its
whole-type relation still lacks `typeWithThis` (`subclassUint8Array`,
`classExtendingBuiltinType`, `classFieldSuperAccessible` lose without it).
Converts `classWithMultipleBaseClasses`, `elaboratedErrors`,
`genericArrayExtenstions`, `implementArrayInterface`,
`untypedFunctionCallsWithTypeParameters1`, `classImplementsMergedClassInterface`,
`mergedInterfacesWithInheritedPrivates`, `mergedInterfacesWithInheritedPrivates2`.

## 22. `super` accessibility head: TS2513 and TS2855 (tsr-2zk.4)

`checkPropertyAccessibilityAtLocation` (`checker.go:11786`) starts its
`isSuper` arm before any accessibility modifier: an abstract member is TS2513
(`Abstract method '{0}' in class '{1}' cannot be accessed via super
expression.`), then a non-static member with an `isClassInstanceProperty`
declaration (`utilities.go:1017`: a class property without `accessor`, or a
JS expando assignment rooted at `this`, not `C.prototype.x`/`C.x`) is TS2855.
`property_accessibility_error` returned before reaching either for a public
member. Same declaration choice (`modifier_declaration_of`), no cache.
Controls: `super.g()` (method), `super.h` (`accessor`) and static `super.s`
stay silent; `super.p` on a private field is TS2855, not TS2341, as natively.
Converts `classFieldSuperNotAccessible`, `classFieldSuperNotAccessibleJs`,
`classAbstractSuperCalls`.

## 23. TS7053/TS7015 for a `string`/`number` index (tsr-2zk.4)

`getPropertyTypeForIndexType`'s no-index-signature arm (`checker.go:27129`)
also runs for an index with no property name: under `noImplicitAny`, an object
with no applicable (nor `string`) index info reports TS7015 at the argument
when it has a `number` index, else TS7053 at the access chained to `No index
signature with a parameter of type '{0}' was found on type '{1}'.` Only the
literal-key arm was ported. `check_computed_index_implicit_any` now asks it for
an index typed exactly `string`/`number` (a for-in key over numeric property
names reads `number`, `isForInVariableForNumericPropertyNames`), a certified
receiver (`receiver_type_is_the_declared_one`) and a single object apparent
type whose index infos are published (`index_infos_are_certified`: the
completeness walk, a captured member image, or class/interface declarations
plus bases whose computed names are all `Symbol.x` — any other computed name
may late-bind an index signature this port does not publish). Declines:
unions/intersections/generic receivers, class static sides, JS literals,
const enums, `get`/`set` members (TS7052), and a for-in key over a generic
object, whose upstream type is `Extract<keyof T, string>`
(`getTypeForVariableLikeDeclaration`), not the `string` this port answers.
An unwidened object literal answers the union of its members instead.

The literal-key TS7053 now carries its chain (`Property '{0}' does not exist
on type '{1}'.`), as `NewDiagnosticChainForNode` builds it. No cache.
Converts `noImplicitAnyForIn`, `for-inStatementsArrayErrors`.

## 24. A generic reference target's `this` exists before its members are read (tsr-2zk.4)

`getDeclaredTypeOfClassOrInterface` gives a class or interface with local type
parameters its `thisType` when the declared type is created, and
`resolveTypeReferenceMembers` pads every reference's arguments with the
reference itself (`getTypeWithThisArgument`). This port mints `this` lazily on
the first `this` node (`this_types` per class, `this_type_nodes` per interface
declaration), so `instantiate_for_reference_with_this` substituted `this` only
once some check had resolved one: `x.slice` on `[number, string] | [number,
string, string]` was one signature, or natively two (one instantiation per
tuple receiver) only when `lib.es5.d.ts` had been checked first.
`ensure_generic_reference_this_type` (`members.rs`) now mints it before the
substitution: a class through `class_instance_this_type`, an interface into
`this_type_nodes` for every interface declaration not yet minted, so a later
`this` node of any of them resolves to the same identity. A declaration minted
earlier keeps its own (§166 split mint, `declared.rs`). No new cache; one
lookup per generic reference read. Not ported here: the non-generic arms
(`kind == Class`, `!isThislessInterface`), whose eager mint would instantiate
every non-generic class member read; `typeParameterExtendingUnion1/2`
(`T extends Cat | Dog`, `a.run`) wait on them.
Converts `sliceResultCast`.

## 25. A miss on `this` reports against the class (tsr-2zk.4)

`checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11349`) calls
`reportNonexistentProperty(right, IfElse(isThisTypeParameter(leftType),
apparentType, leftType))`: a miss on a polymorphic `this` receiver is asked
and printed against its apparent type, the class (`Property 'foo' does not
exist on type 'A'.`, `D<T>` for a generic class), including the TS2576 static
and TS2551 suggestion arms. `check_nonexistent_property` printed `this`. The
property-access arms now use `containing` (`is_minted_this_type` →
`apparent_type`); element access keeps its receiver
(`getPropertyTypeForIndexType` has no such substitution). No cache.
Converts 25 cases, among them `autoLift2`, `es6DeclOrdering`, `statics`,
`typeOfThisInInstanceMember`, `destructuringParameterProperties2/5`.
