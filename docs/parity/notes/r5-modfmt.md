# r5-modfmt: module-format checks (tsr-2zk.1002: `.985`, `.993`)

Lane `r5-modfmt`, round 5 (`docs/parity/round5.md`). Source population:
`docs/parity/notes/r5-variants2.md` §4.1 row 1 — diagnostics upstream gates on
a file's **emit module format** or on `c.moduleKind`, 94 configured rows over
41 cases at that lane's branch point. Pinned upstream: `vendor/typescript-go`
@ `5b1047d`.

## 1. Shape of the change

Every check lives in `crates/tsr-checker/src/module_format.rs`, reached from
**one hook** in the check walk (`check_node` → `check_module_format`, after
`check_unreachable`, with the node's own `ambient` context) plus two arms in
`check.rs` (the `export =` tail of `check_export_assignment_alone` and the
TS1192 synthetic-default conjunct). Each check is a position test on the node
the walk hands it plus, where upstream asks the program, one `ModuleHost`
query. No cache, side table or traversal is added (the checker port
convention has nothing to record).

**The program seam already existed.** ADR-0041's `ModuleHost` carries
`implied_node_format_for_emit` (`Program.GetImpliedNodeFormatForEmit`,
`program.go:1554`), implemented on `tsr_compiler::Program` over the loader's
`implied_node_format_for_emit` (`ast.GetImpliedNodeFormatForEmitWorker`).
`emit_module_format_of` composes `GetEmitModuleFormatOfFile` from it the way
`ast.GetEmitModuleFormatOfFileWorker` does. A checker without a host answers
`None`, which is what a program with no per-file formats answers.

**Options.** `ModuleFormatOptions` (one field on `Checker`, filled in
`apply_compiler_options` — ADR-0042's copy-what-you-read pattern) carries
`GetEmitModuleKind()`, `GetEmitModuleDetectionKind()` and `noEmit`. A checker
that never sees options keeps `module_kind: None` and the hook is inert, so
every `Checker::new` call site without a program behaves as before.

## 2. What was ported

| code | upstream | TSR |
|---|---|---|
| TS1202 | `checkImportEqualsDeclaration` external arm, `checker.go:5494` | `check_import_equals_module_format` |
| TS1203 / TS1218 | `checkExportAssignment` `export =` tail, `:5669` | `check_export_equals_module_format` |
| TS2441 | `checkCollisionWithRequireExportsInGeneratedCode` `:10463`, `checkCollisionWithGlobalObjectInGeneratedCode` `:10482` | `check_collisions_in_generated_code` |
| TS2725 | `checkClassNameCollisionWithObject` `:10611` | `check_class_name_collision_with_object` |
| TS1216 | `checkGrammarVariableDeclaration` marker arm, `grammarchecks.go:1600`/`:1614` | `check_es_module_marker` |
| TS1375/1378/1309, TS2853/2854, TS1431/1432 | top-level arms of `checkGrammarAwaitOrAwaitUsing` `:1689` and `checkGrammarForInOrForOfStatement` `:1205` | `check_top_level_await` |
| TS7059 / TS7060 | `checkAssertion` `checker.go:12288`, `checkGrammarArrowFunction` `grammarchecks.go:772` | `check_reserved_type_assertion`, `check_reserved_arrow_type_parameters` |
| TS18057 | `checkModuleExportName` `checker.go:5388` | `check_module_export_name` |
| TS1192 | `canHaveSyntheticDefault`'s usage block `:14818`, `exportDefaultSymbol` via `resolveExportByName` `:14551` | `check_module_has_default_export` (check.rs) |

### 2.1 Judgement calls

- **`c.moduleKind` is not the checker's shared `module_kind`.** That field
  maps an unset `module` at `target >= es2015` to `ES2015`; upstream's
  `GetEmitModuleKind` (`core/compileroptions.go:202`) climbs to
  `ES2020`/`ES2022`/`ESNext` with the target. Measured: with the shared field,
  `compiler/awaitInNonAsyncFunction` (`@target: esnext`, no `@module`) lost
  RIGHT → WRONG on two invented TS1378/TS1432 (upstream's kind there is
  `ESNext`, which permits top-level await). The lane's checks read
  `options.emit_module_kind()` instead. Fixing the shared field is outside
  this lane (it feeds every other `module_kind` reader); see §5.
- **TS1202 is gated on `c.moduleKind`, not the file's format** — a `.cts`
  under `module: esnext` still reports (`impliedNodeFormatEmit1`'s `/i.cts`).
  Ported as written.
- **TS1203's ambient conjunct is now ported.** The old arm declined every
  ambient file (§783 in `checker-notes-diag2.md`) for want of
  `impliedNodeFormat`; with the seam, `other.d.mts` reports and a `.d.cts`
  does not, as upstream.
- **The collision checks hook declarations, not a statement scan.** Upstream
  calls `checkCollisionsForDeclarationName` from seven `checkXxx`
  functions; both TS2441 arms need `GetDeclarationContainer` to be the source
  file, which only top-level declarations (and binding elements rooted in a
  top-level variable) have. The walk visits each of those node kinds, so the
  hook tests the container exactly as upstream does rather than enumerating
  top-level statements.
- **`await using` is found from the statement.** This port's parser flags an
  `await using` list `USING` and drops the `await`; `grammar.rs` already
  recovers it as the gap between a modifier-less statement's start and its
  list's start, and `await_using_keyword_start` reuses that rule. A
  `for (await using …)` initializer has no such gap and is not reported.
- **`AwaitContext`.** Upstream sets it on top-level statements it reparses
  (`reparseTopLevelAwait`), which are only the ambiguous `await (x)` /
  `await [x]` spellings this parser leaves as identifiers; an
  `AwaitExpression` built at top level never carries it, so the arm runs for
  every one the walk finds.
- **`IsEffectiveExternalModule` with `moduleDetection`.** The top-level-await
  arm asks whether the file is a module *as the parser decides it*
  (`GetExternalModuleIndicatorOptions`, `ast/parseoptions.go:19`): import/export
  syntax, `force`, or under `auto` a file `isFileForcedToBeModuleByFormat`.
  `is_effective_external_module_for_format` ports that predicate for the
  checker's question; the `auto` + `react-jsx` arm needs "contains a JSX tag"
  and **declines** (no report) rather than guess. The binder still decides
  module-ness from syntax alone (§5).
- **TS7060's trailing comma** is read from source text after the single type
  parameter (the parser records no trailing comma on type-parameter lists),
  on the error path only, and declined without a host that supplies text.
- **TS1192 asks the usage-aware `canHaveSyntheticDefault`** that symbols.rs
  already had (`can_have_synthetic_default_for_usage`), and its
  `exportDefaultSymbol` conjunct now goes through `resolve_export_by_name`
  (made `pub(crate)`), as upstream's `resolveExportByName` does. Switching only
  the first lost `conformance/exportAssignmentOfExportNamespaceWithDefault`
  (an `export =` of a namespace with a `default` member reads as having a
  default upstream); the second conjunct is what fixes it.

### 2.2 Option readers (`tsr-2zk.993`)

r5-variants2 (64d9890) made the harness apply every declared option; these
are the readers that were missing. All in `module_format.rs`, gated on one
`ModuleFormatOptions` field each.

| option | code | upstream | note |
|---|---|---|---|
| `allowUmdGlobalAccess` | TS2686 | `errorOrSuggestion(AllowUmdGlobalAccess != TSTrue, …)`, `checker.go:1846` | the report becomes a suggestion, which no list here carries; one guard in `check_umd_global_reference` |
| `erasableSyntaxOnly` | TS1294 | `shouldCheckErasableSyntax` at `checker.go:2667`, `:5076`, `:5165`, `:5469`, `:5595`, `:12293` | parameter properties, enums, instantiated namespaces, `import =`, `export =`, `<T>` assertions; the assertion span runs to the operand's full start, found by stepping back over whitespace |
| `noFallthroughCasesInSwitch` | TS7029 | `checkCaseBlock` tail, `checker.go:4196` | **no flow.rs edit was needed**: the binder already records `FallthroughFlowNode` (`BindResult::fallthrough_flow`) and `is_reachable_flow_node` is `pub(crate)` |
| `noPropertyAccessFromIndexSignature` | TS4111 | `checker.go:11361`, inside the property-access index-signature arm | **not built**: its only home is `access_member_lookup`'s index-info arm (`members.rs:855`), a type lookup that is re-entered for one node, so a report there needs a report-once guard in main's active file. One row (`noPropertyAccessFromIndexSignature1`); left to the members lane |
| `moduleDetection` | — | `GetExternalModuleIndicatorOptions` | §5: a binder change, shipped as a measured diff |

## 3. Measured

Frozen baseline `d64a532a` → this commit, full dumps, unfiltered:

- **diagnostics: 54 rows convert** (49 `WRONG → RIGHT`, 5 `EMPTY_WRONG →
  EMPTY_RIGHT`); 50 configured, 4 plain (`collisionExportsRequireAndAlias`,
  `es6ImportEqualsDeclaration`, `esmModeDeclarationFileWithExportAssignment`,
  `nodeNextCjsNamespaceImportDefault1`). By cause: TS1202 11, TS2441/TS2725/
  TS1216 collisions 18, TS1203 5, top-level await 14, TS7059/7060 4, TS1192 7
  (some rows need two causes).
- **Zero losses** on the diagnostics dump (no `RIGHT`/`EMPTY_RIGHT` row
  changed) and on the types dump (no `RIGHT` line changed).
- **Perf** (median child CPU, new/old, against the baseline binary):
  domain-model 0.994 (21 samples); generic-imports 1.031 at 21, **1.019 at
  41**. `diagnostics_match: true` on both.

The checks run per node in the existing walk; each is a kind match plus, on
the matching kinds only, a handful of node-table reads. The host queries
(`implied_node_format_for_emit`, `file_path`) are reached only after a name
or option test has already matched (`require`/`exports`/`Object`/`__esModule`
names, `.mts`/`.cts` syntax, top-level `await`, `import x = require`).

## 4. Remaining in the population

| codes | rows | hypothesis | where |
|---|---:|---|---|
| TS1470 (`import.meta` in CJS) | 8 | ported by r5-modules (`7b8f2448`, `import_meta.rs`), not on this branch | r5-modules |
| TS1262 extras, TS2552 | ~10 | `await [x]` / `await (x)` at module top level: the parser does not `reparseTopLevelAwait` (`parser.go:513`) | main's parser lane |
| TS1262 missing (`export function await()`) | 2 | the module-scope reserved-word check for a declaration name | `strict_mode.rs` |
| TS2854 in `for (await using …)` | — | the parser drops `await` from the list | parser |
| TS1295 + TS1484 | 4 | `checkAliasSymbol`'s `verbatimModuleSyntax` block (`checker.go:6800-6845`); TS1484/TS2865/TS2866 unported beside it (r5-variants2 bucket 8) | `check.rs` alias checks |
| TS18060 (`import.defer()` under non-esnext) | 4 | `checkGrammarImportCallExpression` `grammarchecks.go:2167`; needs r5-modules' `import.<name>` MetaProperty parse | after r5-modules merges |
| TS18057 rows | 2 | ported; the rows also differ on TS2322 | — |

## 5. Changes outside this lane's files

- **`Checker::module_kind` default** (`checker.rs`, `apply_compiler_options`):
  should be `options.emit_module_kind()` (upstream `GetEmitModuleKind`).
  Today an unset `module` at `target >= es2015` reads `ES2015`. This lane
  reads its own copy; switching the shared field would move every other
  reader (TS1323's `== ES2015` arm, `emit_helpers.rs`, symbols' Node16 tests)
  and needs its own measured commit.
- **`moduleDetection` in the binder** (`tsr-2zk.993`): **shipped as a
  measured diff, `r5-modfmt-module-detection.diff`** (applies on `aabe910b`),
  because it edits `Program::bind_source_files`, which main's perf lane
  changed in the last 36 hours (`fcf22922`). `tsr_binder`'s
  `is_external_module(file)` is statement-only, so under `force` (every
  `node16`..`nodenext` program) and, under `auto`, for files
  `isFileForcedToBeModuleByFormat`, a file without imports/exports bound as
  a script where upstream binds a module. The diff:
  - `loader::force_module_indicator`: `GetExternalModuleIndicatorOptions(...).Force`
    (`ast/parseoptions.go:19`); the `JSX` arm (a file with a JSX tag under
    `auto` + `react-jsx`) is not ported.
  - `tsr_binder::bind_file_forcing_module` / `bind_into_with_jsdoc_forcing_module`:
    new entry points rather than a `FileInfo` field, because `FileInfo` is
    built by literal at ~40 sites including the conformance harness.
  - In the binder, `getExternalModuleIndicator`'s order (module syntax; not
    a declaration or JSON file; then `Force`), and
    **`setCommonJSModuleIndicator`'s `ExternalModuleIndicator != file`
    conjunct** (`binder.go:928`): a forced module still takes a CommonJS
    indicator. The first measurement without that conjunct lost 18 type
    lines (`nodeModulesAllowJsCjsFromJs` ×4 configurations,
    `modulePreserve4`), all `module.exports` in forced JS files.

  Measured against `aabe910b` (full dumps): diagnostics **+2**
  (`compiler/moduleDetectionIsolatedModulesCjsFileScope`,
  `compiler/sideEffectImports3(moduledetection=force,nouncheckedsideeffectimports=true)`,
  both `EMPTY_WRONG → EMPTY_RIGHT`), types unchanged, zero losses on both
  dumps; perf (median child CPU, 21 samples, against `aabe910b`'s binary)
  domain-model 0.948, generic-imports 1.017; `tsr-binder` and
  `tsr-compiler` tests pass.

## 6. How we would know this is wrong

- A plain or configured row gains TS1202/TS1203/TS2441/TS2725/TS1216 in a
  file whose emit format is ESM: the host's `implied_node_format_for_emit`
  and upstream's disagree for that file.
- A top-level-await code appears in a `node16`+ file with no imports or
  exports that upstream treats as a module: the binder/indicator split in §5
  leaking into the checker's predicate.
- A TS1192 change in a non-Node module kind: `can_have_synthetic_default_for_usage`'s
  usage block answering where upstream's `usageMode` is `None`.
