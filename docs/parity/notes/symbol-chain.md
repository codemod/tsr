# Accessible-symbol-chain lane: tsr-2zk.39

## Verified root: UMD aliases in module-object naming

Pinned native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
`internal/checker/symbolaccessibility.go` `trySymbolTable` excludes default,
export-equals, export-specifier and namespace-reexport aliases on a local-name
lookup. It also excludes UMD aliases when the enclosing source is an external
module. `Checker::module_alias_at` omitted these exclusions, although
`best_name` already had them. Consumers are `module_name_at` and
`symbol_chain`, including module-object `type_to_string_at` printing.

Port applies the native exclusions before alias resolution. A namespace
reexport is identified by its parent export declaration's module specifier;
a UMD alias by its **first** declaration, as native `isUMDExportSymbol` does.
`allowUmdGlobalAccess` affects the diagnostic, not this printing exclusion.
No new fallback, cache, alias identity, or scope widening is introduced.

### Ownership and work boundary

- Keys remain binder Program `SymbolId`s in one parser/binder identity space;
  lookup context is the actual enclosing reference `NodeId`, meaning and its
  concrete source file. The original written alias is retained for ordering.
- Existing private Checker alias resolution owns forcing/publication and its
  lifetime. The filter does not publish any semantic result. Excluded aliases
  are unsupported naming candidates, not completed alias-resolution failures.
  Active/absent/completed-success/completed-failure states of alias resolution
  remain unchanged.
- Existing scope-table collection, alias resolution, clone checks, shadow
  checks, and `compare_symbols` selection remain the expensive work boundary.
  Excluded candidates now stop before alias forcing. No reuse is extended.
- Worker counts/copy bytes are unmeasured. Integrator Beads request under
  tsr-2zk.39: measure per-table query/alias forcing/copy counts before expanding
  accessibility reuse; retain the existing tsr-1yb.11 attribution boundary.

### Native controls and regression tests

Built `./cmd/tsgo` directly from the pinned vendor tree using Go 1.26.8.
Native files: `lib.d.ts` exports `X` and `export as namespace N`; a script
and an external module both assign `N` to a variable. Native declaration
output is respectively `typeof N` and `typeof import("./lib")`. Without
`allowUmdGlobalAccess`, the module also reports TS2686 at line 1 column 14;
with it, native exits zero but prints the same import type. Frozen TSR
reported the same TS2686, but current corpus printing was `typeof N`.
The TSR CLI emitted no declaration files for this command, so its emitted
surface is **not** certified. Checker tests exercise the actual site renderer
on the same two-file identity space:
`tests/symbol_chain_native_umd.rs` (two controls), plus all 32 existing
`tests/symbol_chain.rs` tests pass.

### Full-population evidence

Baseline release binary SHA256:
`866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`.
Native binary SHA256:
`7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`.
Both unfiltered verdict dumps completed before and after editing.

- Aligned type verdicts: 477970 rows; RIGHT 469765 -> 469785,
  WRONG 7212 -> 7190, GAP 993 -> 995. Twenty WRONG -> RIGHT lines;
  two WRONG -> GAP lines; zero RIGHT losses. Missing baseline keys: zero.
- Diagnostic cases: 10570 rows, unchanged: RIGHT 4221,
  EMPTY_RIGHT 4968, WRONG 1281, EMPTY_WRONG 100.
  Zero RIGHT/EMPTY_RIGHT losses. Missing baseline keys: zero.
- Full coverage completed for all 12444 discovered cases in a disposable
  tracked-file copy, with the vendor corpus linked read-only, avoiding any
  writes to owned-by-integrator snapshots. Copy metadata reports native SHA
  `unknown` because it has no git directory; the original vendor checkout
  was explicitly verified at the pinned full SHA.
- Coverage: checker_types 8051/9538, assertion lines 469785/478855;
  diagnostics 4221/5502. Filters/variants/exclusions are existing harness
  behavior, not full strict message/length/order certification.
