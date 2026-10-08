# Exact full-corpus oracle (lane: oracle)

Beads: tsr-2zk.47.3, tsr-2zk.47.1, tsr-2zk.47.2, tsr-2zk.47.3.1.

## Commands

```sh
scripts/parity_gate.sh oracle-native /tmp/oracle/native                 # once per native identity
scripts/parity_gate.sh oracle-tsr /tmp/oracle/native /tmp/oracle/base   # committed base tree
scripts/parity_gate.sh oracle-tsr /tmp/oracle/native /tmp/oracle/cand   # committed candidate
scripts/parity_gate.sh oracle-compare /tmp/oracle/base /tmp/oracle/cand
```

`oracle-native` = `full_oracle_run native DIR [--workers N] [--deadline S] [--filter SUBSTRING]`;
`oracle-tsr` = `full_oracle_run tsr NATIVE_DIR DIR [--workers N] [--deadline S]
[--filter SUBSTRING] [--per-process] [--allow-dirty]`. Output directories must be
empty: nothing resumes. Native: `identity.tsv`, `plan.tsv`, `native.tsv`,
`cases/NNNNN/{request,native}.tsv` (console output kept only for failures).
TSR: `identity.tsv`, `results.tsv`, `summary.md`, `workers/N.stderr`, and
`cases/NNNNN/{native,actual}.tsv` only for non-exact cases (`actual.stderr`
for failures).

`oracle-compare` (`full_oracle_run gate`) prints one `LOST`, `MISSING` or
`GAINED` line per case and exits 1 when a key `EXACT` in the base is not `EXACT`
in the candidate or is absent from it (tsr-2zk.47.2). It refuses reports that
are not complete TSR reports or whose native identity (revisions, native
producer sources, native binary, plan, native results, native deadline and
filter) or TSR deadline and filter differ.

### One-command integrator gate (macOS or Linux)

`exact-candidate` = build, `oracle-tsr` into a fresh report, `oracle-compare`
against the base report. Exit 0: no EXACT loss; 1: a `LOST`/`MISSING` line (or
incomparable reports); 2: the TSR run failed (log in `<out>.log`).

```sh
# once per machine: Go matching vendor/typescript-go/go.mod, the pinned Rust.
brew install go                       # macOS; any Go >= go.mod's version
git submodule update --init --recursive
cd "$(git rev-parse --show-toplevel)"   # every command runs inside the checkout

# once per native identity (pinned tsgo + corpus + Go producer sources):
scripts/parity_gate.sh oracle-native ~/oracle/native

# once per base commit (main), from a clean checkout of it:
scripts/parity_gate.sh oracle-tsr ~/oracle/native ~/oracle/main-<sha>

# per candidate, from a clean checkout of the candidate:
scripts/parity_gate.sh exact-candidate ~/oracle/native ~/oracle/main-<sha> ~/oracle/cand-<sha>
echo $?
```

Workers default to every core (`--workers N` overrides). The native directory
is refused, not silently reused, once the pinned revisions or the Go producer
sources change: re-run `oracle-native`. A filtered base (`--filter S`) only
compares with a candidate run under the same filter.

`full_oracle_run rank REPORT [TOP]` prints the root-cause ranking below.

## Identity (`identity.tsv`)

Native run: pinned native revision and clean checkout, corpus submodule
revision and clean checkout, SHA-256 of the two Go producer sources and of the
frozen native test binary (copied read-only into `bin/`), deadline, filter,
worker environment, plan SHA-256, `native.tsv` SHA-256 (each row carries its
artifact's SHA-256), `status` (`running` → `complete`; a failed discovery
writes `discovery-<outcome>` and publishes no population).

TSR run: refuses a native directory that is not `complete`, whose native or
corpus revision differs from the checkout's, whose Go producer sources changed
since, or whose binary, plan or results no longer hash to the recorded values;
every native artifact is re-hashed before it is compared. Records the source
commit (a dirty `crates/`, `xtask/`, `Cargo.*` aborts unless `--allow-dirty`,
which is recorded), the native directory and the SHA-256 of its identity, the
inherited native keys, the frozen TSR worker binary's SHA-256, `tsr_mode`,
deadline, filter, results SHA-256 and `status`.

## Population (expected-independent)

