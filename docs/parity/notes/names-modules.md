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

## §3. Synthetic default import target (cluster `tsr-2zk.16.6`)

**Landed in round 3** — see "Round 3" at the end of this section. The
round-1/round-2 text below records why it was held, and is kept as written.
The held patch (`names-modules-3-synthetic-default.diff`) was removed when
the port landed; it is in git history before that commit.

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

### Round 2: the producer arm landed; the diff is still held back

**Landed:** the producer arm, in `type_id_at_location_tracking`
(`crates/tsr-conformance/src/types_producer.rs`). A type-only import
clause's own name answers `getDeclaredTypeOfSymbol(resolveAlias(alias))`,
including the error type, unlike the specifier arm, which falls through on
error. Measured on its own against the box baseline re-frozen at `c95b9c4`
(after §4 and a `main` merge): `checker_types` **+4 aligned RIGHT lines**
(`exportDefault:6:0`, `filterNamespace_import:1:0`, `importClause_default:1:0`,
`verbatimModuleSyntaxNoElisionESM:6:0`), 0 lost; diagnostics unchanged.

**Held back again: the diff.** Applied on top of the producer arm it measured
diagnostics **+2 cases** (`importEquals1`, `exportEqualsDefaultProperty`),
`checker_types` **+74 RIGHT lines**, 0 lost, and 3 GAP→WRONG. The 3 are the
`extends React.Component<…>` base expressions in
`tsxReactPropsInferenceSucceedsOnIntersections` and
`tsxSpreadDoesNotReportExcessProps`, now resolved and printed `typeof
React.Component` by the producer's base-class workaround. It is not merged
because the tree changed under it. Since round 1, `main` added a guarded
file-module synthetic default (`module_default_target`'s `synthetic` walk in
`symbols.rs`), and its refusals are pinned by unit tests in another stream's
file:

