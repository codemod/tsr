# r6-isolated — option-gated module rules (`tsr-2zk.1125`)

Round 6's isolatedModules / verbatimModuleSyntax / resolution-diagnostic box.
Base: `b18aec06` (main `17265fac` plus bookkeeping), frozen dumps at that
commit. Every expectation below was checked against a native `tsgo` built
from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

**Ownership.** The rules live in this lane's `isolated_alias.rs`. Their call
sites are in main's files (`check.rs`, `symbols.rs`, the resolver), so each
hook is a held diff in this directory, together with the tests that need it.
Until the hook lands, the ported function is unreferenced and carries
`#[allow(dead_code, reason = …)]` naming its diff. That attribute is removed
when the integrator applies the diff.

## 1. `checkExportAssignment`'s single-file-transpilation arms

### What was missing

`check.rs::check_export_assignment_alone` ports `checkExportAssignment`'s
grammar head, TS1120, the ambient entity-name check and the module-format
tail. It skips the middle (pinned `checker.go:5609`–`5650`), which handles a
bare-identifier `export =` / `export default`:

| Code | Arm | Option |
|---|---|---|
| TS1282 / TS1284 | `export =` / `export default` of a name with no value meaning | `VerbatimModuleSyntax` |
| TS1283 / TS1285 | the name is a value, but its alias chain has a type-only declaration | `VerbatimModuleSyntax` |
| TS1291 / TS1292 | an alias whose non-local meanings are type-only, imported without `type` (or type-only in another file) | `GetIsolatedModules()` |
| TS1289 / TS1290 | an alias that reaches a type-only declaration in another file | `GetIsolatedModules()` |
| TS1286 / TS1295 | `export default` in a CommonJS-format file (`isIllegalExportDefaultInCJS`, `getVerbatimModuleSyntaxErrorMessage`) | `VerbatimModuleSyntax` |

The export-specifier arm (TS1205 / TS1448, `checker.go:6806`–`6822`) was
already ported in round 5 (`r5-config.md` §6) and is unchanged.

### The port

`Checker::check_export_assignment_isolated(node, ambient)` in
`isolated_alias.rs`. The hook
(`r6-isolated-export-assignment.diff`) calls it in `check_export_assignment_alone`
right after the TS1120 report, which is where upstream evaluates it.
The helpers it uses mirror upstream one for one:

- `resolveEntityName(id, All, ignoreErrors, dontResolveAlias, node)` is
  `resolve_name_with_export_alias(id, text, SymbolFlags::all())`. For a bare
  identifier with `dontResolveAlias`, upstream resolves the name and stops.
  The reference and the `export` statement share their scopes, so starting
  the walk at the identifier is the same walk.
- `getExportSymbolOfValueSymbolIfExported` is new and has the same body.
- `getSymbolFlagsEx(sym, false, true)` (`excludeLocalMeanings`) is
  `symbol_flags_ex(sym, false, true)`. It is `get_symbol_flags`'s walk,
  starting from empty flags and taking `getExportSymbolOfValueSymbolIfExported`
  of each target. (The first version of this section called it
  `non_local_symbol_flags`; §3 generalised it to both exclusions.) Like that port, an unresolvable hop ends the walk. Upstream's
  `unknownSymbol` answers `All` instead, which can only report *less*
  here: an `All` answer carries `Value`, which turns TS1292 off.
- `getTypeOnlyAliasDeclarationEx(sym, Value)` is
  `type_only_alias_declaration_node_ex`. Upstream loops while the hop has
  neither `Alias` nor `meaning`, reading each hop's
  `aliasSymbolLinks.typeOnlyDeclaration`. `resolveAlias` publishes that link
  as the first type-only declaration on the hop's own remaining chain
  (`markSymbolOfAliasDeclarationIfTypeOnly` copies the immediate target's
  link), so the first hop's answer is the whole walk's answer. Only the
  entry test on `meaning` is left.

