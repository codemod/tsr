# Parity lane: r5-modules2 (tsr-2zk.989, .999, .1060)

Round-5 box on epic `tsr-2zk`, pinned vendor `5b1047d`, successor of
r5-modules (`docs/parity/notes/r5-modules.md`). Items: the
`import-specifier-differs` rows of r5-typetriage, then `tsr-2zk.1060`.

## 1. Baseline and setup

Frozen at `7472473` before any edit: `diagverdictdump` 12,238 rows
(5,375 RIGHT, 5,584 EMPTY_RIGHT, 1,216 WRONG, 63 EMPTY_WRONG),
`verdictdump` 552,533 lines (544,806 RIGHT, 6,769 WRONG, 958 GAP).

PyPI answers 403 here, as for r5-operators3 (§4 there). `assemble.py`'s three
`tomlkit` calls were served by a stdlib-only stand-in (`tomllib` plus a
small TOML writer) in the session scratchpad, reached through `PYTHONPATH`
for one bootstrap run. Nothing of it is committed.

## 2. Where the wrong specifiers come from

r5-typetriage's `import-specifier-differs` rows, regenerated at the
baseline, fall into five clusters. The decisive fact is *which producer*
prints each one, because only one of them is this lane's file:

| Cluster | Example (native / TSR) | Producer |
|---|---|---|
| `.d` suffix kept | `import("./ConstEnum")` / `import("./ConstEnum.d")`; `./foo.html` / `./foo.d.html` | `symbol_chain`'s flat file-module arm (`checker.rs`): spells the module symbol's *name* (`/ConstEnum.d`) as `./{stem}` |
| JS `export =` class | `typeof import(".")` / `typeof import("./index")`; `./foo` / `./foo.d` (`umd9`) | `printing.rs:147`: the same name-based spelling |
| Written import-type text | `import("./node_modules/pkg/import").I` / `import("pkg").I`; `../node_modules/some-dep/dist/inner` / `./inner` | `declared.rs`'s import-type mint (`:5220`): the *written* text, site-independent |
| Written `import()` text | `Promise<{ default: typeof import("./node_modules/pkg/import"); }>` / `…import("pkg")…` | `calls.rs` `check_import_call_expression`'s mint |
| Symlinks / chain choice | `import("typescript-fsa").A` / `…/src/impl`; emotion `JSX` | `GetEachFileNameOfModule`'s symlink cache; `getSymbolChain`'s alias choice — not specifier text |

Native has one producer for all of them: the node builder prints every
module reference through `getSpecifierForModuleSymbol`
(`nodebuilderimpl.go:1249`), at print time, relative to the *enclosing*
file. The port has four, three of which spell text decided at mint time or
from the module symbol's name. None of the three is owned by this lane
(`checker.rs`'s symbol-chain printer is main's `tsr-2zk.39`; `printing.rs`
is r5-shapes'; `declared.rs` is r5-declared2's; `calls.rs` is main's), so
§3 lands the one faithful specifier function they should all call, and §4
ships each producer's change as a measured diff.

## 3. `getSpecifierForModuleSymbol` / `GetModuleSpecifiers`, ported

`Checker::module_specifier_for_file` (`module_specifiers.rs`) is
`GetModuleSpecifiers`' file arm (`modulespecifiers/specifiers.go:19`,
`computeModuleSpecifiers` `:359`) for the node builder's preferences.
`Checker::module_specifier_for_symbol_in_mode` (`checker.rs`, this lane's
function) keeps the two ambient arms and the no-host fallback and hands
every module with a source file to it. Ported, each against its anchor:

- **The node builder's preferences** (`nodebuilderimpl.go:1296`):
  `ImportModuleSpecifierPreferenceProjectRelative` and the `.js` ending
  preference when the builder's resolution mode (override, else the file's
  default) is ESM.
- **`computeModuleSpecifiers`' order**: the existing import (unchanged,
  r5-modules §4), then `tryGetModuleNameAsNodeModule` for a path under
  `node_modules` (r5-modules §5), then `getLocalModuleSpecifier`.
