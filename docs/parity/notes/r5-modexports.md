# Lane notes: r5-modexports (`tsr-2zk.991`, `tsr-2zk.992`)

Round-5 cloud box on epic `tsr-2zk`. Native source is
`vendor/typescript-go/internal/checker/` @ `5b1047d`. Baseline frozen at
integration head `4fac351`: diagnostics 10,929 right (5,348 RIGHT + 5,581
EMPTY_RIGHT) of 12,238 cases, types 543,975 RIGHT of 552,533 aligned lines.
Every number below comes from unfiltered `diagverdictdump` / `verdictdump`
runs against that baseline.

**Both items touch `symbols.rs`, which is main's active file, so neither is
committed as code.** Each item ships as a measured diff in this directory,
and the integrator lands it. Numbers are taken with the diff applied. A diff
carries its own new module (`crates/tsr-checker/src/module_exports.rs`), the
one-line `lib.rs` registration, and its test file. Committed without its
`symbols.rs` callers, that module would be dead code and fail
`clippy -D warnings`. The diffs apply in order: string names first, then
synthetic defaults.

## 1. String-literal module export names (`tsr-2zk.991`)

Diff: [`r5-modexports-string-names.diff`](r5-modexports-string-names.diff).

### Forcing constraint

ES2022 lets a specifier name an export with a string literal:
`export { x as "<X>" }`, `import { "<X>" as y }`, `export * as "<Z>"`.
Upstream never tells the two spellings apart once it has the name. Every
reader goes through `Node.Text()`:

- `getExternalModuleMember` (`checker.go:14667`) reads
  `specifier.PropertyNameOrName().Text()`;
- `getTargetOfImportSpecifier` and `getTargetOfExportSpecifier`
  (`checker.go:14647`, `:14951`) test `ast.ModuleExportNameIsDefault`
  (`internal/ast/utilities.go:2539`), which is `Text() == "default"`.

The binder already keys `exports` by the same text (`export_name` in
`crates/tsr-binder/src/binder.rs`, the port of `getDeclarationName`). So
`export { x as "<X>" }` was bound under `<X>`, but the checker's lookup
returned `None` for any `StringLiteral` name. The comment there gave the
reason: "the two spellings have not been checked to agree". This item checks
them. The binder stores the literal's cooked value (`StringLiteral.text`),
and that is what `Text()` returns. The test
`a_string_literal_name_that_spells_an_identifier_export_resolves` pins it:
`import { "v" as y }` finds `export const v`.

### What was ported

- `module_export_name_text` (`Node.Text()` of a `ModuleExportName`) and
  `module_export_name_is_default` (`ast.ModuleExportNameIsDefault`), in the
  new module `module_exports.rs`.
- `get_external_module_member` looks up string-literal names.
- Both `default` tests (`import_specifier_target`, `export_specifier_target`)
  go through `module_export_name_is_default`, so `{ "default" as d }` takes
  the default road, as upstream's does.