`full_oracle_native.go` (a `go test -overlay` of the pinned
`internal/testrunner`, checkout untouched) emits the plan in `runCompilerTests`
order: regression then conformance runner, `EnumerateTestFiles` order, a
source's configurations sorted by name (`GetFileBasedTestConfigurations`
builds them from a Go map range, so its own order is random per process; the
set is not). `skippedTests` rows are `LISTED_SKIP`; a configuration expansion
that fails (`t.Fatal`/panic) is one `DISCOVERY_FAILED` row. Discovery is
bounded (900 s); timeout or failure aborts the run. No committed baseline is
read by either producer.

## Per-case outcome

Native, once per native identity (bounded process, default deadline 60 s, kill
and reap): `getCompilerFileBasedTest` → `newCompilerTest` →
`SkipUnsupportedCompilerOptions` (skipped → `NATIVE_SKIPPED`) → records of
`c.result.Diagnostics` and `DoTypeAndSymbolBaseline`'s walk;
`TS_TEST_PROGRAM_SINGLE_THREADED=true` (one checker, the test default).

TSR, per candidate: `full_oracle::actual` with the native configuration map
verbatim. Case program = `types_producer::program_and_config_for_case` at
`/.src`; one configured checker; `compileFilesWithHost` collection (config,
parse, JS syntax, `bind_and_check_diagnostics` with the program's bind
diagnostics, include processor, isolated-declaration diagnostics) then
`sort_and_deduplicate_located_diagnostics`; type rows via
`types_producer::render_file` through the same checker, `hadErrorBaseline =
!diagnostics.is_empty()`, over the harness `toBeCompiled ++ otherFiles` filtered
to loaded files (JSON included), skipped by `@noTypesAndSymbols`.

Default `serve` mode: one long-lived `full_oracle_actual --serve` per worker
slot, one case per request line, each case on a fresh thread (no thread-local
state crosses cases; the only process-wide input is the immutable bundled-lib
text). A per-case deadline kills the worker (`TSR_TIMEOUT`); a panic or abort
ends the worker, the case is `TSR_FAILED` with that case's stderr, and the slot
starts a new worker, so no case runs after another case's unwinding.
`--per-process` runs one process per case; the two modes must publish
byte-identical artifacts (checked over the full corpus, below).

## Exactness

Exact = equal complete record streams: every diagnostic in published order
(file, UTF-16 start/length, code, category, flattened message), its metadata,
message chain and related information recursively; every `.types` section and
row (file, placement line, node text, type text). Rows plus the unit text
determine the `.types` file, so equal rows are equal baselines. Comparisons are
raw except the printed file name, which both sides pass through
`removeTestPathPrefixes`; TSR's `/.ts-lib/` lib mount is spelled as the native
`bundled:///libs/` first. A file-less diagnostic's position is published as `-`
on both sides (no baseline or CLI prints it; native uses -1 or 0). Symbols
baselines, JS/declaration emit and source maps are outside this oracle.

## Failure classes

First difference, diagnostic half before type half. Diagnostic half: equal
groups in another order → `order-only`; else by `(file, start, len, code)`
multisets: only native → `missing`, only TSR → `extra`, both with the same
`(file, code)` → `span-only`, same spans → `code-at-same-span`, else
`missing+extra`; same locations: chain records → `chain`, related records →
`related-info`, category → `category`, head text → `message-text-only`, else
`metadata`. Type half: section list → `section-set`/`no-sections`; first
differing row → `type-text` (same node and line), `extra-row`/`missing-row`
(one-row shift), `line-placement`, `node-selection`. Details carry the code or
`native -> tsr` type text.

## Measurement at `c4bd3a6d` (origin/main `23711d1d` + oracle; 2026-10-08, 14 workers, 60 s deadlines)

Native run: native `5b1047d1`, corpus `4d4f005c`, Go producer
`decc2e4b…` / `59076d08…`, native binary `807a9a1e…`, plan `04db1d44…`
(reproduced byte-identically by a second discovery), native results
`83802b0f…`; 368 s wall, once. TSR report: worker `9ca79c73…`, results
`af5af5cf…`.

**Exact 7,884 / 12,797 = 61.61%** (7,698 at `52c2759a`; the gate against the
report of origin/main `d779b044` lists 147 gains, 0 losses). Population 14,960
rows: 14,915 configured cases + 45 `skippedTests`; 2,118 configurations are
`SkipUnsupportedCompilerOptions` skips (ES5 target, AMD/UMD/System,
node10/classic resolution, `baseUrl`, `outFile`, `esModuleInterop`/
`allowSyntheticDefaultImports`/`alwaysStrict` false). Both excluded sets have
no native baseline. 0 discovery failures, 0 native failures or timeouts.