- **`GetAllowedEndingsInPreferredOrder`** (`preferences.go:147`) whole,
  including the `.ts` ending: `shouldAllowImportingTsExtension(options,
  importingSourceFile.FileName())` is true for `allowImportingTsExtensions`
  *or a declaration-file importer*, so an ESM `.d.ts` under node16+ gets
  `[Ts, Js]`. `getPreferredEnding` / `getModuleSpecifierEndingPreference`
  (`:68`, `:121`) with `inferPreference` (`:28`) and
  `usesExtensionsOnImports` (`:18`) over the file's whole `Imports()` list.
  Before, `allowImportingTsExtensions` declined (`None`).
- **`processEnding`** (`specifiers.go:636`) whole: `.json`/`.mjs`/`.cjs`
  kept; `.mts`/`.cts` kept when `.ts` outranks `.js`; `.d.mts`/`.d.cts`
  mapped by `GetJSExtensionForDeclarationFileExtension`; the
  `foo.d.json.ts` → `foo.json` remap (`TryGetRealFileNameForNonJSDeclarationFileName`,
  `util.go:159`), which r5-modules declined; and the `Ts` ending's
  declaration-file rule.
- **`tryGetModuleNameAsNodeModule`'s endings** are
  `getAllowedEndingsInPreferredOrder(ResolutionModeNone)` (`:760`), the
  file's default mode, not the override mode it was given before.

`printing.rs`'s `module_specifier_uses_js_ending` had no caller left
(its whole job is `getPreferredEnding` here) and was removed, the one edit
in that file: clippy's dead-code lint is a workspace gate.

Not ported, each a decline toward the relative answer and named here:

- **`paths`, `rootDirs`, `package.json` `imports`** in
  `getLocalModuleSpecifier`: the checker's host does not expose them (the
  `ModuleHost` lives in `resolution.rs`, not this lane's). Every
  `import-specifier-differs` row is reached without them.
- **`tryGetAnyFileFromPath`** (keep `/index` when `dir.ts` exists beside
  `dir/`): a file-system probe the host does not answer.
- **Symlinks and redirects** (`GetEachFileNameOfModule`): the module path
  list is the file's own path, so `computeModuleSpecifiers`' loops run once.
- **The second ambient arm** (`export =` of a namespace inside
  `declare module "m"`, `specifiers.go:121`): needs `GetSymbolAtLocation`
  through `&mut self`; the specifier function is `&self`.
- **`originalModuleSpecifier`** (`nodebuilderimpl.go:1274`): printing
  *inside* an import declaration reads that import's mode.
- **`links.specifierCache`**: no cache. The answer is a pure function of the
  program and `(symbol, file, mode)`, so recomputing gives native's answer;
  the cost is paid only where a module reference is printed without an
  alias. Checker port convention: native operation `specifierCache`; this
  port holds no table, so there is no key, owner or publication state to
  record; the expensive work is the existing-import walk and the dynamic
  `Imports()` walk, the latter only for a file flagged
  `POSSIBLY_CONTAINS_DYNAMIC_IMPORT` (or JS) whose statement-level imports
  did not decide the ending.

### 3.1 Measured

Unfiltered against the frozen baseline: **no line moves** on either dump
(0 changes in either direction). This is expected and is the point of the
commit: every currently-RIGHT specifier this function spells is still
spelled the same, and every WRONG row is printed by a producer that does not
call it yet (§2). The producers' diffs in §4 are what convert, and they
are measured on top of this commit.

Perf against the baseline binary: median child CPU (21 samples)
domain-model 0.971, generic-imports 1.029; callgrind Ir
(`--singleThreaded --pretty false`) generic-imports 342,896,129 →
342,858,563 (−0.011%), domain-model 1,196,126,364 → 1,195,895,570
(−0.019%). Neither bench prints a computed module specifier.

