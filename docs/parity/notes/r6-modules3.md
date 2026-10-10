# r6-modules3 — module and alias rules, continued (`tsr-2zk.1146`)

Round 6's successor to r6-modules2 (`r6-modules2.md`). This lane owns
`isolated_alias.rs`, `meaning_mismatch.rs`, `export_star_conflicts.rs` and
`crates/tsr-compiler`. Every expectation below was checked against a native
`tsgo` built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

## 0. Base

Batch BP (r6-modules2) had not landed on `claude/beautiful-shannon-ar5gh0`
when this box started. r6-modules2's branch edits `isolated_alias.rs`, so this
branch merges it (`f4ad599`), as r6-modules2 merged r6-isolated. The
measurement base is main `e6eadf4` plus that merge, plus BP's seven held diffs
in `r6-modules2.md` §10's apply order, applied in the working tree and never
committed: the state BP lands. Dumps frozen at that state, unfiltered and
including the configured suites: 12,238 diagnostics rows (5,623 RIGHT, 5,606
EMPTY_RIGHT) and 556,303 type rows (550,236 RIGHT).

Every diff below applies on that state, after BP's diffs, in this file's order.

Setup: PyPI is blocked, so `assemble.py`'s three `tomlkit` calls ran against a
stdlib-only stand-in kept outside the repo (`r5-operators3.md` §4).

## 1. `export default a.b` as an alias (item 1, a held diff)

### Classification against native

r6-modules2 §5 admitted `export = a.b` as an alias (`ExpressionIsAlias`,
`ast/utilities.go:1872`) and declined `export default a.b`. Admitted, the
default import's type reference lost `exportDefaultProperty2` 1:1 (`x : B` →
`error`). Native `tsgo` on the case plus two uses (`const y: number = x;
const z: string = B;`) reports TS2322 on both: `x: B` is the interface and the
value `B` is the static's `number`.

### Cause

The default import's type road is `get_type_from_type_reference`'s §493 arm
(`declared.rs`). It resolves the alias to `C.B`, a merged `Property|Interface`
symbol, and the declared type already dispatches on flags
(`getDeclaredTypeOfSymbol`, the `INTERFACE` arm). What failed was §493's
name-agreement gate, `declaration_written_name`: it read only
`Declarations[0]` (the `static B` property) and only for a class, interface or
enum declaration, so it answered `None` and the road returned `error`.

The gate stands for the name the declared type prints. That name is
`getNameOfSymbolAsWritten` (`nodebuilderimpl.go:988`):
`core.FirstNonNil(symbol.Declarations, ast.GetNameOfDeclaration)`, the first
declaration **with a name**, of any kind. The class/interface mint in the same
file already reads it that way (`declared.rs`, the `name_id()` walk). So the
gate now reads the same thing. Nothing else about the road changes.

### The diff: `r6-modules3-export-default-entity.diff`

- `declared.rs::declaration_written_name`: the first named declaration, as
  above. `declared.rs` is r6-declared2's lane.
- `symbols.rs::declaration_of_alias_symbol`: the dotted-entity arm for an
  `ExportAssignment` drops `is_export_equals`, and the decline comment says why.
  `export_assignment_target` already resolves both kinds.
- `crates/tsr-compiler/tests/r6_modules3_export_default_entity.rs`: native's
  two TS2322 lines.

It applies after BP's `r6-modules2-export-equals-entity.diff`, whose
predicate it edits.

### Measured

Against the base, unfiltered:

- types **+4 lines**, all `exportDefaultProperty2` (3 GAP → RIGHT, 1 WRONG →
  RIGHT); every line of the case is RIGHT. Diagnostics unchanged. Zero losses
  on both dumps; `slowcases` clean.
- Callgrind Ir (release, whole process, `--singleThreaded --pretty false`):
  domain-model 1,093,411,506 → 1,093,404,018, generic-imports 343,103,450 →
  343,064,303.

Native falls back to `checkExpressionCached` and the resolved property when
`resolveEntityName` fails on the left (`getTargetOfAliasLikeExpression`,
`checker.go:15004`), as for `export default obj.prop` where `obj` is a value.
`export_assignment_property_access_target` has no such fallback, for `export
=` as before; such an alias answers no target. No row in the dumps changed
because of it.

