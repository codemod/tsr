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
