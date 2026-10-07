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
