# Parity lane: r5-modules (tsr-2zk.994)

Round-5 box on epic `tsr-2zk`, pinned vendor `5b1047d`. The lane's three
clusters come from r5-variants2's census (`docs/parity/notes/r5-variants2.md`
§4 on its branch): module specifiers in printed types (`tsr-2zk.989`),
`import.meta` (`tsr-2zk.990`) and `checkImportAttributes` (`tsr-2zk.986`).
Their population is mostly configured rows (`case(opt=val)` keys, ADR-0047),
so every loss check below joins both dumps on plain *and* configured keys.

## 1. Baseline

Frozen at `a7d108d` merged with `origin/main` `13da1cf` (docs-only
commits), before any edit: `diagverdictdump` 12,238 rows (5,076 RIGHT,
5,554 EMPTY_RIGHT, 1,515 WRONG, 93 EMPTY_WRONG), `verdictdump` 552,533
lines (543,125 RIGHT, 8,295 WRONG, 1,113 GAP).

Setup note: this container could not install the bootstrap's hash-pinned
`tomlkit` (PyPI is blocked by the egress policy, as crates.io is). The
assembler's two `tomlkit` calls (`parse` and `dumps` of a `Cargo.toml`, plus
`inline_table`) were served by a 40-line local stand-in over the standard
library's `tomllib` and a minimal TOML writer, kept in the session scratchpad
and not committed. The vendored crates it produced checksum-match
`Cargo.lock` (the assembler writes the lock's checksum; cargo accepted every
one), so the build is the same build.

## 2. `import.meta` (tsr-2zk.990)

### 2.1 The forcing constraint was the parser, not the checker

The census filed this as "checkMetaProperty is not ported". Porting it
changed nothing: the corpus has no `MetaProperty` for `import.meta` to check.
`parse_call_or_member_expression` read `import` as a `KeywordExpression` and
the member loop then ate `.meta` as a property access (the gap
`tsr-parser/src/references.rs` already documented as bd tsr-9or.4, with a
unit test asserting it). Upstream's `parseLeftHandSideExpressionOrHigher`
(`parser.go:5176`) builds the `MetaProperty` before any member chain: an
`import` not followed by `(`/`<` but followed by `.` is `import.<name>`,
named by `parseIdentifierName`. That arm is now ported beside the existing
`new.target` one.

`tsr-parser` is owned by no round-5 box, which is why the change was made
here rather than shipped as a diff; it is its own hunk in the lane commit
and the integrator can take it separately. Its second effect is upstream's
too: `is_file_probably_external_module`'s `getImportMetaIfNecessary` arm
(written against the upstream node, as its docs said) is now live, so the
loader treats a file whose only module indicator is `import.meta` as a
module. The unit test that asserted the gap flips, as its comment said it
must.

### 2.2 The checker half

`crates/tsr-checker/src/import_meta.rs`:

- `check_meta_property_type` — the type half of `checkMetaProperty`
  (`checker.go:10753`) for `import.*`: `checkImportMetaProperty` (`:10782`)
  answers `getGlobalImportMetaType()` (`getGlobalType("ImportMeta", 0)`,
  `emptyObjectType` without the lib interface) for `meta` and `errorType`
  otherwise; `import.defer` is `errorType`. One arm in
  `check_expression_worker`'s dispatch.
- `check_meta_property_reports` — the reports, from the check walk's
  `MetaProperty` visit (beside `checkNewTargetMetaProperty`'s TS17013, which
  that walk already owned): `checkGrammarMetaProperty`
  (`grammarchecks.go:1831`, TS17012 / TS18061 / TS1005 for `import.defer`
  without a call, behind `!file_has_parse_errors` as `grammarErrorOnNode`
  is) and the module-kind errors, TS1470 under `node16`..`nodenext` for a
  file whose implied format is not ESM, TS1343 below `es2020` other than
  `system`.

The implied format is read through `ModuleHost::implied_node_format_for_emit`.
That is `GetImpliedNodeFormatForEmitWorker`, not
`sourceFileMetaData.ImpliedNodeFormat`, but under an emit module kind in
`node16`..`nodenext` — the only arm that asks — the worker returns the
metadata's field unchanged (`ast/utilities.go:2574`), so no new host
question was added.

Rejected: reporting from the expression arm. Upstream's reports are a side
effect of `checkExpression`; this port's query road reports nothing, and the
walk already visits every `MetaProperty` once, so the reports live there.

### 2.3 Measured

