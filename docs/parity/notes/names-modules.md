# Parity notes — names-modules lane (`tsr-2zk.6`)

Judgment calls made while porting name, module and alias resolution for epic
`tsr-2zk`. Every number is measured with `diagverdictdump` / `verdictdump`
against the frozen box baseline (branch base `0d996e8`), unfiltered.

## §1. Ambient modules are `ValueModule`; local aliases take `getSymbol`'s meaning test

Clusters: `tsr-2zk.16.23` AMBIENT-MODULE-BIND-VALUEMODULE, plus the EXTRA 2307
bucket of the lane brief.

### The forcing constraint

`declare module "Bar" { export type T = number }` in a script was bound as a
`NamespaceModule`, because the binder's kind-only `classify` chose the flag from
`GetModuleInstanceState` for every module declaration. Upstream does that only
for namespaces and external augmentations: `bindModuleDeclaration`'s ambient
arm (`internal/binder/binder.go:773-781`) declares every other ambient module
— string-named, or `declare global` in a script — as `ValueModule`
unconditionally. `tryFindAmbientModule` (`checker.go:15533`) looks the quoted
name up with `ValueModule` meaning, so a type-only ambient module was invisible
to module resolution and every import of it reported TS2307. That was the
whole of `ramdaToolsNoInfinite2`'s 131 extra TS2307s.

The ambient arm is now in `Binder::classify` (`crates/tsr-binder/src/binder.rs`),
using a shared `is_module_augmentation_external` that reads
`ast.IsModuleAugmentationExternal` (`utilities.go:1694`) off the parent chain.
`is_merged_global_augmentation` uses the same helper.

### The second half: an alias in `locals` must carry the meaning

Making those modules resolvable exposed the next difference. Upstream's name
resolver reads every table through `Checker.getSymbol` (`checker.go:2176`):
an alias whose own flags lack the meaning is a hit only when
`getSymbolFlags(alias)` — its target's flags, or `All` when the target is
unknown — carries it. TSR's `lookup_scoped` accepts any alias, because the
binder cannot resolve one. The checker already supplies a target-meaning
callback for the exports arm (`resolve_name_with_export_alias`); it is now also
consulted for an alias found in `locals`, but only from that checker wrapper
(`filter_local_aliases`), and only a definite `Some(false)` rejects. A rejected
local falls through to the location's exports arm and then outward, which is
upstream's nil `result`.

Consequence: `import type { Base }` of an *interface* no longer answers a
value lookup for `class C extends Base`. Upstream then runs the failure
cascade, whose second arm is `checkAndReportErrorForExtendingInterface`
(`checker.go:11666`, TS2689). That arm is ported in `meaning_mismatch.rs`,
gated to a class's `extends` clause, because this port's `is_value_reference`
also visits `interface I extends A` and `implements` names (upstream never
resolves those as values). The unit test
`a_class_extends_over_a_type_only_import_still_reports` asserted TS1361 for
this exact pair. `tsc` 6.0.2 reports TS2689, so the test now asserts TS2689.
TS1361 is the code for a type-only import of a *class* (`conformance/extendsClause`).

### Measured

Diagnostics +9 cases, 0 lost: `classExtendsInterface`,
`classExtendsInterfaceInModule`,
`declFileAmbientExternalModuleWithSingleExportedModule`,
`duplicateIdentifierRelatedSpans6`/`7`,
`moduleSharesNameWithImportDeclarationInsideIt5`, `typeReferenceDirectives5`/`13`,
`tsxElementResolution19`. `checker_types` +111 aligned RIGHT lines, 0 lost.
The self-comparison wall ratio is 0.97 on `domain-model` and 0.95 on
`generic-imports` (9 samples), with matching diagnostics. The callback runs only
for an alias hit whose own flags lack the meaning, and `resolve_alias` is
already memoised.

### Refused, with the number: imports inside an ambient module stay in its exports

Upstream's `declareModuleMember` (`binder.go:399-404`) files every import
except an exported `import x =` in `locals`, inside an ambient module too.
`is_exported_from_container` still follows ExportContext there, so
`declare module "m" { import { I } from "n"; export type K = I; }` cannot
resolve `I`. Porting that line converted 2 more diagnostics cases
(`ramdaToolsNoInfinite2`, `moduleAugmentationInAmbientModule5`) and about 12
type lines. It also **lost 13 RIGHT type lines**, so it is held back. Each
loss was a line that matched only because the name failed to resolve and the
written text was printed:

- `privacyImportParseErrors` ×4: `var v: x` where `import x = require("m")`
  names an unresolvable module. Upstream resolves the alias to `unknownSymbol`
  and `resolveTypeReferenceName` mints the unresolved written-name type.
  TSR's `get_type_from_type_reference` ImportEquals arm returns `error`
  (`declared.rs`, type-refs box; cluster `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL`).
- `ramdaToolsNoInfinite2` ×9: once the aliases resolve, recursive
  indexed-access aliases are evaluated eagerly (`DropForth<…>` prints its
  `1:` arm). Default alias type arguments are also printed where tsgo keeps
  the written `List` / `_Drop<L2, Key<I>>`. Both are types and printing
  defects outside this lane.

What would make it land: the type-refs box's unknown-target arm, plus the
recursive-alias deferral. Then re-apply the two-arm change in
`is_exported_from_container` and re-measure.

## §2. TS2303 follows `resolveAlias`'s recursion, not two syntactic shapes