## 2. TS2476: the const-enum arm of `checkElementAccessExpression` (item 2, a held diff)

### Classification against native

`checkElementAccessExpression` (`checker.go:8157`) reports TS2476 ("A const
enum member can only be accessed using a string literal") at an index that is
not `IsStringLiteralLike` when the object type is a const enum's, and returns
`errorType`. TSR's `indexed.rs::element_access_lookup` already returns
`errorType` there (r5-errorsplit6 §2), but nothing reported.

### Where the report goes

The brief named `indexed.rs`. A report there would be wrong for TSR. Its
type computation runs when something asks for the access's type, not once
per node: `E[0];` as a statement, and a function's `return E[k]`, are never
typed, so a probe reported one of native's four lines. TSR's check walk
reports element-access diagnostics from per-node rules
(`check.rs`'s `ElementAccessExpression` arm), and the rule for this exact
function is `index_access_reports.rs::check_element_access_index_type`
("`checkElementAccessExpression` (`checker.go:8146`)"), which already returned
early on a const enum receiver. The report belongs in that early return.
`index_access_reports.rs` is r6-relater2's lane, so this is a diff to it, not
to `indexed.rs`.

The rule keeps its JS-file gate. A JS file's access to a TypeScript const enum
does not report TS2476, which native would. A JS file cannot declare a const
enum, and no row needs it.

### The diff: `r6-modules3-const-enum-index.diff`

- `index_access_reports.rs`: TS2476 at the argument, inside the existing
  `is_const_enum_object_type` exit.
- `crates/tsr-compiler/tests/r6_modules3_const_enum_index.rs`: native's lines
  on a statement, an annotated initializer and a `return`; none on `E["A"]`
  or `` E[`A`] ``.

### Measured

Against the base plus §1's diff, unfiltered:

- **verdict-neutral**: no row changes verdict, no type line moves. The two
  cases that need it change their actual lists only, each gaining exactly
  native's lines: `constEnumErrors` +3 (22,13 / 24,13 / 25,13) and
  `constEnumPropertyAccess2` +1 (14,12). Both stay WRONG on the refused
  TS2475 for unannotated initializers (r6-modules2 §6).
- `slowcases` clean.
- Callgrind Ir: domain-model 1,093,404,314 → 1,093,348,677, generic-imports
  343,059,366 → 343,050,820.

Native `tsgo` and TSR (`--singleThreaded`) print the same four TS2476 lines on
the probe. One thing seen in passing: the multi-threaded CLI drops a
diagnostic reported while another worker's file is typed
(`checker_pool.rs` keeps only diagnostics whose file the worker owns). Not
this lane's; recorded in §9.

## 3. `import x = N.M` through an import alias at the root (item 3, held: one loss)

### Classification against native

r6-modules2 §1's probe: `import { JSXInternal } from "./jsx"; export import
J2 = JSXInternal.HTMLAttributes;` gets TS1269 from native and nothing from
TSR. `resolve_qualified_entity` (`symbols.rs`) looked the root up with
`Binder::resolve_name(…, Namespace)` and then read the **alias symbol's own**
exports, which are empty, so the target was `None` and `check_alias_symbol`
returned before the rule.

### The port

`resolveQualifiedName` (`checker.go:15828`) resolves its left side with
`resolveEntityName(left, Namespace, …, dontResolveAlias=false)`:

- the root's lookup accepts an alias whose target carries `Namespace`
  (`getSymbol`, `:2176`), which is the checker's
  `resolve_name_with_export_alias`;
- a left side whose own flags lack `Namespace` is answered through
  `resolveAlias`;
- an alias merged with a namespace keeps its own exports first and falls back
  to its target's (`:15853`).

Only the leaf stays unresolved, as `import x = N.M`'s `dontResolveAlias` asks.
Native `tsgo` and TSR then print the same lines on the probe (TS1269 twice and
a TS2322 through the resolved interface).

Resolving the target exposed a second break, in TS2437 (`check.rs`, main):
`checkImportEqualsDeclaration` (`:5483`) tests
`resolveEntityName(moduleName, Value|Namespace, …, dontResolveAlias=false)`,
which resolves an alias whose own flags lack the meaning. TSR read the found
symbol's own flags, so `import { Translation } …; import K =
Translation.TranslationKeyEnum` reported "hidden by a local declaration"
against its own import (`declarationEmitEnumReferenceViaImportEquals`,
EMPTY_RIGHT → EMPTY_WRONG). The diff carries that fix too.

### Measured, and why it is held

`r6-modules3-qualified-entity-alias.diff` (`symbols.rs` and `check.rs`, both
main's, plus `crates/tsr-compiler/tests/r6_modules3_import_equals_qualified.rs`),
on the base plus §1–§2:

- types **+13 lines** (6 GAP → RIGHT, 7 WRONG → RIGHT), diagnostics unchanged
  once TS2437 is fixed;
- **one loss**: `externalModuleReferenceDoubleUnderscore1` 0:4 (RIGHT
  `basics.TimeUnit` → WRONG `TimeUnit`). `slowcases` clean.

The loss was right by accident: the qualified name never resolved, and the
line printed its written text. Native prints `basics.TimeUnit` because
`someSymbolTableInScope` (`symbolaccessibility.go:752`) searches the
`declare module` body's **locals** before its exports, and the locals hold
`import basics = require(…)`. That puts `[basics, TimeUnit]` ahead of the
exported `export import TimeUnit` alias. TSR's binder files that unexported
import alias in the module's **exports**:
`binder.rs::is_exported_from_container` applies `declareModuleMember`'s alias
rule (`binder.go:376`–`380`) only when the container is a source file, and
every other container falls through to `export_context`. Its own comment says
the rule holds "in a source file and in an ambient module alike", which the
guards do not do. So `best_name` meets the `basics.TimeUnit` candidate and the
direct `TimeUnit` alias in the **same** table, and the one-element chain wins.

The faithful binder fix (`r6-modules3-binder-alias-locals.diff`: drop the two
source-file guards) was measured on top and **refused**:

- the case goes fully RIGHT (0:2 `typeof basics.TimeUnit` too), and
  `moduleAugmentationInAmbientModule1` (WRONG → RIGHT),
  `moduleAugmentationInAmbientModule5` and `ramdaToolsNoInfinite2`
  (EMPTY_WRONG → EMPTY_RIGHT) convert;
- **−15 type lines**: `moduleAugmentationImportsAndExports3` (5, `B` →
  `any`: an augmentation's `import {B}` is no longer reached from the
  merged interface's member), `privacyImportParseErrors` (4,
  `use_glo_M2_public` → `any`) and `ramdaToolsNoInfinite2` (6);
- `slowcases`: `ramdaToolsNoInfinite2` 240 ms → 41,774 ms (diagnostics) and
  174 ms → 9,800 ms (types).

Those losses mean TSR's name resolution and alias walks read an import inside
a `declare module` or augmentation body only through the exports table. Making
them read the locals as native does is the binder owner's work (MAIN). Item 3
waits for it. **Falsifier**: with the binder fix and that resolution fixed,
item 3's diff should measure zero losses.

## 4. TS6263 files answer no module symbol (item 4)

### Classification against native

`resolveExternalModule` (`checker.go:15214`) sets `sourceFile` only when the
resolution diagnostic is nil or TS6142. Any other diagnostic leaves it nil:
the import resolves to `unknownSymbol`, after the diagnostic is reported. Of
those diagnostics, only TS6263 (an arbitrary extension's `.d.<ext>.ts` without
`allowArbitraryExtensions`) reaches a file that is in the program. A `.js` or
`.json` resolution with its diagnostic is not loaded in the first place.
r6-isolated §5 recorded TSR's `resolve_external_module_name` answering the
module symbol there as an accepted difference.

### The port

`Checker::resolution_keeps_source_file` (`isolated_alias.rs`, committed) is
that condition: `GetResolutionDiagnostic` over the resolver's extension (the
existing `resolution_diagnostic`), true when it is none or TS6142. A non-
declaration TypeScript file's extension is always `.ts`, `.tsx`, `.mts` or
`.cts`, whose diagnostic is none or TS6142, so the host is asked for the
extension only for a file stamped `JAVASCRIPT_FILE` (JS and JSON) or a
declaration file. The pre-filter decides nothing native would not.

The hook, `r6-modules3-resolution-source-file.diff` (main's `symbols.rs`, and
the `dead_code` allow it retires), calls it in `resolve_external_module_name`
after the host answered a file and returns `None` when it is false. No cache,
side table or traversal: one flag read per resolution, and one host lookup
for a declaration or JS file.

### Measured

On the base plus §1, §2 and §5 (attributed by case; the two populations are
disjoint):

- types **+11 lines**, all
  `declarationFileForHtmlImport(allowarbitraryextensions=false)`. Zero losses,
  diagnostics unchanged, `slowcases` clean.
- Callgrind Ir: §1 + §2 + §5 → with the hook, domain-model 1,093,496,420 →
  1,094,802,257 (+0.12%), generic-imports 343,067,329 → 343,089,749. The
  domain-model delta is not this hook's work. Under a line-tables build,
  `resolve_external_module_name` runs 264 times on domain-model and costs
  290,176 Ir **inclusive** of the hook. The unfiltered first version measured
  the same delta (+1.04M). The rest is code layout, which moves by this much
  between two builds of near-identical code (r6-modules2 §8 recorded 0.06%
  between runs of one binary's pairs).

Native and TSR print the same TS6263 line on a probe, and nothing else. The
types for such an import read `error` in TSR where native prints `any`; that
is the unresolved-alias call road, not this condition.

## 5. Module-object naming through a namespace's exports (item 5)

### Classification against native

`importAliasAnExternalModuleInsideAnInternalModule` 0:2 and 0:3 print `typeof
r` where native prints `typeof C`: inside `namespace m_private`, `export import
C = r` names `r`'s module. Native's `trySymbolTable` iterates **every** alias
of a scope table (`getSymbolTableAliases`, `symbolaccessibility.go:562`), and
`m_private`'s exports are visited before the file's locals. TSR's
`module_alias_at` (main's `checker.rs`) visits exports too, but admitted only
an import-equals alias with an **external** module reference (`= require(…)`).
The comment said internal aliases "stay excluded", with no measured reason.

### The diff: `r6-modules3-module-alias-exports.diff`

The exports arm admits any sole import-equals declaration. Default and
`export=` exclusions, the shadow and meaning checks, and the per-table order
are unchanged.

### Measured

- types **+2 lines** (0:2 and 0:3, WRONG → RIGHT); zero losses, `slowcases`
  clean (same run as §4).
- The rest of the case (`C.m.foo()`, 0:4–0:9) reads `any`/`error`: the
  property access through an `export import` alias of a module. Not this
  diff.

Neither §4 nor §5 changes a diagnostic, so their pins are the conformance
lines above, not new compiler tests.

## 6. r6-triage's module/alias rows (item 6)

r6-triage's table (`r6-triage.md` on its branch, `b2fc79b`) assigns these
rows to this lane (as r6-modules2): #29 MODULE-RESOLUTION-DIAGNOSTICS (12
diagnostics cases), #62 MEANING-MISMATCH-REPORTS (7), #96
IMPORT-TYPE-AND-ALIAS-MEANING (5), #134 JS-REQUIRE-CALL-MODULE-RESOLUTION (4),
#138 RESOLVE-EXTERNAL-MODULE-EXTENSION-ARMS (4), #139
TYPE-ONLY-EXPORT-CHAIN-RESOLUTION (4) and #194 PROGRAM-VERIFY-COMPILER-OPTIONS
(3). #123 QUALIFIED-NAME-LEFT-ALIAS-RESOLVE (4, MAIN) is §3.

Taken first: #62, whose home is this lane's `meaning_mismatch.rs`. Six of its
seven cases miss TS2702/TS2713 (`checkAndReportErrorForUsingTypeAsNamespace`,
`checker.go:1608`), and `moduleAsBaseType` misses one TS2709. TS2702/TS2713 is
already ported and held as `r5-smallcodes3-type-as-namespace.diff`
(`r5-smallcodes3.md` §3.2, +3 rows / −1). Its loss, `callbackTagNamespace`,
waits on the parser reading a dotted `@callback` name
(`parseJSDocTypeNameWithNamespace`, `parser/jsdoc.go:992`).
`tsr-parser/src/jsdoc.rs::parse_callback_tag` still reads one identifier, so
the blocker holds. The diff no longer applies to `check.rs` either. Rebasing
it now would produce the same measured loss, so it is not redone here.

The rest of the rows were not reached this session.

## 7. The diffs together

Apply order, each checked with `git apply` on main `e6eadf4` + this branch +
BP's seven diffs (`r6-modules2.md` §10), in sequence:

1. `r6-modules3-export-default-entity.diff` (§1), after BP's
   `r6-modules2-export-equals-entity.diff`;
2. `r6-modules3-const-enum-index.diff` (§2);
3. `r6-modules3-resolution-source-file.diff` (§4);
4. `r6-modules3-module-alias-exports.diff` (§5).

Held, not landable (a measured loss each, §3): `r6-modules3-qualified-entity-alias.diff`
(applies after 1–4) and the refused `r6-modules3-binder-alias-locals.diff`.

Diffs 1–4 stacked, against the frozen base (unfiltered, both dumps, configured
suites included):

- types **+17 lines** (3 GAP → RIGHT, 14 WRONG → RIGHT), zero losses:
  `exportDefaultProperty2` 4, `declarationFileForHtmlImport(allowarbitraryextensions=false)`
  11, `importAliasAnExternalModuleInsideAnInternalModule` 2.
- diagnostics: no verdict changes; `constEnumErrors` and
  `constEnumPropertyAccess2` gain native's four TS2476 lines.
- `slowcases` clean on both dumps.
- `cargo test --workspace --release`: 3,624 passed, 0 failed. `cargo fmt
  --check` clean. Clippy (stable 1.97) reports only pre-existing code
  (`enum_initializer.rs`, `index_signatures.rs`, `printing.rs`,
  `signatures.rs`, `symbols.rs:4421`, `templates.rs`, `unique_symbols.rs`,
  `tsr-dts/tests/accessibility.rs`), none of it in these diffs.
- Callgrind Ir: domain-model 1,093,411,506 (base) → 1,094,802,257 (+0.13%),
  generic-imports 343,103,450 → 343,089,749. §4 accounts for the
  domain-model delta: the code the stack runs there is bounded by 290k Ir,
  and the rest moves with layout.

## 8. Commit on this branch

Only `isolated_alias.rs::resolution_keeps_source_file` (§4's condition, with a
`dead_code` allow naming its hook diff) and these notes and diffs. Everything
else in this round is in files other lanes own.

## 9. What remains, with causes

- **Item 3** (`import x = N.M` through an import alias): held on one loss,
  which waits on the binder filing an unexported import alias of a `declare
  module`/augmentation body in its locals (`binder.go:376`–`380`), and on
  name resolution and alias walks then reading those locals (the binder fix
  alone: −15 type lines, `ramdaToolsNoInfinite2` 240 ms → 41.8 s). Owner:
  MAIN (binder, `symbols.rs`).
- **TS2475 on unannotated initializers** (r6-modules2 §6, refused +1/−3):
  `constEnumErrors` and `constEnumPropertyAccess2` need it besides §2's
  TS2476. Waits on lazy function-like signatures (main).
- **TS2702/TS2713** (§6): waits on dotted `@callback`/`@typedef` names
  (parser, main; JSDoc binder arm, r6-jsdoc).
- **`export default obj.prop` with a value `obj`** (§1): native's
  `checkExpressionCached` fallback in `getTargetOfAliasLikeExpression` is not
  ported, for `export =` either. Owner: main (`symbols.rs`).
- **The unresolved-alias call road** (§4): an import that resolves to
  `unknownSymbol` types its uses `error` where native prints `any`.
- **`C.m.foo()` through an `export import` alias of a module** (§5, 0:4–0:9 of
  the same case).
- **The multi-threaded CLI** (§2): a diagnostic reported while one worker
  types a node of a file another worker owns is dropped by
  `checker_pool.rs`'s owner filter. Seen once on a probe, not measured.
- r6-triage rows #29, #96, #134, #138, #139 and #194 were not reached.
