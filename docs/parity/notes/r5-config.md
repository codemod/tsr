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
| `tsxSpreadChildrenInvalidType` | jsx=react-jsx: missing TS7026 ×6 | global `JSX` declared, no `react/jsx-runtime` | under `react-jsx` native resolves the JSX namespace from the implicit import source (`getJsxNamespaceContainerForImplicitImport`), finds none, and every intrinsic element is implicitly `any`; TSR falls back to the global `JSX` | rule | `jsx_intrinsic.rs` |
| `emitDecoratorMetadata_isolatedModules` | module=esnext: missing TS1272 ×3 | type-only names in decorated signatures | `markDecoratorAliasReferenced` → `markEntityNameOrEntityExpressionAsReference(…, forDecoratorMetadata)` (`checker.go:28686`, `:28857`). **Not ported**: nothing in the checker reads `emitDecoratorMetadata` | rule (missing) | new module + `check.rs` hook |
| `exportDeclaration` | isolatedModules=true: missing TS1289 | `export = A` of a type-only import | `checkAliasSymbol`'s `GetIsolatedModules()` arm (`checker.go:6800`–`6850`). **Not ported** in `symbols.rs::check_alias_symbol` | rule (missing) | `symbols.rs` (main) |
| `isolatedModulesSketchyAliasLocalMerge`, `isolatedModulesShadowGlobalTypeNotValue` | every configuration: missing TS2865/TS2866 (isolatedModules) and TS1484/TS1295 (verbatimModuleSyntax) | | same `checkAliasSymbol` arm: TS2865/TS2866 read `IsolatedModules.IsTrue()` directly, TS1484 and the CommonJS-file TS1295 read `VerbatimModuleSyntax` | rule (missing) | `symbols.rs` (main) |
| `bundlerSyntaxRestrictions` | module=preserve: extra TS2309 | `export = {}; export {};` | `checkExternalModuleExports`: `hasExportedMembers` is false (no export but `export=`) | rule | `check.rs` (main) |
| `declarationFileForHtmlImport`, `declarationFilesForNodeNativeModules` (×3) | allowArbitraryExtensions=false: missing TS6263 (node modes: TS2306 instead) | `import "./file.html"` resolved to `file.d.html.ts` | `resolveExternalModule`'s arbitrary-extension arm (`checker.go`, `Module_0_was_resolved_to_1_but_allowArbitraryExtensions_is_not_set`). **Not ported**: no TSR file names the message | rule (missing) | `module_*.rs` (shared) |
| `bundlerImportTsExtensions` | allowImportingTsExtensions=false: also missing TS5097 ×5 | `.ts` specifiers | `resolveExternalModule`'s `ResolvedUsingTsExtension && !AllowImportingTsExtensionsFrom(file)` arm (`checker.go:15238`). **Not ported**; TS2846/TS6142 missing in every configuration | rule (missing) | `module_*.rs` (shared) |
| `importTag15` | es2015 missing TS2823, esnext missing TS2857 | JSDoc `@import … with {…}` | `checkImportAttributes` on a JSDoc import tag | rule | JS (r5-js) |
| `nodeModulesJson` | node18+: also missing TS1543 ×3 | JSON import without `with { type: "json" }` | `checkImportAttributes`' Node18+ JSON arm | rule | `import_attributes.rs` |
| `regExpWithOpenBracketInCharClass` | es2015: also missing TS1501 | `/[[]/v` | the scanner's regular-expression flag check against `languageVersion` | rule | scanner (main) |
| `regularExpressionScanning` | target-dependent TS1501/TS1503 lines on top of a uniform scanner gap | | same | rule | scanner (main) |

Of the 27, **one is harness plumbing** (the `message` category, fixed below).
The other 26 are option-gated rules whose arm is missing or wrong. One
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