| outcome | cases |
|---|---|
| EXACT | 7884 |
| WRONG | 4910 |
| TSR_TIMEOUT | 2 (`recursiveConditionalCrash3`, `relationComplexityError`) |
| TSR_FAILED | 1 (`constructorWithIncompleteTypeAnnotation`: reversed span 6665..6663) |
| NATIVE_SKIPPED (excluded) | 2118 |
| LISTED_SKIP (excluded) | 45 |

WRONG halves: diagnostics only 2,738, types only 1,111, both 1,061.

| primary class | cases | top details |
|---|---|---|
| diag:missing | 1684 | TS2322 177, TS2339 87, TS2345 68, TS2741 43 |
| types:type-text | 1109 | any -> error 117, number -> any 42, number -> error 29 |
| diag:chain | 520 | TS2322 286, TS2345 38, TS2430 32, TS2416 25 |
| diag:related-info | 438 | TS2403 45, TS2741 41, TS2554 36, TS2322 35 |
| diag:missing+extra | 339 | TS2322 37, TS2353 17, TS2345 15, TS2304 10 |
| diag:extra | 307 | TS2304 68, TS2322 51, TS1254 44, TS18048 10 |
| diag:span-only | 288 | TS2322 136, TS1163 12, TS1206 12, TS2390 10 |
| diag:message-text-only | 180 | TS2339 46, TS2314 20, TS2411 18, TS2322 10 |
| diag:code-at-same-span | 43 | TS2322 4, TS2741 4, TS1109 3, TS2693 3 |
| types:section-set | 2 | `augmentExportEquals2`, `referenceTypesPreferedToPathIfPossible` |

Type half (2,172 WRONG cases with a type difference): type-text 2,139,
node-selection 22 (parser recovery), missing-row 5, extra-row 4, section-set 2.
First type-text difference by answer: both print a type 1,239; TSR `any` where
native has a type 455; TSR `error` 396; native `any`/`error`, TSR a type 39;
same union members in another order 11.

| code | missing | extra | same location |
|---|---|---|---|
| TS2322 | 424 | 283 | 470 |
| TS2345 | 120 | 33 | 91 |
| TS2339 | 141 | 15 | 84 |
| TS2304 | 57 | 122 | 1 |
| TS2741 | 73 | 9 | 68 |
| TS2552 | 6 | 89 | 46 |
| TS1005 | 33 | 24 | 41 |
| TS2403 | 15 | 0 | 66 |
| TS2554 | 29 | 7 | 45 |
| TS2769 | 40 | 3 | 27 |
| TS7006 | 56 | 9 | 4 |
| TS2353 | 55 | 2 | 8 |
| TS1254 | 0 | 48 | 0 |

Systemic next lanes, by cases: (1) assignability reporting — missing
TS2322/TS2345/TS2741 heads and elaboration chains; (2) type answers — 851
first rows where TSR prints `any`/`error`; (3) related information (438); (4)
TS2322 error-node spans (136 span-only); (5) TS2304/TS2552 extras (211) and
program/option diagnostics TSR has no producer for (TS5053, TS5055, global
TS2318).

## Gate cost and mode identity

`serve` and `--per-process` reports at the previous main (`d779b044` + oracle)
are byte-identical on all 14,960 rows (outcome, classes, details, native and
TSR artifact SHA-256); wall 954 s vs 1,529 s on 14 workers. Native artifacts
are reused; the per-candidate cost is the TSR run alone: median 1.06 s per
case, p99 1.55 s, about 13,200 worker-seconds; 910 s wall at `c4bd3a6d`.