Commit 1 against the baseline, unfiltered: **diagnostics +14 rows**
(`importMeta` ×3 configurations, `nodeModulesImportMeta` ×4,
`nodeModulesAllowJsImportMeta` ×4, `importMetaPropertyInvalidInCall`,
`dynamicImportDeferInvalidStandalone`, `compiler/misspelledNewMetaProperty`
— the last two from `checkGrammarMetaProperty`'s TS1005/TS17012 arms),
**types +144 lines**, 6 lines GAP → WRONG (below), zero losses on either dump.

Left, with the piece each waits for:

- **The `meta` name line** (`>meta : ImportMeta`, 47 lines: 3 per
  `importMeta` configuration, 2 per `nodeModules*ImportMeta` configuration,
  2 per `importMetaNarrowing` configuration). Upstream types it through
  `getRegularTypeOfExpression`'s `IsRightSideOfQualifiedNameOrPropertyAccess`
  redirect, whose third arm is `KindMetaProperty` (`ast/utilities.go:3700`).
  The types producer ports the property-access and qualified-name arms only;
  the meta-property arm is a harness change (`types_producer.rs`, not owned
  here) and ships as a diff in the lane report.
- **`importMetaNarrowing`** (2 configurations, 6 lines now WRONG rather than
  GAP, plus its diagnostics row). The binder's own `is_external_module`
  (`tsr-binder/src/binder.rs:4921`) still omits the `import.meta` indicator,
  so the file binds as a script, `declare global` reports TS2669 and the
  augmentation does not reach `ImportMeta`. The binder is not owned here;
  the fix (reuse `tsr_parser::is_file_probably_external_module`, or the same
  `MetaProperty` walk) is in the lane report. After it, the narrowing of
  `import.meta.foo` is flow work (`isMatchingReference`'s `MetaProperty` arm,
  `flow.rs`, main's file).
- **`new.target`'s type** (`checkNewTargetMetaProperty`'s return,
  `checker.go:10768`) keeps the dispatch's previous `errorType`. Not this
  lane's cluster; no census row names it.

## 3. `checkImportAttributes` (tsr-2zk.986)

`crates/tsr-checker/src/import_attributes.rs`, one hook line at the end of
the check walk's `ImportDeclaration` and `ExportDeclaration` arms (where
`checkImportDeclaration`, `checker.go:5330`, and `checkExportDeclaration`,
`:5548`, call it) and one in its `ImportTypeNode` visit.

- `check_import_attributes` is `checkImportAttributes` (`checker.go:5408`)
  in upstream's order: the type-only-with-override early return; TS2823
  when `ModuleKind.SupportsImportAttributes` is false (`node18`..`nodenext`,
  `preserve`, `esnext`); TS2856 when the specifier's
  `GetEmitSyntaxForUsageLocation` is CommonJS (the host question
  `emit_syntax_for_usage_location` already answers it; the eight-line
  string-literal guard is repeated here rather than widening
  `symbols.rs`'s private `usage_emit_syntax`, which this lane may not touch);
  TS2857 on a type-only declaration; TS1454 for a resolution-mode override
  on a value import. Every report is a `grammarErrorOnNode`, so all of them
  stand behind `!file_has_parse_errors`. A declaration outside a source
  file, module block or module declaration is the
  `checkGrammarModuleElementContext` bail both callers take first.
- `resolution_mode_override` is `getResolutionModeOverride`
  (`checker.go:3332`) with its three `reportErrors` arms (TS1464, TS1463,
  TS1453). `checkImportAttributes` passes `reportErrors = isTypeOnly`;
  `checkImportType` (`:3327`) passes `true`, which is the
  `check_import_type_attributes` hook.
- **Parser span, fixed alongside:** an import type's `ImportAttributes` node
  started at the outer `{` of `{ with: { … } }`. Upstream's
  `parseImportAttributes(currentToken, skipKeyword=true)` takes `pos` after
  `with:`, so the node — and TS1464's span — is the inner `{ … }`
  (`tsr-parser/src/module.rs`, `parse_import_type_attributes`).

**Not ported:** the opening relation check
(`checkTypeAssignableTo(getTypeFromImportAttributes(node), ImportAttributes
| undefined)`, TS2322 on a non-string value). `getTypeFromImportAttributes`
builds a synthetic object-literal type, and this port's store has no
constructor for one outside `check_object_literal`'s own walk; building a
second one here would be a new type identity the convention would need to
own. It is the only arm whose absence matters, and only to the TS2322 rows
(`importAttributes6` ×3 configurations, `importAttributes9`,
`compiler/importAssertionNonstring`); every grammar arm runs after it
unconditionally upstream, so its absence changes no other report. Those rows
also want TS2858 (`checkGrammarImportAttribute`'s value check), which is not
this function.

Remaining rows in the cluster are other callers: JSDoc `@import`
(`importTag15`, a `JSImportDeclaration` upstream, a side table here),
triple-slash `resolution-mode` (TS1453 in
`nodeModulesTripleSlashReferenceModeOverrideModeError`, a program
diagnostic), and the dynamic-import option checks in `import_call.rs`
(`importAttributes1`/`importAssertion1(module=commonjs)`: TS1009, TS1450).