- `tests/module_default_file_owner.rs`: `only_complete_chains_within_the_naming_bound_are_admitted`
  and `unsupported_alias_shapes_and_cycles_decline_before_following_them`
  assert `resolve_alias == None` for 13 `export =` wrapper shapes. With the
  diff, the import **clause** resolves every shape to the wrapper's `export=`.
  The specifier forms (`{ default as x }`, `export { default as y }`) keep the
  declines, so the two forms disagree. Upstream resolves both
  (`getExternalModuleMember`'s `default` arm).
- `symbols.rs`'s own `semantic_type_naming_targets_preserve_identity_and_decline_incomplete_routes`
  asserts the same `None` for an ambient `export =` namespace as its
  precondition.

The faithful fix updates those guards to upstream's answers and gives the
specifier path the same synthetic arm. That is the owner's call, routed in
the round-2 report.

Two variants were measured and refused:

- **Ambient modules only** (file modules left to the guarded walk):
  +22 lines and **2 diagnostics lost** (`esModuleInteropDefaultMemberMustBeSyntacticallyDefaultExport`,
  `nodeNextEsmImportsOfPackagesWithExtensionlessMains`).
- **`resolveExportByName` in `can_have_synthetic_default`'s declaration-file
  arm** (look through `export =` for `default` / `__esModule`, with
  `isSyntacticDefault`). It is needed for the jsx naming test's `marked`
  module (`const __esModule: true` inside an `export =` namespace), but alone
  it **lost `nodeNextEsmImportsOfPackagesWithExtensionlessMains`**: upstream
  returns `true` from the `node16` usage block (ESM importing CommonJS) before
  the declaration-file arm. That is the `true` branch the diff declines (see
  above). The two must land together.

### Round 3: landed as upstream's whole function

`module_default_target` (`crates/tsr-checker/src/symbols.rs`) is now
`getTargetOfModuleDefault` (`checker.go:14536`) with `dontResolveAlias =
true`, for every caller: the import clause, `import { default as x }` and
`export { default as y } from`. In order: the `module.exports` arm
(unchanged); the real `default` through `resolveExportByName` (skipped for a
shorthand ambient module); then, when `canHaveSyntheticDefault` or
`isOnlyImportableAsDefault` holds, `resolveExternalModuleSymbol(module)` — the
immediate `export=` symbol, or the module itself. The synthetic arm overrides
a real `.default`, as upstream's comment says it must.

Removed with it, because upstream has none of them: the guarded file-module
walk (CommonJS only, TS only, at most 8 hops, uncloned, no signatures), the
clause-only JSON and JS arms (both are `canHaveSyntheticDefault`'s JS arm —
a JSON file is parsed `JavaScriptFile | JsonFile`, `parser.go:306`), and the
ambient-only, variable-target-only gate.

`can_have_synthetic_default_for_usage` is `canHaveSyntheticDefault`
(`checker.go:14818`) whole, against the host's real
`emit_syntax_for_usage_location` / `implied_node_format_for_emit` (added since
round 1, so the extension-only approximation the round-1 patch carried is
gone):

- the usage block: ESM usage of a CommonJS file under `node16`..`nodenext`
  answers `true`; ESM usage of an ESM file answers `false`;
- the declaration-file arm through `resolveExportByName`, so an `export =`
  value's own `default` property (if `isSyntacticDefault`) or `__esModule`
  property suppresses the synthetic default;
- the TS (`export =`) and JS arms as before.

These two pieces were measured to need each other (round 2): the
`resolveExportByName` arm alone lost
`nodeNextEsmImportsOfPackagesWithExtensionlessMains`, and the `true` branch
alone made `import * as ns` print `typeof d` (§5 is what fixes that).

The usage-free [`Checker::can_have_synthetic_default`] in `check.rs` stays
for TS1192 and the two missing-member readers (`missing_default_established`,
the TS2305 `default` arm). It is a hub-file function; routing the
diagnostics through the usage-aware port is a separate measured change.

**The tests that pinned the old declines now assert upstream's answer.**
`tests/module_default_file_owner.rs` (in `tsr-conformance`):
`only_complete_chains_…` became `chains_past_the_naming_bound_still_resolve`
(depth 5 resolves and types as the module, like depth 4), and
`unsupported_alias_shapes_and_cycles_decline_…` became
`every_alias_shape_resolves_to_the_immediate_export_equals` (all 13 shapes
resolve to the wrapper's `export=`; the three self-importing wrappers still
type as `errorType`). `symbols.rs`'s
`semantic_type_naming_targets_…` precondition now expects the `export=`
symbol, asserted before the publication snapshot because
`resolveExportByName` reads the `export=` value's type. Two
`tests/cross_file_aliases.rs` controls (`a_default_{named_import,re_export}_…`)
expected `errorType`; upstream types both as the whole `export =` value
`{ default: number; }` — still not the numeric `default` property the tests
guard against.

Measured with §5, unfiltered, against the box baseline at `d109b0c`:
diagnostics **+2 cases** (`exportEqualsDefaultProperty`, `importEquals1`),
`checker_types` **+100 aligned RIGHT lines**, **0 lost** in either. Largest
gains: `allowSyntheticDefaultImports9` 10, `importEquals1` 9,
`nodeNextCjsNamespaceImportDefault1`/`2` 9 each, `modulePreserve4` 7.
Median CPU self-ratio 1.006 (`domain-model`) and 0.981 (`generic-imports`),
21 samples, diagnostics matching.

## §4. Non-global module augmentations merge (`mergeModuleAugmentation`, cluster `tsr-2zk.38` / `tsr-2zk.16.15`)

### The forcing constraint

`declare module "./observable" { interface Observable<T> { map… } }` in a
module file never reached the module it augments. The binder merged only
`declare global` blocks (`merge_globals`); `merge_symbol`'s doc listed module
augmentation as not merged because it "needs module resolution". Every member
an augmentation adds was therefore missing: false TS2339 on augmented members
(`moduleAugmentationImportsAndExports*`, `umd-augmentation-1`–`4`,
`augmentExportEquals3`/`4`/`6`), false TS2305 on augmented exports
(`declarationEmitRedundantTripleSlashModuleAugmentation`,
`selfNameModuleAugmentation`), and missing merge diagnostics
(`duplicateIdentifierRelatedSpans_moduleAugmentation` TS2451,
`augmentExportEquals7` TS2649).

Upstream does it in `initializeChecker`'s last loop (`checker.go:1384-1391`),
after every file is bound and the globals exist, through
`mergeModuleAugmentation` (`:1405-1448`).

### What was built

- **The binder records the augmentations** (`Binder::external_module_augmentation_name`,
  `crates/tsr-binder/src/binder.rs`): a string-named module declaration in an
  ambient context placed where `IsModuleAugmentationExternal` holds — the same
  two shapes `collectModuleReferences` collects. One record per augmentation
  *symbol*, which is upstream's `Declarations[0] != moduleNode` gate. A nested
  relative name is excluded later, by the resolver callback, because the binder
  has no `tspath` (adding the dependency would change `Cargo.lock`).
- **`BindResult::merge_module_augmentations`** (`crates/tsr-binder/src/lib.rs`)
  runs once all files are bound, from `Program::bind_source_files`
  (`crates/tsr-compiler/src/lib.rs`). The program supplies resolution:
  `tryFindAmbientModule` then the loader's resolved module in the usage's mode —
  the same two steps the checker's `resolve_external_module_name` takes. Per
  augmentation it follows `export =`, requires `Namespace` meaning, runs the
  `export *` arm (an augmentation export naming a re-exported symbol merges into
  that symbol), and then `mergeSymbol(target, augmentation)` through the
  binder's existing in-place `merge_symbol`.
- **A refused top-level merge leaves no redirect** (`Binder::merge_pairs`).
  `merge_symbol` records `source → target` *before* its excludes test, which
  the globals pass relies on; upstream's `recordMergedSymbol` runs only on the
  union path. Without taking the redirect back, `augmentExportEquals7`'s
  augmentation (refused with TS2649) resolved to the `var lib` it failed to
  merge into and its name printed `typeof lib` instead of `typeof import("lib")`.

Alternative rejected: running the merge inside the checker. That is
upstream's location, but the checker holds `&BindResult`, many checkers share
one bind (`tsr-execute`'s pool), and the binder already does the global half
in place. The binder is the only owner that can mutate symbols; the compiler
is the only owner of resolution; the callback joins them.

### Consequences that had to be ported with it

Merging adds declarations from another file to a module symbol, and three
readers assumed that could not happen:

1. **Printing a module's specifier** (`Checker::module_specifier_for_symbol`,
   extracted from `type_to_string_at_worker`). It required exactly one
   declaration. It now mirrors `getSpecifierForModuleSymbol`:
   `tryGetModuleNameFromAmbientModule` first (a string-named declaration that is
   not a relative external augmentation), then the source-file declaration,
   spelled relative to the reference's file with `index` stripped. A
   `node_modules` file and the `.mts`/`.cts` forms decline (gap).
2. **`Checker::is_ambient_module`** asked whether *any* declaration was a
   string-named module. An augmentation of `export = moment` (a
   function-and-namespace) adds a `declare module "moment"` declaration to the
   function, which then printed as `typeof import("moment")` (5 lines lost in
   `moduleAugmentationDuringSyntheticDefaultCheck`). It now asks upstream's
   `IsAmbientModuleSymbolName(symbol.Name)`: the binder's quoted name.
3. **TS2395** (`merged_export_spaces.rs`) read only the `export` keyword. An
   augmentation's `interface Observable<T>` is exported by its ambient export
   context, which is `getEffectiveDeclarationFlags`' ambient arm
   (`checker.go:3701`); without it the pair read as exported plus local
   (false TS2395 in `moduleAugmentationExtendFileModule1`/`2`). The arm applies
   only in a *module* container: a script declares into `Locals`, has no
   `ExportSymbol`, and upstream's check returns early (measured: applying it to
   script `.d.ts` files lost `classAndInterfaceMerge.d` and
   `declarationFilesWithTypeReferences1`/`4`).
