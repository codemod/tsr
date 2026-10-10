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

## 6. The file-module arm asks only the resolver (r7-printer §1.3 retired)

r7-shared's `f11e37b8` resolves `import EnumA = Enum.A` beside
`export type EnumA` (`importedEnumMemberMergedWithExportedAliasIsError`),
which was §1.3's reason to keep the flat same-file arm. Measured on the
batch-6 base against §5's commit, both dumps unfiltered, each step
**+0 / −0** on types and diagnostics:

1. the same-file case goes to the resolver, keeping the `unresolved_export`
   decline;
2. the `unresolved_export` decline goes too (its history: §106's "chain1"
   217 losses, measured before the resolver existed);
3. the stem/directory computation and the `same_file`/`held_by_exports`
   tests, now read by nothing, are deleted.

So `symbol_chain`'s file-module arm is native's question alone: the
resolver's `getSymbolChain` rooted at a file module spells
`import(<specifier>)`. A refusal (a bare name where native qualifies) can
now only come from the resolver's walk or from `getSpecifierForModuleSymbol`,
not from a port-only gate.

## 7. `getTypeNameForErrorDisplay` for r7-reports (TS2719)

Requested by r7-reports through the integrator: `reportRelationError`'s
same-name arm (`incompatibleAssignmentOfIdenticallyNamedTypes`, TS2719)
prints both types with `getTypeNameForErrorDisplay` (`relater.go:1297`),
which is `typeToStringEx(t, nil, TypeFormatFlagsUseFullyQualifiedType)`.

**What native does with no enclosing declaration.** `lookupSymbolChainWorker`
(`nodebuilderimpl.go:1070`) still builds a chain under
`UseFullyQualifiedType`, except for a type parameter (printed bare).
`someSymbolTableInScope` then visits only `c.globals`, so a name is
accessible only as a global, and otherwise `getSymbolChain` qualifies it by
its containers up to a global or a module. A module is spelled by
`getSpecifierForModuleSymbol` with no `enclosingFile`
(`nodebuilderimpl.go:1265`): its ambient name, or its file's name.

**The API.** `Checker::type_name_for_error_display(TypeId) -> String`
(checker.rs): the site-free print with the type's own name (a reference's
target, a class or interface, an alias-named composite, an enum) replaced
by that chain (`fully_qualified_symbol_text`). Reduced: type arguments and
members keep their site-free print, where native qualifies every nested
reference too. No cache. It has no caller until r7-reports' TS2719 arm
lands (`allow(dead_code)` names that consumer); the unit test
`printing::tests::a_name_for_error_display_is_fully_qualified` pins
`M.N.C` for a class in nested namespaces and `T` for a type parameter.

Measured: no corpus line reaches it (both dumps unchanged against §6's
commit).

## 8. An array literal's tuple image prints at the site

**Forcing constraint.** `declFileTypeAnnotationTupleType`:
`var k: [c, m.c] = [new c(), new m.c()]` prints the literal's type
`[c, m.c]` natively; the port printed `[c, c]`, although `new m.c()` alone
printed `m.c`.

**Cause.** `checkArrayLiteral` returns `createArrayLiteralType` of the
contextual tuple (`checker.go:8103`), a separate reference image flagged
`ArrayLiteral` (`widening.rs`, `array_literal_bases`). `tuple_text_at`
(printing.rs) admits only a tuple `create_tuple_type` minted (its
`tuple_types` canonical entry), so the image fell back to its minted text,
whose elements were printed inside view. Native's `typeReferenceToTypeNode`
never reads the `ArrayLiteral` flag: the image prints exactly as its base.

**The port.** The canonical test compares against the image's base
(`array_literal_bases`) when there is one. The image carries the base's
labels, optional mask, rest tail and variadic records (copied when it is
minted), so the other declines still read the same facts. No cache or table.

**Measured** against §7's commit (`deda2594`), both dumps unfiltered: types
**+6 / −0** (`declFileTypeAnnotationTupleType` 0:7, 0:18;
`jsxChildrenSingleChildConfusableWithMultipleChildrenNoError` 0:15, 0:16,
0:20; `declarationEmitTypeParameterNameShadowedInternally` 0:7),
diagnostics unchanged. Coverage with §5–§8 on `92fe8f05`: `checker_types`
8,782 → 8,796, `checker_types_configured` 1,784 → 1,788. CPU new/old,
21 samples: domain-model 1.002, generic-imports 1.023. Test:
`tests/array_literal_tuple_site.rs`.

## 9. A module object is named by `getSymbolChain` (SYMBOL-CHAIN-CANDIDATE-EXPORTS-OF-SYMBOL)

**Forcing constraint.** `exportAsNamespace{1,2,3}` (three module modes
each): in `2.ts`, `import * as foo from './1'` where `1.ts` writes
`export * as ns from './0'`; `foo.ns`'s type, module `0`'s namespace
object, prints `typeof foo.ns` natively and `typeof import("./0")` in the
port.

**Cause.** The module-object arm of `type_to_string_at_worker` (checker.rs)
asked `module_name_at`, which finds only an in-scope alias that names the
module directly, and otherwise spelled the import form. Native's
`getAccessibleSymbolChain` reaches the module through `trySymbolTable`'s
`getCandidateListForSymbol`: the alias `foo` resolves to module `1`, whose
exports hold the namespace re-export `ns` (a local-name lookup excludes it,
an exports lookup does not, `symbolaccessibility.go:571`), so the chain is
`[foo, ns]`.

