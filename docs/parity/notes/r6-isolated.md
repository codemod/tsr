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
  `non_local_symbol_flags`. It is `get_symbol_flags`'s walk, starting from
  empty flags and taking `getExportSymbolOfValueSymbolIfExported` of each
  target. Like that port, an unresolvable hop ends the walk. Upstream's
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
`non_local_symbol_flags` stops early, misses `Value`, and reports TS1292
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
