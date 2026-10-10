# r6-modules4 — alias resolution and symbol merging (`tsr-2zk.1162` …)

Round 6's successor to r6-modules3 (`r6-modules3.md`). This lane owns
`isolated_alias.rs`, `meaning_mismatch.rs`, `export_star_conflicts.rs` and
`crates/tsr-compiler`, and is the single owner of alias resolution and symbol
merging (`resolveAlias`, `resolveEntityName`, `mergeSymbol`, module
augmentation, `getTargetOf*`). `symbols.rs` and the binder are MAIN, so that
work ships as diffs here. Expectations were checked against a native `tsgo`
built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

The session was stopped by the integrator's usage-checkpoint wrap-up after
item 1. Items 2–10 were not started; §4 lists them with what was learnt.

## 0. Base

Batch BZ (r6-modules3) had not landed on `claude/beautiful-shannon-ar5gh0`.
This branch merges r6-modules3's branch onto main `c3c42d0` (`79e286d`; the
`isolated_alias.rs` conflict keeps main's removal of
`decorated_node_can_be_decorated`, landed by BP's grammar diff, and takes
r6-modules3's `resolution_keeps_source_file`). The measurement base is that
merge plus BZ's four landable diffs in `r6-modules3.md` §7's order, applied
and never committed here. Dumps frozen at that state, unfiltered and including
the configured suites: 12,238 diagnostics rows (5,642 RIGHT, 5,606
EMPTY_RIGHT) and 556,303 type rows (550,366 RIGHT).

Setup: PyPI is blocked, so `assemble.py`'s three `tomlkit` calls ran against a
stdlib-only stand-in kept outside the repo (`r5-operators3.md` §4).

## 1. `resolveIndirectionAlias` (item 1, `tsr-2zk.1162`, a measured diff)

### Cause

