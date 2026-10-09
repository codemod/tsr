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