Cluster: the MISSING 2303 bucket (`recursiveExportAssignmentAndFindAliasedType1`–`6`,
`circular1`, `circular3`).

### The forcing constraint

Upstream reports `Circular definition of import alias` from `resolveAlias`
(`checker.go:16266`) when `popTypeResolution` fails. It fails for **every alias
whose resolution recursed back into itself**, because the target lookups
(`resolveEntityName`, `getExternalModuleMember`, `resolveExternalModuleSymbol`)
end in `resolveSymbol`. That function recurses into any non-local alias
(`IsNonLocalAlias`: an alias without its own value, type or namespace meaning).
§955/§956 (`docs/architecture/checker-notes-diag2.md`) ported two syntactic
shapes of that: an `import A = B; import B = A` chain, and
`import self = require("m")` inside `declare module "m"`. They missed `export =`
cycles through a file module and cross-file `export type { A } from` cycles.

### What was built

`circular_alias.rs` walks the same recursion. At each alias, it takes the
symbol upstream would recurse into: the module's `export=` for a
`require`/namespace import, the `Namespace`-meaning hit for `import x = y`, or
[`Checker::resolve_alias`]'s immediate target for every other form. The walk
continues only while the hop is a non-local alias. It reports when the walk
reaches the start, and stops on a repeat that is not the start (that alias only
*leads into* a cycle; upstream's frame pops `true` there). The check runs at
every alias-declaration check site: import-equals, import/export specifiers,
import clause, namespace import/export, and export assignment.

Alternative rejected: making `resolve_alias` recursive with a resolution frame.
That is upstream's shape, but `resolve_alias` is the non-recursive base every
types consumer relies on. Its own doc records that the frame could not fire
there. Changing it is a types-wide change for one diagnostic.

### Measured

Diagnostics +8 cases, 0 lost. `checker_types` unchanged. The self-ratio is
0.96 on `domain-model` and 1.009 on `generic-imports` (21 samples; it read 1.11
at 9 samples). The walk is per alias *declaration*, never per reference.

### Not converted

`exportAsNamespaceConflict` needs `initializeChecker`'s merge order
(`checker.go:1320-1343`). UMD `GlobalExports` enter `globals` first-in-wins,
*then* global augmentations merge into them, so `export = N` meets the UMD
alias. This binder keeps them apart, so the cycle never forms. That is a
`tsr-binder/src/lib.rs` globals change.

## §3. Synthetic default import target — built, measured, held back (cluster `tsr-2zk.16.6`)

**Not merged.** The patch is
`docs/parity/notes/names-modules-3-synthetic-default.diff`; it applies to
`crates/tsr-checker/src/symbols.rs` at this lane's head.

### What it ports

`getTargetOfModuleDefault`'s synthetic arm (`checker.go:14578-14585`): when
`canHaveSyntheticDefault` holds and the module has no `default`, the import
resolves to `resolveExternalModuleSymbol(module)`. That is the `export =`
target, or **the module itself** when there is none. It replaces
`module_default_target`'s ambient-only, variable-target-only gate. The gate
existed for naming (`tsr-4jk`); the old comment recorded 6 G→W on
`importEquals1` and 10 R→W on `typeof z4`. Neither loss reproduces on the
current tree.

It also ports the usage-dependent half of `canHaveSyntheticDefault`
(`:14823-14833`), so `.mts` importing `.d.mts`/`.mts` (both ESM) gets no
synthetic default. That half reads emit formats the checker cannot see
directly:

- `GetImpliedNodeFormatForEmit` comes from the host's default resolution mode
  under `node16`..`nodenext`, and otherwise from the extension. A `.ts`/`.js`
  file's package.json `type` is not visible to the checker, so it answers
  `None`. Upstream also answers `None` exactly when that `type` is absent.
- The usage emit syntax for an import declaration is the importing file's
  emit format.

A host method answering both directly would remove the approximation. It
needs a forwarding impl in `tsr-compiler/src/lib.rs`, outside this lane.

The `true` branch (ESM importing CommonJS under node16+) is **not** taken. With
it, `nodeNextCjsNamespaceImportDefault1` lost 3 lines: the default alias and
`import * as ns` then share one module symbol, and `ns` printed as `typeof d`.
Upstream keeps them apart with `cloneTypeAsModuleType` (`resolveESModuleSymbol`'s
ESM→CJS arm, cluster `tsr-2zk.16.22`).

### Measured (unfiltered, against the box baseline)

`checker_types` +67 aligned RIGHT lines over this lane's head (most in the
cluster's own cases), plus diagnostics `conformance/importEquals1` WRONG→RIGHT.
**One RIGHT line lost**, `importEquals1:6:0`, so the patch is not merged:

`import type types from './c'` wants `types : any`. Upstream's `getTypeOfNode`
answers a type-only import clause's *name* through `IsTypeDeclarationName` →
`getDeclaredTypeOfSymbol(alias)`, and a module has no declared type. The
producer (`crates/tsr-conformance/src/types_producer.rs`, near its type-only
specifier arm) ports that arm only for `ImportSpecifier`/`ExportSpecifier`, so
it falls to `get_type_of_symbol` and prints the now-resolved `typeof types`.
Before the patch, the line matched only because the alias did not resolve.

**To land:** extend the producer's type-only arm to an `ImportClause` whose
`phase_modifier` is `type`. Then apply the diff and re-measure. Perf was not
measured; the arm runs once per default import.
