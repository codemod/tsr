# r6-specifiers: module specifiers through symlinks, and TS2883 (tsr-2zk.999, .1098, .16.59, .16.77)

Round-6 box. Owned file: `crates/tsr-checker/src/module_specifiers.rs`.
Everything else ships as a measured diff in this directory. Base frozen at
`23e1689` (tip of `claude/beautiful-shannon-ar5gh0`); tsgo `5b1047d`.

## 1. Baseline

Measured on the frozen base, unfiltered: types 549,976 RIGHT / 827 GAP /
5,500 WRONG of 556,303; diagnostics 5,579 RIGHT + 5,603 EMPTY_RIGHT of
12,238.

The four items' 36 named cases, re-measured (non-RIGHT type lines, then
the case's diagnostics verdict):

| Group | Cases | Lines | What prints |
|---|---|---|---|
| symlinked workspace packages (`symlinkedWorkspaceDependenciesNoDirectLink*`, ×4) | 4 | 18 | `Foo` where native prints `import("package-a").Foo` (`…/cls` for the deep one) |
| symlinked packages re-exported through another package (`declarationEmitReexportedSymlinkReference{,2,3}`, `declarationEmitForGlobalishSpecifierSymlink{,2}`, `symbolLinkDeclarationEmitModuleNames{,RootDir}`) | 7 | 33 | the bare name, or a specifier for the declaring file where native names a re-exporting module |
| re-exporting container (`typesVersionsDeclarationEmit.multiFileBackReferenceToUnmapped`) | 1 | 9 | `import("ext/other").A2` where native prints `import("ext").A2` |
| printer qualification inside composites (`declarationEmitCommonJsModuleReferencedType` tuple, `declarationEmitObjectAssignedDefaultExport` intersection, `legacyNodeModulesExportsSpecifierGenerationConditions` namespace member, `reactTransitiveImportHasValidDeclaration`) | 4 | 21 | the bare member name |
| `duplicatePackage` | 1 | 7 | `X` where native prints `import("a/node_modules/x").default` |
| `.16.77` leftovers not about specifiers (`inferrenceInfiniteLoopWithSubtyping`, `jsDeclarationsWithDefaultAsNamespaceLikeMerge`, `mergeSymbolReexport*`, `inlineJsxFactoryDeclarationsLocalTypes`, `umd8`) | 7 | 24 | §5 |
| already RIGHT on the base | 12 | 0 | — |

TS2883 (`declarationEmit…`: inferred type cannot be named without a
reference to a `node_modules` path): `declarationEmitCommonJsModuleReferencedType`,
`declarationEmitObjectAssignedDefaultExport`,
`declarationEmitReexportedSymlinkReference3` and
`declarationEmitUsingTypeAlias1` are WRONG.

`moduleResolutionWithSymlinks{,_withOutDir}` (r6-smallcodes5 §4 routes them
to `.1098` as "the loader does not realpath it") are **not** a symlink
defect: the loader realpaths `library-a` (one file in `--listFiles`). A
probe with no symlink at all reproduces the `any`:

```ts
// b.ts
type T = number; export { T }
// app.ts
import { T } from "./b";
let w: T;
const q: string = w;   // native: TS2322 + TS2454; TSR: nothing (w is any)
```

`export { T }` of a local, used as a type, reads `any`. That is
`export_specifier_target` / the alias's declared type (`symbols.rs`, main's),
not this lane.

## 2. `GetEachFileNameOfModule` and the program's symlink cache

### 2.1 The forcing constraint

Native names a realpath'd module by every path that reaches it.
`getAllModulePathsWorker` (`modulespecifiers/specifiers.go:198`) asks
`GetEachFileNameOfModule` (`:260`), which walks the module file's ancestor
directories and, for each one the program's symlink cache knows as the real
directory of some symlink, adds the path through that symlink.
`computeModuleSpecifiers` (`:359`) then loops over those paths: a path
through `node_modules` yields a package-name specifier
(`tryGetModuleNameAsNodeModule`), which beats every relative one.