Tests: `cargo test --workspace --release` passes except
`objects::an_unrecognised_position_keeps_the_unique_type`, which fails
identically at the baseline (`7472473`, r5-instexpr's landed patch), not
here.

### 3.2 `symbolToTypeNode`'s import-type arm (commits 2 and 3)

What a printer writes inside `import(…)` is not `getSpecifierForModuleSymbol`
alone: `symbolToTypeNode` (`nodebuilderimpl.go:659`-`:707`) first asks
whether, under `node16`/`nodenext` resolution, the target file is emitted
as ESM while the context file is not; if so it generates the specifier in
ESM mode and writes `, { with: { "resolution-mode": "import" } }`.

`Checker::import_type_argument` (`module_specifiers.rs`) is that arm, and
`Checker::module_specifier_for_symbol` (this lane's function in
`checker.rs`, called by every printer that writes `import(…)`) now returns
it. `GetEmitModuleFormatOfFile` is `ModuleHost::implied_node_format_for_emit`,
the same host question r5-modules §6's tracker already uses.

**Correction (commit 3).** Commit 2 also ported the arm's second half: a
specifier still diving into `/node_modules/` is regenerated in the swapped
mode and written with that mode's attribute. That half runs only when
`FlagsAllowNodeModulesRelativePaths` is unset (`:678`), and `typeToString`
always sets it: its flags include `FlagsIgnoreErrors` (`printer.go:202`),
which contains `FlagsAllowNodeModulesRelativePaths`
(`nodebuilder/types.go:61`). So no printed type ever swaps; only
declaration emit does, and r5-modules §6's tracker (`symbol_access.rs`
`inferred_type_reports`) already asks the swapped mode itself. The swap was
inert on the committed producers, so commit 2's measurement stands, but the
import-call producer diff (§4) exposed it:
`nodeModulesImportAttributesTypeModeDeclarationEmitErrors` printed
`typeof import("pkg", { with: { "resolution-mode": "import" } })` where
native prints `typeof import("./node_modules/pkg/import")`. Commit 3 removes
the swap.

Rejected: writing the attribute in each producer. Native has one place for
it, and the producers already call `module_specifier_for_symbol`.

Measured, unfiltered against the frozen baseline: **types +21 lines**
(21 WRONG → RIGHT, zero losses; diagnostics unchanged):
`nodeModulesDeclarationEmitDynamicImportWithPackageExports` 6 lines in each
of node18/node20/nodenext (`Promise<typeof import("package/mjs", { with:
{ "resolution-mode": "import" } })>`), and `esmModuleExports1`,
`esmModuleExports2(esmoduleinterop=true)`, `esmModuleExports3` one each.
Perf: median child CPU (21 samples) domain-model 0.994, generic-imports
1.014; Ir 342,875,206 (−0.006%) and 1,195,792,683 (−0.028%) against the
baseline's 342,896,129 and 1,196,126,364.

## 4. Producer diffs (not committed; files owned elsewhere)

Each diff applies on this lane's head and routes one producer through
`module_specifier_for_symbol`, so the printed text is native's
`symbolToTypeNode` argument instead of a name- or written-text spelling.

- `r5-modules2-symbol-chain-specifier.diff` (`checker.rs` `symbol_chain`,
  main's `tsr-2zk.39`): the flat file-module arm spells
  `module_specifier_for_symbol(parent, reference)` instead of `./{stem}`.
  The arm's gates (`imported_here`, `same_file`, the flat-directory slice)
  are unchanged; only the text changes.
- `r5-modules2-export-equals-class-specifier.diff` (`printing.rs:147`,
  r5-shapes): the `typeof import(…)` of a class that is its file's
  `export =`.
- `r5-modules2-import-call-specifier.diff` (`calls.rs`
  `check_import_call_expression`, main): the `typeof import(…)` namespace
  mint of an `import()` call spells the computed argument at the call
  site, falling back to the written text where none is computed. The mint
  stays keyed by its text, so it is still one type per spelling.

Measured, all three applied on commit 3, unfiltered: **types +59 lines**
(59 WRONG → RIGHT), diagnostics unchanged, **zero losses** on either dump
against the frozen baseline. By diff (each touches disjoint cases):

- symbol chain, 26: `inlineJsxFactoryDeclarationsLocalTypes` 7,
  `jsDeclarationsWithDefaultAsNamespaceLikeMerge` 5,
  `declarationEmitTransitiveImportOfHtmlDeclarationItem` 4 (the
  `foo.d.html.ts` → `./foo.html` remap), `inlineJsxFactoryLocalTypeGlobalFallback`
  4, `constEnumNoPreserveDeclarationReexport` 2,
  `inferrenceInfiniteLoopWithSubtyping` 2, `mergeSymbolReexportInterface` 1,
  `mergeSymbolReexportedTypeAliasInstantiation` 1;
- `export =` class, 13: `jsDeclarationsExportAssignedClassExpressionAnonymous(target=es2015)`
  5 (`typeof import(".")`), `multiImportExport` 5, `umd9` 2, `umd8` 1;
- `import()` call, 20: `nodeModulesImportAttributesTypeModeDeclarationEmitErrors`
  3 × 4 configurations, `nodeModulesDeclarationEmitDynamicImportWithPackageExports`
  2 × 3, `parseAssertEntriesError` 1, `parseImportAttributesError` 1.

Measured and rejected: **`declared.rs` `get_type_from_import_type_node`**
returning the member's declared type (native `resolveImportSymbolType`)
instead of the written-text mint. It is the faithful shape, and it fixes
`nodeModulesImportTypeModeDeclarationEmit1` and friends, but the symbol
chain does not qualify a module in a nested directory (the flat-directory
gate above), so `import("./inner").SomeType` written in
`node_modules/some-dep/dist/index.d.ts` printed bare `SomeType`: 2 RIGHT →
WRONG in `declarationEmitUsingTypeAlias1` alone on the target set. It waits
for `symbol_chain`'s file-module arm to cover nested directories (main,
`tsr-2zk.39`); with that, the same one-line change should convert the
written-text rows of `nodeModules{,ImportAttributes}TypeModeDeclarationEmit*`
(3 lines × 4 configurations each) and `declarationEmitUsingTypeAlias1`.