**The port.** Where no direct alias names the module, the arm asks
`symbol_chain_text_at` (§5) for the module and spells a chain that is not
rooted at a module. A module-rooted chain keeps the existing import form.

**A spelling the chain text does not port.** The first build measured
+36 / **−10** (`privacyImportParseErrors`, `privacyGloImportParseErrors`,
`ambientExternalModuleInsideNonAmbient`): an ambient `declare module "abc"`
nested in an ambient namespace `m2` printed `typeof m2."abc"` where native
prints `typeof import("abc")`. `createAccessFromSymbolChain` writes a
member whose name is no identifier as an indexed access, never as a dotted
segment, and `symbol_chain_text_at` does not port that arm, so it now
declines such a chain (`objects::is_identifier_text`, the ASCII subset of
`scanner.IsIdentifierText`) and the existing roads print the line as
before. Why native's chain for those modules is `["abc"]` rather than
`[m2, "abc"]` was not investigated; the decline only stops the port from
inventing a spelling native never writes.

**Measured** against §8's commit (`bc18f861`), both dumps unfiltered: types
**+36 / −0** (`exportAsNamespace{1,2,3}` ×3 modes, 4 lines each),
diagnostics unchanged. Coverage: `checker_types` 8,799 → 8,801,
`checker_types_configured` 1,787 → 1,797. CPU new/old, 21 samples:
domain-model 1.002, generic-imports 0.981. jsTyping error lines against
pinned tsgo: none added, none lost. Test: `tests/module_rooted_symbol_chain.rs`'
sixth case. Still open in the cluster: `nodeColonModuleResolution` (`typeof
ph.constants` for a member of an ambient module reached through an
import), which takes the namespace-member road, not this arm.

## 10. A baked import type is re-spelled from the chain's module root

**Forcing constraint.** `nodeModulesImportAttributesTypeModeDeclarationEmit{,Errors}`
and `nodeModulesImportTypeModeDeclarationEmit1` (four module modes each):
in a CommonJS `index.ts`,
`import("pkg", { with: {"resolution-mode": "import"} }).ImportInterface`
types `b`, and native prints `b : import("./node_modules/pkg/import").ImportInterface`.
The port printed `import("pkg").ImportInterface`.

**Cause.** The port bakes an import type node's text as written (without
its attributes). `qualified_name_at` re-spells only a bare or dotted
identifier path (§5), so the baked `import("pkg")` root was printed as is.
Native names the interface through `symbolToTypeNode`: the chain is
`[import.d.ts, ImportInterface]`, and `getSpecifierForModuleSymbol` asks
`GetModuleSpecifiers` under the importing file's default mode (CommonJS;
the `.types` writer's `FlagsIgnoreErrors` carries
`AllowNodeModulesRelativePaths`, so no mode swap is tried).
`tryDirectoryWithPackageJson` finds `pkg`'s `exports` and, under the
`require` conditions, no entry for `import.d.ts`: blocked, so the specifier
is the relative path through `node_modules`. The port's
`import_type_argument` already computed exactly that; it was never asked.

**The port.** `split_around_qualified_name` also accepts a baked
`import("…")` root followed by `.`-separated identifiers ending in the
type's name, so §5's re-spell replaces the whole path from
`symbol_chain_text_at`.

**A prerequisite in `module_specifiers.rs`: case-folding `HasPrefix`.** The
first build measured +44 / **−3** (`symbolLinkDeclarationEmitModuleNamesImportRef`
0:0–0:2, `useCaseSensitiveFileNames: false`): the re-spell printed
`import("../../../folder/node_modules/styled-components/typings/styled-components")`
where native keeps `import("styled-components")`. The loader spells the
realpath'd module `/.src/folder/…` while the importing file is
`/.src/Folder/…`, and `node_module_specifier` compared the
top-level `node_modules` prefix with a case-sensitive `starts_with`.
Native's `tryGetModuleNameAsNodeModule` uses
`stringutil.HasPrefix(info.SourceDirectory, pathToTopLevelNodeModules,
caseSensitive)`, which folds case on a case-insensitive host. The port now
reads the host's `compare_paths_options` and folds ASCII case there. (Why
the loader lower-cases the realpath'd name was not investigated; native's
file names keep the file system's spelling. Routed to the loader's owner.)

**Measured** against §9's commit (`fd274090`), both dumps unfiltered: types
**+44 / −0** (the three `nodeModules*TypeModeDeclarationEmit*` cases ×4
modes, 3 lines each; `declarationEmitUsingTypeAlias1` 6;
`allowsImportingTsExtension` 2), diagnostics unchanged. jsTyping error lines
against pinned tsgo: none added, none lost. Coverage: `checker_types`
8,799 → 8,802, `checker_types_configured` 1,787 → 1,809. Callgrind Ir
(`--singleThreaded --noEmit`): domain-model 944,901,321 → 944,972,902
(+0.008%), generic-imports 223,701,625 → 223,696,421 (−0.002%). Child-CPU
new/old measured 1.044 (21 samples) and 1.038 (41) on domain-model while
the host's absolute times were twice their earlier values (load ≈ 3.5 on
four cores); the flat Ir says the work did not change. Test:
`tests/module_rooted_symbol_chain.rs`' seventh case.
