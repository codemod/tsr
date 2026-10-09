# r6-specifiers2: export-specifier aliases, `processEnding`'s file probe, and the `.16.77` leftovers (tsr-2zk.1149)

Round-6 box, succeeding r6-specifiers (`docs/parity/notes/r6-specifiers.md`,
batch BS). Owned: `crates/tsr-checker/src/module_specifiers.rs`,
`crates/tsr-checker/src/resolution.rs`, `crates/tsr-module`. Everything else
ships as a measured diff in this directory. tsgo `5b1047d`; every expectation
below was checked against a native `tsgo` built from that pin by
`scripts/offline-cargo/build-tsgo.sh`.

Base frozen at `5e4d21b` (tip of `claude/beautiful-shannon-ar5gh0` when the
box started; BS had not landed). Base: types 556,303 aligned lines;
diagnostics 12,238 rows.

## 1. `export { T }` read through an import — `r6-specifiers2-pure-alias-type-reference.diff`

### 1.1 The cause

r6-specifiers §1's probe, with no symlink:

```ts
// b.ts
type T = number; export { T }
// app.ts
import { T } from "./b";
let w: T;
const q: string = w;   // native: TS2322 + TS2454; TSR (base): nothing
```

Not `export_specifier_target` (`symbols.rs`): traced, the import alias
resolves to the export specifier's alias, and that one resolves to the type
alias `T`, both correctly. The `any` comes from the **consumer**:
`get_type_from_type_reference`'s §491 road (`declared.rs`, r6-declared2's
file) calls `resolve_alias` **once** for an unrenamed import specifier, lands
on the export specifier's symbol (flags `ALIAS` only), fails the `TYPE` test
and answers `errorType`.

Native's `resolveAlias` (`checker.go:16266`) does not stop there: when the
target `IsNonLocalAlias(target, Value|Type|Namespace)` (`ast/utilities.go:2608`
— a pure alias, or an alias with `Assignment`) it continues through
`resolveIndirectionAlias` (`:16293`). This port's `resolve_alias` is the
one-step `getTargetOfAliasDeclaration` memo, and each consumer chooses how far
to follow it.

`export { T as U }` / `import { U }` worked on the base only by accident of a
different road (the renamed-specifier primitive arm, which already calls
`resolve_alias_fully`).

### 1.2 The diff

