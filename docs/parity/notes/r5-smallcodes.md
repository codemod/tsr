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
| TS2688 (missing) | `tripleSlashTypesReferenceWithMissingExports` ×5 | `fileLoader` type-reference processing (`fileloader.go:512`) → `processingDiagnosticKindUnknownReference` (`processingDiagnostic.go:49`) | The unresolved-directive arm was never pushed into the loader's existing diagnostics channel. (First triaged as "no channel"; corrected in §3.2.) | **fixed, §3.2** |
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

Probed against native `tsgo` with the shapes reduced to one `.d.ts` in
`node_modules/react` and an importing module:

| `index.d.ts` | tsgo | TSR before |
|---|---|---|
| `export = React; export as namespace React; declare namespace React {}` | nothing | TS2686 at `export = React` |
| the same plus `declare const y: typeof React;` | TS2708 at `typeof React` | TS2686 + TS2708 |
| `declare namespace React { const q: number }` (instantiated) | nothing | nothing |

The second row shows that upstream's `Value` lookup never returns the
global UMD alias here. It fails and falls through to the namespace-meaning
retry that reports TS2708. The reason is `getSymbol(symbols, name, meaning)`:
an alias is admitted only when `getSymbolFlags(alias)&meaning != 0`. The UMD
alias resolves to the module's `export =`, a non-instantiated namespace, which
has no `Value` flag. The binder's `resolve_name` tests only the alias's own
flags, so it returned the alias and the UMD check fired.

**Fixed (commit 2).** `check_umd_global_reference` now declines when the
resolved symbol is an alias whose `get_symbol_flags` lacks `VALUE`. This is
the condition under which upstream's lookup does not return the symbol at
all. The faithful home is `resolve_name`'s table lookup in the binder, which is
main's and is shared by every caller. The decline is confined to this rule,
where the extra symbol is the only effect. Measured: +4 cases
(`jsxNamespaceImplicitImport…FromConfigPickedOverGlobalOne` ×2,
`…FromPragmaPickedOverGlobalOne`, `reactTransitiveImportHasValidDeclaration`),
zero losses on both dumps, types identical, Ir +0.003% / −0.003%.

