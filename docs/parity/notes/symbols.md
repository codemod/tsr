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
