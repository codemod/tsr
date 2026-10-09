# r6-typesroots2 — eight `checker_types` root clusters (`tsr-2zk.16.*`)

Round-6 parity box on epic `tsr-2zk`, the second pass over the stale
`.16.x` roots after r6-typesroots (`docs/parity/notes/r6-typesroots.md`,
whose method this note follows: re-measure, classify against native, port,
ship each hook as a diff). The lane owns only the files it creates, plus
r6-typesroots' files, and this note. Every hook in another lane's file ships
as a measured diff under `docs/parity/notes/r6-typesroots2-*.diff`. The query
it calls lands in the box's own file under `#[expect(dead_code)]` until the
diff removes the allowance. Native source is `vendor/typescript-go` @
`5b1047d`.

## 0. Baseline and setup

Frozen base, first: `e2a7b73` (tip of `claude/beautiful-shannon-ar5gh0`
at dispatch, before batch BL). Every measurement in §2, §3 and §5 was taken
there first. Batch BL (r6-typesroots) then landed, and the brief freezes the
base at the tip once it has: **re-frozen at `e6eadf4`** (types 550,221 RIGHT
/ 5,324 WRONG / 758 GAP of 556,303; diagnostics 12,238 cases; Ir
domain-model 1,092,128,580, generic-imports 343,045,098; coverage
checker_types 8,542 of 9,538, diagnostics 4,689 of 5,502). The branch merges
that tip, and both diffs were re-measured on it (§6): each gives the same
transitions there as on `e2a7b73`.

- types 550,122 RIGHT / 5,413 WRONG / 768 GAP of 556,303;
- diagnostics: 12,238 cases;
- Ir (`valgrind --tool=callgrind`, release `tsr`, `-p <project> --noEmit
  --singleThreaded true --pretty false`): domain-model 1,091,529,168;
  generic-imports 343,048,474.