4. **TS2391** (`check_function_or_constructor_symbol`, `check.rs`) declines a
   symbol whose declarations span files. `missingFunctionImplementation2`'s
   augmentation adds an ambient `declare function f` to the other file's `f`.
   An ambient declaration only resets the adjacency chain in that loop, so when
   declarations span files the ambient ones are dropped and the rest are
   checked in their own file.

### Measured (unfiltered, against the box baseline at `15f1743`)

Diagnostics **+23 cases**: `augmentExportEquals3`/`4`/`6`/`7`,
`declarationEmitForModuleImportingModuleAugmentationRetainsImport`,
`declarationEmitRedundantTripleSlashModuleAugmentation`,
`duplicateIdentifierRelatedSpans_moduleAugmentation`, `mixedExports`,
`moduleAugmentationDoesNamespaceEnumMergeOfReexport`,
`moduleAugmentationEnumClassMergeOfReexportIsError`,
`moduleAugmentationImportsAndExports1`/`4`/`5`/`6`, `moduleAugmentationNoNewNames`,
`module_augmentUninstantiatedModule2`, `stackDepthLimitCastingType`,
`typeReferenceDirectives9`, `selfNameModuleAugmentation`,
`umd-augmentation-1`–`4`. `checker_types` **+209 aligned RIGHT lines**
(190 WRONG→RIGHT, 19 GAP→RIGHT), **0 RIGHT lines lost**.

