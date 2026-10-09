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
