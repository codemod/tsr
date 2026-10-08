# Parity lane: r5-variants2 (tsr-2zk.965)

Configured rows after [ADR-0047](../../adr/0047-configuration-varied-cases-are-judged-per-configuration.md)
and [r4-variants](r4-variants.md): which of their failures are the harness's
and which are the checker's. Measured at the round-5 branch point `ac56208`
(vendor `5b1047d`). The harness causes found are fixed in
`crates/tsr-conformance`; the checker causes are ranked in §4 and are **not**
fixed here.

## 1. Method: what separates a harness cause from a checker cause

A configured row compiles `case(<configuration>)` with that configuration's
values applied by `CaseEntry::load`. A failure is the **harness's** when the
configuration's options do not reach the compilation the way native's harness
(`internal/testutil/harnessutil`: `CompileFiles`, `SetOptionsFromTestConfig`,
`getOptionValue`, `CompileFilesEx`) puts them there. Two probes were run over
every configured row of both frozen dumps (`diagverdictdump`, `verdictdump`,
keys `suite/case(<configuration>)`):

1. **Insensitivity.** For each pair of configurations of one case: is this
   port's output *identical* across the pair while native's expected output
   *differs*? That is the signature of an option that never reaches the
   compiler. Diagnostics: 138 such pairs (at least one side non-passing), over 47 cases. Types: 11 line
   pairs, over 3 cases. Each was then traced to whether the varied option
   reaches `CompilerOptions` (harness) or reaches it and is not read where
   native reads it (checker). **Every one** was the second kind: e.g.
   `jsx=react-jsx` reaches `options.jsx`, and TS2875 is not ported
   (`jsx_factory.rs` says so); `noImplicitAny=true` reaches the checker, and
   TS7018 for an object-literal property is not reported.
2. **Option plumbing, from native's side.** Every option in native's vary-by
   set (`configuration::VARY_BY`, 72 names) was looked up in
   `apply_test_directives`. **27 were dropped**: the harness wrote nothing for
   them, though `CompilerOptions` has a field for each (`moduleDetection`,
   `noPropertyAccessFromIndexSignature`, `noFallthroughCasesInSwitch`,
   `allowUmdGlobalAccess`, `erasableSyntaxOnly`, `emitDecoratorMetadata`,
   `noEmit`, `emitDeclarationOnly`, `rewriteRelativeImportExtensions`,
   `stableTypeOrdering`, `declarationMap`, `downlevelIteration`, …), and so
   were options outside the vary-by set (`noCheck`).
   Native has no such list: it applies every directive whose name is a
   declared option. That is a harness cause and is fixed (§2.1).
3. **The per-configuration trace rows** (§3)
   surfaced a third: list-valued directives were
   trimmed entry by entry where native keeps a string entry's whitespace
   (§2.2).

Also checked and found faithful: default lib per target
(`default_lib_file_name` against `targetToLibMap`, `enummaps.go:207`),
enum-value parsing (`ScriptTarget`/`ModuleKind`/`JsxEmit::parse` lowercase
as `getOptionValue` does), `had_error_baseline` for a configured entry (it
reads the *suffixed* stem, so a clean configuration of an erroring case is
not marked as erroring), and the root-file heuristic (shared with the plain
rows).

## 2. Harness causes fixed

### 2.1 Every directive naming an option is applied through the option table

`trace_case::apply_declared_directives` runs before the hand-written map in
`apply_test_directives`: each directive is looked up case-insensitively in
`tsr_tsoptions::declarations::COMPILER_OPTIONS` (`getCommandLineOption`,
`harnessutil.go:1151`), converted by kind (`getOptionValue`, `:417`) and
handed to the declaration's own setter (`ParseCompilerOptions`). The
hand-written map then re-reads the options it names and overrides, so every
option it already handled is set exactly as before — the change is confined
to the options it dropped (the 27 vary-by ones above, plus `noCheck` and any
other option the table declares).

Alternatives considered:

- **Replace the hand map with the table outright.** Rejected *for this lane*:
  the map carries decisions that are not the table's (`lib` kept as names,
  `rootDirs`/`typeRoots`/`outDir` made absolute against the harness's own
  current directory, the §-numbered rationale for each). Folding them is a
  refactor with no measured gain and a real risk of moving plain rows; with
  the table first and the map over it, the falsifier is simple — a plain row
  moving on an option the map names would mean the two disagree.
- **Add the dropped options to the hand map.** Rejected: that is a second list that drifts
  again the next time upstream or `CompilerOptions` gains an option.

Measured (full dumps, branch point → this change): **diagnostics plain +3**
(`compiler/noCheckDoesNotReportError`, `noCheckNoEmit`,
`noCheckRequiresEmitDeclarationOnly`: `EMPTY_WRONG` → `EMPTY_RIGHT`, because
`program_diagnostics::skip_type_checking` reads `no_check`), **configured
rows unchanged**, types unchanged, zero losses. Of the newly applied options
only `noCheck` is read by anything a judged row runs today (`strip_internal`
and `remove_comments` are read by declaration emit only); the rest now reach
`CompilerOptions` and wait for their readers. `moduleDetection` is the one that matters most and has no
reader in the parser/binder (`isFileForcedToBeModuleByFormat`); see §4.