**Two diagnostics verdicts move from EMPTY_RIGHT to EMPTY_WRONG, and they are
oracle artifacts, not losses**: `duplicateIdentifierRelatedSpans6`/`7` are
`@pretty: true` tests. Their `.errors.txt` is ANSI-coloured, and
`crates/tsr-conformance/src/errors_baseline.rs` parses it as *no*
diagnostics. The baseline itself records TS2300 on all six members — exactly
what this port now reports, because the two `declare module "someMod"`
blocks now merge. They were EMPTY_RIGHT only because nothing merged. Fix
belongs in the baseline parser (not this lane's file).

### Declined, each a gap rather than a wrong merge

- **Pattern ambient targets** (`declare module "*.foo"`): upstream merges the
  pattern *into* the augmentation unidirectionally and keeps the result in
  `patternAmbientModuleAugmentations`. This port has no such table.
  *(Landed in round 3, §6.)*
- **`export =` through anything but a local name**: the binder cannot resolve
  `export = require(…)` or a qualified name; such an augmentation is skipped.
- **Re-exported aliases**: `mergeSymbol` resolves an alias target
  (`resolveSymbol`) and merges into what it names
  (`moduleAugmentationDoes{Interface,Namespace}MergeOfReexport`). The binder's
  `merge_symbol` still declines an alias on either side (`bd tsr-y4u.12`).
- **TS2649 for a non-namespace target** (`Cannot augment module '{0}' because
  it resolves to a non-module entity`, `checker.go:1445`) is not reported; the
  augmentation is just not merged.

## §5. An ESM namespace import of a CommonJS file is a module clone (cluster `tsr-2zk.16.22`, part)

`resolveESModuleSymbol` (`checker.go:15568`) clones the module for a
namespace import when `isEsmCjsRef` holds (ESM usage, CommonJS target file),
even without signatures, and the clone's type gains a synthetic `default`
(`getTypeWithSyntheticDefaultImportType`). Without it, `import d from
'./a.cjs'` (now the module itself, §3) and `import * as ns from './a.cjs'`
share one module type, and the printer names it by the first alias it finds
(`ns : typeof d`).

`module_clone_type` already built clones for class/function targets in
`module_value_clones`; it now also clones a `VALUE_MODULE` target when
`namespace_import_is_esm_cjs_ref` holds, with no signature requirement and no
copied properties (member reads delegate to the source module, the existing
clone convention). `module_clone_default_symbol` answers `default` with the
`export=` symbol as before, or — for an ESM-to-CJS reference of a module
without one — the module itself, gated by the usage-aware
`can_have_synthetic_default_for_usage`. The clone prints through
`module_clone_name_at`, which names it by the alias whose type it is (`typeof
ns`).

Not ported: the other two clone triggers for a value module — a module type
with a real `default` property (`getPropertyOfTypeEx(typ, "default")`) under
an ES-syntax namespace import, and `getTypeWithSyntheticDefaultOnly` (JSON
under `node16`+). Both stay on the uncloned road.

Converted: `nodeNextCjsNamespaceImportDefault1`/`2` fully RIGHT (16 lines
each), counted in §3's measurement.

## §6. Pattern ambient modules resolve, and augmentations of them merge one way

Cases: `ambientDeclarationsPatterns_merging1`/`2`/`3` (false TS2664, missing
TS2305/TS2339).

### The forcing constraint

`resolveExternalModule` (`checker.go:15364-15372`) ends, after the ambient
lookup and the program's file resolution both miss, with
`core.FindBestPatternMatch` over the program's `declare module "prefix*suffix"`
declarations. This port stopped at the file resolution: every import of
`"a.foo"` answered `None` (types `errorType`; TS2307 suppressed by
`has_pattern_ambient_module`'s silence), and a module augmentation
`declare module "a.foo" { … }` in a module file reported TS2664 `cannot be
found`, because `check_module_augmentation_name` asks the same resolver.

And when the augmentation's name does resolve to a pattern,
`mergeModuleAugmentation` (`checker.go:1422-1432`) merges the other way
round: `mergeSymbol(augmentation, pattern, unidirectional = true)`, then
records the result under the augmentation's *name* in
`patternAmbientModuleAugmentations`. An import of `"a.foo"` then sees the
pattern's exports plus the augmentation's; an import of `"b.foo"` sees only
the pattern's.

### What was built

- `BindResult::pattern_ambient_module` (`crates/tsr-binder/src/lib.rs`):
  `FindBestPatternMatch` over the quoted single-`*` `ValueModule` globals
  (upstream's `patternAmbientModules` list is per declaration in file order;
  the globals are a hash table, so ties on prefix length go to the earliest
  first declaration), then the recorded augmentation for that exact name.
  No cache: it runs only for a specifier the ambient table and the program
  both failed to resolve, and only in programs that declare a pattern
  (`has_pattern_ambient_modules`).
- The checker's `resolve_external_module_name` and the compiler's
  augmentation-resolution callback (`Program::merge_module_augmentations`)
  call it after their file resolution misses, which is upstream's order.
- `merge_module_augmentations` takes the pattern arm before the ordinary
  merge: `merge_pairs(&[(augmentation, pattern)])`, then restores the
  `merged` redirect table to what it was before the call. `merge_symbol`
  records `source → target` at every level as it unions, and a
  unidirectional merge records none (`recordMergedSymbol` is skipped when
  `unidirectional`), so restoring the whole table is exactly upstream's
  effect without a new flag through the decls box's `merge_symbol`. A second
  augmentation of the same name then resolves (through the table) to the
  first one and merges into it bidirectionally, as upstream does
  (`ambientDeclarationsPatterns_merging2`).
- The table rides `Binder` across `resuming`/`into_result`
  (`pattern_ambient_module_augmentations`), like every other `BindResult`
  field.

In place rather than upstream's clone: upstream's unidirectional merge clones
the (non-transient) augmentation and records `augmentation → clone`. Here the
augmentation symbol itself receives the pattern's exports, and a same-named
export of both is unioned into the *augmentation's* export (`merging3`'s
`OhNo`). Nothing but the augmentation reaches those symbols, so the in-place
union is observably the same; the pattern's own symbols are only read.

### Measured

Unfiltered, against the box baseline re-frozen at `8297e51` (§3 plus an
integration-branch merge): diagnostics **+3 cases**
(`ambientDeclarationsPatterns_merging1`/`2`/`3`), `checker_types` **+21
aligned RIGHT lines** (`ambientDeclarationsPatterns` 13, the three merging
cases 8), **0 lost** in either.

The TS2307 rule (`module_specifier_unfindable`, `check.rs`) declined whenever
*any* pattern module existed, because a match was possible and could not be
tested. It now declines only when a pattern matches. Measured with the rest of
this section: no diagnostics verdict moved from it alone.