`resolveAlias` (`checker.go:16266`) answers `getTargetOfAliasDeclaration`,
and when that target is a **pure** alias (`ast.IsNonLocalAlias(target,
Value|Type|Namespace)`, `ast/utilities.go:2608`) it resolves on through
`resolveIndirectionAlias` (`:16293`), copying the target's
`typeOnlyDeclaration` back. A target merged with another meaning stops the
chain (`getSymbolFlags`' doc comment, `:16341`). TSR's `resolve_alias`
answered only the first hop, so `let x: C1` through `import { C1 }` of a
local `export { C1 }` read `error`/`any`, and every consumer that iterated
(`resolve_alias_fully`) walked on through merged alias+type symbols where
native stops.

### The port: `r6-modules4-resolve-alias-indirection.diff`

One coherent change to alias resolution. Each part is the native step it
mirrors; the later parts exist because the first changes what "one hop" means.

1. `symbols.rs::resolve_alias`: after the worker, a pure-alias target is
   resolved through `resolve_alias` and merged (`:16280`).
   `is_non_local_pure_alias` is `IsNonLocalAlias`.
2. The memo entry (`AliasTarget::Resolved`) also records the worker's own
   answer; `immediate_alias_target` reads it. It is native's first hop
   (`getImmediateAliasedSymbol`'s value, `:2155`), published in the same
   entry, so no second worker run. Consumers that walk a chain alias by alias
   read it instead of `resolve_alias`:
   - the two type-only walks (`isolated_alias.rs::type_only_alias_declaration_node(_ex)`,
     `check.rs::type_only_alias_declaration`). Native gets the same answer
     by copying `typeOnlyDeclaration` from each pure hop; TSR has no link
     table, and visiting the same hops is equivalent. Without this, TS1361 was
     lost in `importEquals1` and `exportDefault` (typeOnly).
   - `circular_alias.rs` (TS2303) walks `get_target_of_alias_symbol` (the
     uncached worker): a cycle now completes `resolve_alias` as `None`, so the
     one-hop walk is what still sees it. Without it, 15 TS2303 cases lost.
3. `getTargetOf*`'s `dontResolveAlias` as native passes it, so the first hop
   is native's first hop:
   - `getTargetOfImportClause` passes `true` (`:14531`): `module_default_target`
     gains the flag and returns the module's `default` alias unresolved; the
     specifier forms pass `false` and resolve only a pure alias
     (`resolveSymbolEx`). It used to resolve any alias, "one more hop".
   - `getTargetOfImportEqualsDeclaration`'s `require` arm passes `true`
     (`:14447`): the `export =` symbol itself, not its `resolve_alias`.
4. `getTargetOfExportSpecifier`'s local arm (`:14977`) is `resolveEntityName`,
   whose `getSymbol` (`:2176`) accepts an alias by its target's meaning:
   `export_specifier_target` uses `resolve_name_with_export_alias`, and the
   binder's exports arm (`tsr-binder/src/lib.rs`) asks the checker for an
   identifier import-equals (`import M_A = M_M`) as it already did for
   `= require(…)`. `namespace M { export import M_A = M_M; export { M_A as a } }`.
5. TS2437 (`check.rs::check_module_hidden_by_local`, `:5483`):
   `resolveEntityName(…, Value|Namespace, …, dontResolveAlias=false)` answers
   a found alias lacking the meaning through `resolveAlias`. Now forced: with
   (1), `import x = require("m"); import y = x` resolved `y` to the module and
   reported `x` as hiding it (4 cases lost). This is the same fix as the
   `check.rs` half of r6-modules3's held qualified-entity diff, which should
   drop that hunk when it is restacked (item 2).
6. `get_type_of_alias`'s value road reads through a pure-alias first hop.
   Native's chain can end at `resolveESModuleSymbol`'s cloned module symbol
   for an `import * as ns` hop (`cloneTypeAsModuleType`, `:15721`); TSR has
   no clone symbol (item 4) and applies the clone as the namespace-import
   alias's own type. Reading the end of the chain skipped that transform
   (`crashDeclareGlobalTypeofExport` `export as namespace foo` printed `Root`
   for `{ default: Root; }`). Retire this when item 4 lands clone symbols.
7. Tests: `symbols.rs`' naming test and
   `tsr-conformance/tests/module_default_file_owner.rs` pinned the first hop
   through `resolve_alias`; they now pin `immediate_alias_target`, and the
   naming test also pins `resolve_alias` = `Original`.

`resolve_alias_fully` is unchanged. Making it native's `resolveSymbol` (stop
at a merged target) is the rest of "over-walks merged alias+type-alias
symbols" (`noCrashOnImportShadowing` 2:0/6/8/10 `typeof B` for `typeof
OriginalB`); it has 30+ callers and was not measured.

Checker port convention: no new cache. The existing `aliasTarget` memo
gains its `immediateTarget` field (same key: the alias `SymbolId` as passed;
owner: one `Checker`; published together when the worker returns; `None`
while resolving; no receiver context). Work boundary unchanged: one worker per
alias.

### Measured

Against the base, unfiltered, both dumps, configured suites included:

- diagnostics **+6 cases** (`allowImportClausesToMergeWithTypes`,
  `mergeSymbolReexportInterface`, `moduleResolutionWithSymlinks`,
  `moduleResolutionWithSymlinks_withOutDir`, `chained`, `chained2`), **zero
  losses**. Counts 5,648 RIGHT / 5,606 EMPTY_RIGHT.
- types **+115 lines** (550,366 → 550,481 RIGHT), **zero losses**; largest:
  `es6ExportEqualsInterop` 10, `constEnumNoEmitReexport` 8,
  `declarationsForIndirectTypeAliasReference` 8,
  `importAliasAnExternalModuleInsideAnInternalModule` 6,
  `typeofAnExportedType` 6, the symlink pair 8, `privacyGloImport(ParseErrors)` 8.
- `slowcases` clean on both dumps.
- `cargo test --workspace --release` passes; `cargo fmt` clean; clippy (stable)
  reports only pre-existing code (`signatures.rs`, `symbols.rs:4496`'s nested
  fn, `enum_initializer.rs`, `index_signatures.rs`, `printing.rs`,
  `templates.rs`, `unique_symbols.rs`, `tsr-dts/tests/accessibility.rs`).
- **Perf: UNMEASURED.** The wrap-up came before the Ir/CPU run. The added
  work is one extra `resolve_alias` per pure-alias target (memoised) and the
  type-only walks reading the same memo; it needs the domain-model and
  generic-imports check before landing.

Applies on this branch plus BZ's four diffs (`r6-modules3.md` §7, 1–4).

## 2. Item 10 (`tsr-2zk.1258`): the multi-threaded CLI drop — not run

`checker_pool.rs` mirrors native ownership (`checkerpool.go`): a checker
publishes only diagnostics for files it owns. A single- vs multi-threaded
difference can only come from TSR reporting a diagnostic on a lazy path (type
computation reached from another file) that native reports from the owning
file's check walk. The experiment was prepared and not run: a materializer
writes the 2,149 multi-file conformance cases to disk and a runner diffs
`--singleThreaded` against `--checkers 4` per case (both scripts were in the
session scratchpad, `/tmp/box/mt/`, not committed). Still to do.

## 3. Diffs in apply order

1. `r6-modules4-resolve-alias-indirection.diff` (§1): +6 diagnostics cases,
   +115 type lines, 0 lost; perf UNMEASURED.

## 4. What remains, with causes

- **Item 1 remainder**: `noCrashOnImportShadowing` (`resolve_alias_fully` as
  `resolveSymbol`, above); `exportNamespace9`,
  `exportEqualsOfModule`, `enums`, `exportTypeMergedWithExportStarAsNamespace`
  (`typeof import("./Something")` naming: symbol chain, `tsr-2zk.39`, routed);
  `emitDecoratorMetadata_isolatedModules` 3:23 `t1.T1` naming (routed, `.39`).
- **Item 2** (`tsr-2zk.1256`, binder alias rule at module bodies + the
  qualified-entity diff): not started. r6-modules3's qualified-entity diff's
  `check.rs` hunk is now inside §1 and must be dropped when restacked; its
  `resolve_alias_fully` uses should become `resolve_alias`.
- **Items 3–9** (`.1180`, `.1181`, `.1194`, `.1210`, `.1208`, `.1211`,
  `.1257`): not started. Live counts on the base: `.1180` 9 diagnostics cases
  (the three `packageJsonImportsErrors` are already RIGHT); `.1181` 11 cases
  (§1.6 is its stand-in); `.1194` 9 cases (binder-level
  `merge_module_augmentations` follows `export =` only through a non-alias
  target); `.1210` 6 diagnostics cases, of which §1 converts
  `mergeSymbolReexportInterface`; `.1208` `chainedImportAlias`,
  `aliasInaccessibleModule2` and `es6ImportNamedImportInIndirectExportAssignment`
  were already RIGHT on the base and §1 converts
  `importAliasAnExternalModuleInsideAnInternalModule` (6) and the privacy
  pair (8); `declFileForExportedImport` (5) remains; `.1211` §1 converts
  `allowImportClausesToMergeWithTypes`.
- **Item 10**: §2.
- Routed, left alone: symbol-chain printing (`tsr-2zk.39`), module
  specifiers (r6-specifiers2).
