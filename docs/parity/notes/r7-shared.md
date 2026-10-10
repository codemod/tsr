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

## 2. `mergeSymbol` through an alias target (MERGE-SYMBOL-RESOLVE-ALIAS-TARGET, `.38`)

### Cause

`mergeModuleAugmentation` (`checker.go:1397`) runs `mergeSymbol(mainModule,
augmentation.Symbol)`, whose `mergeSymbolTable` recursion meets the target
module's export entries. An entry that is an **alias** (`export default I`,
`export type { Row } from "./common"`) is not merged as an alias:
`mergeSymbol` resolves the non-transient target (`resolveSymbol`,
`:14153`) and either merges the source into a clone of the resolved symbol,
or — the source's excludes hitting the resolved flags — reports
`reportMergeSymbolError(target, source)` and **returns `source`**.
`mergeSymbolTable` stores the return value (`target[id] = merged`,
`:14117`), so the conflicting augmentation declaration displaces the alias
in the module's exports. The binder declined every alias-target merge
(`alias_merges`, "nothing here follows aliases", `bd tsr-y4u.12`); only the
checker's TS2300/TS2451 report ran.

Second, `mergeSymbolTable` inserts `getMergedSymbol(sourceSymbol)` for a name
the target lacks. The binder inserted the raw source, so an augmentation's
`interface EventList` merged through `export *` into the re-exported
declaration (`:1433-1441`) entered `index`'s exports unmerged, and a second
augmentation of `./index` merged into that stale symbol instead of the one
`./eventList` exports.

### The port (`crates/tsr-binder`)

