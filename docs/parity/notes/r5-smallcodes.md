# r5-smallcodes: ten small sole-code clusters

Lane on epic `tsr-2zk`, vendor `5b1047d`. Target: the diagnostics cases whose
**only** wrong code (expected-minus-actual and actual-minus-expected code sets,
positions ignored) is one of TS2686, TS2688, TS2306, TS2880, TS2883, TS1238,
TS2652, TS18060, TS2538 or TS2307.

Frozen baseline: branch head `d57fffe` (batch AD snapshots). Diagnostics dump
12,238 rows; types dump 552,533 lines, 545,044 RIGHT, 906 GAP, 6,583 WRONG.
Cases were listed with a throwaway script over `diagverdictdump` (WRONG and
EMPTY_WRONG rows whose differing code set is exactly one target code). Probes
use a native `tsgo` built from the pinned submodule
(`scripts/offline-cargo/build-tsgo.sh`).

## 1. Triage

| Code | Cases | Native site | Root cause in TSR | Where the fix lives |
|---|---|---|---|---|
| TS2652 (missing) | `defaultExportsCannotMerge01`–`04` (es2015) (4); `jsFileCompilationBindMultipleDefaultExports` (also a TS2528 column, not sole) | `checkExportsOnMergedDeclarations` (`checker.go:6909`), its `commonDeclarationSpacesForDefaultAndNonDefault` arm (`:6952`) | `merged_export_spaces.rs` computed the default-common set only to keep those declarations out of TS2395 and never reported TS2652. | **fixed, §2.1** |
| TS2306 (extra) | `reactJsxReactResolvedNodeNext`, `…NodeNextEsm`, `sideEffectImports3` (auto/legacy, `noUncheckedSideEffectImports`) (4) | `resolveExternalModule`, `!isSideEffectImport(errorNode)` (`checker.go:15358`) | `check_untyped_module_import`'s not-a-module arm reported on `import "./script"`. | **fixed, §2.2** |
| TS2686 (extra) | `jsxNamespaceImplicitImport…FromConfigPickedOverGlobalOne` ×2, `…FromPragmaPickedOverGlobalOne`, `reactTransitiveImportHasValidDeclaration`, `umdGlobalAugmentationNoCrash`, `umdNamespaceMergedWithGlobalAugmentationIsNotCircular` (6); `jsdocReferenceGlobalTypeInCommonJs` is a missing one (JSDoc, r5-jsdoc4) | `onSuccessfullyResolvedSymbol` (`checker.go:1840`) | Two shapes: `export = React` beside a non-instantiated `declare namespace React {}` (4), and a `declare global { const React }` that collides with the UMD name (2). §3.1. | §3.1 |
| TS2688 (missing) | `tripleSlashTypesReferenceWithMissingExports` ×5 | `fileLoader` type-reference processing (`fileloader.go:512`) → `processingDiagnosticKindUnknownReference` (`processingDiagnostic.go:49`) | The loader has no processing diagnostics at all (`loader.rs` test `an_unknown_reference_lib_adds_nothing`), and the harness reads none. | loader + harness (integrator); §3.2 |
| TS2880 (missing) | `importAssertionsDeprecated`, `…Ignored`, `importTypeAssertionDeprecation`, `…Ignored` (4) | parser: `tryParseImportAttributes` (`parser.go:2497`), export declaration (`:2565`), import type (`:3039`) | The parser accepts `assert` silently in the import/export declaration forms; the import-type form already reports. | parser (main's); §3.3 |
| TS2883 (missing) | `declarationEmitCommonJsModuleReferencedType`, `…ObjectAssignedDefaultExport`, `…ReexportedSymlinkReference3`, `declarationEmitUsingTypeAlias1` (4) | declaration emit, module specifier into a nested `node_modules` | Same cause as `tsr-2zk.999`. | r5-modules2 |
| TS1238 (missing) | `constructableDecoratorOnClass01`, `decoratorCallGeneric`, `decoratorOnClass8`, `esDecorators-arguments` (4) | `checkDecorator` → `getResolvedSignature` → `resolveDecorator` (head `getDiagnosticHeadMessageForDecoratorResolution`) | No decorator call resolution exists in TSR; no TS1238/TS1240/TS1241 site at all. | `calls.rs` (main's); §3.4 |
| TS18060 (missing) | `dynamicImportDefer` (commonjs, es2015, es2020, nodenext) (4) | `checkGrammarImportCallExpression`'s `import.defer` arm (`grammarchecks.go:2167`) | §3.5 | §3.5 |
| TS2538 (missing) | `errorElaboration`, `identifierStartAfterNumericLiteral`, `asyncFunctionDeclarationParameterEvaluation` ×2, `asyncGeneratorParameterEvaluation` ×3 (7) | `getPropertyTypeForIndexType` (`checker.go:27106`, `:27206`) | §3.6 | §3.6 |
| TS2307 (missing) | `importInsideModule`, `noCrashOnParameterNamedRequire`, `privacyGloImportParseErrors`, `tslibInJs`, `emitModuleCommonJS` ×2 (6) | `resolveExternalModule` | §3.7 | §3.7 |

## 2. Fixed here

### 2.1 TS2652

Upstream reports both arms of the final loop:

```go
if declarationSpaces&commonDeclarationSpacesForDefaultAndNonDefault != 0 {
    c.error(name, diagnostics.Merged_declaration_0_cannot_include_a_default_export_declaration_…)
} else if declarationSpaces&commonDeclarationSpacesForExportsAndLocals != 0 {
    c.error(name, diagnostics.Individual_declarations_in_merged_declaration_0_must_be_all_exported_or_all_local, …)
}
```

TSR already computed the default-common set; it only skipped those
declarations. The report is now emitted. The six call sites are unchanged, so
a merge whose only member kinds are a function and an interface still runs
the check only when some other hooked kind is present. That is upstream's
behaviour too: `checkFunctionDeclaration` is not a call site, and the
`defaultExportsCannotMerge01` function is reported from the namespace's call.
TSR's once-per-symbol set reports each declaration once. Upstream's
once-per-kind re-runs produce identical diagnostics, which the diagnostic
collection deduplicates.

Measured: +4 cases (`defaultExportsCannotMerge01`–`04`, es2015). No other
row changed.

### 2.2 TS2306 on side-effect imports

`resolveExternalModule` reports `File_0_is_not_a_module` only
`if errorNode != nil && moduleNotFoundError != nil && !isSideEffectImport(errorNode)`.
`check_untyped_module_import` had the side-effect gate only on its JavaScript
arm (`errorOnImplicitAnyModule`). The not-a-module arm now has it too,
through a port of `isSideEffectImport` (`checker/utilities.go:229`).

Measured: +4 cases (`reactJsxReactResolvedNodeNext`, `…Esm`,
`sideEffectImports3` auto/legacy with `noUncheckedSideEffectImports`).

Both changes together: diagnostics +8, zero losses on both dumps, the types
dump is byte-identical in its verdict columns, and slowcases reports only the
four KNOWN_SLOW cases. Ir (callgrind, `--singleThreaded`): domain-model
1,200,220,508 → 1,199,401,579 (−0.07%), generic-imports 342,911,212 →
342,901,910 (−0.003%). Median child CPU against the base binary read 1.039 on
domain-model at 41 samples, but a base-against-itself run in the same session
read 1.011, and the base slot moved by 3.7% between runs. Ir is the
deterministic measure, and it is flat.

## 3. Open clusters

### 3.1 TS2686

Analysis is in progress; this section records the hypothesis to test with
native `tsgo`.

- `export = React` beside `declare namespace React {}` (4 cases). The
  namespace is non-instantiated, so a `Value` lookup skips it and finds the
  global UMD alias. Upstream nevertheless reports nothing.
- `declare global { const React }` against `export as namespace React` (2
  cases). Upstream reports TS2451 on both declarations, which means the merge
  failed and the globals entry keeps the augmentation's variable. A variable
  declaration is not a `NamespaceExportDeclaration`, so `Every` fails. TSR's
  `global_exports` table answers the UMD alias instead.

### 3.2 TS2688

The faithful port is a loader processing diagnostic for an unresolved
`/// <reference types>`, plus harness plumbing that reports program
diagnostics. Neither the loader's diagnostics channel nor the harness is
owned here.

### 3.3 TS2880

A parser diff. Upstream always reports it, even under `ignoreDeprecations`;
the `…Ignored` baselines carry it.

### 3.4 TS1238

`resolveDecorator` is a `resolveCall` client (calls.rs, main's), so it is
not portable from this lane.

### 3.5–3.7

To be filled in as each cluster is probed.
