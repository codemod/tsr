# r7-shared — shared-contract lane (`tsr-2zk.1268`)

Round 7's shared-contract owner: `symbols.rs`, signature construction in
`signatures.rs`, `members.rs`, mapper/instantiate code, `resolution.rs` and
`crates/tsr-binder`. Two strands on separate branches: `box/r7-shared`
(independent ports, this note's §1–) and `box/r7-shared-cutover` (the
rejected alias/mapper cutover, §C). Expectations are checked against the
pinned native baselines (`vendor/typescript-go` @ `5b1047d`).

Base: `origin/main` `9020aa67`, frozen unfiltered with
`scripts/parity_gate.sh freeze`: 12,238 diagnostics rows (5,741 RIGHT, 5,610
EMPTY_RIGHT) and 556,357 type rows (551,176 RIGHT, 673 GAP, 4,508 WRONG).

## 0. Cluster re-measure on the base

r6-triage's cases (`notes/r6-triage-issues.json`, cut at `e6eadf4`) re-read
on `9020aa67`:

| Cluster | Still failing |
|---|---|
| RESOLVE-ES-MODULE-SYMBOL-CLONE | 11 of 11 (`nodeModules1`/`nodeModulesAllowJs1` ×4 modes, 24 lines each; `exportsAndImports4` ×2; `transformNestedGeneratorsWithTry`) |
| MODULE-AUGMENTATION-MERGE | 8 of 9 (`augmentExportEquals3–7`, `checkerInitializationCrash`, `mergeSymbolReexportedTypeAliasInstantiation`, `moduleAugmentationOfAlias`) |
| RESOLVE-ALIAS-INDIRECTION | 7 of 18 (batch CG landed r6-modules4 §1; the rest is r6-modules4 §4) |
| MERGE-SYMBOL-RESOLVE-ALIAS-TARGET | 4 of 6 diagnostics, 1 types |
| DEFAULT-EXPORT-ALIAS-CLASS-MERGE | 3 of 7 |
| IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET | 1 of 7 (`declFileForExportedImport`) |

## 1. `resolveESModuleSymbol`'s default-member clone (RESOLVE-ES-MODULE-SYMBOL-CLONE)

### Cause

`resolveESModuleSymbol` (`checker.go:15568`) answers a namespace import with
`cloneTypeAsModuleType(symbol, moduleType, referenceParent)` (`:15721`) when
the module type has signatures, a `default` property, or the import is an
ESM-to-CommonJS reference (`:15609-15618`). The clone is a **new symbol**:
the alias resolves to it, not to the module, so a later `import a =
require("./t1")` is the only alias that resolves to the module itself, and
the printer's `trySymbolTable` alias loop names the module `typeof a`, each
namespace import naming its own clone (`typeof c`, `typeof e2`).

For the `default`-property arm with a structured type, `moduleType` is
`getTypeWithSyntheticDefaultImportType`, which is `t` itself when the module
cannot have a synthetic default (it already has a real one). The port
returned the plain module type there, `None` from
`namespace_import_default_member_type`, so no clone existed and every alias of
the module looked alike (`exportsAndImports4`: `typeof a` for `c`/`e2`).

### The port

`namespace_import_default_member_type` now clones the module value when the
synthetic type is the value itself, exactly as the other two arms of
`module_clone_type` already do: a new anonymous `TypeId` with the same text
and symbol, recorded in `module_value_clones` as `(alias, value)` with no
signatures. This port has no clone symbol (`symbol_access.rs`'s private
symbols are not binder ids, and `resolve_alias` answers `SymbolId`), so the
clone is the alias's value type, the established stand-in
(`checker-notes-nameres.md` §14).

Checker port convention: no new cache. The existing `module_value_clones`
side table (key: the clone `TypeId`, minted once per namespace-import alias
inside `get_type_of_alias`, whose `symbol_types` memo owns it; value: the
alias and source value; published when the alias type completes; receiver
context: the alias). Member reads go through the existing clone arm of
`get_property_of_type_ex` (`members.rs`), which reads the source's members;
the work boundary is unchanged (one clone per alias, no member copy).

### Measured (symbols.rs alone)

+15 type lines, 3 cases (`exportsAndImports4(target=es2015)`,
`exportsAndImports4-es6`, `unusedImports_entireImportDeclaration`); zero
type or diagnostic losses. Coverage: `checker_types` 8,679/9,538,
`checker_types_configured` 1,753/1,928, `diagnostics` 4,786/5,502,
`diagnostics_configured` 955/1,091. Median child-CPU new/old (21 samples):
domain-model 0.9966, generic-imports 1.0077. `cargo test --workspace
--release` passes; clippy reports only the pre-existing nested fn at
`symbols.rs:4513`.

### Routed: the printer half (r7-printer, `checker.rs`)

`checker.rs::alias_targets_module_clone` (the `module_alias_at` exclusion
of an alias that resolves to a clone, `trySymbolTable`'s `resolveAlias`
comparison) admits only clones of class/function targets. Native's alias
resolves to a clone for every arm of `resolveESModuleSymbol`, so the test
should be "the alias's value is a module clone", whatever the target. With
that one change (`r7-shared-printer-module-clone.diff`) on top of §1:
**+211 type lines, 13 cases**, zero losses — the four `nodeModules1` and
four `nodeModulesAllowJs1` modes (24 lines each: `typeof m26` for `typeof
m4`, the ESM-to-CJS clone arm), `unusedImports11/12`, and §1's three.

### Remaining

`transformNestedGeneratorsWithTry` (4 lines): native's first condition is
`hasSignatures(typ)` on the type, not the target's declaration kind; an
`export = Bluebird` of a `const Bluebird: typeof Promise` is a variable with
construct signatures, which `module_clone_type` declines ("variable exports
whose value happens to be callable are a different symbol shape"). Needs the
synthetic spread of `PromiseConstructor` plus `default`, printed in the
spread's member order.
