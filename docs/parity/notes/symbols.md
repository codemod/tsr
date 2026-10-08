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