The loader (`tsr-compiler`) loads by realpath, as native does, so a
workspace package linked into `packageC/node_modules/package-a` is the
program file `/workspace/packageA/index.d.ts`. Before this port the
specifier code saw only that path: no `node_modules`, no package name.

The cache itself (`Program.GetSymlinkCache`, `compiler/program.go:2059`)
has two halves:
1. `SetSymlinksFromResolutions`: every module and type-reference
   resolution whose `OriginalPath` differs from its `ResolvedFileName`;
   `guessDirectorySymlink` strips their shared trailing components to infer
   the directory link.
2. For the `package.json` directory of every file that may be emitted, each
   runtime dependency (`dependencies`, `peerDependencies`,
   `optionalDependencies`) not already known is resolved as a package
   directory (`ResolvePackageDirectory`) and its realpath recorded. This is
   the half the `NoDirectLink` cases need: `packageC` never imports
   `package-a`; only `packageB` does, through `packageB/node_modules`, which
   is not an ancestor of `packageC`.

### 2.2 What landed in `module_specifiers.rs` (commit 1)

- `KnownSymlinks` (`internal/symlinks/knownsymlinks.go`): `SetFile`,
  `SetDirectory`, `ProcessResolution`, `guessDirectorySymlink`,
  `HasDirectory`, `DirectoriesByRealpath`. Pure data; built by the program,
  read-only to the checker. Its checker-port record is on the type: native
  operation `Program.GetSymlinkCache`; keys the `toPath` of the symlink and
  of the real directory, with a trailing separator; owner the program;
  published whole before any checker exists; no receiver context; the
  expensive work is the build, reads are hash lookups.
- `each_file_name_of_module` (`GetEachFileNameOfModule`, `preferSymlinks =
  true`) and `all_module_paths` (`getAllModulePathsWorker`: nearest the
  importing directory first, then `comparePathsByRedirect`).