**Open (2 cases, binder).** `umdGlobalAugmentationNoCrash` and
`umdNamespaceMergedWithGlobalAugmentationIsNotCircular` declare
`declare global { const React }` against `export as namespace React`.
`initializeChecker` puts the UMD alias in `globals` first-in-wins
(`checker.go:1322`). Then `mergeModuleAugmentation` merges the augmentation's
`const` into it. `mergeSymbol` resolves the non-transient alias target
(`checker.go:14153`), finds the module symbol excluded by
`BlockScopedVariableExcludes`, reports TS2451, and **returns `source`**.
`mergeSymbolTable` stores that return value, so `globals["React"]` becomes
the `const`, and `Every(NamespaceExportDeclaration)` fails. The binder's
`merge_symbol` declines alias merges (`binder.rs`, `bd tsr-y4u.12`) and keeps
the alias in `globals`. Fixing it means the conflict arm replaces the globals
entry with the source, which is binder work (main's).

### 3.2 TS2688

The loader already had a diagnostics channel (`LoaderDiagnostic`, §240),
and the harness already reads it through `include_processor_diagnostics`.
The triage row above was wrong to say neither existed. What was missing was
only the arm itself. `fileLoader`'s type-reference processing
(`fileloader.go:512`) appends `processingDiagnosticKindUnknownReference` for
an unresolved `/// <reference types>`. `toDiagnostic`
(`processingDiagnostic.go:49`) renders it as
`Cannot_find_type_definition_file_for_0` at the reference, with the name as
written.

**Fixed (commit 7).** `resolve_type_reference_directives` pushes that
diagnostic in the else arm of its resolved test. The `pkg` resolution
already failed correctly: `exports: "some-other-thing.js"` hides `types`
under both bundler and node16+. Measured against commit 5 (the reporter
diff not applied): +5 cases (`tripleSlashTypesReferenceWithMissingExports`
×5), zero losses on both dumps, types identical, slowcases clean, Ir
+0.002% domain-model / +0.008% generic-imports. Loader unit test
`an_unresolved_type_reference_directive_is_ts2688`.

Not ported: the automatic-type-directive form (`fileloader.go:277`,
`processingDiagnosticKindExplainingFileInclude`). It is a global diagnostic
for an unresolvable `types` entry, and no target case needs it.

`loader.rs` itself is not in another box's ownership. r5-modules2 owns its
*file flags*, and this arm touches none.

### 3.3 TS2880 (measured diff for the parser)

`r5-smallcodes-import-assertions.diff` ports the three parser reports.
`tryParseImportAttributes` (`parser.go:2497`) accepts `assert` only without a
preceding line break and reports at the keyword. The export-declaration form
(`:2565`) and the import-type form (`:3039`) do the same. Upstream reports
TS2880 regardless of `ignoreDeprecations`; the `…Ignored` baselines carry it.
The dynamic `import(…, { assert: … })` form is checker-side and was already
reported (`import_call.rs`).

`parse_import_attributes` serves the import, export and JSDoc `@import`
forms, as upstream's `tryParseImportAttributes` does. The export form in
upstream also requires a module specifier. TSR parses attributes after
`export { a }` with no `from`, as before; the diff does not change that.

The diff updates the `import_types` parser test, which asserted that
`assert` parses silently, and adds `import_assertions_parse_with_ts2880`.

Measured against `base3` (commit 3): +4 cases (`importAssertionsDeprecated`,
`…Ignored`, `importTypeAssertionDeprecation`, `…Ignored`), zero losses on
both dumps, types identical, slowcases clean. Ir (corrected, §4):
domain-model 1,200,318,372 → 1,200,337,632 (+0.002%), generic-imports
342,880,757 → 342,880,840 (+0.00002%). Parser and workspace tests pass;
clippy and fmt are clean on the diff.

### 3.4 TS1238 (routed to main's calls lane)

`checkDecorator` → `getResolvedSignature` → `resolveDecorator`
(`checker.go:8743`). The decorator node is the call-like there:
`getEffectiveDecoratorArguments` (`:30142`) synthesizes its arguments, and
`getDecoratorCallSignature` builds the expected ES or legacy decorator
signature. `getDecoratorArgumentCount` and `getLegacyDecoratorArgumentCount`
(`:9183`) supply the arity message "The runtime will invoke the decorator
with N arguments…". TSR's `resolveCall` port (`calls.rs`) takes only call
expressions, and no decorator call resolution exists.
`constructableDecoratorOnClass01` would need only the
`len(callSignatures) == 0` arm (an `invocationErrorDetails` chain under the
head message). The other three need `resolveCall` with decorator arguments.
This is a `calls.rs` feature, not a diff-sized change.

### 3.5 TS18060

`checkGrammarImportCallExpression` (`grammarchecks.go:2162`) reports TS18060
on an `import.defer(…)` call outside `esnext`/`preserve`. Its plain `import(…)`
arm is TS1323 under `es2015`, which `check_dynamic_import_module_kind` already
had. TSR only had the import-*clause* form (`check_deferred_import_clause`).
`ast.IsImportCall` admits a `MetaProperty` callee only for `import.defer`, so
`import.meta(…)` stays an ordinary call.

**Fixed (commit 3).** The call form is now an arm of the same function.
Measured: +4 cases (`dynamicImportDefer` commonjs, es2015, es2020,
nodenext).

### 3.6 TS2538

Probe (`target es2017`, no annotations):

```ts
async function f1(x, { [z]: y }) { }        // tsgo: TS2304 + TS2538 'any'; TSR: TS2304 only
const x = ({ [foo.bar]: c }) => undefined;  // same
declare const o: { a: number };
const { [q]: w } = o;                       // both: TS2304 + TS2538
```

The variable-declaration form already worked. The parameter form failed at
two points, one behind the other:

1. **The implied type (fixed, commit 5).** An unannotated, uncontextual
   pattern parameter's type is `getTypeFromObjectBindingPattern`
   (`checker.go:17921`). That function **skips** an element whose computed
   name is not `isTypeUsableAsPropertyName`, setting
   `ObjectLiteralPatternWithComputedProperties`. So `{ [foo.bar]: c }` is
   `{}`, and the baseline prints `({ [foo.bar]: c }: {}) => any`.
   `object_pattern_implied_type` declined the whole pattern instead, which
   made the parameter `error`, and `check_binding_element_index_access`
   returned on that `any`. It now skips such elements. Only a
   context-independent name expression is checked, as before. A unique-symbol
   name, which upstream keeps as a member, still declines.
   `ObjectLiteralPatternWithComputedProperties` itself is not a type flag here.
   Its consumers (`isExcessPropertyCheckTarget`, the optionality copy) already
   read the pattern syntactically (`objects.rs` `matching_pattern_element`,
   `assignreport.rs`). Measured alone: +2 type lines (`errorElaboration`
   0:17/0:18 `({ [foo.bar]: c }: {}) => any`), no diagnostics change, zero
   losses.
2. **The reporter gate (measured diff for r5-relater6,
   `r5-smallcodes-binding-pattern-index-image.diff`).**
   `report_missing_index_signature` declines an object whose `TypeData` is
   `Named { members: None }` unless it is a reference or a tuple. A binding
   pattern object keeps its complete image in `object_literal_members`
   (`binding_patterns.rs` `binding_pattern_object`), so the gate treated a
   complete `{}` as unenumerated. The diff admits a type with an
   `object_literal_members` entry. Measured on top of commit 5 (`202b6e0`):
   +6 diagnostics cases (`errorElaboration`,
   `asyncFunctionDeclarationParameterEvaluation` ×2,
   `asyncGeneratorParameterEvaluation` ×3), zero losses on both dumps, types
   identical, slowcases clean, Ir −0.003% domain-model and −0.003%
   generic-imports, checker tests pass.

`identifierStartAfterNumericLiteral` (×4 TS2538 `null`) is `3in[null]` after
a scanner error: an element access with a `null` key, the `:27206` arm with
`TypeToString(null)`. It is unprobed and stays with r5-relater6.

### 3.7 TS2307

Probed with native `tsgo`:

- `importInsideModule` / `privacyGloImportParseErrors`: `import foo =
  require("m")` inside a non-ambient namespace. `checkImportEqualsDeclaration`
  stops at TS1147 (`checkExternalImportOrExportDeclaration` returns false), so
  **the check never resolves the module**. Upstream's TS2307 comes from the
  *use* `foo.x`: `resolveAlias` → `getTargetOfImportEqualsDeclaration` →
  `resolveExternalModuleName` with error reporting. With the use removed,
  tsgo reports TS1147 alone. TSR's TS2307 is a check-time emitter gated by
  `external_import_is_positioned_for_resolution`, so the lazy alias-resolution
  report is missing. The faithful home is the alias-target path in
  `symbols.rs` (main's).
- `noCrashOnParameterNamedRequire`, `tslibInJs`, `emitModuleCommonJS` ×2: JS
  `require(…)` calls. In the first, `require` is a parameter, and tsgo still
  reports, because the JS reparser turns `const x = require("…")` into an
  import syntactically. This is binder/reparser territory (main's), with
  `module_specifiers`/`module_exports` adjacent (r5-modules2). Not probed
  further.

## 4. Correction: Ir for commits 2 and 3

The Ir figures first written for commits 2 and 3 and for the parser diff
were taken from a stale `tsr` binary. `cargo build -p tsr-conformance
--examples -p tsr` builds only the examples, not the `tsr` binary, so the
"after" binary was commit 1's. Re-measured on binaries rebuilt at each commit
(callgrind, `--singleThreaded --pretty false`):

| Binary | domain-model Ir | generic-imports Ir |
|---|---|---|
| base `d57fffe` | 1,200,220,508 | 342,911,212 |
| commit 1 `066297b` | 1,199,401,579 (−0.07%) | 342,901,910 (−0.003%) |
| commit 2 `b06602c` | 1,199,514,643 (+0.009%) | 342,877,152 (−0.007%) |
| commit 3 `4e639ba` | 1,200,318,372 (+0.067%) | 342,880,757 (+0.001%) |
| parser diff on commit 3 | 1,200,337,632 (+0.002%) | 342,880,840 (+0.00002%) |
| commit 5 (implied type) | 1,199,653,527 (−0.055%) | 342,890,683 (+0.003%) |

Each delta is against the row above, except the last two, which are
against commit 3. None of the changes is on a path the bench projects
exercise heavily. domain-model moves by about ±0.07% between binaries that
differ only in cold code, which is code-layout noise; generic-imports stays
within ±0.01%. The commit messages of `b06602c` and `4e639ba` carry the stale
numbers. This table supersedes them.
