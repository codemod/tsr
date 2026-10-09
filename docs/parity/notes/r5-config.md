# r5-config — configuration-dependent failures and option plumbing

Round-5 lane on epic `tsr-2zk`: `tsr-2zk.1108` (failures that depend on the
configuration) and `tsr-2zk.1087` (the types producer's compile root). Owns
the compiler-options module (`crates/tsr-core/src/options.rs`), the
harness's per-configuration application and compile root
(`crates/tsr-conformance`), and this file. Native source is
`vendor/typescript-go` @ `5b1047d`.

**Frozen base:** `fafecee` (integration head when the box started).
Diagnostics 11,035 right (5,440 RIGHT + 5,595 EMPTY_RIGHT) of 12,238 rows;
of those rows, 2,422 are configured (`case(opt=val)`, ADR-0047) and 2,173 of
them are right (89.7%). Types 548,907 RIGHT of 556,291 aligned lines. Every
number below is from unfiltered `diagverdictdump` / `verdictdump` runs
against that base, compared with `cut -f1,2`.

## 1. Which configured failures depend on the configuration

### Method

A configured row is one compilation (ADR-0047), so "depends on the
configuration" is measurable: group a case's configured rows, compute each
row's (missing, extra) diagnostic multiset against its own baseline, and ask
whether the rows disagree. If every configuration of a case misses and adds
exactly the same diagnostics, the failure is a property of the source, not of
any option, and belongs to whichever lane owns that rule. The script is
`cfgsplit.py` (kept outside the repo; it reads only the dump's columns 1–4).

At the base, 167 varied cases have at least one configured row that is not
right:

- **27 are configuration-dependent**: the rows' diffs differ (table below).
- **140 are uniform**: every configuration has the same diff. Most are a
  single surviving configuration (`target=es5` is skipped upstream), so the
  test cannot separate them; §3 lists the ones whose failing rule reads an
  option anyway.

The brief's list was taken at the batch-AH base. Of its 19 cases,
`sideEffectImports3` and `dynamicImportDefer` are now right in every
configuration; the other named cases are in the table.

### The 27 configuration-dependent cases

"Rule" is the native function whose output differs; "where" is the file that
owns the TSR side. **Plumbing** means the option does not reach the rule
correctly; **rule** means the option arrives and the option-gated arm itself
is missing or wrong. Except for the fixed row, the "native rule" column is a
diagnosis read from the native source and the case, not a measured fix; the
owning lane confirms it.

| Case | Configurations wrong (of total) | Missing / extra | Native rule | Kind | Where |
|---|---|---|---|---|---|
| `importAttributes1`, `importAssertion1` | esnext extra TS1450 ×2; commonjs extra TS1450, missing TS1009 | TS1450 is a **`message`**-category line; the harness dropped it from the baseline | `errors_baseline::parse` | harness | **fixed, §2** (esnext rows); commonjs keeps TS1009 (trailing comma in `import(…, …,)`, `checkGrammarForDisallowedTrailingComma`, grammar) |
| `contextuallyTypedOptionalProperty` | eopt=true: extra TS18048 | `y => y > 0` sees `y: number \| undefined` | contextual type of an optional property under `exactOptionalPropertyTypes` keeps `missingType`, which `getTypeOfPropertyOfContextualType`/`removeMissingType` strips | rule | `contextual.rs` (main) |
| `discriminateWithOptionalProperty4` | eopt=false: missing TS18048 | `z.a.toString()` receiver | property-access `checkNonNullExpression` (r5-operators3 §3) | rule | property access (main) |
| `intersectionsAndOptionalProperties2`, `…3` | eopt=true: TS2322 where native says TS2412 | head message | `reportRelationError`'s exact-optional arm (`Type_0_is_not_assignable_to_type_1_with_exactOptionalPropertyTypes_Colon_true…`) | rule | `assignreport.rs` / `relater.rs` (r5-relater7) |
| `multipleBaseInterfaesWithIncompatibleProperties2` | eopt=true: missing TS2320 | `Partial<{port:number}>`'s `port` vs `port?: number \| undefined` | the homomorphic mapped optional property's type is `addOptionality(…, isOptional)` = `number` + `missingType` under eopt, so it is not identical to `number \| undefined` | rule | `mapped.rs` (r5-mapped5); `heritage_conformance.rs` is right for its input |
| `deleteExpressionMustBeOptional_exactOptionalPropertyTypes` | eopt=true: missing TS2790 at 27, 30; both: extra TS2790 at 37, 39 | | `checkDeleteExpressionMustBeOptional`: under eopt, an optional property whose declared type contains `undefined`-as-missing is not deletable | rule | `delete_operand.rs` |
| `controlFlowAliasingCatchVariables` | useUnknownInCatchVariables=true: missing TS18046 | `e.toUpperCase()` on `unknown` | property-access `checkNonNullExpression` (r5-operators3 §1) | rule | property access (main) |
| `destructureCatchClause` | useUnknownInCatchVariables=true: also missing TS2339/TS2488 at lines 2, 3, 11 | destructuring an `unknown` catch variable | binding-pattern element types from `unknown`; the rest is uniform | rule | destructure (main) |
| `functionsWithImplicitReturnTypeAssignableToUndefined` | strictNullChecks=true: missing TS2366 at 28 | `function f5(): {}` with an implicit return | `checkAllCodePathsInNonVoidFunctionReturnOrThrow`: `undefined` is not assignable to `{}` under strictNullChecks | rule | `flow.rs` (main) |
| `usingDeclarationsWithObjectLiterals2` | noImplicitAny=true: missing TS7018 ×2 | `using x = { [Symbol.dispose]() {…}, y: null }` | `reportWideningErrorsInType` for a `using` initializer | rule | widening report (main) |
| `inKeywordTypeguard` | strict=true: also missing TS18046 at 155 | `"a" in x`, `x: unknown` | `checkInExpression`'s `checkNonNullType` (r5-operators3 §2 diff) | rule | `assignreport.rs` (diff held by the integrator) |
| `jsxFragmentFactoryReference` | jsx=react: missing TS2879 | `<></>` with no `React` in scope | `checkJsxFragment` → `getJsxFactoryEntity`'s fragment arm, `React` namespace symbol not found | rule | JSX (r5-jsx lanes) |
| `tsxSpreadChildrenInvalidType` | jsx=react-jsx: missing TS7026 ×6 | file-level `declare namespace JSX`, no `react/jsx-runtime` | under `react-jsx` a file with a JSX tag is an **external module** (`getExternalModuleIndicator`'s JSX arm), so its `namespace JSX` is module-local and `getJsxNamespaceAt`'s global fallback finds nothing. *Corrected:* the first version of this row blamed the implicit-import road in `jsx_intrinsic.rs`; that road is faithful | plumbing | **fixed, §5** (`tsr-compiler` loader) |
| `emitDecoratorMetadata_isolatedModules` | module=esnext: missing TS1272 ×3 | type-only names in decorated signatures | `markDecoratorAliasReferenced` → `markEntityNameOrEntityExpressionAsReference(…, forDecoratorMetadata)` (`checker.go:28686`, `:28857`). **Not ported**: nothing in the checker reads `emitDecoratorMetadata` | rule (missing) | new module + `check.rs` hook |
| `exportDeclaration` | isolatedModules=true: missing TS1289 | `export = A` of a type-only import | `checkExportAssignment`'s `GetIsolatedModules()` arm (`checker.go:5583`, the TS1289 report at `:5639`). *Corrected:* the first version of this row named `checkAliasSymbol`; its arm (§6) does not reach `export =` | rule (missing) | `checkExportAssignment` |
| `isolatedModulesSketchyAliasLocalMerge`, `isolatedModulesShadowGlobalTypeNotValue` | every configuration: missing TS2865/TS2866 (isolatedModules) and TS1484/TS1295 (verbatimModuleSyntax) | | same `checkAliasSymbol` arm: TS2865/TS2866 read `IsolatedModules.IsTrue()` directly, TS1484 and the CommonJS-file TS1295 read `VerbatimModuleSyntax` | rule (missing) | `symbols.rs` (main); **diff, §6** (SketchyAliasLocalMerge ×3 converted) |
| `bundlerSyntaxRestrictions` | module=preserve: extra TS2309 | `export = {}; export {};` | `checkExternalModuleExports`: `hasExportedMembers` is false (no export but `export=`) | rule | `check.rs` (main) |
| `declarationFileForHtmlImport`, `declarationFilesForNodeNativeModules` (×3) | allowArbitraryExtensions=false: missing TS6263 (node modes: TS2306 instead) | `import "./file.html"` resolved to `file.d.html.ts` | `resolveExternalModule`'s arbitrary-extension arm (`checker.go`, `Module_0_was_resolved_to_1_but_allowArbitraryExtensions_is_not_set`). **Not ported**: no TSR file names the message | rule (missing) | `module_*.rs` (shared) |
| `bundlerImportTsExtensions` | allowImportingTsExtensions=false: also missing TS5097 ×5 | `.ts` specifiers | `resolveExternalModule`'s `ResolvedUsingTsExtension && !AllowImportingTsExtensionsFrom(file)` arm (`checker.go:15238`). **Not ported**; TS2846/TS6142 missing in every configuration | rule (missing) | `module_*.rs` (shared) |
| `importTag15` | es2015 missing TS2823, esnext missing TS2857 | JSDoc `@import … with {…}` | `checkImportAttributes` on a JSDoc import tag | rule | JS (r5-js) |
| `nodeModulesJson` | node18+: also missing TS1543 ×3 | JSON import without `with { type: "json" }` | `checkImportAttributes`' Node18+ JSON arm | rule | `import_attributes.rs` |
| `regExpWithOpenBracketInCharClass` | es2015: also missing TS1501 | `/[[]/v` | the scanner's regular-expression flag check against `languageVersion` | rule | scanner (main) |
| `regularExpressionScanning` | target-dependent TS1501/TS1503 lines on top of a uniform scanner gap | | same | rule | scanner (main) |

Of the 27, **two are plumbing**: the harness's `message` category (§2) and
the loader's module detection under `react-jsx` (§5), both fixed. The other
25 are option-gated rules whose arm is missing or wrong. One
plumbing divergence that none of the 27 isolates is in the checker's own
option reads: `module_kind` (§3).

## 2. `message`-category baseline lines were dropped (fixed)

`errors_baseline::parse` read a header line only when it began
`error TS` or `warning TS`. Native writes the category with `Category.Name`
(`internal/diagnostics/diagnostics.go:29`), which also produces `message`
and `suggestion`. TS1450 (*Dynamic imports can only accept a module
specifier and an optional set of attributes as arguments*) is a `Message`
diagnostic that `checkGrammarImportCallExpression` reports into the same list
as every error, and this port's `import_call.rs` already reports it. The
baseline side lost it, so `importAttributes1(module=esnext).errors.txt` —
whose only lines are two TS1450 — read as an empty baseline, and every
faithful TS1450 scored as an extra.

The corpus has 9 `message` header lines (all TS1450, 6 baselines) and 36
`suggestion` lines (all TS6807, one baseline, `compiler/overshifts`).
`message` is now read. `suggestion` is not: this port has no TS6807
producer, so reading it would turn `overshifts` (RIGHT today) WRONG for a
missing producer, not a harness fact. When TS6807 is ported, add
`suggestion TS` to `strip_category` in the same commit.

**Measured** against `fafecee`: diagnostics +3 rows
(`importAssertion1(module=esnext)`, `importAttributes1(module=esnext)`
EMPTY_WRONG → RIGHT; `importCallExpressionGrammarError` WRONG → RIGHT), zero
losses, no key missing; types byte-identical in columns 1–2 (548,907 RIGHT);
`slowcases` clean on both dumps. The change is harness-only: the `tsr`
binary is not rebuilt by it, so there is no performance number to take.

## 3. The checker's option reads, audited against `NewChecker`

`Checker::apply_compiler_options` (`checker.rs`, main's) against
`NewChecker` (`checker.go:905`–`960`) and the `core.CompilerOptions`
getters (`internal/core/compileroptions.go:195`–`370`), which
`tsr-core/src/options.rs` ports. The getters match native
(`GetEmitScriptTarget`, `GetEmitModuleKind`'s ladder,
`GetModuleResolutionKind`, `GetResolveJsonModule`, `GetStrictOptionValue`,
`GetIsolatedModules`, `ShouldPreserveConstEnums`,
`GetEmitModuleDetectionKind`, `GetEffectiveTypeRoots`). The reads:

| Field | Native | TSR | Verdict |
|---|---|---|---|
| `module_kind` | `GetEmitModuleKind()` (`checker.go:915`) | hand-derived: `module`, else `target >= ES2015 ? ES2015 : CommonJS` on the **raw** `target` | **diverges** for every case with no `@module`: an unset target gives `CommonJS` where native gives `ES2022`; `es2020`+ give `ES2015` where native gives `ES2020`/`ES2022`/`ESNext` |
| `standard_class_fields` | `GetUseDefineForClassFields()` | same; the ES2022 leg of `GetEmitStandardClassFields` is applied by `class_fields.rs::get_emit_standard_class_fields` | faithful (the field name says "emit standard" but holds the use-define value) |
| `allow_synthetic_defaults` | not read by the pinned checker | computed, read by nothing | dead field |
| `allow_importing_ts_extensions` | `GetAllowImportingTsExtensions()` = flag **or** `rewriteRelativeImportExtensions`; TS5097 uses `AllowImportingTsExtensionsFrom` (adds declaration files) | the flag alone | diverges, but only `module_specifiers.rs` reads it and no TS5097 producer exists (§1) |
| strict family, `noImplicitOverride`, `noImplicitReturns`, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`, unused, unreachable, `isolatedModules`, JSX namespace/factory | as native | as native | faithful |

**`module_kind`, measured.** `r5-config-module-kind.diff` replaces the
hand derivation with `options.emit_module_kind()`. Against `771746f`:
both dumps **byte-identical** (columns 1–4 of the diagnostics dump; the
types dump with its guard columns stripped). 666 corpus files set neither
`@module` nor `@target` and 536 more set only an ES2020+ target, so the
divergence is reachable; none of the 32 `module_kind` reads produces a
different diagnostic or type line on them. It is a faithful port with no
measured gain and no loss, shipped as a diff because `checker.rs` is
main's; it removes a trap for the next rule that reads `module_kind` on an
unset-module case (`import_meta.rs`'s ES2020 test is one).

## 4. `tsr-2zk.1087`: the producer compiles in `/.src` (held diff)

### The constraint

The native runner names every unit `GetNormalizedAbsolutePath(name,
currentDirectory)` with `currentDirectory` defaulting to `srcFolder`, `/.src`
(`compiler_runner.go:521`, `createHarnessTestFile`). `types_producer` (and so
the `diagnostics` suite, which builds its program through
`program_and_config_for_case`) compiled in `/`. That is invisible for a unit
the case names absolutely and for every printed name (baselines strip
`/.src/` through `removeTestPathPrefixes`), and visible wherever a path is
*derived* from the current directory. The one measured case is
`compiler/referenceTypesPreferedToPathIfPossible`: `@types: *` loads
`/.src/node_modules/@types/node` only because `GetEffectiveTypeRoots` walks up
from `/.src` (r5-align §2.4).

### What the move costs on its own

`CURRENT_DIRECTORY = "/.src"` in `types_producer.rs`, and the two places in
`diagnostics_suite.rs` that re-derived the producer's directory read that
constant, measured unfiltered against `771746f`:

- diagnostics: byte-identical (columns 1–4);
- types: +5 RIGHT and +3 newly aligned RIGHT lines in the target case, and
  **356 RIGHT → WRONG lines in 26 cases**, every one the same shape: native
  `import("./ConstEnum").MyConstEnum`, TSR the bare `MyConstEnum`. (Eleven
  more lines turn RIGHT in `constEnumNoEmitReexport` and `exportNamespace1`
  only because the same arm now declines where native happens to print the
  bare name; they go back with the companion change and are not counted.)

The losses come from one heuristic in `checker.rs`
(`symbol_to_string`'s file-module arm, §106 of `checker-notes-narrow.md`):
it spells `import("./name").` only when the module symbol's name is a single
segment under `/` (`module_name.strip_prefix('/')` with no further `/`),
because module names are absolute paths and every unit used to sit at `/`.
Under `/.src` every unit's name is `/.src/name`, so every such module looked
nested and the arm declined. The same root assumption sits in the no-host
fallback of `module_specifier_for_symbol_in_mode` (`./name` when the name has
one segment under `/`); it does not fire in the harness, which always has a
host.

### The companion change

`r5-config-src-root.diff` makes the heuristic's stem relative to the
reference file's directory when the module is under it, and relative to `/`
otherwise. When the reference file's directory *is* `/` this is the old
computation exactly, so at the old root the only reachable difference is a
reference file in a subdirectory with the module beside it.
`file_mentions_module_specifier` compares the stem with the written
specifier minus `./`, so it reads the same stem unchanged.

| Measured against `771746f` | diagnostics | types RIGHT | lost RIGHT lines | missing RIGHT keys |
|---|---|---|---|---|
| heuristic alone, root `/` | identical | 548,907 (=) | 0 | 0 |
| `/.src` alone | identical | 548,570 (−337) | 356 | 0 |
| **both (the diff)** | identical | **548,915 (+8)** | **0** | **0** |

The heuristic alone changes one WRONG line's text
(`caseInsensitiveFileSystemWithCapsImportTypeDeclarations:0:2`/`:3`, a
reference under `/repo/src`: `import("./types").Merge<…>` for
`Merge<…>`; native prints `TypeB` for both). With both, the only verdict
changes are the eight `referenceTypesPreferedToPathIfPossible` lines: `0:0`–`0:2`
WRONG → RIGHT, `0:3`–`0:4` GAP → RIGHT, `1:0`–`1:2` new and RIGHT.
`slowcases` is clean on both dumps; `cargo test -p tsr-conformance` passes.

### Why it is a diff, not a commit

The losses are fixed in `checker.rs`, which this lane must not touch, and
the harness half without it loses 356 lines. The two land together or not
at all. `@currentDirectory` directives keep their meaning: they are resolved
against the default (`printed_name` already resolved them against `/.src`).

**Falsifier.** If a later case shows native resolving a *relative* path
against `/` — a type root, a `paths` base, a relative `@currentDirectory` —
the default is wrong for that suite and the move should be reverted there.

## 5. Module detection under the automatic JSX runtime (fixed)

`GetExternalModuleIndicatorOptions` (`ast/parseoptions.go:19`) has two
option-driven arms under `moduleDetection: auto`: `Force`
(`isFileForcedToBeModuleByFormat`, ported as
`loader::force_module_indicator`) and `JSX`, set for `jsx: react-jsx` and
`react-jsxdev`, which makes any non-declaration file holding a JSX tag a
module (`isFileModuleFromUsingJSXTag`, `:122`). The loader's doc comment
recorded the JSX arm as not ported. Without it, a `.tsx` file with no
`import` stayed a script under the automatic runtime, so its
`declare namespace JSX` was global, `getJsxNamespaceAt`'s global fallback
(`jsx.go:1334`) found `IntrinsicElements`, and every intrinsic tag was
typed where native reports TS7026.

The port: `loader::jsx_tags_force_module` is the option half;
`Program::force_module_indicator` adds the tag half, a scan of the file's
node kinds for `JsxOpeningElement`, `JsxSelfClosingElement` or
`JsxFragment` (`walkTreeForJSXTags`). The scan runs only for a file this
parser reads with JSX (`ScriptKind::allows_jsx`, `.tsx`/`.jsx`): no other
file can contain a tag here, which is what native's `SubtreeContainsJsx`
pruning answers for those files without walking them. (This port parses
`.js` without JSX, a parser difference outside this lane; if that changes,
the gate must widen with it.) The bool feeds the binder exactly as
`Force` does; native's precedence (statement indicators first, declaration
files never) is preserved because the binder adds the statement indicators
itself and the predicate excludes declaration files.

**Measured** against `771746f`, unfiltered:

- diagnostics +1: `tsxSpreadChildrenInvalidType(jsx=react-jsx,target=es2015)`
  WRONG → RIGHT; zero losses, no key missing;
- types +12 RIGHT lines (548,907 → 548,919), all in the same configuration,
  which becomes a passing `checker_types_configured` entry; zero losses;
- `slowcases` clean on both dumps;
- perf, median child CPU over 21 samples, new/base: domain-model 0.988,
  generic-imports 1.015, `diagnostics_match: true` (neither project has a
  `.tsx` file, so the scan never runs there).

Tests: `crates/tsr-compiler/tests/jsx_module_detection.rs` (tag and
fragment under both automatic runtimes; classic runtime, `preserve`, an
untagged file and `moduleDetection: legacy` stay scripts).

Only one configured case moved, although more vary `@jsx` across the
classic and automatic runtimes. Why each of the others is insensitive was
not checked case by case; a file that already imports something is a
module either way, and a `JSX` declared in a separate `.d.ts` does not
depend on the `.tsx` file's module-ness.

## 6. `checkAliasSymbol`'s isolatedModules / verbatimModuleSyntax arms (held diff)

`symbols.rs::check_alias_symbol` ported only the head of `checkAliasSymbol`
(`checker.go:6736`): resolution and the TS2440/TS2441 conflict arm. Every arm
after it (`:6788`–`6858`) was missing, and nothing else in the checker
reported their codes:

| Code | Arm | Option read |
|---|---|---|
| TS2865 | an import whose local symbol also has a value (`appearsValueyToTranspiler`) | `IsolatedModules.IsTrue()` itself, not `GetIsolatedModules()` |
| TS1484 / TS1485 / TS1288 | an import naming a type / a type-only declaration / an internal `import =` of a type | `VerbatimModuleSyntax` |
| TS1269 | `export import` of a type | `GetIsolatedModules()` |
| TS1205 / TS1448 | re-exporting a type / a type-only declaration | `GetIsolatedModules()`; `VerbatimModuleSyntax` or a type-only declaration in another file |
| TS1295 / TS1286 | ESM syntax in a CommonJS-format file | `VerbatimModuleSyntax`, the file's emit format |
| TS1293 | ESM syntax in a CommonJS file under `module: preserve` | `moduleKind` |
| TS2748 | importing an ambient const enum | `VerbatimModuleSyntax` |

That is a family of option-gated rules, all absent, which is why the
`isolatedModules*`/`verbatimModuleSyntax*` cases failed in every
configuration (§1 counted the configured ones as dependent, since their
expected codes differ per configuration).

`r5-config-isolated-alias.diff` ports them in a new
`crates/tsr-checker/src/isolated_alias.rs`, called from the end of
`check_alias_symbol` with the pieces that function already resolved
(local symbol, target, target flags, whether the conflict arm reported). It
adds two raw option reads to `Checker` (`isolated_modules_option`,
`verbatim_module_syntax`; the existing `isolated_modules` is
`GetIsolatedModules()`). Helpers it needed and did not find:
`IsTypeOnlyImportOrExportDeclaration`, `getTypeOnlyAliasDeclaration` as a
node (check.rs's `type_only_alias_declaration` answers only which kind),
`IsInternalModuleImportEqualsDeclaration`, `getIsolatedModulesLikeFlagName`
and `getVerbatimModuleSyntaxErrorMessage`. The ambient test is
`declaration_is_in_an_ambient_context` (this parser never sets
`NodeFlagsAmbient`). `addTypeOnlyDeclarationRelatedInfo` is not attached:
related information is outside the oracle and the call sites have no
related list.

**Measured** against `6449b75`, unfiltered:

- diagnostics +4 rows: `isolatedModulesReExportType` and the three
  `isolatedModulesSketchyAliasLocalMerge` configurations, WRONG → RIGHT;
  zero losses, no key missing;
- 12 rows changed their reported list; **every added diagnostic is in its
  baseline** and none was removed (checked row by row: the other 8 rows stay
  WRONG on codes outside this arm);
- types byte-identical; `slowcases` clean;
- perf, median child CPU over 21 samples, new/base: domain-model 0.979,
  generic-imports 1.018, `diagnostics_match: true`;
- `cargo test --workspace --release` passes; the diff adds
  `crates/tsr-compiler/tests/isolated_alias.rs` (TS2865 under
  `isolatedModules`, TS1484 + TS1295 under `verbatimModuleSyntax`, nothing
  under neither).

What still blocks the rest of the family, not this arm:

- `isolatedModulesShadowGlobalTypeNotValue`: TS2866 is
  `checkIdentifier`'s "conflicts with global value used in this file", a
  different function; `good.ts`'s TS1295 is on an alias whose target is
  `export = globalThis.console`, which `resolve_alias` does not resolve, so
  `check_alias_symbol` returns before any arm.
- `exportDeclaration(isolatedmodules=true)`: TS1289 is
  `checkExportAssignment`'s arm (`export = A` of a type-only import), not
  `checkAliasSymbol`.
- `isolatedModulesAmbientConstEnum`, `verbatimModuleSyntaxAmbientConstEnum`:
  the remaining TS2748 is the property-access arm (`E.X` on an ambient const
  enum, `checkPropertyAccessExpression`).
- `isolatedModulesExportImportUninstantiatedNamespace` (TS1269 on an
  uninstantiated namespace), `isolatedModulesExportDeclarationType` (TS1292),
  `verbatimModuleSyntaxNoElision*` (TS1282–TS1285): `checkExportSpecifier` /
  `checkExportAssignment` arms.

## 7. Summary and routing

**Committed** (branch head `9ca32e6` plus this section): §2 (harness,
`errors_baseline.rs`) and §5 (loader, `tsr-compiler`). `coverage` at the
head against the snapshots committed at `fafecee`:

| Row | `fafecee` snapshot | head |
|---|---:|---:|
| `checker_types` | 8,376 / 9,538 | 8,376 / 9,538 |
| `checker_types_configured` | 1,683 / 1,928 | 1,684 / 1,928 |
| `diagnostics` | 4,594 / 5,502 | 4,595 / 5,502 |
| `diagnostics_configured` | 846 / 1,089 | 849 / 1,091 |

`diagnostics_configured`'s denominator grows by two because the two
`(module=esnext)` rows of §2 now have a non-empty baseline.

**Held diffs**, each measured alone on the stated base, zero losses:

| Diff | Base | Gain | Why held |
|---|---|---|---|
| `r5-config-module-kind.diff` | `771746f` | none (byte-identical dumps) | `checker.rs` |
| `r5-config-src-root.diff` (`.1087`) | `771746f` | +8 type lines (`referenceTypesPreferedToPathIfPossible`) | `checker.rs` heuristic must land with the harness half (§4) |
| `r5-config-isolated-alias.diff` | `6449b75` | +4 diagnostics rows | `symbols.rs`, `checker.rs`, `lib.rs` |

**Routed** (option-gated rules whose arm is missing or wrong; §1 has the
evidence per case):

- `resolveExternalModule`'s resolution-diagnostic branch
  (`module.GetResolutionDiagnostic`, `module/util.go:125`) is ported only for
  its JS/JSX/TSX arms (`check.rs::check_untyped_module_import`, which also
  returns early for any resolution that produced a module symbol). TS6263
  (`allowArbitraryExtensions`, 4 configured rows) and TS5097
  (`AllowImportingTsExtensionsFrom`, `checker.go:15238`) need the branch to
  run on every resolved file, with the resolved extension. Under the node
  modes TSR answers TS2306 for the `.d.html.ts` file instead, so the
  resolution's module-ness differs too. Owner: `check.rs` / `module_*.rs`.
- `markDecoratorAliasReferenced` → `markEntityNameOrEntityExpressionAsReference`
  (`checker.go:28686`, `:28857`) is not ported; nothing reads
  `emitDecoratorMetadata`. TS1272 in `emitDecoratorMetadata_isolatedModules`.
  A new module plus a `check.rs` hook on decorated declarations.
- `checkExportAssignment` / `checkExportSpecifier`'s isolatedModules arms
  (TS1289, TS1292, TS1282–TS1285); `checkIdentifier`'s TS2866; the
  property-access TS2748 (§6's remainder).
- exactOptionalPropertyTypes: contextual optional-property type
  (`contextual.rs`), the mapped optional modifier's `missingType`
  (`mapped.rs`), `reportRelationError`'s TS2412 head (`assignreport.rs` /
  `relater.rs`), `checkDeleteExpressionMustBeOptional` (`delete_operand.rs`).
- `useUnknownInCatchVariables`, `strictNullChecks`, `noImplicitAny` rows:
  property-access TS18046/TS18048, `flow.rs`'s TS2366 for `{}`, and a
  `using` initializer's widening report (TS7018).
- `jsx: react`: TS2879 for a fragment with no `React` in scope.
- `bundlerSyntaxRestrictions(module=preserve)`: an extra TS2309 for
  `export = {}; export {};` (`checkExternalModuleExports`'
  `hasExportedMembers`).

`allow_synthetic_defaults` on `Checker` is computed and read by nothing; the
pinned checker does not read `allowSyntheticDefaultImports` at all.