### 2.2 List directives: `ParseListTypeOption`'s whitespace rule

`trace_case::parse_list_type_option` is `ParseListTypeOption`
(`tsoptions/commandlineparser.go:339`): the whole value is trimmed and split
on `,`; an entry of a *string* list is kept as written (dropped only when
empty), an entry of an *enum* list (`lib`, the only one upstream) is trimmed.
The harness trimmed every entry. `conformance/customConditions` sets
`// @customConditions: webpack, browser`, and native's trace records the
conditions `'webpack', ' browser'` — leading space and all. Both configured
traces failed on exactly that line before, and one `.types` line of
`customConditions(resolvepackagejsonexports=true)` was WRONG for the same
reason (the condition set decides which `exports` arm resolves).

Measured (with the §3 rows in place): `module_resolution_configured` 40/42 → **42/42**,
`file_loader_configured` 40/42 → **42/42**, types **+1 line**
(`conformance/customConditions(resolvepackagejsonexports=true):3:0`
WRONG → RIGHT), plain rows unchanged (no plain case writes a string list with
a space after the comma that any judged row depends on), zero losses.

## 3. `module_resolution` and `file_loader` judge varied traces per configuration

`trace_case::prepare` now skips a varied case as itself
(`CaseEntry::is_expanded`, the same predicate `diagverdictdump` uses) and
names the configured rows; for a configured entry `case.load()` has already
applied the configuration and `baseline_path` is the suffixed
`case(<configuration>).trace.json`, so the judgement needs no second path.
`ModuleResolutionConfigured` and `FileLoaderRequestsConfigured` are the two
suites over `Corpus::configured` (`Suite::per_configuration`), exactly as
`diagnostics_configured` is — both halves of a trace, so the two suites keep
sharing one denominator per population (`trace_case`'s module docs).

`module_suite::configured_tests` pins what the 14 varied `@traceResolution`
cases expand into: **42 configurations judged, 0 skipped by upstream's
option predicate, 1 empty trace** (`compiler/libReplacement(libreplacement=false)`:
without `libReplacement` the lib files do not go through the module resolver
and the case imports nothing else), and asserts no configuration is skipped
for any other reason. The plain-run bucket test is unchanged (14 varied).

| row | before | after |
|---|---:|---:|
| `module_resolution` | 95/95 | 95/95 |
| `module_resolution_configured` | — | **42/42** |
| `file_loader` | 96/96 | 96/96 |
| `file_loader_configured` | — | **42/42** |

## 4. Checker causes, ranked

Population at the branch point: `diagnostics_configured` 445 non-passing
rows (417 `WRONG`, 28 `EMPTY_WRONG`); `checker_types_configured` 1,887 WRONG
aligned lines. Diagnostics are bucketed by the code that differs (a row is
"sole" when every differing code is in that bucket, so fixing the bucket
alone converts it); types by case and by the shape of the difference. Counts
are rows (configurations), not cases.

### 4.1 Diagnostics

| # | root cause | native anchor | rows touched / sole | cases | owning file (TSR) |
|---:|---|---|---:|---:|---|
| 1 | Module-format grammar not gated by the file's emit format: TS1202/1203 (import/export assignment in ESM), TS1216 (`__esModule`), TS2441 (`require`/`exports` collision), TS2725 (`Object` class name), TS1295, TS1470 (`import.meta` in CJS), TS1309/2854/1378 (top-level `await`), TS7059/7060, TS18057, TS18060, TS1262, TS1192 | `checkImportEqualsDeclaration` `checker.go:5461`, `checkExportAssignment` `:5583`, `checkCollisionWithRequireExportsInGeneratedCode` `:10463`, `checkGrammarModuleElementContext` `grammarchecks.go:206`, `grammarchecks.go:1617,1718` | 94 / 71 | 41 | `tsr-checker/src/check.rs`, `grammar.rs` |
| 2 | Import attributes/assertions gated by `module`: TS2823, TS2856, TS2857, TS1454, TS1453 | `checkImportAttributes` `checker.go:5408` | 59 / 49 | 19 | `tsr-checker/src/check.rs` (not ported) |
| 3 | Target/`useDefineForClassFields`-gated class checks: TS2818 (super in static members), TS2373, TS2301, TS2729, TS2611/2610, TS2699, TS18037 | `checker.go:10606`, `:1857`, `:1525`, `:11722`, `checkKindsOfPropertyMemberOverrides` `:4536` | 39 / 34 | 23 | `tsr-checker/src/check.rs` |
| 4 | TS2875: automatic JSX runtime module does not resolve | `checkJsxPreconditions` `jsx.go:159`, `getJSXRuntimeImportSpecifier` `jsx.go:1488` | 17 / 16 | 7 | `tsr-checker/src/jsx_factory.rs` (documented as not ported) |
| 5 | Decorator grammar (TS1497, TS1239, TS1146 under `experimentalDecorators`) | `grammarchecks.go:190`, `checker.go:8791` | 14 / 8 | 12 | `tsr-checker/src/check.rs`, parser |
| 6 | Declaration-emit portability (TS2883, TS2694) | node builder / `checker.go` declaration diagnostics | 13 / 9 | 4 | `tsr-checker/src/printing.rs` |
| 7 | `.ts`-extension / arbitrary-extension imports (TS5097, TS2846, TS6142, TS6263) | `checker.go:15215-15257` | 11 / 7 | 5 | `tsr-checker/src/symbols.rs` (module resolution errors) |
| 8 | `isolatedModules`/`verbatimModuleSyntax` checks (TS1484, TS1289, TS2865/2866) | `checker.go:6813`, `:5647` | 7 / 3 | 3 | `tsr-checker/src/check.rs` |
| 9 | Regular-expression scanner by target (TS1530, TS1508, TS1518, …) | `scanner/regexp.go` | 6 / 1 | 5 | `tsr-scanner` |
| 10 | TS17006 (unary `-` on the left of `**`) | `parser.go:4701` | 5 / 4 | 5 | `tsr-parser` |
| — | long tail (TS2339 31, TS2322 27, TS2304 13, …, each case-specific) | — | 397 rows touch some code outside 1–10 | — | — |

### 4.2 Types

| # | root cause | native anchor | WRONG lines | cases | owning file (TSR) |
|---:|---|---|---:|---:|---|
| 1 | Printed module specifiers in types (`import("./node_modules/inner/other.js").Thing` printed as `Thing`), with the awaited namespace of a dynamic import reading `any` | `getSpecifierForModuleSymbol` `nodebuilderimpl.go:1249`; `getAwaitedType` `checker.go:31253` | 375 | 31 | `tsr-checker/src/printing.rs` |
| 2 | `import.meta` unimplemented (`ImportMeta` reads `any`) | `checkMetaProperty` `checker.go:10753`, `checkImportMetaProperty` `:10782`, `getGlobalImportMetaExpressionType` `:24682` | 197 | 4 | `tsr-checker/src/expressions.rs` (not ported) |
| 3 | String-literal module export names (`export { x as "<X>" }`) resolve to `any` | `getExternalModuleMember` `checker.go:14667` (with `ModuleExportName` string names) | 190 | 1 | `tsr-checker/src/symbols.rs` |
| 4 | JSON modules under `node16`+ ESM: synthetic `default` shape (`{ default: {…} }`) and `any` members | `getTypeWithSyntheticDefaultOnly` `checker.go:15632`, `getTypeWithSyntheticDefaultImportType` `:15646` | 184 | 2 | `tsr-checker/src/symbols.rs` |
| 5 | ESM importing CJS dynamically: missing synthetic `default` on the namespace | same as 4 | 64 | 4 | `tsr-checker/src/symbols.rs` |
| 6 | Other node16-module-format shapes (import attributes on types, `defer`) | — | 38 | 4 | — |
| — | long tail (`inKeywordTypeguard` 44, `objectLiteralErrors` 30, `privateNameInInExpression` 28, `octalIntegerLiteral` 27, `bigintWithoutLib` 27, …) | — | 839 | 141 | — |

Buckets 1–5 of §4.2 are 1,010 of 1,887 lines in 42 cases: the configured
types population is `module`-varied node16/nodenext cases far more than the
plain one, which is why its line rate trails.

## 5. Not fixed here, and why

- **The types producer's current directory is `/`, native's is `/.src`**
  (`types_producer::program_and_config_for_case`, §539 there). For a case
  mixing absolute and relative unit names the two put the units in different
  directories relative to each other. Seven corpus cases do that without
  `@currentDirectory`; none of their configured rows differed in a way this
  explains, and the function sits beside the render/writer guards another
  lane owns this round, so it is recorded rather than changed.
- **The newly applied options with no reader** (§2.1) are checker/parser
  work: `moduleDetection` in the parser's external-module indicator,
  `noPropertyAccessFromIndexSignature` (TS4111), `noFallthroughCasesInSwitch`
  (TS7029), `allowUmdGlobalAccess` (TS2686), `erasableSyntaxOnly` (TS1294).
- **The tsr binary is unchanged** by this lane (every edit is in
  `crates/tsr-conformance`), so the §5 perf gate does not apply.

## 6. How we would know this is wrong

- A plain row moves on an option the hand-written map names: the table and
  the map disagree about that option, and the map's reading is suspect.
- `module_suite::configured_tests` changes bucket sizes, or names a
  configuration skipped for a reason other than upstream's option predicate.
- A future insensitivity probe (§1, probe 1) finds a pair where the varied option
  does not reach `CompilerOptions`: a harness cause this lane missed.
