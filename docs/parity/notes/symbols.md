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

## Export-default local name in ambient modules (tsr-2zk.6)

`NameResolver.Resolve`'s default arm runs for a source file or a module
declaration with `NodeFlagsAmbient` that is not `declare global`; the exports
arm then skips the internal `default` key (`if name != InternalSymbolNameDefault`).
TSR's parser never sets `NodeFlags::AMBIENT`, so the arm never fired inside
`declare module "foo"`. The binder now publishes the parser's ambient context
for module declarations as `NodeFacts::AMBIENT_CONTEXT` (declaration file,
`declare`, or an ambient enclosing module; a string-literal name alone does
not set it), read by `resolve_name` and the suggestion walk. Fact keyed by the
module declaration node, owned by the file bind, published once at bind.
Converts `compiler/es5ExportDefaultClassDeclaration4` (target=es2015) and
`compiler/defaultIsNotVisibleInLocalScope` (+2 type rows).

## `arguments` outside functions resolves normally (tsr-2zk.6)

`NameResolver.Resolve` synthesizes `arguments` only in function-like
containers; elsewhere the name walks the ordinary scopes, so a declared
`var arguments` resolves. `check_value_identifier`'s §951 arm reported TS2304
without resolving; it now asks `resolve_name_with_export_alias` at
`Value | ExportValue` first. Converts
`conformance/emitArrowFunctionWhenUsingArguments03` (alwaysstrict=true,
target=es2015) and `..._ES6` (alwaysstrict=true) in the exact oracle; not in
the legacy suite.

## Interface `implements` is never resolved (tsr-2zk.6)

`checkInterfaceDeclaration` visits only `extends` heritage types; an
interface's `implements` clause is the parser's TS1176 and no type in it is
resolved. `check_type_reference_name`'s `implements` arm now requires a
class-like owner. Converts `conformance/parserInterfaceDeclaration2`.