`declared.rs`, the §491/§493 alias road: after `resolve_alias`, follow the
target while it is a pure alias (`IsNonLocalAlias`'s exact test), as
`resolveIndirectionAlias` does. Bounded at 8 links, the same policy as
`resolve_alias_fully`. Plus `crates/tsr-compiler/tests/r6_specifiers2_export_specifier_alias.rs`
(the probe, and a renaming chain `C → B → class A` that must report TS2741).

**A name gate was measured and refused.** The §491 comment says the
unrenamed specifier's local name and the target's name "agree by
construction"; a followed chain breaks that (`export { MyClass as MyClass2 }`).
The first version declined (answered `errorType`) when the followed target's
name differed from the local name. Measured: +18 types, +1 diagnostics case.
Without the gate: **+28 types, +4 diagnostics cases**, 0 lost either way. The
printer already names the type at its site (r6-accessible's
`IsTypeSymbolAccessible` arm): native prints `J` for `import { J }` of
`interface I {}; export { I as J }` (tsgo `--declaration`: `q: J[]`), and so
does TSR without the gate. `moduleResolutionWithSymlinks{,_withOutDir}` are
exactly that shape (`let y: MyClass2` prints `MyClass`, the name in scope),
and converting them needs the gate gone. Native has no such gate.

### 1.3 Measured (against `5e4d21b`, unfiltered)

- **Types +28, 0 lost**: `declarationsForIndirectTypeAliasReference` 8,
  `moduleResolutionWithSymlinks` 4, `moduleResolutionWithSymlinks_withOutDir` 4,
  `noCrashOnImportShadowing` 2, `emitDecoratorMetadata_isolatedModules` ×2
  modules 2 each, `exportDeclaration` ×2 1 each, `generic` 1, `enums` 1,
  `chained` 1, `decoratorMetadataTypeOnlyExport` 1.
- **Diagnostics +4, 0 lost**: `mergeSymbolReexportInterface`,
  `moduleResolutionWithSymlinks`, `moduleResolutionWithSymlinks_withOutDir`,
  `chained`.
- `slowcases` clean on both dumps.
- Ir (`valgrind --tool=callgrind`, release `tsr -p … --singleThreaded
  --noEmit`): dm 1,090,963,052 → 1,090,894,305 (−0.006%), gi 343,064,681 →
  343,072,112 (+0.002%).

**The corpus count the brief asked for**, measured with a throwaway
instrument on the road (not committed), over the whole types run: on the base
the road met an import whose one-step target is a pure alias **50 times**
(`B` 12, `T1` 8, `Row2` 5, `zzz` 4, `StringHash{,2}` 4 each, `MyClass2` 4, `A`
4, and `X`, `TypeFlag`, `SymbolFlags`, `Foo`, `D` once each). Every one of them
answered `errorType`. With the diff, **0** stop on a pure alias: 483 road
queries reach a type, and 5 reach a target with no type meaning
(`zzz` ×4 and `A` ×1, a value-only export, where `errorType` is native's
answer). The `Row2` hits now resolve, but their lines stay WRONG for another
reason (§4.2).

Owner: **routed to r6-modules4** (`tsr-2zk.1162`, RESOLVE-ALIAS-INDIRECTION).
The integrator made r6-modules4 the single owner of `resolveAlias`'s
pure-alias chains mid-round. This diff is a consumer-side port in
`declared.rs`, not a `symbols.rs` change; if r6-modules4 makes
`resolve_alias` itself follow `resolveIndirectionAlias`, the loop here
becomes a no-op and should be deleted with it. Independent of every other
diff here. §4.1's UMD diff stacks on it.

## 2. Default type arguments in `ObjectAssignedDefaultExport` — `r6-specifiers2-reused-module-member.diff`

### 2.1 Not elision: node reuse

r6-specifiers §4 read the six lines (`StyledComponent<"div">` where TSR
printed `StyledComponent<"div", DefaultTheme, {}, never>`) as the node
builder dropping type arguments equal to their defaults. **Native has no such
elision**: the alias arm (`nodebuilderimpl.go:3366`) maps every
`alias.TypeArguments()`. The short form is the **written return annotation,
reused**: `div: (a: TemplateStringsArray) => StyledComponent<"div">`
prints its signature through `serializeReturnTypeForSignature`, which reuses
the annotation node (`tryReuseExistingTypeNode`). At the importing file
`StyledComponent` is not in scope, so `tryVisitTypeReference`
(`nodecopy.go:416`) takes the `introducesError` arm: `serializeTypeName`
(`:436-449`) names the symbol through `symbolToTypeNode` and keeps the
**written** type arguments: `import("styled-components").StyledComponent<"div">`.
tsgo `--declaration` on a reduced probe agrees:
`d: (a: TemplateStringsArray) => import("sc").SC<"div">`.

TSR already reuses the node (it prints `SC<"div">` in the declaring file).
At the other file, `serialize_type_name` (`node_reuse.rs`) names a module
member only through `parameter_source_symbol_name_at`, a bounded walk of the
scope tables at the site. That walk finds nothing for an unimported module's
member and answers `None`, so the whole reuse declines (`unnameable`) and the
type is serialized structurally, with every argument.

### 2.2 The diff

`node_reuse.rs` (no owner this round), `serialize_type_name`'s module-member
arm: when the table walk declines, name the symbol through `symbol_chain`
(the printer's `getSymbolChain` port, which spells `import("…")` for an
unreachable module), as the arm's non-module branch already does. Plus
`crates/tsr-conformance/tests/r6_specifiers2_reused_module_member.rs` (the
reduced probe, against tsgo's answer).

**Refused first: `reference_text_at`.** It printed the same text, measured
+15 types and 0 lost, but failed
`signatures::tests::parameter_source_views_qualify_bound_names_and_reject_unloaded_imports_without_work`:
a full print at `importedView` changed the type store, and that test asserts
that printing is read-only. `symbol_chain` passes it.

### 2.3 Measured (against `5e4d21b`, unfiltered)

- **Types +14, 0 lost**: `declarationEmitObjectAssignedDefaultExport` 6 (all
  six lines), `duplicatePackage` 3 (the parameter slots `(x: import("a/node_modules/x").default) => void`),
  `declarationEmitPartialNodeReuseTypeOf` 3,
  `importShouldNotBeElidedInDeclarationEmit` 2.
- Diagnostics: no change.
- `slowcases` clean; `cargo test --workspace --release` green; clippy reports
  the same 12 pre-existing stable-toolchain errors with and without it, none
  in these lines.
- Ir: dm 1,090,963,052 → 1,090,926,551 (−0.003%), gi 343,064,681 →
  343,096,295 (+0.009%).

`declarationEmitObjectAssignedDefaultExport`'s remaining line (2:19) is
r6-specifiers §4's: `NonReactStatics` takes the alias
`hoistNonReactStatics.` (the accessibility walk, `tsr-2zk.39`). Owner of the
diff: whoever takes `node_reuse.rs`; it touches no other file. Independent
of §1.

## 3. `processEnding` keeps `/index` beside a same-named file (`tryGetAnyFileFromPath`)

### 3.1 The constraint

`declarationEmitCommonJsModuleReferencedType` (with r6-specifiers' §3.4
composite-slots diff) prints `import("foo/other").OtherIndexProps` where
native keeps `foo/other/index`. `processEnding`'s minimal arm
(`specifiers.go:674`) drops `/index` only when `tryGetAnyFileFromPath(host,
withoutIndex)` (`util.go:194`) finds no `withoutIndex + ext` on disk, and
`foo/other.d.ts` exists. The probe asks the file system, which the checker's
host does not keep past loading (r5-modules §4.2).

The extension set: `GetSupportedExtensions({allowJs: true}, [.node, .json])`.
Its extras are `ScriptKindExternal` and `ScriptKindJSON`, and
`GetSupportedExtensions` (`tsoptions/tsconfigparsing.go:1828`) adds an extra
only for `ScriptKindDeferred`, or for JS/JSX when JS is allowed. So neither
is added, and the set is exactly `AllSupportedExtensions`.

### 3.2 What landed in this lane (commit 3)

- `ModuleHost::any_file_from_path(path) -> Option<bool>` (`resolution.rs`),
  defaulted to `None`.
- `process_ending`'s `Ending::Minimal` arm (`module_specifiers.rs`) keeps
  `no_extension` when the host answers `Some(true)`. `None` reads as "no
  file", which is the pre-port answer, so alone the commit changes nothing.

### 3.3 The host half — `r6-specifiers2-any-file-from-path.diff`

`crates/tsr-compiler/src/lib.rs` (r6-modules3's) and a new
`crates/tsr-compiler/tests/r6_specifiers2_any_file_from_path.rs`.

- `index_directories_with_file`, after loading, beside
  `package_jsons_for_specifiers`: for each program file `D/index.*`, it
  probes `D + ext` over `AllSupportedExtensions` once and records `D →
  bool`, keyed by the normalized absolute path. Files whose stem is not
  `index` are skipped before any normalization.
- `Program::any_file_from_path` normalizes its input against the program's
  current directory (native's `GetNormalizedAbsolutePath(…,
  host.GetCurrentDirectory())`) and answers the map.

Checker-port record: native operation `ModuleSpecifierGenerationHost.FileExists`
through `tryGetAnyFileFromPath`, asked by `processEnding`. Key: a
directory's normalized absolute path. Value: whether a sibling file exists.
Owner: the program. Published whole at construction, so there is no
partial state; `None` means not probed. No receiver context. The
expensive work is at most 12 `file_exists` calls per `index.*` program
file, done once.

**The accepted limitation.** Native asks about any path. The
`getLocalModuleSpecifier` caller (`specifiers.go:509`) passes a path
relative to the importing file, which native then resolves against the
**current directory**. This map holds only program-file directories, so
that relative form lands on a key exactly when the importing file sits in
the current directory. Elsewhere it answers `None` (no file), where native
would probe the cwd-relative path, which is almost never a real file.
Rejected: keeping the loader's file system or its resolver alive in
`Program`. The program is shared across checker threads by reference, and
r5-modules §5.1 refused the resolver for the same reason.

When the symlink cache lands (BS §3.1), symlinked module paths reach
`process_ending` too, and those are not program-file paths. The map would
then need their `index` directories as well. Falsifier: a symlinked
`…/index.d.ts` beside a same-named file printing without `/index`.

### 3.4 Measured

- Commit 3 alone, and commit 3 with this diff, against `5e4d21b`: both dumps
  byte-identical (types and diagnostics verdicts; 0 changed).
  `slowcases` clean.
- With r6-specifiers' `composite-slots.diff` underneath (its tuple slots
  print at the site): against base + composite-slots, **types +4, 0 lost**.
  That is all four of `declarationEmitCommonJsModuleReferencedType`'s
  remaining lines, so the case is fully RIGHT. Diagnostics unchanged;
  `slowcases` clean.
- Ir (alone + diff): dm 1,090,331,932 → 1,090,781,890 (+0.04%), gi
  343,096,589 → 343,142,635 (+0.013%). The base binary itself moved 0.06%
  on dm between two runs. An earlier build that normalized every file's path
  measured gi +0.1%, which is why the stem test runs first.

## 4. The `.16.77` leftovers

### 4.1 `umd8` — `r6-specifiers2-umd-global-type-reference.diff` (+3, routed with §1)

`declare let y: Foo`, where `Foo` is the UMD global (`export as namespace
Foo` of a module with `export = Thing`). Native's `resolveTypeReferenceName`
calls `resolveAlias` for every alias kind:
`getTargetOfNamespaceExportDeclaration` gives the module's `export =`
symbol, a pure alias, and `resolveIndirectionAlias` reaches the class.
`declared.rs`'s alias road admits only import specifiers and import
clauses, so the UMD alias fell through to `get_type_reference_type` on the
alias itself and answered `errorType`.

The diff (applies on §1's) admits `NamespaceExportDeclaration` to the road.
**Measured** against base + §1: **types +3, 0 lost** (`umd8` 0:2 `number`,
0:3 and 0:5 `() => number`); diagnostics unchanged; `slowcases` clean. Its
`y` lines (0:1, 0:4) still differ: native names the `export =` class
`import("./foo")`, the module itself, and that is the chain printer
(`tsr-2zk.39`). Owner: routed to r6-modules4 with §1.

### 4.2 `mergeSymbolReexportInterface`, `mergeSymbolReexportedTypeAliasInstantiation`

`index.d.ts` writes `export type { Row2 } from './common'`, and an
augmentation `declare module '.' { type Row2 = … }` merges into that module.
Native's `mergeSymbolTable` stores `mergeSymbol`'s return value. For the
alias target, `mergeSymbol` resolves it (`checker.go:14153-14163`), finds
the interface or type alias that the augmentation's `type` excludes, and
reports the merge error. It then **returns the source**, so `index`'s
export `Row2` becomes the augmentation's type alias, and `Row2<string>`
prints that generic alias. TSR's `Binder::merge_symbol`
(`crates/tsr-binder/src/binder.rs:1119-1137`) declines an alias target
(`alias_merges`, `bd tsr-y4u.12`: the binder cannot resolve aliases), so the
table keeps the re-export. Traced:
`Row2#27417: ALIAS, 1 declaration` after merging, while `C` merged.
§1 converts the diagnostics case of `mergeSymbolReexportInterface` (TS2300
is the checker's `report_merge_conflicts`), not the types.

Cause: `Binder::merge_symbol`'s alias-target arm and the table store. Owner:
the binder (MAIN). It needs the alias resolution that `bd tsr-y4u.12`
records as missing.

### 4.3 `legacyNodeModulesExportsSpecifierGenerationConditions` (3 lines)

`(await import("inner"))` types as
`get_type_with_synthetic_default_import_type` (`module_exports.rs`):
`get_spread_type(namespace, wrapper)`. The spread's members are
`AnonymousProperty` slots that carry the text minted when the spread was
built (`x: () => Thing`), with no declaration left to reuse at the site.
Native's spread symbols keep their declarations, so the member prints
through `serializeTypeForDeclaration` at the site, and `Thing` (not
accessible there) becomes `import("./node_modules/inner/private").Thing`.
Cause: `get_spread_type`'s property mint (`spreads.rs`, r6-errorsplit2)
and the anonymous-property print (`objects.rs`/`printing.rs`, r6-printer4).
§2's diff does not reach it, which was measured: no change in this case.

### 4.4 `inlineJsxFactoryDeclarationsLocalTypes` (3 lines)

Reduced and checked against tsgo: a parameter annotation
`{ children?: predom.JSX.Element[] }`, written where `predom` is imported and
printed in another file. Native prints
`import("./renderer2").predom.JSX.Element`. Traced in TSR: the visitor
correctly declines `predom` at the site (`track_existing_leftmost_identifier`:
no binding there), so `serialize_type_name` runs. Its non-module-member arm
then calls `reference_text_at`, which spells the namespace path
`predom.JSX.Element` from the container chain without the module qualifier
that native's `symbolToTypeNode` adds when no alias reaches the module. Cause:
`reference_text_at` / `qualified_name_at` (`checker.rs`) for a namespace
member of an unimported module, which is the chain printer (`tsr-2zk.39`,
main).

### 4.5 `inferrenceInfiniteLoopWithSubtyping` (2 lines)

Native prints `ObjMapReadOnly<T>`'s declared type, and its instantiation, as
`Readonly<{ [key: string]: Readonly<T>; }>`: the outer alias is lost.
`getTypeFromTypeAliasReference` does pass `ObjMapReadOnly` as the new alias,
but `Readonly` is a homomorphic mapped type. `instantiateMappedType`
(`checker.go:22567-22570`) maps its type variable through
`mapTypeWithAlias(…, alias)`, which applies the alias only to a **union**
(`:25554`). For an object it calls `instantiateConstituent`, which calls
`instantiateAnonymousType(t, …, nil)` (`:22565`) and inherits the target's
own alias, `Readonly<…>`. TSR's `instantiate_mapped_type_worker`
(`mapped.rs`) carries no alias, and TSR names the reference by the written
alias. Cause: alias propagation through `instantiateMappedType`'s
homomorphic arm (`mapped.rs` / `declared.rs`). Owner: r6-declared2.

## 5. `duplicatePackage`: `GetRedirectTargets` (item 5)

Of its 7 lines, §2's diff converts the three parameter slots. The other four
print `X` where native prints `import("a/node_modules/x").default` /
`import("c/node_modules/x").default`. Two pieces are missing:

- the `.default` naming of a default-exported class (`tsr-2zk.39`, main);
- `GetRedirectTargets` (`compiler/program.go:162`): native names `b`'s copy
  through `c/node_modules/x`, a path that **redirects** to the first loaded
  copy (`filesparser.go:433`, same package name and version). The loader
  already records the inverse (`LoadedFiles::package_redirects`, duplicate
  → target, `loader.rs:1897`), and `Program::from_root_files` aliases the
  duplicate's path to the target's file. The specifier side consumes
  redirects in `GetEachFileNameOfModule` (`specifiers.go:274`), which is
  r6-specifiers' commit 1 and **has not landed on main** (batch BS pending
  when this box stopped).

Not built. The plan, for whoever lands it after BS:
`ModuleHost::redirect_targets(path) -> Vec<String>` in `resolution.rs`.
The program inverts `package_redirects` once, keeping load order as
`redirectTargetsMap` appends in task order. `each_file_name_of_module`
appends the targets after the imported file name, as native does. Alone it
converts **no line**: every `duplicatePackage` line that needs the
redirect path also needs the `.default` naming. Falsifier: with `.39`'s
default naming in, `duplicatePackage:0:2/0:8` print `a/node_modules/x`
until this lands.

## 6. Routed, not touched

- The container walk / alternative containers (`getWithAlternativeContainers`):
  `tsr-2zk.39`, main's, assigned to a human. r6-specifiers §5's 44 lines stay
  there.
- `r6-specifiers-symlink-chain-gate.diff` (r6-specifiers §3.3): held on `.39`.
- §1 and §4.1: routed to r6-modules4 (`tsr-2zk.1162`).

## 7. Summary

| § | What | Where | Measured (vs `5e4d21b`) |
|---|---|---|---|
| 1 | pure-alias chain on the import type road | `r6-specifiers2-pure-alias-type-reference.diff` (`declared.rs` + test) → r6-modules4 | types +28, diag +4, 0 lost |
| 2 | out-of-scope module member through `symbol_chain` in reused annotations | `r6-specifiers2-reused-module-member.diff` (`node_reuse.rs` + test) | types +14, 0 lost |
| 3 | `tryGetAnyFileFromPath` | commit `a59af55` (lane) + `r6-specifiers2-any-file-from-path.diff` (`tsr-compiler`) | alone 0; on composite-slots +4, 0 lost |
| 4.1 | UMD global on the import type road | `r6-specifiers2-umd-global-type-reference.diff` (on §1) → r6-modules4 | types +3 on §1, 0 lost |

Apply order: §1, §4.1, §2, §3's diff. §2 and §3 are independent of §1.