That cost is TSR checking the bundled libraries. The native harness never
does: `CompileFiles` defaults `SkipDefaultLibCheck` to true
(`harnessutil.go:99`), and `program_and_config_for_case` does not. With that
default ported (plus `createProgram`'s `SingleThreaded`) the run takes 125 s
wall at `c4bd3a6d` (0.1 s per case; serve/per-process byte-identical at the
previous main), so a 16-core gate fits in 10 minutes. The port is **held**: it
gains `genericPrototypeProperty2` and `plainJSReservedStrict` but loses
`compiler/sliceResultCast.ts`, whose EXACT depends on checking `lib.es5.d.ts`
first. For `x: [number, string] | [number, string, string]`, native prints
`x.slice` as a union of two distinct instantiated signatures (also with
`@skipDefaultLibCheck: false`); TSR prints that union only when
`lib.es5.d.ts` was checked before the case, and one signature otherwise. The
union projection (`get_type_of_property_with_this_argument`, `members.rs`) must
keep one instantiation per tuple receiver (`getTypeWithThisArgument`)
independent of check order; that is a checker fix outside this lane. The
legacy dumps (`parity_gate.sh compare`) show no transition from the held
port.

## Root causes ranked (`c4bd3a6d` report)

`full_oracle_run rank` attributes every non-exact case to its first
difference (`full_oracle::blocker`, diagnostic half first): the missing
(native-side) or extra (TSR-side) diagnostic, the moved diagnostic for
span/code classes, the first differing chain (`C`) or related (`R`) record for
`diag:chain`/`diag:related-info` (code `TSa in TSb` = record `a` under head
`b`), or the first differing type row. A case counts once. Buckets are
`(class, code, native operation, node<parent>)`:

- diagnostics: the native operation is every non-editor tsgo function that
  names the record's message (`native_emitters`, scanned from the pinned
  sources). For relation codes (TS2322/2345/2741, chain codes) the emitter is
  `reportRelationError`/`reportError`, so the node column names the caller:
  `Identifier<VariableDeclaration>` = `checkVariableLikeDeclaration`'s
  initializer check, `~ReturnStatement` = the return-statement check (`~`: no
  node has exactly the native span, here the `return` keyword);
- type rows: `getTypeOfNode`'s dispatch for the walked node (a declaration name
  reads `getTypeOfSymbol`, anything else `check<Kind>`) and the answer (`TSR
  error`, `TSR any`, `native any`, `both types`).

Node kinds come from TSR's parse of the native configuration; nothing here
feeds a verdict. 4,913 non-exact cases fall into 1,623 buckets; the top 30
cover 1,344.