## 5. `tsr-2zk.1060`: JSON files and `import()` of an `export =` module

### 5.1 JSON files are JavaScript files (committed)

tsgo's parser gives a `ScriptKindJSON` file `NodeFlagsJavaScriptFile |
NodeFlagsJsonFile` (`internal/parser/parser.go:306`), so `ast.IsInJSFile` is
true inside a JSON file. This port's two parse sites (`loader.rs`'s
`load_task` and `Program::in_arena` in `lib.rs`) stamped
`JAVASCRIPT_FILE` from the `.js`-family extensions only; the JSON parser
(`tsr-parser/src/json.rs`) stamps `JSON_FILE` itself. Both sites now add
`JAVASCRIPT_FILE` to a root the JSON parser stamped, which is the
`ScriptKindJSON` test rather than a second reading of the extension.
`tests/module_host.rs` pins both sites.

Measured: **inert** on both dumps (unfiltered, against commit 3), as
r5-modexports §5 predicted: a JSON module binds `export =`, so
`canHaveSyntheticDefault`'s TypeScript and JavaScript arms agree, and no
corpus JSON file has an `"__esModule"` key. Landed anyway: it is the
upstream flag, and the falsifier is a JSON file with `"__esModule"`.
Perf: median child CPU (21 samples) domain-model 1.027, generic-imports
1.005; Ir 342,875,228 / 1,195,112,994 (−0.006% / −0.085% against the
baseline). One flag read per parsed file.

### 5.2 `import()` of an `export =` module (diff)

`checkImportCallExpression` (`checker.go:8305`-`:8310`) types the call as
`Promise<getTypeWithSyntheticDefaultImportType(getTypeOfSymbol(esModuleSymbol), …)>`
with `esModuleSymbol = resolveExternalModuleSymbol(moduleSymbol)`. For an
`export =` module that is the *target's* type. The port's `import()` mint
(`calls.rs`) stands for the module symbol's own type, so r5-modexports §3
skipped `getTypeWithSyntheticDefaultImportType` for such modules.

`r5-modules2-import-call-export-equals.diff` adds
`Checker::import_call_module_type` (`module_exports.rs`): the mint for a
module without `export =`, else `get_type_of_symbol(es_module)`, then the
synthetic-default import type. The `calls.rs` call site replaces the
decline with it. Both halves ship as one diff because the function's only
caller is in `calls.rs` (main's); committed alone it would be dead code.

Measured on commit 4 (unfiltered): **types +19 lines** (18 WRONG → RIGHT,
1 GAP → RIGHT), **diagnostics +1 case** (`esModuleInteropImportCall`
EMPTY_WRONG → EMPTY_RIGHT), zero losses. Converted:
`esModuleInteropImportCall` 8, `modulePreserve4` 7,
`importCallExpressionInExportEqualsCJS` 3, `errorForConflictingExportEqualsValue` 1.
It applies independently of `r5-modules2-import-call-specifier.diff`
(different hunks of the same function).