**Not ported, deliberately:** `markLinkedReferences(node,
ReferenceHintExportAssignment)`. It only publishes the alias-referenced mark
(§3 owns that side table), and no report in this arm reads it.
`addTypeOnlyDeclarationRelatedInfo` (the TS1377 related span on TS1289) is
not attached either. The diagnostics oracle compares file, position and code,
and round 5's arm made the same call.

No cache, side table or traversal is added. Each call resolves one name and
walks its alias chain once, on a statement that occurs at most a few times per
file.

### Measured

Base `b18aec06`, unfiltered dumps, with the hook applied:

- diagnostics **+4 rows**, WRONG → RIGHT:
  `exportDeclaration(isolatedmodules=true)` (TS1289 at `/d.ts(2,10)`, the
  probe target), `isolatedModulesExportDeclarationType` (TS1292 ×2),
  `verbatimModuleSyntaxNoElisionCJS` (TS1282, TS1283) and
  `verbatimModuleSyntaxNoElisionESM` (TS1284, TS1285);
- one more row changes its list without changing its verdict:
  `modulePreserve4` gains TS1286 at `/f.cts(1,1)`, which is in its baseline;
- every added diagnostic is in its baseline and none was removed. Zero
  losses, no key missing, types byte-identical, `slowcases` clean on both
  dumps;
- native `tsgo` on `exportDeclaration` and on test2/test3 of
  `isolatedModulesExportDeclarationType` prints the same codes at the same
  positions;
- perf: median child CPU new/base, domain-model 0.997 over 21 samples and
  generic-imports 1.013 over 41 samples (the first 21-sample run read
  1.164, which is noise on a 0.07 s run). `diagnostics_match: true`.
  Callgrind Ir: domain-model 1,090,799,869 → 1,091,809,769 (+0.09%),
  generic-imports 343,073,512 → 343,070,463;
- `cargo test --workspace --release` passes with the diff applied. The diff
  adds `crates/tsr-compiler/tests/r6_isolated.rs`, which covers TS1289 (and
  silence without the option), TS1292 and TS1284.

### How this would be wrong

A TS1289/TS1292 report on an `export =` whose name upstream resolves to a
value through a hop this port's `resolve_alias` cannot follow. In that case
`symbol_flags_ex` stops early, misses `Value`, and reports TS1292
where upstream is silent. No such row appears in the dumps.

## 2. TS2866 and `checkConstEnumAccess` (TS2475, TS2748)

### TS2866: `resolveNameHelper`'s success tail

Upstream reports TS2866 (pinned `checker.go:1872`–`1885`) while it is still
inside name resolution, on every successful `resolveName` whose meaning holds
every `Value` bit, in an external module, under
`compilerOptions.IsolatedModules`. The flag is read directly, not through
`GetIsolatedModules()`. The rule fires when the result is the global of that
name and the file's own top level holds a non-value meaning of the name
through an import that is not type-only. The error goes on the import
declaration.

`Checker::check_import_conflicts_with_global_value(node, result, text)` in
`isolated_alias.rs`. TSR's `BindResult::resolve_name` takes `&self` and
cannot report, so the report stays with the checker. The hook
(`r6-isolated-global-value.diff`) calls it in `check.rs::check_value_identifier`,
right after `report_type_only_alias_used_as_value`. That function is
`resolveNameHelper`'s previous success arm (TS1361/TS1362), so the two run in
upstream's order on the same `(node, symbol)`.

- `isGlobal` (`getSymbol(c.globals, name, meaning) == result`) and
  `nonValueSymbol` (`getSymbol(lastLocation.Locals(), name, ^Value)`) use a
  new `get_symbol_in`. It is `getSymbol`'s body for a table entry already
  looked up by name: the merged symbol if it carries `meaning`, or if it is
  an alias whose chain (`get_symbol_flags`) does.
- `lastLocation` is the reference's source file. When the result is a
  global, the walk has just left that file: a module's names are found in
  its own locals or above them, and only `c.globals` is above the file.