- `BindResult::merge_through_alias_targets`: after each augmentation's
  `merge_pairs`, every alias merge it declined is re-decided as upstream
  does. `resolve_alias_for_merge` is `resolveSymbol` over the forms this
  binder can follow with its own name resolution and the program's module
  resolution callback — `export default X`/`export = X` of an identifier,
  local and re-exported export specifiers, import specifiers and default
  imports of modules without `export =` — to the first non-alias symbol,
  merged, bounded (16 hops, cycle check). A conflict stores the source in the
  alias's slot and keeps the pair for `Checker::report_merge_conflicts`
  (unchanged, so TS2300/TS2451 still land on the alias's declarations); a
  merge unions the source into the resolved symbol in place (the binder's
  in-place merge stands for `cloneSymbol`, per `merge_symbol`'s doc), stores
  it in the slot and drops the pair.
- `Binder::merge_symbol` inserts `merged_symbol(source)` for a name the
  target lacks, in both `members` and `exports`.

Rejected: resolving through the checker. `merge_module_augmentations` runs
before any checker exists and the checker holds the `BindResult` immutably;
deferring the merge to the checker would make member tables checker-relative,
which is the shared-across-checkers state ADR-0003's side tables avoid.
Accepted limitation: an alias the binder cannot follow (an `export =`
member, a qualified import-equals, a namespace import) keeps the decline,
and upstream's `unknownSymbol` arm (an unresolvable alias takes the source)
is not taken, because "cannot follow here" is not "resolves to nothing".
`mergedParent` (`:14118`, re-parenting the merged symbol to the augmented
module) is not ported: in-place merging would re-parent the original
declaration, which upstream's clone never does.

Checker port convention: no cache or side table; the binder's existing
`merged` redirect map is the publication (written once per merge, before
the program is shared). The work boundary is one bounded syntactic alias
walk per declined augmentation merge.

Falsifier: a case where an augmentation merges into an alias whose target
this walk resolves differently from the checker's `resolve_alias_fully`
(the two must agree; a TS2300 on a pair the binder merged would show it).

### Measured (against §1's freeze)

+4 type lines, +4 diagnostics cases, zero losses:
`moduleAugmentationOfAlias` (diagnostics and types),
`mergeSymbolReexportedTypeAliasInstantiation` (diagnostics and types),
`mergeSymbolRexportFunction`, `mergeMultipleInterfacesReexported`.
Coverage: `checker_types` 8,681/9,538, `checker_types_configured`
1,753/1,928, `diagnostics` 4,788/5,502, `diagnostics_configured` 955/1,091.
Median child-CPU new/old (21 samples, against §1's binary): domain-model
1.0259, generic-imports 1.0117. Tests pass; clippy clean in touched code
(`tests/jsdoc_import_duplicates.rs`'s `implicit_clone` is pre-existing).

### Remaining in the augmentation clusters

- `augmentExportEquals3/4/6` and `augmentExportEquals5`'s `typeof e`
  lines: printer. A symbol with a non-global augmentation declaration is
  printed as an import type (`symbolToTypeNode`'s
  `hasNonGlobalAugmentationExternalModuleSymbol` arm): `typeof
  import("./file1")`. The merge already adds the declaration; routed to
  r7-printer.
- `augmentExportEquals5` `x.id` (3 lines), TS2454: §3 (held).
- `augmentExportEquals7`: `import * as lib` of an `export =` of a
  `var`+`namespace`: the synthetic default wrapper prints `{ default: () =>
  void; }`; `module_clone_type` / `getTypeWithSyntheticDefaultOnly` arm.
- `checkerInitializationCrash`: a `declare global` alias merge (`export
  import VNode = react.ReactNode` against `type VNode`) needs the
  qualified import-equals through a UMD `export =`, which the binder walk
  declines; TS2300 ×2 and `VNode` missing.

## 3. HELD: `resolveESModuleSymbol`'s pure-alias step in `getExternalModuleMember`

### Cause

`getExternalModuleMember` (`checker.go:14667`) reads members off
`resolveESModuleSymbol(moduleSymbol)`, whose second step (`:15570`) resolves
an `export =` symbol that is a pure alias (`export = e`, `IsNonLocalAlias`)
through `resolveIndirectionAlias`. `get_external_module_member` stopped at
`resolveExternalModuleSymbol(…, dontResolveAlias = true)`, so for `export =
e` it read the exports of the `export=` alias symbol itself (empty) and the
module's own table (only `export=`): `import { Request } from "express"`
resolved to nothing although the augmentation had merged `Request` into
`e` (`augmentExportEquals5`, `x.id : any`, TS2454 missing).

### Measured, and why it is held

`r7-shared-HELD-export-equals-pure-alias.diff`, against batch-2 main
`1b466dc8`: +3 type lines and +1 diagnostics case (`augmentExportEquals5`),
**−8 type lines** — refused. The losses are a printer gap the resolution
exposes, not a wrong target: `contextuallyTypedJsxAttribute2` ×7 and
`declarationEmitExportAssignedNamespaceNoTripleSlashTypesReference` 2:2
import `ElementType` / `Component` by name from react's `export = React`.
Unresolved, the reference printed its written text (`ElementType`,
`Component`), which matched native by accident; resolved, TSR prints the
reference with its filled default arguments (`ElementType<any>`,
`Component<any, {}, {}>`), where native's node builder omits type arguments
that equal the declared defaults for a reference written without them.
Re-land when the printer drops default-equal trailing arguments (routed,
r7-printer); the diff then needs re-measuring.

## 4. `resolveQualifiedName`'s left side through an alias (QUALIFIED-NAME-LEFT-ALIAS-RESOLVE)

### Cause

`import EnumA = Enum.A` after `import { Enum } from "./enum"`:
`getSymbolOfPartOfRightHandSideOfImportEquals` → `resolveEntityName` →
`resolveQualifiedName` (`checker.go:15828`) resolves the left side with
`resolveEntityName(left, Namespace, …, dontResolveAlias = false)`, whose
`resolveSymbol` follows the pure import alias to the enum, then reads `A`
from its exports; a merged alias whose own exports lack the name falls back
to its target's (`:15852`). `resolve_qualified_entity` read `exports` off
the alias symbol as found and answered `None`, so `EnumA` (merged with an
exported type alias in `importedEnumMemberMergedWithExportedAliasIsError`)
read `any`; r7-printer had kept an older same-file arm to cover that line.

### The port

Each left segment is merged, a pure alias (`is_non_local_pure_alias`) is
resolved through `resolve_alias`, and a missing name on a merged alias falls
back to the alias target's exports. No cache: `resolve_alias`'s existing
memo is the only state touched.

### Measured (against batch-4 main `660718af`)

+9 type lines, zero losses, diagnostics unchanged; 2 cases:
`importedEnumMemberMergedWithExportedAliasIsError` (the routed 1:1 line)
and `declarationEmitEnumReferenceViaImportEquals` (7 lines);
`leaveOptionalParameterAsWritten` 1:6 also turns right. One line moves
GAP→WRONG: `leaveOptionalParameterAsWritten` 2:5, where `export import Foo
= a.Foo` inside `declare global { namespace teams.calling }` now resolves as
native resolves it, and the printer spells the reference
`teams.calling.Foo | undefined` where native's accessible-chain search
answers `import("./a").Foo | undefined` (routed, r7-printer).
Coverage: `checker_types` 8,738/9,538, `checker_types_configured`
1,771/1,928, `diagnostics` 4,849/5,502, `diagnostics_configured` 968/1,091.
Median child-CPU new/old (21 samples): domain-model 1.0009,
generic-imports 1.0108. Workspace clippy (`--all-targets -D warnings`) clean;
`cargo test --workspace --release` passes.

## 5. ROUTED: native `resolveSymbol` for the symbol-chain comparisons (RESOLVE-ALIAS-INDIRECTION remainder)

### Cause

`resolveSymbolEx` (`checker.go:16258`) follows an alias only when it is pure
(`IsNonLocalAlias`); a symbol merged with another meaning (`import * as B`
merged with `interface B` and re-exported, `noCrashOnImportShadowing`) is its
own answer. `resolve_alias_fully` walks on through such merged symbols to the
end of the chain. In the printer's `trySymbolTable` comparisons
(`module_alias_at` and its siblings in `checker.rs`) that made `import { B }
from "./a"` look like an alias of module `./b`, so the module printed `typeof
B` where native, finding `B` resolves to a.ts's merged symbol, names it
through `import * as OriginalB` (`typeof OriginalB`).

### Measured and why it is not one global change

Making `resolve_alias_fully` itself stop at merged symbols: +11 type lines,
**3 losses** (`mergedWithLocalValue` 1:0, `shadowedInternalModule` 0:36,
`verbatimModuleSyntaxNoElisionCJS` 5:3). Those reach
`get_declared_type_of_alias`, which is native `getDeclaredTypeOfAlias` —
`getDeclaredTypeOfSymbol(resolveAlias(symbol))`, recursing through a merged
alias — so the walking form is the native one there. The 34 callers mirror
different native operations and have to be switched one by one.

`r7-shared-printer-resolve-symbol.diff` adds `Checker::resolve_symbol`
(symbols.rs, native `resolveSymbol`) and switches the seven `checker.rs`
symbol-chain call sites to it, keeping `get_declared_type_of_alias` on
`resolve_alias_fully`. Against `f11e37b8`'s freeze: **+11 type lines, 0
lost**, diagnostics unchanged; cases `noCrashOnImportShadowing` and
`typeAndNamespaceExportMerge`, plus five lines of
`exportTypeMergedWithExportStarAsNamespace`. `cargo clippy -p tsr-checker
--all-targets -D warnings` is clean with it applied. It cannot land from this
lane: the call sites are r7-printer's, and `resolve_symbol` alone would be
dead code under the clippy gate. Routed whole to the integrator.

Remaining `resolve_alias_fully` callers to audit against their native
operation (resolveSymbol vs resolveAlias vs getSymbolFlags' chain walk):
declared.rs ×3, flow.rs ×2, jsx_*.rs ×3, node_reuse.rs ×2,
import_type_meaning.rs ×2, symbols.rs ×6, one each in emit_helpers,
enum_initializer, members, meaning_mismatch, merge_conflicts, printing,
symbol_accessibility.
