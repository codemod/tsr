# Symbols lane notes

Binder, name/module/alias resolution and accessible symbol chains. One entry
per landed port; each names the pinned native operation and the boundary.

## Block-scoped exported declarations (tsr-2zk.16.292)

Native `bindBlockScopedDeclaration` (`binder.go`) calls `declareModuleMember`
only when `blockScopeContainer` is a ModuleDeclaration or (external/CommonJS)
SourceFile; any other block declares into the block's locals, so
`namespace N { { export function f() {} } }` does not publish `N.f`. The
binder's export branch now requires the locals owner to be the container or
the block to be a SourceFile/ModuleDeclaration. No cache or side table.
Converts `compiler/innerModExport1` (3 type rows + diagnostics) and
`compiler/innerModExport2`.

## Export assignment parent symbol (tsr-2zk.16.293)

Native `bindExportAssignment` declares into `GetExports(container.Symbol())`
with parent `container.Symbol()`, the lexical container's own symbol, not the
binder's inherited member-owner cursor. `function f() { export = 0 }` no
longer collides (spurious TS2300) with a file-level `export =`. Reads the
existing `node_symbols` slot of `self.container`; no new table. Converts
`compiler/exportInFunction` (1 type row).

## Same-file module container in getSymbolChain (tsr-2zk.16.67)

Native `getSymbolChain` qualifies through the file module when
`needsQualification` holds; `forEachSymbolTableInScope` reads the reference
file's own `exports`, and the file module itself has no accessible chain, so
`getSpecifierForModuleSymbol` spells `import("./self").X`. `symbol_chain`
declined every same-file container. It now declines only an exported symbol
whose name `resolve_name` could not resolve (TSR resolution gap, the chain1
guard); a conflicting declaration split out of the exports table, or a name
shadowed by another symbol, reaches the specifier. No cache; one extra
`resolve_name` on the already-qualifying same-file path. Native control
(`tsgo --declaration`): `interface d {} export class d {}` emits
`import("./a").d`, a sibling `export class e {}` emits `e`.
Converts 9 rows: enumAssignmentCompat6 (3), exportInterfaceClassAndValue,
giant, importedEnumMemberMergedWithExportedAliasIsError, mergedDeclarationExports,
moduleDuplicateIdentifiers, privacyImportParseErrors.

## Spelling suggestions walk the resolver (tsr-2zk.6)

Native `getSuggestedSymbolForNonexistentSymbol` is `resolveNameForSymbolSuggestion`:
`NameResolver.Resolve` with `Lookup = getSuggestionForSymbolNameLookup`
(`getSymbol`, else `getSpellingSuggestionForName` over that one table). Every
arm keeps its meaning mask and acceptance rules (`useResult`, type parameter
container, static/heritage/computed declines), and the first table yielding a
near-miss ends the walk; globals come last with the primitive alias extras.
`names_in_scope_with_meaning` collected every table at once, including class
property members (native looks there at `meaning & Type`), and took the
globally closest name with a lexical tie-break. It is replaced by
`BindResult::resolve_name_for_suggestion` (binder walk, `SuggestionHost` for the
checker's lookup and `requiresScopeChange`) and
`Checker::suggested_symbol_for_nonexistent_symbol` (`name_suggestion.rs`;
`compareSymbols` tie-break, `tryResolveAlias` for alias candidates). Value
sites pass `Value | ExportValue`, type sites `Type`, the JSX factory its own
flags. A `declare global` symbol is native `InternalSymbolNameGlobal`, never a
candidate. No cache or side table; error path only (native caches only
`declarationRequiresScopeChange`). Related info (TS2728) is not emitted; the
checker has no related-information producer yet.
Native controls: `class C { nodeType = 1; m() { NodeType; } }` is TS2304;
`var Abcde; function f() { var abcdx; abcde; }` suggests `abcdx` (base
suggested `Abcde`). Converts `conformance/parserRealSource11` (56 TS2552 → TS2304).