- The import declaration is the first declaration of kind
  `ImportSpecifier | ImportClause | NamespaceImport | ImportEqualsDeclaration`.
  `ast.IsTypeOnlyImportDeclaration` is the import half of round 5's
  `is_type_only_import_or_export_declaration`.

**Scope of the hook.** Upstream reports from every value-meaning
`resolveName`. The hook covers the expression identifier
(`getResolvedSymbol`), which is the reference shape in every TS2866
baseline. A value-meaning resolution elsewhere, such as a shorthand property
or an `export =` operand, would be a missing TS2866, never a wrong one.

Measured alone against `b18aec06`:
`isolatedModulesShadowGlobalTypeNotValue(isolatedmodules=true,verbatimmodulesyntax=false)`
goes WRONG → RIGHT (+1). The `(true,true)` row gains the same two expected
TS2866 lines and stays WRONG on `good.ts`'s TS1295 (below). Zero losses,
types identical, slowcases clean. CPU new/base: domain-model 1.021,
generic-imports 1.020, 21 samples each, `diagnostics_match: true`. Callgrind
Ir is unchanged within run-to-run variance: domain-model 1,091,499,458 →
1,091,494,287, generic-imports 343,090,570 → 343,056,142. The function
returns on its first read without `isolatedModules`. Native `tsgo` agrees on
the reduced fixture (`bad.ts(1,10)` TS2866).

### `checkConstEnumAccess`