Setup as r5-operators3 §4: PyPI is blocked, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept in the session
scratchpad, outside the repo. The native probe oracle is
`scripts/offline-cargo/build-tsgo.sh`'s `tsgo` (Go 1.26.8, `Version
7.1.0-dev`). Native types were read from its diagnostics (assign the probed
expression to `never`).

## 1. Re-measurement on the frozen base

Failing type lines (non-RIGHT) across each cluster's listed cases on
`e2a7b73`. A case is "fixed" when every one of its lines is RIGHT.

| Cluster | Failing cases | Lines | Fixed cases | Classification |
|---|---|---|---|---|
| `.16.74` CONDITIONAL-INLINE-NODE-INSTANTIATION | 12 of 12 | 121 | — | §2 (held diff) |
| `.16.69` TYPE-PARAMETER-CONSTRAINT-NODE-REUSE | 10 of 16 | 115 | 6 | §4: mostly other roots |
| `.16.79` TUPLE-OPTIONAL-ELEMENT-OPTIONALITY | 4 of 11 | 85 | 7 | §4: root fixed |
| `.16.65` SHADOWED-TYPEPARAM-RENAME | 17 of 19 | 80 | 2 | §5 (diff) |
| `.16.73` IMPORT-TYPE-NODE-TYPEOF | 13 of 13 | 44 | — | §3 (diff) |
| `.16.76` QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE | 11 of 11 | 37 | — | §4 |
| `.16.6` SYNTHETIC-DEFAULT-IMPORT-TARGET | 3 of 12 | 10 | 9 | §4: resolution fixed |
| `.16.7` NULL-WIDENING-TYPES-MISSING | **0 of 12** | 0 | **12** | already fixed |

Fixed cases, by cluster:

- `.16.7`: all twelve (conditionalExpressions2, declFileRegressionTests,
  null, overloadResolutionOverNonCTObjectLit,
  typeParameterFixingWithConstraints, arrayLiterals2ES5,
  computedPropertyNames5_ES6, decrementOperatorWithAnyOtherType,
  incrementOperatorWithAnyOtherType, objectLiteralWidened,
  propertyNameWithoutTypeAnnotation, symbolProperty19). Closeable.
- `.16.6`: allowSyntheticDefaultImports1/4, esModuleInterop,
  esModuleInteropDefaultMemberMustBeSyntacticallyDefaultExport,
  esModuleInteropEnablesSyntheticDefaultImports,
  exportAssignmentWithoutAllowSyntheticDefaultImportsError,
  jsxSpreadFirstUnionNoErrors, nodeNextEsmImportsOfPackagesWithExtensionlessMains,
  nodeNextImportModeImplicitIndexResolution2.
- `.16.79`: destructuringAssignmentWithDefault,
  optionalTupleElementsAndUndefined, tupleTypes,
  typeGuardNarrowsIndexedAccessOfKnownProperty1, unionOfArraysFilterCall,
  callWithSpread5, mappedTypesGenericTuples.
- `.16.69`: cannotIndexGenericWritingError, circularContextualReturnType,
  contextualSignatureInObjectFreeze, genericFunctionsAndConditionalInference,
  inlinedAliasAssignableToConstraintSameAsAlias, spreadObjectOrFalsy.
- `.16.65`: intersectionsOfLargeUnions, intersectionsOfLargeUnions2.

## 2. `.16.74`: a conditional node instantiates for every consumer (HELD diff)

**Forcing constraint.** `getConditionalTypeInstantiation`
(`checker.go:22485`) instantiates an anonymous conditional type node wherever
a mapper reaches it: a signature parameter or return, or a type argument
inside one. `instantiate_conditional_node` (`declared.rs`, r6-declared)
walked to the node's root and answered `errorType` unless that root was a
type parameter's declared constraint. Its comment recorded the reason as
"opening them propagates unsupported consumer types", with no number.
Witness: `createMachine<TTypesMeta …>(config, implementations: TTypesMeta
extends TypegenEnabled ? ActionFunction<…> : ActionFunction<{ type: string }>)`
left `(ev) => …` and `ev` as gaps
(contextualSignatureConditionalTypeInstantiationUsingDefault).

**Port.** `r6-typesroots2-HELD-conditional-node-consumers.diff` removes the
root walk and the gate. Native has no consumer test. The deferred-result
filter below it (a result that stays a deferred conditional is still the
gap, because the mapper-applied node builder is not ported) is unchanged.

**Measured** (alone on `e2a7b73`): types **+35** (14 GAP→RIGHT, 21
WRONG→RIGHT), **zero type losses**, 15 GAP→WRONG, 1 WRONG→GAP. slowcases
clean. Converted: contextualSignatureConditionalTypeInstantiationUsingDefault
5 (fully RIGHT), templateLiteralTypes2 10, reverseMappedTypeIntersectionConstraint
4, doYouNeedToChangeYourTargetLibraryES2015 4, booleanFilterAnyArray 3, and
others.

**Why it is held: one diagnostics loss.**
complicatedIndexesOfIntersectionsAreInferencable goes EMPTY_RIGHT →
EMPTY_WRONG: a TS2339 on `props.foo`, because `props` is now `object` where
native infers `{ foo: string }`. The gate's opening only exposes it. On the
base, with no conditional in sight:

```ts
interface FC<V> { initialValues: V; validate?: (props: V) => void; }
declare function F4<V = object>(x: Readonly<FC<V>>): void;
F4({ initialValues: { foo: "" }, validate: p4 => { p4; } });
```

TSR answers `p4 : V`; native (pinned `tsgo`) answers `{ foo: string }`. The
same call through `Pick<FC<V>, …>` or a bare `FC<V>` is right. Native gets
`V` fixed by `contextuallyCheckFunctionExpressionOrObjectLiteralMethod`
(`checker.go:10152`), which instantiates the contextual SIGNATURE with the
inference context's mapper. `instantiateInstantiableTypes` would not reach
inside the `Readonly` member's function type, so the signature path is
where it happens. TSR's contextual parameter read (`contextual.rs`, main)
reaches a parameter type through a homomorphic mapped member without that
step. Owner: main (`contextual.rs` / `calls.rs`). The diff applies as is
once that lands, and that loss check is its falsifier.

The 15 GAP→WRONG lines are other clusters' causes, now reachable:
contextualParamTypeVsNestedReturnTypeInference2/3 (`a: any` where native
infers `a: string`, MERGED-FUNCTION-INTERFACE),
complicatedIndexesOfIntersectionsAreInferencable (the `object` above), and
declarationsIndirectGeneratedAliasReference (`ns.default` where native prints
`import("mod").default`, SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER).

## 3. `.16.73`: the value-meaning import type, `typeof import("m")…` (diff)

**Forcing constraint.** `getTypeFromImportTypeNode` (`checker.go:24575`)
takes `targetMeaning = Value` for `typeof import(…)`.
`get_type_from_import_type_node` (`declared.rs`) declined it at its first
line (`if node.is_type_of … return error`). Every one of the cluster's 13
cases had a gap or an `any` downstream of that decline.

**Port.** `crate::import_type_value_meaning` is the `IsTypeOf` arm:

- the module through `resolveExternalModuleSymbol(inner, false)`, so an
  `export =` target with its alias resolved;
- unqualified: the module symbol, when it has Value meaning;
- qualified: each identifier read as a **property** of the previous
  symbol's type (`getPropertyOfTypeEx(getTypeOfSymbol(getMergedSymbol(
  resolveSymbol(ns))), name)`), not through its exports table. This is the
  native difference from the type-meaning walk. An interface member is
  therefore a miss (TS2694, `errorType`), and `typeof import("m").A.foo`
  reaches a static method;
- `resolveImportSymbolType`'s Value arm:
  `getInstantiationExpressionType(getTypeOfSymbol(symbol), node)` on the
  unresolved symbol. `typeof import("./input.js").myFunction<any, { slug:
  'hello' }>` instantiates.

Anything this port cannot follow (an alias it does not resolve, a property
miss, a module without Value meaning) answers `None`, and the caller keeps
its gap. Diagnostics are not computed here.

**Hooks** (r6-declared's files, so a diff):
`get_type_from_type_node`'s `ImportTypeNode` arm asks the new query first.
The hook sits at the dispatch, not inside `get_type_from_import_type_node`,
so it does not share context lines with r6-typesroots' import-type-meaning
hook (§6). `instantiation_type_arguments` (`instantiation_expressions.rs`)
also reads an `ImportTypeNode`'s arguments, as native's
`node.TypeArgumentList()` does.

**Measured** (alone on `e2a7b73`): types **+30** (21 WRONG→RIGHT, 9
GAP→RIGHT), **zero losses** on both dumps; diagnostics **+2** WRONG→RIGHT
(declarationEmitWithInvalidPackageJsonTypings,
exportAssignmentExpressionIsExpressionNode). slowcases clean. Ir ×1.00038
domain-model, ×1.00004 generic-imports. Fully RIGHT after:
declarationEmitWithInvalidPackageJsonTypings, importTypeTypeofClassStaticLookup,
moduleResolutionWithRequireAndImport, importAttributes10,
jsDeclarationsTypeReassignmentFromDeclaration; partly: importTypeAmbient,
importTypeGenericTypes, importTypeLocal (each `typeof import(…)` line),
typeofImportInstantiationExpression 2, umdGlobalAugmentationNoCrash 1,
jsDeclarationsParameterTagReusesInputNodeInEmit2 2.

**GAP→WRONG, 4 lines, stated.** jsDeclarationEmitDoesNotRenameImport now
computes the class, and prints it `Test` / `typeof Test` where native writes
`import("./Test.js").default`. No chain names the default-exported class at
that JS site, and this port's printer names it by its declared name. That is
the symbol-chain printer (`tsr-2zk.39`, main's claim), not this resolution.

**Falsifier.** `crates/tsr-checker/tests/import_type_value_meaning.rs`
(shipped in the diff) pins a property walk to a static method, a value
member, written type arguments, and the type-only-member miss, each
expectation read from the pinned `tsgo` first.

## 4. Clusters classified without a port

Each failing line of these clusters' cases was classified against the
pinned native baseline. None still fails for the cluster's own root, so
nothing is ported for them here.

- **`.16.7` NULL-WIDENING-TYPES-MISSING**: all twelve cases RIGHT on the
  base. Closeable.
- **`.16.79` TUPLE-OPTIONAL-ELEMENT-OPTIONALITY**: seven of eleven cases
  RIGHT. The other four (85 lines) carry **no** optional-element line: no
  wanted text with `?]`, `?,` or `| undefined]` fails. They are other roots:
  mappedTypesArraysTuples' `Promise<Awaitified<[…]>>` where native resolves
  the homomorphic tuple mapping to `Promise<[number]>` (and four
  `[number, string, ...boolean[]]` gaps); variadicTuples1's literal
  retention (`[number, true]`, `true[]`) and rest-parameter expansion of a
  labelled tuple (`(x: number, b: boolean, ...args: string[])`);
  genericRestParameters1 and variadicTuples2's `any` callees (the §2
  conditional-consumer gate, and others). Closeable as to its own root.
- **`.16.6` SYNTHETIC-DEFAULT-IMPORT-TARGET**: nine of twelve RIGHT. The
  resolution half is done: `import React from "react"` reaches the
  namespace. The remaining 10 lines print it `typeof import("react").React`
  where native writes `typeof React`, the default-import alias found by
  `getAccessibleSymbolChain`'s `trySymbolTable` (TRY-SYMBOL-TABLE-DEFAULT-IMPORT-ALIAS).
  That is the symbol-chain printer, `tsr-2zk.39`, which main has claimed.
- **`.16.76` QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE**: all 11 cases fail,
  but 31 of the 37 lines already resolve the right type and choose a
  different NAME: `M.T0` for `T0`, `Points.Point` for `A.Point`,
  `server.IServer` for `import("./…_server").IServer`, `Math2d.Point` for
  `m.Point`, `teams.calling.Foo` for `import("./a").Foo`. That is the same
  symbol-chain printer (`tsr-2zk.39`). The rest: umd8 (6 lines) is a UMD
  global alias in type position (`declare let y: Foo` over `export = Thing`),
  which needs both the alias road `declared.rs` declines for ES and UMD
  aliases (§158's naming wall) and r6-typesroots' `export =` instance print
  (`r6-typesroots.md` §8); chained2 (4) is a type-only re-export used as a
  value; enumLiteralAssignableToEnumInsideUnion (1) is enum-union print
  order.
- **`.16.69` TYPE-PARAMETER-CONSTRAINT-NODE-REUSE**: six of sixteen RIGHT.
  Of the 115 lines in the other ten, only **3** differ in a constraint alone:
  declFileRestParametersOfFunctionAndFunctionType (`(...args: any)` written,
  `any[]` printed), styledComponentsInstantiaionLimitNotReached (now RIGHT
  through §5), typeParameterConstraints1 (`T extends any` written; native
  prints `unknown` because `getConstraintFromTypeParameter` maps an `any`
  constraint to `unknownType` and the reuse test then fails). Plus
  correlatedUnions' 3 `keyof { … }` constraints. The other ~105 belong to
  other roots: objectFreeze, objectFreezeLiteralsDontWiden and
  objectFromEntries (40 lines) are literal retention under a primitive
  constraint (`U extends string | bigint | …`) and a contextual function
  return (`=> false`); mappedTypeIndexedAccessConstraint (44) is
  `PartMappings[K]` against `SetOptional<…>[K]`, an alias print. The
  constraint printer is `signatures.rs` (r6-lazytext).

## 5. `.16.65`: the shadow rename resolves where the node builder does (diff)

**Forcing constraint.** `typeParameterToName` (`nodebuilderimpl.go:1404`)
renames a printed type parameter when
`typeParameterShadowsOtherTypeParameterInScope` (`:1396`) finds its name
resolving, from `ctx.enclosingDeclaration`, to a different type parameter.
The `.types` writer passes the assertion's parent as that declaration
(`type_symbol_baseline.go:395`). So an arrow `<T>(x: T) => x` inside
`function f<T>` prints `<T_1>(x: T_1) => T_1`
(`subtypesOfTypeParameter.types:222`), and so does an interface member's
`<T>(x: T) => T` inside `interface I<T>` (`subtypesOfUnion.types:81`).
`signature_to_string_at` (`signatures.rs`, r6-lazytext) anchored the test at
the signature's OWN declaration instead, and the §19 text pass excluded the
whole ancestor chain of that declaration. Both choices declined exactly the
case native renames: a signature nested under the declaration of the
parameter it shadows.

**Port.** Five pieces, one diff, because each was the measured cause of a
loss once the anchor moved:

1. **The anchor.** `rename_type_parameters_for_site` (`inference.rs`, main)
   resolves from `parent(reference)`, the native enclosing declaration, for
   every printer that calls it (signature, composite, type literal, callable
   object). `signature_to_string_at` passes the site like the others.
2. **`resolveNameHelper`'s computed-name arm** (`nameresolver.go:216`,
   `tsr-binder`, main): a class's or interface's type parameters do not
   resolve from its member's computed name. Without it,
   `class C<T> { [foo<T>()]() {} }` renamed `foo`'s own `T`
   (computedPropertyNames32/35, 4 R→W).
3. **`useResult`'s type-parameter half** (`nameresolver.go:54-70`, same
   function): from a function-like's locals, a type parameter is visible
   only when the walk arrives from the return type, a parameter, a
   type-parameter declaration or a JSDoc tag. The binder had ported only the
   non-type-parameter half. Without it, `[foo<T>(a)]<T>(a: T)` saw the
   method's own `T` from its computed name
   (typeParametersAndParametersInComputedNames, 1 R→W).
4. **`enterNewScope`'s synthesized scope**
   (`crate::render_scope_resolution`, new): inside a signature print, the
   builder's lookups see the signature's type parameters first (the
   `NodeFlagsSynthesized` arm of `useResult`). Point 3 made the printer's
   lookups that started at the printed identifier lose them:
   `function a4<A>(x: A) { return new A() }` printed `=> A` where native
   writes `=> globalThis.A` (declarationEmitTypeParameterNameInOuterScope,
   2 R→W). `needs_qualification`, `own_name_alias_at`, `qualified_name_at`
   and `symbol_chain`'s export test (`checker.rs`, main) now resolve through
   `resolve_name_at_print_site`, which reads `render_type_parameter_scope`
   before the binder.
5. **The rename clone keeps the written constraint**
   (`carry_written_annotations_through_rename`, `node_reuse.rs`, no owner).
   `typeParameterToDeclaration` (`:1611`) keeps the parameter's identity under
   a rename, so its constraint is still the written node's type and is
   reused with the renamed names. TSR's rename is a clone with fresh
   parameters, which evaluated `[null] extends [T] ? any : never` to `never`
   (conditionalTypeAssignabilityWhenDeferred) and lost an alias name
   (genericFunctionsAndConditionalInference). The clone now re-reads the
   constraint node under the render's allocations. Where the original's
   build already decided to reuse it (its baked `written_constraint`), that
   decision is kept without re-deciding, because a conditional constraint
   node mints a fresh type per `getTypeFromTypeNode` call here and the
   identity test cannot hold for it.

   The renamed annotation's blanket refusal of any node that declares type
   parameters (`cx.declares_type_parameters`) is narrowed to native's actual
   condition: a declared name already claimed in this render (an allocation
   or an enclosing render's parameter), or resolving at the site to a type
   parameter, would be renamed by `typeParameterToName`, so it is still
   refused. Any other keeps its written name, as native's does. Without
   this, a renamed return `{ [K in PublicKeys1<keyof Obj>]: … }` fell back to
   a structural print without its `import("./internal")` qualifier
   (declarationEmitInlinedDistributiveConditional).

**Measured** (alone on `e2a7b73`): types **+37 WRONG→RIGHT**, **zero losses**
on both dumps, diagnostics unchanged. slowcases clean. Ir ×1.00049
domain-model, ×1.00005 generic-imports. Converted: subtypesOfTypeParameter 6,
subtypesOfTypeParameterWithConstraints2 6, typeParametersAvailableInNestedScope3
3, instanceMemberInitialization 3, declarationEmitTypeParameterNameShadowedInternally
3, declarationEmitShadowing 3, subtypesOfUnion 2,
subclassWithPolymorphicThisIsAssignable 2, declarationEmitNestedGenerics 2,
awaitedTypeStrictNull 2, computedPropertyNames33_ES5/ES6 1+1,
twiceNestedKeyofIndexInference 1, styledComponentsInstantiaionLimitNotReached
1, genericMemberFunction 1.

**Alternative rejected.** Keeping the declaration anchor and widening §19's
text substitution to nested declarations was the obvious patch. It renames
by spelling, not by identity, and the computed-name and outer-class cases
above show that the spelling test answers wrong exactly where native's
scope rules differ.

**Remaining, with cause.** The by-text half of `typeParameterToName`
(`typeParameterNamesByText`): a name already claimed anywhere in the same
print, not only in scope, takes the next suffix
(typeParametersAvailableInNestedScope3's inner `<T_2>`,
declarationEmitTypeParameterNameShadowedInternally's `[T_1, T]` tuple,
inferredReturnTypeIncorrectReuse1's `fn_1`). `rename_type_parameters_for_site`
records that the by-text half "regressed 763 lines when built", because this
port's print units are not native's print contexts. That needs a
print-context owner in the printer lane, not this anchor.

**Falsifier.** `crates/tsr-conformance/tests/r6_typesroots2_shadowed_rename.rs`
(in the diff) pins each of the five pieces with a pinned-baseline line.

## 6. Diffs, in apply order

Every diff applies to this branch's tip (`e6eadf4` merged, plus this lane's
commits). The held diff applies there too.

1. `r6-typesroots2-import-type-value-meaning.diff` (§3): `declared.rs`
   (r6-declared), `instantiation_expressions.rs` (r6-declared),
   `import_type_value_meaning.rs`, tests. +30 types, +2 diagnostics, zero
   losses.
2. `r6-typesroots2-shadowed-type-parameter-rename.diff` (§5):
   `tsr-binder/src/lib.rs` (main), `checker.rs` (main), `inference.rs`
   (main), `node_reuse.rs` (no owner), `signatures.rs` (r6-lazytext),
   `render_scope_resolution.rs`, tests. +37 types, zero losses. Independent
   of 1.

**Re-measured on `e6eadf4`** (batch BL landed). Each alone gives the same
transitions as on `e2a7b73`. The stack 1+2: types **+67** (58 WRONG→RIGHT,
9 GAP→RIGHT; 4 GAP→WRONG, §3), diagnostics **+2**, **zero losses** on both
dumps, slowcases clean, `cargo test --workspace --release` passes. Ir
×1.00073 domain-model, ×1.00011 generic-imports. Coverage: checker_types
8,542 → **8,556** of 9,538 (89.56% → 89.70%), lines 473,297 → 473,358;
checker_types_configured 1,722 → 1,724; diagnostics 4,689 → **4,691** of
5,502. Neither diff shares context lines with batch BL's landed hooks.

Held, not for application:
`r6-typesroots2-HELD-conditional-node-consumers.diff` (§2, `declared.rs`):
+35 types, 1 diagnostics loss. It waits on the contextual signature's
inference-mapper instantiation through a homomorphic mapped member (main).
