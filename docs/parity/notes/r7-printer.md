# r7-printer: symbol chains, accessibility and module specifiers (`tsr-2zk.1272`)

Lane r7-printer (epic `tsr-2zk`), round 7 (`docs/parity/round7.md`). Owned:
`checker.rs` (symbol chain, best name, qualified names), `symbol_access.rs`,
`symbol_accessibility.rs`, `module_specifiers.rs`, `printing.rs`,
`node_reuse.rs`. Vendor pinned at `5b1047d`.

## 0. Base and method

- **Frozen base: `87146adf`** (main after batch 1, r7-flow and r7-shared).
  Dumps, both unfiltered: types 556,357 lines, 551,197 RIGHT / 673 GAP /
  4,487 WRONG. The box first froze at dispatch (`9020aa67`, 551,176 RIGHT);
  §1 measured the same delta on both.
- Gates are box-protocol §5: `scripts/parity_gate.sh compare` against the
  frozen base (types and diagnostics, unfiltered), the coverage bin,
  `cargo test --workspace --release`, clippy, fmt, and the 21-sample
  child-CPU comparison against the frozen binary.
- **Native `.types` oracle.** The pinned compiler test runner is built once
  (`go test -c ./internal/testrunner` in `vendor/typescript-go`); a probe is
  copied to `testdata/tests/cases/compiler/`, run with
  `-test.run TestLocal/<probe>.ts`, and its `testdata/baselines/local`
  output read, then both are deleted (r5-printer3 §1). Every native line
  quoted below was read that way. Where a native *mechanism* is claimed
  (which cache answered), it was read from a temporarily instrumented runner
  (a `fmt.Fprintf` in the Go function named, reverted after the run).
- r6-triage's cluster lists (`r6-triage-issues.json`, cut at `e6eadf4`) were
  re-measured on the base: of the queue's ten clusters, EXPORT-SPECIFIER-
  NOT-IN-SCOPE (23/23 cases still non-RIGHT), SPECIFIER-FOR-MODULE-SYMBOL
  (28/30), LOCAL-IMPORT-EQUALS-ALIAS (18/19), NEEDS-QUALIFICATION (16/16),
  CANDIDATE-EXPORTS-OF-SYMBOL (10/10), TYPE-ALIAS-ACCESSIBILITY-GATE (9/11),
  ALIAS-OVER-ADMITTED (5/5), UMD-ALIAS-EXCLUSION (4/6), NESTED-QUALIFICATION
  (4/5) and DEFAULT-IMPORT-ALIAS (3/11).

## 1. A file-module container is named by the resolver's `getSymbolChain`

### 1.1 Forcing constraint

`es6ExportClauseWithoutModuleSpecifier`, `exportsAndImports{1,3}` and 21
more cases (r6-triage row 6, 23 cases):

```ts
// server.ts
export class c {}
// client.ts
export { c } from "./server";
>c : typeof import("./server").c     // native
>c : typeof c                         // port
```