- `module_specifier_for_file` now runs `computeModuleSpecifiers`' loop over
  the module paths: the first package-name specifier wins; otherwise the
  first relative specifier from a path that is in `node_modules` when any
  path is ("the relative path through node_modules, so that the declaration
  emitter can produce a portability error", `:451`).

Native's symlink sets are `SyncSet`s with no iteration order; here they keep
insertion order, and `all_module_paths` sorts, so the order never reaches
an answer.

Not ported: `IsRedirect` (project-reference outputs; this port has none),
`GetRedirectTargets` (duplicate-package redirects are a loader table the
checker's host does not expose; `duplicatePackage` needs it), and the global
typings cache stop (empty in every program here).

**The checker's `ModuleHost` has no symlink question**, and `resolution.rs`
is not this lane's file, so commit 1's `known_symlinks()` answers `None`:
with one path per module the loop is the old single-path answer. Measured
byte-identical on both dumps (cut to the first four columns),
`slowcases` clean, Ir dm 1,091,526,164 → 1,091,528,623 (+0.0002%), gi
343,649,439 → 343,643,627 (−0.002%).

### 2.3 The host half — `r6-specifiers-symlink-cache.diff`

Not built in this lane's files; see §3 for the diff and its numbers.

## 3. The diffs, in apply order

All three apply cleanly, in this order, on commit 1. Measured unfiltered
against the frozen base, each on top of the ones before it.

### 3.1 `r6-specifiers-symlink-cache.diff` — the program's symlink cache

Files: `tsr-module` (`resolver.rs`, `package_json.rs`), `tsr-compiler`
(`loader.rs`, `lib.rs`; r6-modules2's), `tsr-checker/src/resolution.rs`
(no owner) and the one-line hookup in `module_specifiers.rs`.

- `ResolutionRequest.original_path`: the resolver's `OriginalPath`, kept
  for every module and type-reference request.
- `Program` builds `KnownSymlinks` after loading:
  `SetSymlinksFromResolutions` over those requests, then
  `add_dependency_symlinks` (`GetSymlinkCache`'s dependency half,
  `compiler/program.go:2068`) over `sourceFileMetaDatas.PackageJsonDirectory`
  of every file `source_file_may_be_emitted`, once per directory.
- `Resolver::resolve_package_directory` (`ResolvePackageDirectory`,
  `module/resolver.go:331`): `resolvePackageDirectoryOnly` stops
  `loadModuleFromSpecificNodeModulesDirectory` at the package directory;
  `createResolvedModuleHandlingSymlink` realpaths it. A resolver is built
  only when some dependency needs it.
- `PackageJson.dependencies` / `optionalDependencies` and
  `runtime_dependency_names` (`GetRuntimeDependencyNames`).
- `package_jsons_for_specifiers` also reads the package root of every
  symlinked path a program file has (`tryDirectoryWithPackageJson` reads the
  directory the path names, which for a linked package is the link); so
  `has_node_modules_files` turns on for a program whose `node_modules` holds
  only links.
- `ModuleHost::known_symlinks`, defaulted to `None`; `Program` answers its
  cache when non-empty.

Rejected: retaining the loader's resolver for `ResolvePackageDirectory`.
The loader drops it when loading ends (r5-modules §5.1), and its cache keys
are the resolver's; a fresh, untraced resolver asks only
`directory_exists` and `realpath`, as native's package-directory-only state
does. Rejected: building the cache lazily on the first specifier question,
as native's `getValue` does: the checker holds the program by shared
reference across threads (r5-modules §5.1), and the build is empty work for
a program without symlinked resolutions or dependency-bearing
`package.json` files, which is both benches.

**Measured alone** (commit 1 + this diff): both dumps byte-identical to the
base. Nothing prints differently until `symbol_chain` reaches the
specifier for a symlinked module (§3.3). Ir: dm within the run-to-run
spread (±0.07% on identical binaries), gi −0.008%.

### 3.2 `r6-specifiers-pure-alias-scope.diff` — `resolveAlias` in `alias_in_scope_for` (+38)

File: `checker.rs` (main's). Independent of §3.1.

Found while measuring §3.3: with the gate open,
`declarationEmitReexportedSymlinkReference2` lost
`MetadataAccessor.create` → `typeof MetadataAccessor` (native keeps the
bare name; TSR spelled the specifier). `trySymbolTable`'s alias arm
(`symbolaccessibility.go:562`) compares `c.resolveAlias(alias)` with the
symbol, and `resolveAlias` (`checker.go:16266`) follows the target through
every **pure** alias (`resolveIndirectionAlias` when
`IsNonLocalAlias(target, Value|Type|Namespace)`). `alias_in_scope_for`
compared one step (`resolve_alias`), so an import of a re-export
(`import {X} from "pkg2"` where `pkg2` writes `export {X} from "pkg1"`)
never made `X` accessible.

The first version followed every alias (`resolve_alias_fully`) and lost
`importElisionConstEnumMerge1` (4 lines): there `import { Enum }` merges
with `namespace Enum`, so the target carries `Namespace` and native stops
there. The diff stops exactly where `IsNonLocalAlias` does.

**Measured** (commit 1 + §3.1 + this): **types +38 lines, 0 lost**
(`constEnumNoEmitReexport` 8, `exportNamespace1` 3, `exportNamespace5` 3,
`exportDeclaration_moduleSpecifier` 3, and 21 lines across 17 JSX/React
cases whose `React` import is a re-export); diagnostics unchanged;
`slowcases` clean. Ir: dm **+0.087%** (1,091.5 M → 1,092.49 M, four runs
each; the spread on identical binaries is ±0.07%), gi −0.003%. The cost is
the deeper resolution itself (first-time `resolve_alias` of each re-export
chain's next link, mostly allocation): native's `trySymbolTable` resolves
every alias in every table it visits fully. CPU (median user+sys, base
binary in the tsgo slot): dm 1.014 at 41 samples, gi 0.994 at 21.

### 3.3 `r6-specifiers-symlink-chain-gate.diff` — HELD on `tsr-2zk.39`

Files: `checker.rs` (`symbol_chain`'s `node_modules` arm, main's) and
`module_specifiers.rs` (`module_has_symlinked_node_modules_path`).

`symbol_chain` hands a file module to `getSpecifierForModuleSymbol` only
when its stored path contains `node_modules/` (r5-modules §5.2); a
realpath'd workspace package never does. The diff adds
`computeModuleSpecifiers`' `importedFileIsInNodeModules` over the symlinked
paths.

**Measured** on §3.1 + §3.2: types **+16, 0 lost** (the four
`symlinkedWorkspaceDependenciesNoDirectLink*` cases: 2:1–2:5 and the deep
one's 2:0–3:2); diagnostics **+1, −2**:
`declarationEmitReexportedSymlinkReference3` converts (TS2883), and
`declarationEmitReexportedSymlinkReference` / `…2` go EMPTY_RIGHT →
EMPTY_WRONG with a TS2883 native does not report.

The losses are the container, not the specifier. Native's
`getContainersOfSymbol` puts the **re-exporting** modules the enclosing file
can reach (`getWithAlternativeContainers`, `getAlternativeContainingModules`)
before the declaring file: it prints `import("@raymondfeng/pkg2").IdType`,
which has no `/node_modules/`, so no TS2883. TSR has only the declaring
file, reachable only through `pkg2/node_modules/@raymondfeng/pkg1`, and
reports. Reference3's conversion is the same mismatch landing on the right
code and position (its type lines stay WRONG: native's container is
`pkg1/dist/index`, TSR's `pkg1/dist/types`). That container walk is the
symbol-chain printer (`tsr-2zk.39`, main's), which this lane does not port.

Falsifier: with `getWithAlternativeContainers` in `symbol_chain`, this diff
measures ≥ +16 types and the two EMPTY_RIGHT cases stay right. If they
still report TS2883 then, the gate is wrong.

### 3.4 `r6-specifiers-composite-slots.diff` — intersections and plain tuples at the site (+14, TS2883 +1)

Files: `printing.rs` (r6-lazytext's; two functions beside `union_text_at`)
and `checker.rs` (main's; two calls after `union_text_at` in
`type_to_string_at_worker`). Independent of §3.1–§3.3; measured on §3.1 +
§3.2.

The node builder prints every slot of a composite through
`typeToTypeNodeHelper`, so a constituent from another module takes its
`import("…")` qualifier wherever it sits. This port mints composite text
at creation (inside view) and re-renders at the site only the shapes that
have an at-site arm: unions (`union_text_at`), references, signatures,
type literals. Intersections and tuples had none, so their members printed
bare: `[SomeProps, OtherProps, …]` where native prints
`[import("foo").SomeProps, …, import("foo/node_modules/nested").NestedProps]`.
That bare print is why `declarationEmitCommonJsModuleReferencedType`'s
TS2883 never reached the r5-modules §6 sink (r6-smallcodes4 §4).

- `intersection_text_at`: an intersection no alias names prints each
  constituent through `type_to_string_at`, parenthesised by the same
  `binds_below_intersection` rule its minted text uses.
- `tuple_text_at`: a plain tuple (`create_tuple_type`'s; no labels,
  optional, rest or variadic elements, which keep their minted text) prints
  each element at the site.

Either declines, and the minted text stands, when any slot declines.

**Measured** unfiltered, on §3.1 + §3.2: **types +14, 0 lost**
(`accessorsOverrideProperty8` 3, `tsxSpreadDoesNotReportExcessProps`,
`reactReadonlyHOCAssignabilityReal`, `reactHOCSpreadprops`,
`reactDefaultPropsInferenceSuccess`, `jsxElementType` 2 each,
`declarationEmitTypeParameterNameShadowedInternally` 1); **diagnostics
+1, 0 lost** (`declarationEmitCommonJsModuleReferencedType`, TS2883);
`slowcases` clean; Ir dm 1,092.49 M → 1,092.40 M (three runs each), gi
within ±0.01%. The case's four type lines stay WRONG on one member:
`import("foo/other").OtherIndexProps` where native keeps
`foo/other/index`, because `processEnding` keeps `/index` when a file
shares the directory's name (`tryGetAnyFileFromPath`, `util.go:194`,
`foo/other.d.ts` here). That probe asks the file system, which the
checker's host does not keep after loading (r5-modules §4.2); the fix is a
loader-side precompute like `extensionless_relative_import`, not built.

## 4. TS2883 (item 3)

| Case | State | Cause |
|---|---|---|
| `declarationEmitCommonJsModuleReferencedType` | RIGHT with §3.4 | tuple slots at the site |
| `declarationEmitReexportedSymlinkReference3` | RIGHT with §3.3 (held) | by coincidence: wrong container, right code and position (§3.3) |
| `declarationEmitObjectAssignedDefaultExport` | WRONG | with §3.4 the intersection prints at the site, but `NonReactStatics` takes the alias `hoistNonReactStatics.` (`best_name` finds a namespace import in `styled-components`' own file); native has no accessible chain and spells `import("styled-components/node_modules/hoist-non-react-statics")`. The accessibility walk, `tsr-2zk.39`. Its other six lines want `StyledComponent<"div">`: native drops the trailing type arguments equal to their defaults (the node builder's default-argument elision), a printer piece, not a specifier. |
| `declarationEmitUsingTypeAlias1` | WRONG | r6-smallcodes4 §3.2's import-type diff, then the alias name through `getSymbolChain` (`tsr-2zk.39`) |

## 5. Remaining, with causes

| Case(s) | Lines | Cause | Owner |
|---|---|---|---|
| `declarationEmitReexportedSymlinkReference{,2,3}`, `declarationEmitForGlobalishSpecifierSymlink{,2}`, `symbolLinkDeclarationEmitModuleNames{,RootDir}`, `typesVersionsDeclarationEmit.multiFileBackReferenceToUnmapped`, `reactTransitiveImportHasValidDeclaration` | ~45 | the container: native names the symbol through a module that re-exports it and that the enclosing file reaches (`getWithAlternativeContainers` / `getAlternativeContainingModules`, `symbolaccessibility.go`); TSR names the declaring file | `tsr-2zk.39` (main) |
| `symlinkedWorkspaceDependenciesNoDirectLink*` (4 cases, 16 lines) | 16 | convert with §3.1 + §3.3; §3.3 is held on the row above | §3.3 |
| `declarationsIndirectGeneratedAliasReference`, `duplicatePackage` | 11 | a default-exported class prints by its local name (`Ctor`, `X`) where the chain ends in the export name (`import("mod").default`); `duplicatePackage` also needs `GetRedirectTargets` (the loader's duplicate-package table) | `tsr-2zk.39`; `tsr-compiler` |
| `legacyNodeModulesExportsSpecifierGenerationConditions` | 3 | the members of `(await import("inner"))`'s synthesized namespace object print from minted text, not at the site | printer (main / r6-lazytext) |
| `declarationEmitCommonJsModuleReferencedType` | 4 | `tryGetAnyFileFromPath` (§3.4) | loader + `module_specifiers.rs` |
| `moduleResolutionWithSymlinks{,_withOutDir}` | 8 + 2 diagnostics cases | `export { T }` of a local type reads `any` (§1); not symlinks | `symbols.rs` (main) |
| `inferrenceInfiniteLoopWithSubtyping` | 2 | `Readonly<{ [key: string]: Readonly<T>; }>` prints as the alias `ObjMapReadOnly<T>`: alias naming of an instantiated alias, not a specifier | printer / `declared.rs` |
| `jsDeclarationsWithDefaultAsNamespaceLikeMerge` | 5 | a JS default-export namespace merge types `computed` as an index signature (`{ [x: string]: Computed; }`) | JS binder/checker (main) |
| `mergeSymbolReexportInterface`, `mergeSymbolReexportedTypeAliasInstantiation` | 3 | `Row2` reads `any`: a module augmentation into a re-exported interface/alias (r6-smallcodes4 §5's TS1362 cause) | binder (main) |
| `inlineJsxFactoryDeclarationsLocalTypes` | 3 | `children?: predom.JSX.Element[]` inside a JSX props literal keeps its minted text | printer (main / r6-lazytext) |
| `umd8` | 6 | a UMD global used from a module reads `any` | binder/`symbols.rs` (main) |

The `.16.77` rows after the first three were classified by their printed
difference only; none prints a module specifier wrong.