The same-file `export { "<X>" as y }` with no module specifier keeps
answering `None`. That is upstream's `case ast.IsStringLiteral(name):
resolved = nil` (`checker.go:14966`), and it is correct, not a gap.

### Measured (diff applied, against `4fac351`)

- Types: 543,975 → 544,150 RIGHT (**+175 lines**). 170 of them are in
  `arbitraryModuleNamespaceIdentifiers_module` (17 per configuration × 10),
  3 in `bigintArbirtraryIdentifier`, and 2 in
  `arbitraryModuleNamespaceIdentifiers_exportEmpty`.
- Diagnostics: 5,348 → 5,349 RIGHT
  (`arbitraryModuleNamespaceIdentifiers_exportEmpty` WRONG → RIGHT).
- **Losses: none** on either dump.
- Coverage run: `checker_types` 8,222 → 8,225 / 9,538, assertion lines
  471,199 → 471,234; `checker_types_configured` 1,637 / 1,928, lines
  72,746 → 72,916; `diagnostics` 4,515 → 4,516 / 5,502;
  `diagnostics_configured` 833 / 1,089 unchanged.
- Callgrind Ir (`tsr -p … --noEmit --singleThreaded true`, release):
  generic-imports 373,068,425 → 373,060,918 (−0.002%), domain-model
  1,233,593,901 → 1,233,928,147 (+0.027%). Re-running the base binary moves
  domain-model by ±0.03%, so both are noise. The change adds no work to an
  identifier name: it is the same match, with one more arm.

### What remains in the case, and why it is not here

Two lines per configuration stay WRONG (`>importTest : "someType"`, and the
same line for `reimportTest`). In both, a **renamed** type-only import
(`import { type "<A>" as typeA }`) is used in a type position. The identifier
spelling, `import { type someType as T }`, gaps the same way. `declared.rs`
`get_type_from_type_reference` declines renamed ES import specifiers (§491:
the target's declared type would print the target's name where upstream
prints the local one, the §158 per-site naming wall). These two lines are
literal types, which print the same at every site. Allowing them is a change
to `declared.rs`, which r5-declared owns, so it is reported, not made (§5).

## 2. JSON modules under node16+ ESM: `{ default: T }` (`tsr-2zk.992`)

Diff: [`r5-modexports-synthetic-default.diff`](r5-modexports-synthetic-default.diff)
(applies after §1's).

### Forcing constraint

In Node.js, ESM gets no named exports from a JSON module.
`isOnlyImportableAsDefault` (`checker.go:14800`) encodes that rule, and two
places read it:

- **`getTargetOfModuleDefault`** (`checker.go:14536`): a default import is
  the module itself. This was already ported
  (`symbols.rs` `module_default_target`).
- **`getTypeWithSyntheticDefaultOnly`** (`checker.go:15632`): a namespace
  import (`resolveESModuleSymbol`, `checker.go:15568`) or a dynamic
  `import()` (`checkImportCallExpression`, `checker.go:8307`) of such a
  module is `{ default: T }`. This was missing. The port answered the JSON
  object itself, so `ns.default` read `any` and `ns.version` read `number`
  where native reports TS2339.

### What was ported

- `get_type_with_synthetic_default_only` and
  `create_default_property_wrapper_for_module` (`checker.go:15707`), in
  `module_exports.rs`. `module_clone_type` calls the first for a namespace
  import before any other arm, as `resolveESModuleSymbol` does.
  `check_import_call_expression` (`calls.rs`) calls it for a dynamic import.
- **The wrapper's member has no symbol.** Native mints a transient alias
  symbol, `default`, whose `aliasTarget` is the module. This checker cannot
  add symbols to the binder's store (the limitation `get_external_module_member`
  already records for `combineValueAndTypeSymbols`). The member is an
  `AnonymousProperty` that carries the module's type directly, through the
  constructor `getRestType`'s objects use (`mint_rest_properties`).
  Property access, enumeration, assignability and printing already read that
  representation without a symbol. Before relying on it, I ran a probe,
  `const {a, ...r} = o; const x: number = r.b; r.c`, through the release
  binary. It reported TS2322 and TS2339 on such an object, as native does.
  - Rejected: overloading `module_value_clones`. Its member reads fall
    through to the source type for every name other than `default`, which
    would expose the JSON members that native hides. Its site printer also
    declines a source that is not `TypeData::Anonymous`. Each would need a
    special case in `members.rs` and `checker.rs`.
  - What would make the rejected option win: a checker-owned transient
    symbol store, which `symbol_access.rs`'s `PrivateSymbol` begins but no
    checker path uses yet. With it, the wrapper becomes native's alias
    member, and this representation should be replaced.

### `import … with { type: "json" }` declined through the default road

`import_clause_default_target` and both `default`-specifier arms declined any
import that carried attributes. §292 added that decline when attribute
validity was unported, because "upstream errors the whole import".
`getTargetOfImportClause` (`checker.go:14528`) never reads the attributes.
Native types `pkg` in `import pkg from "./package.json" with { type: "json" }`
as the JSON object, and `checkImportAttributes` reports on the attributes
alone. r5-modules has since ported that check (`import_attributes.rs`), so
the decline now only blocked correct types. It is removed, which narrows a
decline toward upstream. The two lines §292 cited, `importAttributes7/8`
`>a : { a: string; … }`, were WRONG (`any`) at the baseline and are RIGHT
now.

## 3. `getTypeWithSyntheticDefaultImportType`: CommonJS targets

A namespace import or dynamic import of a module that **can have a
synthetic default for this usage** (`canHaveSyntheticDefault`,
`checker.go:14818`, already ported as
`can_have_synthetic_default_for_usage`) reads as the module type spread with
`{ default: <the module> }` (`checker.go:15646`). Per emit, `__importStar`
wraps a module with no `__esModule` marker.

- **Namespace imports.** `resolveESModuleSymbol` asks for it when the module
  type has signatures, a `default` member, or is an ESM-to-CJS reference. The
  port already had the signature and ESM-to-CJS arms (`module_clone_type`,
  names-modules notes §5). The **`default`-member arm** was missing. It is
  `namespace_import_default_member_type`. Example: `import * as ns from
  "./package.json"` in a `.cts` file, where `package.json` has a `"default"`
  key. When the module cannot have a synthetic default, native's answer is
  the plain module type cloned. The port keeps its existing plain answer, so
  that case changes nothing.
- **Dynamic `import()`.** `checkImportCallExpression` asks for it on every
  resolved module. The port asks only when the module has no `export =`,
  because its `typeof import("m")` mint stands for the module symbol's type,
  not the `export =` target's. Asking for an `export =` module would spread
  the wrong members. That case stays as it was, recorded here as a decline:
  it waits for an import-call type built from
  `getTypeOfSymbol(resolveExternalModuleSymbol(m))`.
- The spread is the port's existing `getSpreadType` (`spreads.rs`
  `get_spread_type`). The diff changes only its visibility to `pub(crate)`.
  The members are ordered as native orders them: the module's properties by
  declaration, then the synthetic `default`, which has no declaration.

### Convention record (`docs/conventions.md`, checker ports)

- **Pinned native operation:** `cachedTypes` under
  `CachedTypeKindDefaultOnlyType` and `CachedTypeKindSyntheticType`
  (`checker.go:15635`, `:15648`).
- **Key and owner:** `(SyntheticDefaultKind, module value TypeId)`, owned by
  `Checker::synthetic_default_types` (`checker.rs`). Native keys by `t.id`
  alone. The kind is the cache-kind half of native's `CachedTypeKey`.
- **Publication:** inserted once, complete, never revised. The wrapper is
  minted before insertion, and nothing can re-enter, because building it
  reads only the already computed value type and its printed text.
- **Receiver or alias context:** none. Native's synthetic-import cache
  ignores the usage even though `canHaveSyntheticDefault` depends on it: the
  first usage to ask decides for every later usage of the same value type.
  That quirk is kept, not corrected. Changing it would change which types
  are shared.
- **Expensive work:** `get_type_of_symbol` of the module (already cached
  per symbol) and the spread. Both sit behind `isOnlyImportableAsDefault`
  (one module-kind comparison) or behind `can_have_synthetic_default_for_usage`.
  Ordinary ES imports of TypeScript modules return at the
  `usageMode == ESNext && targetMode == ESNext` arm or at the `export=` test.

### Measured (both diffs applied, against `4fac351`; "item" = this diff alone, on top of §1's)

- Types: 544,150 → 544,455 RIGHT (**+305 lines** for this item; +480 for
  both diffs over the baseline).
  - `nodeModulesResolveJsonModule`: 37 per configuration × 4.
  - `nodeModulesDeclarationEmitDynamicImportWithPackageExports`: 22 × 3.
  - `nodeModulesJson`: 11 × 4.
  - `nodeModulesImportAttributes` and `nodeModulesImportAssertions`:
    4 × 4 each.
  - `dynamicImportsDeclaration` 6, `crashDeclareGlobalTypeofExport` 3,
    `resolutionModeCache` 2, and `importAttributes7`, `importAttributes8`,
    `importAttributes11` and `intersectionsAndEmptyObjects` 1 each.
- Diagnostics: unchanged (5,349 RIGHT, 5,581 EMPTY_RIGHT).
- **Losses: none** on either dump, against both the baseline and the §1
  state.
- Coverage run: `checker_types` 8,225 → 8,229 / 9,538, assertion lines
  471,234 → 471,249; `checker_types_configured` 1,637 / 1,928, lines
  72,916 → 73,206; `diagnostics` 4,516 / 5,502 and `diagnostics_configured`
  833 / 1,089 unchanged.
- Callgrind Ir, this item against §1's binary: generic-imports
  373,065,836 → 373,061,986 (−0.001%), domain-model 1,233,590,432 →
  1,233,614,660 (+0.002%). Neither project has a node16 JSON import or a
  CommonJS dynamic import, so neither reaches the new work.

## 4. Environment

The offline bootstrap's `assemble.py` needs `tomlkit`, and PyPI answers 403
(r5-operators3 §4). I used a stdlib-only stand-in for `parse` (`tomllib`),
`inline_table` and `dumps`, kept in the session scratchpad and not committed,
and pointed `bootstrap.sh` at it through `PYTHONPATH` for one run. The edit
to `bootstrap.sh` was reverted before any commit. Running `verdictdump` with
`RAYON_NUM_THREADS=3` fit in memory on the 4-vCPU, 15 GB container.

## 5. Needed changes outside owned files

- **`declared.rs` `get_type_from_type_reference`, §491's alias road
  (r5-declared).** Admit a *renamed* ES import specifier when the target's
  declared type is site-independent, such as a literal or a primitive.
  `resolveTypeReferenceName` resolves through the alias regardless of the
  rename. Only the printed name depends on the site. Unlocks 2 lines × 10
  configurations of `arbitraryModuleNamespaceIdentifiers_module`, and the
  three TS2322s that case's `.errors.txt` records.
- **`spreads.rs` `spread_properties` (unowned).** A property copied from a
  `Function`-flagged symbol should keep native's method form.
  `getSpreadSymbol` returns the property symbol itself, and the node builder
  prints a `Function | Method` property with call signatures as a method
  (`f(): Promise<void>`). The port sets `method` from `METHOD` alone, so it
  prints `f: () => Promise<void>`. Blocks
  `nodeModules{,AllowJs}SynchronousCallErrors` (4 lines × 4 configurations
  each). Those lines also need `default: typeof mod2`, the import site's
  alias name for the module, which is the printing lane's site-naming work.
- **Printed module specifiers (r5-modules, `tsr-2zk.989`).**
  `nodeModulesImportAttributesTypeModeDeclarationEmitErrors` now gets the
  synthetic default (`Promise<{ default: typeof import(…); }>`), but it
  prints the written `"pkg"` where native prints
  `"./node_modules/pkg/import"`.
- **`nodeModulesJson` `typed : typeof typed`.** A default import of a
  `.d.json.ts` module takes the module itself (`hasDefaultOnly`). That part
  is already ported. What is missing is printing the module object as
  `typeof <alias>` through a default-import alias.
- **JSON files are not `JAVASCRIPT_FILE` (`tsr-compiler` loader).** tsgo's
  parser gives a JSON file `NodeFlagsJavaScriptFile | NodeFlagsJsonFile`
  (`internal/parser/parser.go:306`). This port's loader and `Program` stamp
  `JAVASCRIPT_FILE` from the `.js`-family extensions only. Nothing in this
  lane depends on it: a JSON module binds `export=`, so
  `canHaveSyntheticDefault`'s TypeScript arm answers the same `true` as the
  JavaScript arm would. A JSON file with an `"__esModule"` key would tell
  the two arms apart. I wrote a checker-side union of the two flags, found
  it measurably inert, and removed it rather than carry an untested special
  read.