`symbol_chain`'s file-module arm (checker.rs) spelled `import("…")` only
when the reference's file did not mention the module's specifier (the
`imported_here` gate, §106: "our resolver just cannot walk every re-export
form yet") and the specifier had no `/`. Native has no such test.
`symbolToTypeNode` (`nodebuilderimpl.go:644`) asks
`getSymbolChain(symbol, meaning, endOfChain = true, yieldModuleSymbol = true)`
(`:1087`). An export specifier is not a local name
(`trySymbolTable`, `symbolaccessibility.go:573`), so `c` has no accessible
chain in `client.ts`, its container `server.ts` has none either, and the
chain is `[server, c]`: the module root prints as an import type.

The faithful walk has existed since r5-declemit3 and r6-printer4:
`DeclarationEmitResolver::symbol_chain_at` (`symbol_accessibility.rs`),
with `getContainersOfSymbol`, `getWithAlternativeContainers` and
`sortByBestName`.

### 1.2 The port

- `Checker::module_rooted_chain_at` (checker.rs) asks the resolver's
  `symbol_chain_at` and answers `(module, leaf)` when the chain is a file
  module followed by one symbol. `symbol_chain`'s file-module arm spells
  `import(<specifier of that module>).` when the leaf has the symbol's name.
  The root may be a **re-exporting** module the walk prefers
  (`symlinkedWorkspaceDependencies*`, `declarationEmitReexportedSymlinkReference*`,
  `typesVersionsDeclarationEmit.multiFileBackReferenceToUnmapped`), which the
  old arm could not express. The `node_modules` arm's
  `ReportLikelyUnsafeImportRequiredError` (`track_unsafe_import`) now
  follows the chain's root, as native's `chain[0]`.
- `getAlternativeContainingModules`' program-wide fallback
  (`extendedContainers`, `symbolaccessibility.go:217`) is ported in
  `symbol_accessibility.rs`. The checker holds no program file list; the
  node table's `SourceFile` rows are the program's files in program order,
  the order `compare_symbols_key` already relies on. It is cached on the checker
  (`AccessibilityCache::extended_containers`, `program_external_modules`;
  §1.4).

**Why the fallback is part of this item.** Without it the first build
measured +92 / −1 types and **two diagnostic losses**:
`declarationEmitReexportedSymlinkReference{,2}` went EMPTY_RIGHT →
EMPTY_WRONG with a TS2883 native does not report (the outcome r6-specifiers
§3.3 predicted). In `keys.ts`, `import {MetadataAccessor} from
"@raymondfeng/pkg2"` resolves from the import line, but from the printed
expression `resolveExternalModule` reads the file's *default* resolution
mode (`checker.go:15200`). Under `module: commonjs` with the default bundler
resolution that is `None`, while the import was resolved and stored under
its usage mode, `CommonJS` (`getModeForUsageLocation` →
`getEmitSyntaxForUsageLocationWorker`). So the import misses, natively as
in the port, and native falls back to every program module, among them
`@raymondfeng/pkg2`'s index, which `sortByBestName` puts first. The port had
only the declaring `pkg1/dist/types`, reached through `node_modules`, and
reported TS2883.

### 1.3 The same-file arm stays

From the module's own file the old flat arm is kept (the specifier unless
the exported name did not resolve). The resolver's walk measured one loss
there: `importedEnumMemberMergedWithExportedAliasIsError` 1:4, where native
prints `EnumA : import("./alias").EnumA`. `alias.ts` writes
`import EnumA = Enum.A` and `export type EnumA = …`. Native merges the
import alias with the export's local (TS2395); `needsQualification` resolves
that local through `resolveAlias` to the enum member `Enum.A` (type meaning),
so the name needs a qualifier. The port's `resolve_alias` answers `None` for
that alias (the same defect that types the import's own line `any`, 1:1), so
the resolver finds no shadow and answers the bare name. That is
`symbols.rs`'s `get_target_of_alias_symbol` (r7-shared), routed. When it
resolves, the same-file arm can go to the resolver too.

### 1.4 The accessibility caches live as long as the checker

Native keeps `extendedContainersByFile` and `extendedContainers` on the
symbol's links for the checker's lifetime
(`getAlternativeContainingModules`, `symbolaccessibility.go:170`). The
first query in a file whose location resolves one of the file's imports
(a type printed *on the import line*, where `resolveExternalModule` reads
the import's own specifier) caches that answer, and every later query in
the file reuses it, although from those later locations the import misses.
Read on an instrumented runner, the second case of
`tests/module_rooted_symbol_chain.rs`:

```ts
// other.ts
import { make } from "./lib/impl/a";   // >make : () => import("./lib/impl/a").I   (resolves, cached)
export const v = make();               // >v : import("./lib/impl/a").I   (from the cache)
// c.ts
import { v } from "./other";           // resolves "./other", no alias for I: not cached
export const w = v;                    // >w : import("./lib").I   (extendedContainers)
```

**First build, refused by the measurement.** The resolver's
`AccessibilityCache` lived for one question. The corpus measured the same
+123 / −0, but the probe printed `v : import("./lib").I` (each question
asked again from its own location), and domain-model's child CPU measured
**1.096 at 21 samples and 1.108 at 41** against the frozen binary. The
site renderer reaches the arm 40 times while checking domain-model (one
`ModelNNN` class each), every one misses its imports from the printed
location, and each rebuilt the program's module list (0.4 ms: a node-table
scan of 202,052 rows) and every module's `getExportsOfSymbol`.

**Built:** the cache moved to the checker
(`Checker::accessibility_links`, one field beside the module-specifier
tracker). `DeclarationEmitResolver::new` takes it and its `Drop` returns it,
so every resolver (the printer's, `alias_accessibility.rs`'s, declaration
emit's) shares native's lifetime. `getExportsOfSymbol` is memoized there too
(`resolved_exports`, native's `resolvedExports` link). The probe now matches
native line for line, the corpus is unchanged (+123 / −0), and domain-model
measures 1.029, then 1.020 with the exports memo (21 samples; the
identical-binary band is about ±2%).

**Rejected: no cache, and a narrower one.** The per-question cache cost the
1.10 above and is not native's answer for the probe. Caching only the
location-independent pieces (the module list and `extendedContainers`) would
fix the cost and leave the probe wrong. What would make the per-question
form win: a printer that never asks the resolver during checking.
### 1.5 Checker port convention

- *Native operation:* `getSymbolChain` (`nodebuilderimpl.go:1087`) with
  `getContainersOfSymbol`/`getWithAlternativeContainers`/
  `getAlternativeContainingModules` (`symbolaccessibility.go:117`–`:217`).
- *Key identity and owner:* `AccessibilityCache`, owned by the checker
  (`accessibility_links`) and lent to each `DeclarationEmitResolver` for its
  life. `containing_modules` is keyed (symbol, enclosing file) and written
  only when the file's imports found a container; `extended_containers` by
  symbol; `program_external_modules` is the program's module list, built on
  the first fallback; `resolved_exports` by symbol.
- *Publication states:* absent or final, as native's links. Lifetime and
  return path: `DeclarationEmitResolver::new` takes the checker's cache with
  `mem::take`, and the resolver's `Drop` puts it back, so the cache is never
  shared mutably and every resolver sees what earlier ones published. A
  resolver nested inside another (built from the checker the outer one
  holds) borrows an empty cache; when the outer one drops last its cache
  overwrites the inner one's, so the inner one's additions are dropped and
  nothing is corrupted.
- *Receiver/alias context:* the reference node only.
- *Expensive-work boundary:* the walk runs only where `symbol_chain` has
  already found that the bare name needs qualification, the parent is a
  file module and no in-scope alias names it. The fallback scans the node
  table's kind column once per checker and asks
  `getAliasForSymbolInContainer` of each external module once per symbol.

### 1.6 Measured

Both dumps unfiltered (the first two rows against `9020aa67`, the last
against `87146adf`):

| | types | diagnostics |
|---|---|---|
| resolver arm alone (first build) | +92 / −1 | +1 / −2 |
| + fallback, same-file arm kept | +123 / −0 | +1 / −0 |
| + checker-lifetime caches (this commit) | **+123 / −0** (551,320 RIGHT) | **+1 / −0** |

Coverage bin against main's snapshots: `checker_types` 8,680 → 8,706,
`checker_types_configured` 1,753 → 1,759, `diagnostics` 4,791 → 4,792,
`diagnostics_configured` 955 unchanged. Median child CPU new/old against the
frozen `87146adf` binary, 21 samples: domain-model 1.005, generic-imports
0.991; diagnostics match.

Converted cases: the export-specifier cluster
(`es6ExportClauseWithoutModuleSpecifier{,InEs5}`, `exportsAndImports3{,-es6}`,
`constEnum{No,Preserve}EmitReexport`, `reexportWrittenCorrectlyInDeclaration`,
`isolatedModulesReExportType`, `moduleSameValueDuplicateExportedBindings2`,
`importAssertion3`/`importAttributes3` ×2 each,
`jsDeclarationsExportSpecifierNonlocal`, `exportNamespace{1,3,5}`,
`exportDeclaration_moduleSpecifier`, `declarationEmitForTypesWhichNeedImportTypes`,
`declarationEmitReadonlyComputedProperty`,
`declarationEmitStringEnumUsedInNonlocalSpread`, `declarationEmitSymlinkPaths`),
the re-exporting-container cases r6-specifiers §5 routed to `.39`
(`declarationEmitReexportedSymlinkReference{,2,3}`,
`declarationEmitForGlobalishSpecifierSymlink{,2}`,
`symbolLinkDeclarationEmitModuleNames{,RootDir}`,
`typesVersionsDeclarationEmit.multiFileBackReferenceToUnmapped`,
`reactTransitiveImportHasValidDeclaration`), the four
`symlinkedWorkspaceDependenciesNoDirectLink*` cases r6-specifiers §3.3 held,
and `declarationEmitPartialNodeReuseType{Of,References}`,
`declarationEmitInlinedDistributiveConditional`,
`moduleAugmentationImportsAndExports6`. Diagnostics:
`declarationEmitReexportedSymlinkReference3` (TS2883).

### 1.7 A unit test's expectation corrected to native (integrator-approved)

`signatures.rs`' `parameter_source_views_qualify_bound_names_and_reject_unloaded_imports_without_work`
(r7-shared's file) expected the hosted `unknownRouteView`, `defaultRouteView`
and `classRouteView` controls to print `(value: number) => number`. The
pinned native test runner, on the test's own sources (`entry.ts`,
`unknown.ts`, `default.ts`, `class.ts`), prints at all three:

```
>unknownRouteView : (value: import("./entry").ModuleInputs<typeof import("./entry").peer>[0]) => number
>defaultRouteView : (value: import("./entry").ModuleInputs<typeof import("./entry").peer>[0]) => number
>classRouteView : (value: import("./entry").ModuleInputs<typeof import("./entry").peer>[0]) => number
```

and the port now prints exactly that. The source-view half of the test
(`signature_parameter_source_text_at` declines these routes) is unchanged.
Only the whole-print expectation of those three hosted rows moved; the
integrator approved the edit in this commit. The hostless rows are
unchanged: a checker built without a module host names no module from
another file (§1.2's arm asks for a host).

## 2. A module clone hides its module from every alias, whatever the target

Routed from r7-shared (`r7-shared.md` §1, its
`r7-shared-printer-module-clone.diff`, applied unchanged).
`alias_targets_module_clone` (checker.rs, the `module_alias_at` /
`best_name` / `alias_in_scope_for` exclusion that stands in for
`trySymbolTable`'s `resolveAlias(alias) == symbol` test against a clone
symbol) admitted only clones of class or function targets.
`resolveESModuleSymbol` (`checker.go:15568`) clones in every arm
(signatures, a `default` property, an ESM-to-CommonJS reference,
`:15609-15618`), so an alias of any target that resolves to a clone is not
the module's name: in `nodeModules1`, `typeof m26` (an `import m26 =
require`) where the port printed `typeof m4` (an `import * as m4` of a
CommonJS file, a clone). No cache or table: the test reads the existing
`module_value_clones` record of the alias's value.

Measured against §1's commit, both dumps unfiltered: types **+196 / −0**
(`nodeModules1` and `nodeModulesAllowJs1`, 24 lines in each of four modes;
`unusedImports11` 2, `unusedImports12` 1, `importAttributes9` 1),
diagnostics unchanged. Coverage: `checker_types` 8,706 → 8,708,
`checker_types_configured` 1,759 → 1,767. CPU new/old against §1's binary,
21 samples: domain-model 1.009, generic-imports 1.005.

## 3. An augmented `export =` target is spelled through its module

Routed from r7-shared (`r7-shared.md`, MODULE-AUGMENTATION-MERGE remainder),
after its `582a2b0b` gave the target symbol the augmentation's declaration.
Base: main `660718af` (batch 4; §1 and §2 are on it), types 551,974 RIGHT.

**Forcing constraint.** `augmentExportEquals4`, native:

```ts
// file1.ts
class foo {}                 // >foo : import("./file1")
namespace foo { … }          // >foo : typeof import("./file1")
export = foo;
// file2.ts
import x = require("./file1");   // >x : typeof x
declare module "./file1" { … }
```

`symbolToTypeNode` (`nodebuilderimpl.go:651`) spells an import type whenever
`chain[0]` has a declaration satisfying
`hasNonGlobalAugmentationExternalModuleSymbol` (a string-named module
declaration or an external file). The augmentation is a declaration of
`foo`, so even in `file1.ts`, where `foo` is accessible by its own name,
the root qualifies and the name becomes `import("./file1")`. Where an alias
names it (`x`), the chain's root is the alias and nothing changes.

**The port.**

- `qualified_name_at` (checker.rs) and `export_equals_class_text_at`
  (printing.rs, the static side of an `export =` class) ask
  `module_declared_root_text_at` first for a symbol with a string-named
  module declaration. It asks the resolver's `getSymbolChain`
  (`symbol_chain_at`) and, when the root passes native's test, spells
  `import("<specifier>")` followed by the rest of the chain. The resolver's
  chain for `foo` is `[file1]` (its `export =` shortcut,
  `nodebuilderimpl.go:1124`), where native's is `[foo]`; both roots pass
  the test and spell the same specifier, so the test is native's whole
  predicate rather than "the root is the symbol".
- `module_specifier_for_symbol_in_mode` reads the file as
  `ast.GetSourceFileOfModule` does (`ast/utilities.go:3571`): a `SourceFile`
  declaration, else the value declaration's file, else the first declaration
  that is not a module augmentation. Before, only a `SourceFile` declaration
  counted, so `foo` had no specifier.
- No cache or table: the walk is the resolver's, on the checker's
  `accessibility_links` (§1.4). It runs only for a symbol carrying a
  string-named module declaration that is not itself a module.

**Measured** against `660718af`, both dumps unfiltered: types **+19 / −0**
(`augmentExportEquals3/4/6` 3 each, `augmentExportEquals5`'s `typeof e`
lines 3, `umd-augmentation-3/4` 3 each, and
`jsxNamespacedNameNotComparedToNonMatchingIndexSignature` 1), diagnostics
unchanged. Coverage: `checker_types` 8,736 → 8,742. CPU new/old against
the `660718af` binary, 21 samples: domain-model 0.981, generic-imports
0.991. Test: `tests/module_rooted_symbol_chain.rs`'s third case, lines from
the native baseline.

**Remaining in those cases:** `augmentExportEquals5` 1:1 and 2:2–2:5 read
`any` (`import { Request } from "express"` of an augmented member; alias
resolution, r7-shared), and `augmentExportEquals7` 1:0 wants `{ default:
() => void; }` for a namespace import of an `export =` function (the clone's
synthetic `default`, r7-shared §1's remaining).

## 4. A merged `default` symbol is written by its first named declaration

Routed from r7-shared (DEFAULT-EXPORT-ALIAS-CLASS-MERGE, the printer half).

**Forcing constraint.** `exportDefault{Class,Interface,Type}ClassAndValue`:

```ts
const foo = 1
export default foo
export default class Foo {}     // >Foo : foo   (native)   >Foo : Foo   (port)
```

Both statements declare the file's `default` export, which merges into one
symbol whose first declaration is the export assignment (the binder matches
native: the class node's symbol is that export symbol). Native names the
class's instance type with `getNameOfSymbolAsWritten`
(`nodebuilderimpl.go:973`): a symbol named `default`, written as the first
segment of an entity name inside its first declaration's binding context
(`isDefaultBindingContext`: the file or an ambient module), takes the name
of its first named declaration (`ast.GetNameOfDeclaration`, whose
export-assignment arm is the identifier `foo`). The port baked the class's
own name and `qualified_name_at` split the print on the binder name
`default`, so nothing re-spelled it.

**The port.** `default_symbol_text_as_written` (checker.rs) applies that arm
in `qualified_name_at`: in the same binding context, it replaces the baked
name of another declaration of the symbol with the first declaration's
name. Outside the binding context native writes `default` (the chain then
roots at the module); the existing roads keep deciding there, unchanged. No
cache or table.

**Measured** against §3's commit (`eacaa674`), both dumps unfiltered: types
**+3 / −0** (the three cases), diagnostics unchanged. Coverage:
`checker_types` 8,742 → 8,745. CPU new/old, 21 samples: domain-model 0.989,
generic-imports 0.990. Test: `tests/default_symbol_written_name.rs`.

## 5. A written qualifier is re-spelled by the accessible chain (LOCAL-IMPORT-EQUALS-ALIAS / NEEDS-QUALIFICATION, part)

**Forcing constraint.** `aliasBug`:

```ts
namespace foo { export class Provide {} }
import provide = foo;
function use() {
  var p1: provide.Provide;    // >p1 : provide.Provide
  var p2: foo.Provide;        // >p2 : provide.Provide   (native)   foo.Provide (port)
}
```

The `.types` writer prints a variable's *type* (`typeToString`), not its
annotation, so native names `Provide` by `symbolToTypeNode` →
`getSymbolChain`: `getAccessibleSymbolChain` meets the alias `provide` in
the file's locals before it would walk to `Provide`'s parent, and
`trySymbolTable`'s `getCandidateListForSymbol` gives `[provide, Provide]`.
The port bakes a reference type's text from the written entity name
(`checker-notes-qualname.md`'s design W, `declared.rs`), and
`qualified_name_at`'s gate (`split_around_name`) only acts on a bare name,
so a baked `foo.Provide` (or `B.A`, read through
`aliasOnMergedModuleInterface`'s `import foo = require("foo")`) printed
as written.

**The port.** `qualified_name_at` recognizes a print that is a dotted entity
name ending in the type's own symbol name (`[typeof ]Q1.….name[<args>]`,
`split_around_qualified_name`) and re-spells the whole path from the
resolver's chain (`symbol_chain_text_at`: `symbol_chain_at` with
`endOfChain` and `yieldModuleSymbol`, an import type when the root is an
external module, else the chain's names). A bare name keeps the existing
roads (`best_name`, `symbol_chain`); only a written qualifier is replaced.
No cache or table: the walk is the resolver's, on `accessibility_links`.

**A prerequisite in the resolver: `ast.IsExternalOrCommonJSModule`.** The
first build measured +38 / **−6**: `varRequireFromJavascript` and
`varRequireFromTypescript` printed `import("./ex").Crunch` where native
keeps `ex.Crunch`. `use.js` declares `var ex = require('./ex')`, an alias
(the port's binder flags it `ALIAS`, and it resolves to `ex.js`), but the
file has no `import`/`export`, and the resolver's `scope_tables` skipped its
locals as a global script's. Native's test is `ast.IsGlobalSourceFile`, i.e.
not `IsExternalOrCommonJSModule`, and a file that `require`s is a CommonJS
module. `hasNonGlobalAugmentationExternalModuleSymbol` and
`hasExternalModuleSymbol` take the same file test. All three now ask
`Checker::is_external_or_common_js_module` (the binder gives exactly those
files a symbol). Measured alone against §4's commit: +0 / −0 on both dumps;
it is in this commit because §5 exposes it.

**Measured** against §4's commit (`668b4795`), both dumps unfiltered: types
**+38 / −0**, diagnostics unchanged. Converted lines: `aliasBug`,
`aliasErrors`, `aliasOnMergedModuleInterface`,
`declarationEmitUnnessesaryTypeReferenceNotAdded`, `exportEqualErrorType`,
`exportEqualMemberMissing` (1 each), `ModuleWithExportedAndNonExportedImportAlias` 4,
`visibilityOfCrossModuleTypeUsage` 4,
`conflictingDeclarationsImportFromNamespace{1,2}` 4 each, `dynamicNames` 3,
`module_augmentUninstantiatedModule2` 3,
`inlineJsxFactoryDeclarationsLocalTypes` 3, `constEnums` 2,
`emitDecoratorMetadata_isolatedModules` ×2, `exportImportNonInstantiatedModule`,
`moduleAugmentationDuringSyntheticDefaultCheck`, `umd8` (1 each). Coverage:
`checker_types` 8,745 → 8,755, `checker_types_configured` 1,771 → 1,774.
CPU new/old, 21 samples: domain-model 0.991, generic-imports 0.995. Tests:
`tests/module_rooted_symbol_chain.rs`' fourth and fifth cases.