| # | cases | class | code | native operation | node<parent> | side | examples |
|---|---|---|---|---|---|---|---|
| 1 | 173 | types:type-text |  | getTypeOfSymbol (VariableDeclaration name) [both types] | Identifier<VariableDeclaration> |  | `compiler/badInferenceLowerPriorityThanGoodInference.ts`, `compiler/circularObjectLiteralAccessors.ts (target=es2015)`, `compiler/collisionArgumentsInType.ts (alwaysstrict=true)` |
| 2 | 125 | diag:span-only | TS2322 | checker.reportRelationError | ~ReturnStatement<Block> | native | `compiler/accessors_spec_section-4.5_error-cases.ts`, `compiler/arrayAssignmentTest1.ts`, `compiler/arrayAssignmentTest5.ts` |
| 3 | 101 | types:type-text |  | getTypeOfSymbol (FunctionDeclaration name) [both types] | Identifier<FunctionDeclaration> |  | `compiler/anonClassDeclarationEmitIsAnon.ts`, `compiler/arrayFlatNoCrashInference.ts`, `compiler/arrayFlatNoCrashInferenceDeclarations.ts` |
| 4 | 73 | types:type-text |  | getTypeOfSymbol (TypeAliasDeclaration name) [both types] | Identifier<TypeAliasDeclaration> |  | `compiler/aliasOfGenericFunctionWithRestBehavedSameAsUnaliased.ts`, `compiler/computedTypesKeyofNoIndexSignatureType.ts`, `compiler/conditionalTypeGenericInSignatureTypeParameterConstraint.ts` |
| 5 | 69 | diag:missing | TS2339 | checker.checkPropertyAccessExpressionOrQualifiedName / checker.getIntrinsicTagSymbol / checker.getPropertyTypeForIndexType +4 more | Identifier<PropertyAccessExpression> | native | `compiler/accessorInferredReturnTypeErrorInReturnStatement.ts`, `compiler/controlFlowInstanceof.ts`, `compiler/extension.ts` |
| 6 | 62 | types:type-text |  | getTypeOfSymbol (VariableDeclaration name) [TSR error] | Identifier<VariableDeclaration> |  | `compiler/arrayFromAsync.ts`, `compiler/classNonUniqueSymbolMethodHasSymbolIndexer.ts`, `compiler/computerPropertiesInES5ShouldBeTransformed.ts (target=es2015)` |
| 7 | 57 | diag:extra | TS2304 | checker.checkGrammarPrivateIdentifierExpression / checker.getCannotFindNameDiagnosticForName | Identifier<TypeReference> | tsr | `compiler/callsOnComplexSignatures.tsx`, `compiler/contextuallyTypedJsxAttribute2.tsx`, `compiler/contextuallyTypedJsxChildren.tsx` |
| 8 | 51 | diag:chain | TS2322 | checker.reportRelationError | Identifier<BinaryExpression> | native | `compiler/conditionalTypeVarianceBigArrayConstraintsPerformance.ts`, `compiler/controlFlowForStatementContinueIntoIncrementor1.ts`, `compiler/errorMessageOnIntersectionsWithDiscriminants01.ts` |
| 9 | 45 | diag:related-info | TS6203 in TS2403 | checker.addDuplicateDeclarationError / checker.errorNextVariableOrPropertyDeclarationMustHaveSameType | Identifier<VariableDeclaration> | native | `compiler/augmentedTypesVar.ts`, `compiler/capturedLetConstInLoop14.ts (target=es2015)`, `compiler/duplicateIdentifierInCatchBlock.ts` |
| 10 | 43 | diag:extra | TS1254 | checker.checkAmbientInitializer | TrueKeyword<VariableDeclaration> | tsr | `conformance/node/allowJs/nodeModulesAllowJsConditionalPackageExports.ts (module=node16)`, `conformance/node/allowJs/nodeModulesAllowJsConditionalPackageExports.ts (module=node18)`, `conformance/node/allowJs/nodeModulesAllowJsConditionalPackageExports.ts (module=node20)` |
| 11 | 42 | diag:message-text-only | TS2339 | checker.checkPropertyAccessExpressionOrQualifiedName / checker.getIntrinsicTagSymbol / checker.getPropertyTypeForIndexType +4 more | Identifier<PropertyAccessExpression> | native | `compiler/allowSyntheticDefaultImports10.ts`, `compiler/autolift4.ts`, `compiler/detachedCommentAtStartOfFunctionBody1.ts` |
| 12 | 37 | diag:missing | TS2322 | checker.reportRelationError | Identifier<VariableDeclaration> | native | `compiler/aliasDoesNotDuplicateSignatures.ts`, `compiler/deepComparisons.ts`, `compiler/enumAssignmentCompat.ts` |
| 13 | 34 | diag:missing | TS5055 | compiler.verifyCompilerOptions | -<-> | native | `compiler/declarationFileOverwriteError.ts`, `compiler/jsFileCompilationAbstractModifier.ts`, `compiler/jsFileCompilationAmbientVarDeclarationSyntax.ts` |
| 14 | 34 | types:type-text |  | getTypeOfSymbol (VariableDeclaration name) [TSR any] | Identifier<VariableDeclaration> |  | `compiler/arraySigChecking.ts`, `compiler/callbacksDontShareTypes.ts`, `compiler/contravariantOnlyInferenceWithAnnotatedOptionalParameterJs.ts` |
| 15 | 31 | diag:missing | TS7006 | checker.reportImplicitAny | Identifier<Parameter> | native | `compiler/argumentsReferenceInFunction1_Js.ts`, `compiler/contextualOverloadListFromUnionWithPrimitiveNoImplicitAny.ts`, `compiler/contextualSignatureInArrayElementLibEs2015.ts` |
| 16 | 31 | types:type-text |  | checkBinaryExpression [TSR error] | BinaryExpression<ExpressionStatement> |  | `compiler/concatTuples.ts`, `compiler/contextualExpressionTypecheckingDoesntBlowStack.ts (target=es2015)`, `compiler/contextualTypingWithGenericSignature.ts` |
| 17 | 29 | diag:missing | TS2322 | checker.reportRelationError | Identifier<JsxAttribute> | native | `compiler/excessiveStackDepthFlatArray.ts`, `compiler/jsxIntrinsicDeclaredUsingTemplateLiteralTypeSignatures.tsx`, `compiler/jsxNamespacePrefixIntrinsics.tsx` |
| 18 | 28 | diag:extra | TS2322 | checker.reportRelationError | Identifier<VariableDeclaration> | tsr | `compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix3.ts`, `compiler/contextualTypeIterableUnions.ts`, `compiler/correctOrderOfPromiseMethod.ts` |
| 19 | 26 | diag:chain | TS2322 | checker.reportRelationError | Identifier<VariableDeclaration> | native | `compiler/aliasUsageInOrExpression.ts`, `compiler/conditionalExpression1.ts`, `compiler/contextualTypingOfConditionalExpression2.ts` |
| 20 | 26 | types:type-text |  | getTypeOfSymbol (ImportSpecifier name) [both types] | Identifier<ImportSpecifier> |  | `compiler/allowSyntheticDefaultImportsCanPaintCrossModuleDeclaration.ts`, `compiler/declarationEmitAliasInlineing.ts`, `compiler/declarationEmitExportAssignedNamespaceNoTripleSlashTypesReference.ts` |
| 21 | 25 | diag:missing | TS2322 | checker.reportRelationError | Identifier<BinaryExpression> | native | `compiler/aliasAssignments.ts`, `compiler/argumentsBindsToFunctionScopeArgumentList.ts (alwaysstrict=true)`, `compiler/assignmentCompat1.ts` |
| 22 | 25 | diag:missing | TS5110 | compiler.verifyCompilerOptions | -<-> | native | `compiler/elidedJSImport2.ts (module=commonjs)`, `compiler/elidedJSImport2.ts (module=es2022)`, `compiler/jsDeclarationEmitExportedClassWithExtends.ts` |
| 23 | 24 | diag:related-info | TS2728 in TS2729 | checker.checkApplicableSignatureForJsxCallLikeElement / checker.checkIndexConstraintForProperty / checker.checkPropertyNotUsedBeforeDeclaration +6 more | Identifier<PropertyAccessExpression> | native | `compiler/checkInheritedProperty.ts`, `compiler/classMergedWithInterfaceMultipleBasesNoError.ts`, `compiler/classStaticInitializersUsePropertiesBeforeDeclaration.ts` |
| 24 | 23 | diag:chain | TS2326 in TS2322 | checker.hasExcessProperties / checker.propertyRelatedTo / checker.reportError | PropertyAccessExpression<BinaryExpression> | native | `compiler/assignmentCompatability11.ts`, `compiler/assignmentCompatability12.ts`, `compiler/assignmentCompatability13.ts` |
| 25 | 23 | diag:chain | TS2328 in TS2322 | checker.compareSignaturesRelated | Identifier<BinaryExpression> | native | `compiler/assignmentStricterConstraints.ts`, `compiler/contextualSignatureInstatiationContravariance.ts`, `compiler/contextualTyping24.ts` |
| 26 | 22 | diag:missing | TS5102 | compiler.verifyCompilerOptions | -<-> | native | `compiler/blockScopedBindingsInDownlevelGenerator.ts (target=es2015)`, `compiler/sourceMapValidationVarInDownLevelGenerator.ts (target=es2015)`, `conformance/async/es5/asyncArrowFunction/asyncArrowFunction11_es5.ts (target=es2015)` |
| 27 | 22 | types:type-text |  | checkPropertyAccessExpression [both types] | PropertyAccessExpression<CallExpression> |  | `compiler/arrayconcat.ts`, `compiler/circularContextualReturnType.ts`, `compiler/contextualSignatureInObjectFreeze.ts` |
| 28 | 22 | types:type-text |  | getTypeOfSymbol (Parameter name) [both types] | Identifier<Parameter> |  | `compiler/coAndContraVariantInferences3.ts`, `compiler/complicatedIndexesOfIntersectionsAreInferencable.ts`, `compiler/declarationEmitReusesLambdaParameterNodes.ts` |
| 29 | 21 | diag:missing | TS2749 | checker.checkAndReportErrorForUsingValueAsType / checker.resolveQualifiedName | Identifier<TypeReference> | native | `compiler/jsEnumTagOnObjectFrozen.ts`, `compiler/jsExportMemberMergedWithModuleAugmentation.ts`, `conformance/jsdoc/declarations/jsDeclarationsEnumTag.ts (target=es2015)` |
| 30 | 20 | diag:chain | TS2326 in TS2430 | checker.hasExcessProperties / checker.propertyRelatedTo / checker.reportError | Identifier<InterfaceDeclaration> | native | `compiler/addMoreOverloadsToBaseSignature.ts`, `compiler/derivedInterfaceCallSignature.ts`, `compiler/interfaceDeclaration3.ts` |

Non-exact cases by class: diag:missing 1,684; types:type-text 1,109; diag:chain 520; diag:related-info 438; diag:missing+extra 339; diag:extra 307; diag:span-only 288; diag:message-text-only 180; diag:code-at-same-span 43; tsr:timeout 2; types:section-set 2; tsr:error 1.
