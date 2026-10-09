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

Frozen base: `e2a7b73` (tip of `claude/beautiful-shannon-ar5gh0` at
dispatch). Batch BL (r6-typesroots) had not landed there, so its seven
diffs are **not** in this base; §6 says how this lane's diffs compose with
them.

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
| `.16.69` TYPE-PARAMETER-CONSTRAINT-NODE-REUSE | 10 of 16 | 115 | 6 | §4 |
| `.16.79` TUPLE-OPTIONAL-ELEMENT-OPTIONALITY | 4 of 11 | 85 | 7 | §4: root fixed |
| `.16.65` SHADOWED-TYPEPARAM-RENAME | 17 of 19 | 80 | 2 | §4 |
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

## 4. What remains, with causes

Filled in as the remaining clusters are classified (§1 order).

## 6. Diffs, in apply order

Every diff applies to `e2a7b73` plus this branch's commits. Hooks are placed
so that none shares context lines with r6-typesroots' seven diffs
(`r6-typesroots.md`, "Diffs, in apply order"); they apply in either order.

1. `r6-typesroots2-import-type-value-meaning.diff` (§3): `declared.rs`
   (r6-declared), `instantiation_expressions.rs` (r6-declared),
   `import_type_value_meaning.rs`, tests. +30 types, +2 diagnostics, zero
   losses.

Held, not for application:
`r6-typesroots2-HELD-conditional-node-consumers.diff` (§2, `declared.rs`):
+35 types, 1 diagnostics loss. It waits on the contextual signature's
inference-mapper instantiation through a homomorphic mapped member (main).