`checkExpressionEx` (`checker.go:7566`) calls `checkConstEnumAccess`
(`:7573`) on every expression whose type is a const enum's object type. TSR
had none of it: TS2475 ("'const' enums can only be used in property or
index access …") was never reported, and TS2748 came only from
`checkAliasSymbol`'s import arm (round 5).

`Checker::check_const_enum_access(node, t)` in `isolated_alias.rs` is the
whole function:

- `ok` is upstream's disjunction: a property or element access receiver; an
  identifier or qualified name on the right of `import =` or `export =`
  (`isInRightSideOfImportOrExportAssignment`, ported); a type query's
  `exprName`; an export specifier.
- TS2748 runs under `IsolatedModules`, or under `VerbatimModuleSyntax` when
  `ok` holds and `GetFirstIdentifier(node)` does not resolve at meaning
  `Alias`. The second case leaves an imported enum to `checkAliasSymbol`.
  The enum's `ValueDeclaration` must be ambient and the use site must not be
  `IsValidTypeOnlyAliasUseSite`.
- That predicate is ported a second time, as
  `is_valid_type_only_alias_use_site_native`. `check.rs`'s
  `is_valid_type_only_alias_use_site` is TS1361's copy: it admits any
  `export =` operand (§125 of the diag2 notes), which upstream's
  `!IsExpressionNode` disjunct does not. That file is main's, and its
  private helper cannot be called from here. `NodeFlagsAmbient` is
  `declaration_is_in_an_ambient_context`'s ancestor walk, because this parser
  never sets the flag.
- `redirect` (`GetProjectReferenceFromOutputDts`) is nil, as in round 5's
  alias arm.

The hook (`r6-isolated-const-enum-access.diff`) sits in
`expressions.rs::check_expression`, between the worker and the cache write.
That is upstream's position: after `instantiateTypeWithSingleGenericCallSignature`,
which this port folds into the worker. A cached read does not re-run it.
Upstream re-runs it on `checkExpressionEx` but not on a
`checkExpressionCached` hit, and a repeated identical diagnostic is
deduplicated either way.

**What TS2475 still misses, and why.** Upstream types every initializer
while checking a declaration. TSR's statement walk does not call
`check_expression` on an unannotated `var x = E2`, whose type nobody
requests during checking, so `constEnumErrors`' lines 27/28 and
`constEnumPropertyAccess2` stay missing. A report needs the expression to
be typed. Adding a "type everything" pass to fill the gap would be a side
pass, not this rule. Making the walk type each initializer as upstream's
`checkVariableLikeDeclaration` does is main's statement walk
(`check.rs`). The gap is recorded in §5.

Measured alone against `b18aec06`: `isolatedModulesAmbientConstEnum` and
`verbatimModuleSyntaxAmbientConstEnum` go WRONG → RIGHT (+2).
`constEnumErrors` gains its expected TS2475 at `(33,5)` (`foo(E2)`). Every
added line is in its baseline. Zero losses, types identical, slowcases
clean. CPU new/base: domain-model 0.994, generic-imports 1.029, 21 samples
each, `diagnostics_match: true`. Callgrind Ir: domain-model 1,090,802,056 →
1,091,829,625 (+0.09%, within the ±0.07% this harness shows between two runs
of the same base binary), generic-imports 343,073,554 → 343,078,469. The hot
path adds one `store.get` per computed expression. Native `tsgo` agrees on
`isolatedModulesAmbientConstEnum` (`file1.ts(2,16)` TS2748).

No cache, side table or traversal is added by either port.

## 3. `markDecoratorAliasReferenced` (TS1272)

### Upstream

`checkDecorators` (pinned `checker.go:6021`) calls
`markLinkedReferences(node, ReferenceHintDecorator)` (`:6053`) between its
helper requests and the per-decorator checks. Behind the
`canCollectSymbolAliasAccessibilityData` guard (`!VerbatimModuleSyntax`,
`:928`) and the ambient guard (which spares property declarations), that
call reaches `markDecoratorAliasReferenced` (`:28686`). Under
`EmitDecoratorMetadata` it requests the `__metadata` helper and walks the type
annotations that decorator metadata serializes:

- a class's first constructor-with-body parameters;
- an accessor's annotation, or its pair's;
- a method's parameters and return type;
- a property's type;
- a decorated parameter, plus its signature's parameters and return type.

Each annotation goes through `getEntityNameForDecoratorMetadata` and its
type-list rule (`never` always elided; `null`/`undefined` elided without
`strictNullChecks`; a union or intersection survives only if every member
names the same identifier). The resulting entity name goes to
`markEntityNameOrEntityExpressionAsReference(…, forDecoratorMetadata: true)`
(`:28857`). That function resolves the root identifier at
`Type|Alias` (`Namespace|Alias` for a qualified name). For an alias it
either marks it referenced, or, under `GetIsolatedModules()` with an ES
emit module kind, reports TS1272 at the type name when the alias is not a
value and none of its declarations is type-only.

### The port

`Checker::check_decorator_linked_references(node, typed, emit_decorator_metadata)`
and its helpers in `isolated_alias.rs`. The hook
(`r6-isolated-decorator-metadata.diff`) does two things:

- adds `Checker::emit_decorator_metadata` (`checker.rs`, set in
  `apply_compiler_options`; nothing in the checker read the option before);
- calls the function from `check.rs::check_node_worker` right after
  `check_construct_emit_helpers`, the walk's per-node helper-request site,
  which is where `checkDecorators` makes its own requests.

Until the hook lands, the option arrives as a parameter, so the committed code
needs no field.

- **Guard order.** Upstream tests decoratability, then `canCollect…`, then
  ambient, then the option. Only the option and `verbatimModuleSyntax`
  reads come first here: every guard before
  `checkExternalEmitHelpers(…, Metadata)` is free of side effects, and the
  function runs on every node the walk visits. With the reads in upstream's
  order, the first measurement cost domain-model **+0.16% Ir**
  (1,090,821,038 → 1,092,559,075) for the modifier scan on every node.
  Reordered, it is within noise (1,091,508,180 → 1,091,819,762).
- **`ast.NodeCanBeDecorated`** is ported again as
  `decorated_node_can_be_decorated`, because `grammar.rs::node_can_be_decorated`
  is private to that file. The integrator can fold the two together when the
  hook lands.
- `symbolIsValue` reuses `members.rs::symbol_is_value`. The new
  `symbol_flags_ex` generalises §1's walk to both of upstream's exclusions.
  `isConstEnumOrConstEnumOnlyModule` reads `CONST_ENUM` only: this binder has
  no `SymbolFlagsConstEnumOnlyModule`, and the answer gates only the mark.
- The related-information span ("'T1' was imported here") is not attached,
  as in §1.

### The `aliasSymbolLinks.referenced` side table: recorded, not built

Per the checker port convention:

- **Native operation.** `markAliasSymbolAsReferenced` (`checker.go:28822`)
  writes `aliasSymbolLinks.referenced`. For a non-external `import =`, it
  recurses into the reference's first identifier
  (`markIdentifierAliasReferenced`).
- **Identity and owner.** Key: the alias `*ast.Symbol`. Value: one `bool`.
  Owner: the Checker (`aliasSymbolLinks`, a per-Checker link store), for the
  Checker's lifetime. Writes are gated off by `verbatimModuleSyntax`
  (`canCollectSymbolAliasAccessibilityData`).
- **Publication.** Monotone `false → true`, set before the recursion, so a
  cycle of `import =` aliases terminates. There is no provisional state.
- **Consumer.** Only `EmitResolver.IsReferencedAliasDeclaration`
  (`emitresolver.go:693`/`:705`), which drives import elision in the
  JavaScript emitter. No diagnostic reads it: TS1272 sits in the mark's
  `else` and reads `symbolIsValue` and the declarations, not the mark.
- **Work boundary.** One `resolveName` and one alias-chain walk per
  serialized annotation of a decorated declaration under
  `emitDecoratorMetadata`. Cheap, and bounded by decorated declarations.

TSR has no JavaScript emitter that elides imports, and
`tsr-declarations`' import elision answers reachability syntactically
(`transform.rs`). A table nothing reads would be a write-only cache, so the
mark arm is kept for its control flow and publishes nothing (and §1's
`markLinkedReferences(…, ReferenceHintExportAssignment)` is likewise not
ported). The table becomes worth building when an emitter asks
`IsReferencedAliasDeclaration`. Its key would be the merged alias
`SymbolId`, owned by the `Checker`, beside the other alias links.

### Measured

Against `b18aec06`, unfiltered, with the hook and the reordered guard:
`emitDecoratorMetadata_isolatedModules(module=esnext)` goes WRONG → RIGHT
(+1: TS1272 at `index.ts` (9,23), (15,8), (24,28)). The `module=commonjs`
row stays RIGHT. Zero losses, types identical, slowcases clean. CPU
new/base: domain-model 1.004 (21 samples), generic-imports 1.017 (41
samples; 1.032 at 21), `diagnostics_match: true`. Native `tsgo` on the
fixture prints the same three lines. The diff adds
`crates/tsr-compiler/tests/r6_isolated_decorator_metadata.rs`: TS1272 with
the options, silence without `emitDecoratorMetadata`, and silence under
`module: commonjs`.

## 4. The diffs together

Apply order: `r6-isolated-export-assignment.diff`,
`r6-isolated-global-value.diff`, `r6-isolated-const-enum-access.diff`,
`r6-isolated-decorator-metadata.diff`, `r6-isolated-resolution-diagnostic.diff`
(§5). They touch disjoint hunks and apply cleanly together on top of this
branch's `isolated_alias.rs`.

The first four measured together against `b18aec06`:

- diagnostics **+8 rows**, the sum of §1–§3 (4 + 1 + 2 + 1);
- zero losses, every added diagnostic in its baseline, types identical,
  slowcases clean;
- `cargo test --workspace --release`: 3,494 passed, 0 failed.

All five together:

- **+21 rows** (8 + 13);
- zero losses, every added diagnostic in its baseline, types identical,
  slowcases clean;
- 3,496 tests passed, 0 failed.

`coverage` against the snapshots committed at the base:

| Row | base | all five diffs |
|---|---:|---:|
| `checker_types` | 8,489 / 9,538 | 8,489 / 9,538 |
| `diagnostics` | 4,636 / 5,502 | 4,644 / 5,502 |
| `diagnostics_configured` | 894 / 1,091 | 907 / 1,091 |

When the integrator applies a diff, the `#[allow(dead_code, reason = …)]`
on its function should be dropped in the same commit.

## 5. `GetResolutionDiagnostic` beyond JS/JSX/TSX, TS2846 and TS5097

### Upstream

`resolveExternalModule` (pinned `checker.go:15209`) computes
`module.GetResolutionDiagnostic(options, resolvedModule, file)`
(`module/util.go:123`) for every resolved specifier, from the resolver's
`Extension`. The cases are:

- `.ts`/`.d.ts`/`.mts`/`.d.mts`/`.cts`/`.d.cts`: none;
- `.tsx`: TS6142 without `--jsx`;
- `.jsx`: TS6142, then the `allowJs` arm;
- `.js`/`.mjs`/`.cjs`: TS7016 under `noImplicitAny` without `allowJs`;
- `.json`: TS7042 without `resolveJsonModule`;
- anything else, i.e. an arbitrary extension's `.d.<ext>.ts`: TS6263 without
  `allowArbitraryExtensions`, unless the importing file is a declaration
  file.

Any diagnostic but TS6142 keeps the file out (`sourceFile == nil`), and the
not-found tail reports the diagnostic itself (`:15403`). With the file in,
TS6142 is reported, and then the `ResolvedUsingTsExtension` arms run inside
an emittable import (`ast.IsEmittableImport`):

- TS2846 for a declaration-file specifier, with `getSuggestedImportSource`;
- TS5097 when `!AllowImportingTsExtensionsFrom(importing file)`.

### What TSR had

`check.rs::check_untyped_module_import` ported only the JS/JSX/TSX arms.
It runs only when no module symbol resolves, and it reads the extension off
the resolved path. The program kept neither the resolver's `Extension` nor
`ResolvedUsingTsExtension`. A `.d.html.ts` reached through `./x.html` looks
like any `.ts` file by path; upstream's extension is `.d.html.ts`. So:

- TS6263 and TS5097 were never reported;
- TS2846 and a typed `.tsx`'s TS6142 were missing;
- under the node modes, the `.d.node.ts` resolution fell to TS2306 instead
  of TS6263.

### The port

`Checker::check_module_resolution_diagnostic` and the free function
`resolution_diagnostic` (`GetResolutionDiagnostic`) in `isolated_alias.rs`,
plus `is_emittable_import`, `allow_importing_ts_extensions_from` and
`suggested_import_source`. The function reports TS6142 (typed case), TS2846
and TS5097 when the file is in the program. Otherwise it reports the
non-JS resolution diagnostic and returns `true`, so the caller skips its
found-file reports, as upstream's nil `sourceFile` does. The JS arm (TS7016)
stays with `check_untyped_module_import`, which already ports it.

The hook (`r6-isolated-resolution-diagnostic.diff`) spans the loader, the
program, the host trait and the checker:

- `tsr-compiler/src/loader.rs`: `ResolutionRequest` carries the resolver's
  `extension` (a `Cow<'static, str>`, borrowed for every fixed extension)
  and `resolved_using_ts_extension`;
- `tsr-compiler/src/lib.rs`: `ModeResolution` keeps them, and
  `ModuleHost::resolved_module_extension` answers them;
- `tsr-checker/src/resolution.rs`: that trait method, defaulting to `None`;
- `checker.rs`: `resolution_diagnostic_options` (`GetAllowJS`,
  `GetResolveJsonModule`, `allowArbitraryExtensions`,
  `GetAllowImportingTsExtensions`);
- `check.rs::check_module_specifier`: the call, before
  `check_untyped_module_import`.

**The work boundary, measured.** A straight port cost domain-model +0.14% Ir:

- a second `module_resolution_mode` per import;
- `is_declaration_file` per import;
- a `String` per resolution request, cloned through the loader. This was
  most of it: about 1M Ir in `malloc`/`free`.

Three changes bring it to +0.01–0.02% (1,091,463,085 → 1,091,658,944;
1,090,823,057 → 1,090,966,903):

1. The host answers from the resolution every mode agrees on
   (`agreed_resolution`). A disagreement is the same file asked in two
   modes with two answers, and only then is the usage's mode computed.
2. The declaration-file test is asked only by the arbitrary-extension arm.
3. Fixed extensions are borrowed `'static`.

The common import (a `.ts` target with no TS extension in the specifier)
returns before any string work. No cache is added: each fact is read from
the program's existing resolution table.

Deliberately left out:

- `rewriteRelativeImportExtensions`' three arms (`:15261`). They need
  `SourceFileMayBeEmitted`, `GetRedirectForResolution` and the common
  source directory, which the host does not expose; no row in the dumps
  needs them.
- The project-reference `Output_file_0_has_not_been_built` redirect
  (`:15390`). There are no project references in this program.

**Accepted difference.** Upstream's nil `sourceFile` also makes the import
alias resolve to `unknownSymbol`. TSR's `resolve_external_module_name`
(`symbols.rs`) still returns the module symbol for a TS6263 file, so uses of
the import are type-checked against it. That can only add a diagnostic
upstream lacks. None appears in the dumps (the three node-mode rows lose
their wrong TS2306 and add nothing). Making `resolve_external_module_name`
answer `None` for that case is main's file and is left for the integrator.

### Measured

Against `b18aec06`, unfiltered, with the final hook: diagnostics **+13
rows**, WRONG → RIGHT:

- `declarationFileForHtmlImport(allowarbitraryextensions=false)`;
- `declarationFilesForNodeNativeModules(allowarbitraryextensions=false,…)`
  ×3 (TS6263, and the wrong TS2306 gone);
- `bundlerImportTsExtensions` ×4 (TS5097 ×5, TS2846 ×2, TS6142);
- `bundlerRelative1` ×2;
- `moduleResolutionNoTsCJS`, `moduleResolutionNoTsESM`;
- `resolutionCandidateFromPackageJsonField2`.

`checkJsxNotSetError`, `allowsImportingTsExtension` and
`decoratorOnClassConstructor2(target=es2015)` gain expected lines and stay
WRONG on others. Every added diagnostic is in its baseline. Zero losses,
types identical, slowcases clean. CPU new/base: domain-model 1.007,
generic-imports 0.917, 21 samples each, `diagnostics_match: true`. Native
`tsgo` agrees on reduced fixtures, including TS5097 on a root `./c.tsx`
next to its TS6142. The diff adds
`crates/tsr-compiler/tests/r6_isolated_resolution_diagnostic.rs`.

## 6. What remains in the lane, with causes

- **TS1295 on `isolatedModulesShadowGlobalTypeNotValue`'s `good.ts`**
  (`(false,true)` and `(true,true)` rows). The alias target is
  `export = globalThis.console` inside `declare module 'node:console'`.
  `symbols.rs::resolve_alias` does not resolve a property-access
  `export =`, so `check_alias_symbol` returns before the round-5 arm.
  Owner: main (`symbols.rs`).
- **TS2475 for unannotated initializers** (`constEnumErrors` lines 27/28,
  `constEnumPropertyAccess2`). The statement walk never types
  `var x = E2`, so `checkExpressionEx`'s hook never sees it. Owner: main
  (`check.rs`, `checkVariableLikeDeclaration`'s initializer check).
- **`isolatedModulesExportImportUninstantiatedNamespace`** (TS1269 on
  `export import` of an uninstantiated namespace),
  **`isolatedModulesGlobalNamespacesAndEnums`** (TS1280, TS1281),
  **`verbatimModuleSyntaxInternalImportEquals`** (TS2503) and the
  **`blockScopedEnumVariablesUseBeforeDef_*`** TS2450 pair were not in this
  round's items and were not classified.
- **`resolve_external_module_name` answering a symbol for a TS6263 file**
  (§5, accepted difference). Owner: main (`symbols.rs`).
- **`rewriteRelativeImportExtensions`' arms** (§5). They need host facts
  the program does not expose.