- Workspace release tests, workspace all-target clippy `-D warnings`, and
  workspace fmt check passed on installed Rust 1.96.0.

Named target cases now passing checker_types: ambientExportDefaultErrors,
exportAsNamespace.d, jsdocReferenceGlobalTypeInCommonJs, umdGlobalConflict,
umd-augmentation-2, umd1, umd3, umd4, umd5. Other lines converted in
checkMergedGlobalUMDSymbol, exportAsNamespace_augment,
noCrashUMDMergedWithGlobalValue, stackDepthLimitCastingType,
umdGlobalAugmentationNoCrash, umd-augmentation-1, and umd-errors.
Line conversion is not a case pass: e.g. umd-errors remains 10/12 and
umdGlobalAugmentationNoCrash remains 13/15.

### Performance: observed, not a verified release ratio

Existing whole-project harness, fresh processes, interleaved order, 21 pairs,
filesystem warmups excluded, default threading, incremental/composite off.

| Project | candidate/baseline wall | CPU | candidate/native wall | CPU |
|---|---:|---:|---:|---:|
| domain-model | 0.980511 | 0.946887 | 1.035880 | 0.574252 |
| generic-imports | 1.000585 | 1.011392 | 0.930401 | 0.442743 |

Generic-imports 41-pair baseline rerun: wall 0.996703, CPU 0.999115.
Diagnostic fingerprints match on all runs. No measured baseline slowdown.
All reports say `work_comparable: false`,
`complete_input_equivalence_verified: false`,
`actual_checked_work_verified: false`, `verified_wall_ratio: null`.
No complete input-query manifest or actual checked-scope proof was supplied;
these samples do **not** establish the <=0.50 release goal.

## Exact cross-owner prerequisites, not implemented here

1. `crates/tsr-checker/src/resolution.rs` `ModuleHost` and
   `crates/tsr-compiler/src/lib.rs` `impl ModuleHost for Program` expose only
   `file_path` for this naming path. Native
   `internal/modulespecifiers/specifiers.go` `computeModuleSpecifiers` /
   `tryGetModuleNameAsNodeModule` need package-json info (exports, typesVersions,
   typings/types/main), nearest package-json ancestors, source resolution mode,
   compiler options, case sensitivity/current directory and symlink module
   paths. Integrator must provide native-compatible host answers before this
   lane can faithfully replace `module_specifier_for_symbol`'s node_modules
   decline. No path substring heuristic is a replacement. Affects duplicatePackage,
   typesVersionsDeclarationEmit, symlinkedWorkspaceDependencies, reexported
   symlink and legacyNodeModulesExportsSpecifierGenerationConditions targets.
2. Native `getCandidateListForSymbol` calls `getExportsOfSymbol`, which forces
   resolved/derived exports, distinct from raw binder exports. Current
   checker naming paths read raw exports. Required external ownership seam is
   `crates/tsr-checker/src/symbols.rs` export/alias completion; integrator must
   expose completed derived exports with absent/active/failure states and
   separate table identity before widening recursive chain reuse.
   `getContainersOfSymbol` / `getAlternativeContainingModules` additionally
   require Program source/import inventory for reexport containers.
3. `compiler/ambientExportDefaultErrors` diagnostic remains WRONG: native
   `internal/checker/checker.go` `checkExportAssignment` reports TS2714;
   `crates/tsr-checker/src/check.rs` is not owned. This commit fixes its type
   line, not its diagnostic. Route that native diagnostic operation to its owner.
4. `importShouldNotBeElidedInDeclarationEmit` wants `typeof import("umd")`
   and `import("umd").Thing`, but lacks the completed ambient module object
   here (first line now GAP instead of wrong `typeof umd`). Integrator must
   repair the ambient/export-equals symbol producer in `symbols.rs` before
   naming can supply that module type; no suppression or fake module value.

The entire ACCESSIBLE-SYMBOL-CHAIN cluster is not certified complete. Scope,
container alternatives, resolved export tables and host-backed module specifiers
must still be ported with the above ownership contracts; this commit is only
one reproduced native root cause.
