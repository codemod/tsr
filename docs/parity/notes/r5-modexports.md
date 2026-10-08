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

## 5. Needed changes outside owned files

- **`declared.rs` `get_type_from_type_reference`, §491's alias road
  (r5-declared).** Admit a *renamed* ES import specifier when the target's
  declared type is site-independent, such as a literal or a primitive.
  `resolveTypeReferenceName` resolves through the alias regardless of the
  rename. Only the printed name depends on the site. Unlocks 2 lines × 10
  configurations of `arbitraryModuleNamespaceIdentifiers_module`, and the
  three TS2322s that case's `.errors.txt` records.

## 4. Environment

The offline bootstrap's `assemble.py` needs `tomlkit`, and PyPI answers 403
(r5-operators3 §4). I used a stdlib-only stand-in for `parse` (`tomllib`),
`inline_table` and `dumps`, kept in the session scratchpad and not committed,
and pointed `bootstrap.sh` at it through `PYTHONPATH` for one run. The edit
to `bootstrap.sh` was reverted before any commit. Running `verdictdump` with
`RAYON_NUM_THREADS=3` fit in memory on the 4-vCPU, 15 GB container.
