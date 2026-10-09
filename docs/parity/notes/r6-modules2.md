# r6-modules2 — option-gated module rules, continued (`tsr-2zk.1137`)

Round 6's successor to r6-isolated (`r6-isolated.md`). This lane owns
`isolated_alias.rs`, `meaning_mismatch.rs`, `export_star_conflicts.rs` and
`crates/tsr-compiler`. Every expectation below was checked against a native
`tsgo` built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

**Base.** Batch BE (r6-isolated) had not landed on
`claude/beautiful-shannon-ar5gh0` when this box started. r6-isolated's branch
rewrites `isolated_alias.rs` (+1,092 lines), so editing the file on main's
tip would have conflicted with BE. This branch therefore merges r6-isolated's
branch, which touches only that file and its notes. The measurement base is
main `0180457` plus that merge, plus BE's five hook diffs
(`r6-isolated-*.diff`, in §4's apply order) and batch BC's
`r6-smallcodes4-type-only-alias-value.diff`, applied in the working tree and
never committed. Those are the states BE and BC land. Dumps were frozen at that
state: 12,238 diagnostics rows (5,553 RIGHT, 5,599 EMPTY_RIGHT) and 556,303
type rows.

## 0. `getTypeOnlyAliasDeclarationEx`'s loop condition (item 0)

### Upstream

Two functions read `aliasSymbolLinks.typeOnlyDeclaration` (pinned
`checker.go:2133`–`2153`):

- `getTypeOnlyAliasDeclaration(symbol)` returns the link of the symbol
  itself. `resolveAlias` publishes it: the alias's own type-only declaration
  (`markSymbolOfAliasDeclarationIfTypeOnly`, `:15083`), or else the link
  `resolveIndirectionAlias` (`:16293`) copies from its target. That copy only
  happens when the target is a pure alias,
  `ast.IsNonLocalAlias(target, Value|Type|Namespace)` (`:16280`).
- `getTypeOnlyAliasDeclarationEx(symbol, meaning)` loops while the hop is an
  alias **with no `meaning`**, returning the first hop's link that is set.

So an alias merged with a local meaning stops both walks. For example,
`import type { A }` beside `const A` is a value, so the chain behind it
contributes nothing.

### What TSR had

`isolated_alias.rs` had one walk, `type_only_alias_declaration_node`. It
continued through every alias hop, and `_ex` tested `meaning` only on the
first hop. Batch BC gives `check.rs`'s TS1361/TS1362 copy
(`type_only_alias_declaration`) the `Alias && !Value` condition. The
integrator asked for the same condition here.

### The port

- `type_only_alias_declaration_node` (the link) continues to a target only
  while that target `is_non_local_alias(target, Value|Type|Namespace)`. That
  is a new port of `ast.IsNonLocalAlias`, including its `Alias|Assignment`
  disjunct.
- `type_only_alias_declaration_node_ex` is upstream's loop: while the hop is
  `Alias` without `meaning`, it answers the hop's link, else it steps to
  `resolve_alias`. TSR's `resolve_alias` returns the immediate target, while
  upstream's `aliasTarget` has already skipped the pure hops. Re-reading a
  pure hop's link only finds what the earlier link already covered, so the
  extra hops are redundant, never wrong. The walk keeps the 16-hop bound.

The call sites keep upstream's split. `check_export_assignment_isolated`
(`:5616`) uses the `Ex` form. `checkAliasSymbol` (`:6801`),
`markEntityNameOrEntityExpressionAsReference` (`:28870`) and `getSymbolFlags`'
`excludeTypeOnlyMeanings` (`:16374`) use the link. The doc comment's anchor
for `getTypeOnlyAliasDeclaration` said `:1861`, which is TS1361's caller. It
is corrected to `:2133`, and the comment notes the correction.

No cache, side table or traversal is added. The walk is the existing bounded
one with a stricter continuation test.

### Measured

- Verdict-neutral: both unfiltered dumps are unchanged against the base (no
  row changes verdict or list, no type line moves), and `slowcases` is clean
  on both.
- `cargo test --workspace --release`: 3,508 passed, 0 failed.
- Callgrind Ir (release, whole process, `--singleThreaded`): domain-model
  1,091,305,725 → 1,091,324,383 (+0.002%), generic-imports 343,098,569 →
  343,059,109 (−0.01%).
- Native `tsgo` on a reduced fixture with `verbatimModuleSyntax` (`a.ts`:
  `export type A`; `b.ts`: `import type { A }` + `const A` +
  `export { A }`; `c.ts`: `import { A } from "./b"; export default A;`)
  reports nothing in `b.ts` or `c.ts`. The base reported TS1448 in `b.ts`,
  and TS1485, TS1285 and TS1290 in `c.ts`. The port matches native, including
  `d.ts` (`import type { A } from "./b"; export default A;` → TS1285) and a
  pure chain (`g.ts` → TS1485 and TS1448 through `f.ts`'s
  `import type { C }; export { C }`).
- `crates/tsr-compiler/tests/r6_modules2.rs` covers both shapes.

### How this would be wrong

A type-only report that native makes through a hop TSR's `resolve_alias`
answers `None` for. The walk would end early and miss it. That is a missing
report, never an extra one, and no row in the dumps shows it.

## 1. TS1269: `import X = Y` through an import alias (item 1, a held diff)

### Classification against native

`compiler/isolatedModulesExportImportUninstantiatedNamespace` wants TS1269 at
`factory.ts(3,1)`, on `export import JSX = JSXInternal;`, where `JSXInternal`
is `import { JSXInternal } from "./jsx"` of an uninstantiated namespace. The
rule itself is ported (`check_alias_symbol_isolated`, round 5). It never ran:
`check_alias_symbol` (`symbols.rs`) returns before it when
`resolve_alias(JSX)` answers `None`.

`get_target_of_alias_symbol`'s bare-identifier `import =` arm
(`symbols.rs`) resolves the name at `Namespace`. It then accepts only a
symbol whose **own** flags carry `Namespace`, or an alias whose type is a
known module clone. Upstream's `getSymbolOfPartOfRightHandSideOfImportEquals`
(`checker.go:14474`) calls `resolveEntityName(name, Namespace, …,
dontResolveAlias)`. Its `getSymbol` (`:2176`) accepts an alias whose
resolved flags (`getSymbolFlags`) carry the meaning, and the target is the
alias itself. A probe with a local namespace (`export import L = Local;`)
already reported TS1269. Only the alias-to-alias form was missing. Native
`tsgo` reports both.

### The diff: `r6-modules2-import-equals-alias.diff`

Both halves are in main's files:

1. `symbols.rs`, the identifier arm: an alias whose `get_symbol_flags` carry
   `Namespace` is the target, before the module-clone test (which stays for
   the shape it already covers).
2. `checker.rs::export_equals_alias_name_at`. Half 1 alone lost two type
   lines (`es6ImportNamedImportInIndirectExportAssignment` 1:0 and 1:2, RIGHT
   `typeof a` → WRONG `typeof x`). That function's "two-hop signature" is
   documented as `import → export= alias → namespace`. It tested only that
   the immediate target is *an* alias, so once `import x = a` resolves to the
   import alias `a`, `x` matched and renamed the namespace at every site in
   the file. The test now requires the link to be the `export=` symbol, which
   is the shape its own comment and §501 describe. Native names that
   namespace through `getAccessibleSymbolChain`, where `compareSymbols`
   prefers the earlier-declared `a`. With the narrowing, all six lines of the
   case are RIGHT (base: four).

The diff adds `crates/tsr-compiler/tests/r6_modules2_import_equals.rs`:
TS1269 on both the imported and the local namespace.

It also corrects one expectation in `tsr-checker/tests/symbol_chain.rs`
(`module_copy_calls_require_a_certified_empty_global_function`). For the
namespace-only library, the test expected `Tail(3)`/`Linked(4)` (`import
Linked = Head`, `import Tail = Linked`, `Head` a namespace import) to be
`errorType` even where `Head(1)` is an untyped `any` call. That was the old
unresolved alias. Native `tsgo` on the same program: with an empty
`interface Function {}` all three calls are `any`, and with a member on
`Function` all three report TS2349. The corrected arm gives `Tail` and
`Linked` `Head`'s answer.

### Measured

Applied on the base:

- diagnostics **+2 rows**:
  `isolatedModulesExportImportUninstantiatedNamespace` WRONG → RIGHT, and
  `jsxNamespaceImplicitImportJSXNamespace` EMPTY_WRONG → EMPTY_RIGHT (a wrong
  report it made while the alias was unresolved goes away). Zero losses.
- types **+16 lines** (12 WRONG → RIGHT, 4 GAP → RIGHT) across
  `chainedImportAlias` (7), `jsxNamespaceGlobalReexport` (3),
  `jsxNamespaceImplicitImportJSXNamespace` (3),
  `es6ImportNamedImportInIndirectExportAssignment` (2) and
  `aliasInaccessibleModule2` (1). Zero RIGHT lines lost.
- One WRONG line changes text: `importAliasAnExternalModuleInsideAnInternalModule`
  0:2, `export import C = r` inside `namespace m_private`, was `any` and is
  now `typeof r`; native prints `typeof C`. Native's accessibility walk finds
  `C` in `m_private`'s exports table, inner to the file's `r`. TSR's printer
  for a module object (`module_exports_text_at`'s route) does not consult a
  namespace's exports table. That is main's printer, recorded in §9.
- `slowcases` clean on both dumps.
- Callgrind Ir: domain-model 1,091,988,114 → 1,091,279,065 (−0.06%, noise),
  generic-imports 343,063,691 → 343,058,495.
- `cargo test --workspace --release` with the diff applied: 3,509 passed, 0
  failed.

**Native probe, not in the baselines.** `export import J2 =
JSXInternal.HTMLAttributes;` (qualified through the import alias) gets TS1269
from `tsgo` and nothing from TSR with or without the diff:
`resolve_qualified_entity` does not cross an import alias at the root. Recorded
in §9.

## 2. TS1280 and TS1281: global-script namespaces and cross-file enum members (item 2)

### Classification against native

`compiler/isolatedModulesGlobalNamespacesAndEnums` was missing three lines,
and native `tsgo` prints them on the reduced fixture:

- TS1280 at `script-namespaces.ts(1,11)`;
- TS1281 at `enum2.ts(3,9)` and `(4,9)`.

Neither rule existed in TSR.

- **TS1280** is `checkModuleDeclaration`'s `GetIsolatedModules()` arm
  (`checker.go:5168`). The guard is the instantiated-module block (`:5164`):
  `symbol.Flags&ValueModule`, not `inAmbientContext`, and
  `isInstantiatedModule(node, ShouldPreserveConstEnums())`. The rule then
  fires when the file has no `ExternalModuleIndicator`. The error goes on
  the name.
- **TS1281** is reported by the name resolver itself
  (`binder/nameresolver.go:147`–`158`). At an enclosing `EnumDeclaration`,
  the name is looked up in the merged enum's exports. On a hit under
  `GetIsolatedModules()`, it is an error when the declaration is not ambient
  and the member's `ValueDeclaration` is in another file. The walk reports
  only with a `nameNotFoundMessage`, i.e. for an expression identifier.

### The port

Both rules are in `isolated_alias.rs`.

`check_global_script_namespace(node)` carries all of upstream's guards. The
file test is the binder's file symbol in a TypeScript file, which the binder
creates exactly for an external module, `moduleDetection: force` included
(ADR-0041). In a JavaScript file it is `is_external_module_in`, because there
a CommonJS file also gets a symbol. `isInstantiatedModule` with
`ShouldPreserveConstEnums()` is `check.rs`'s private reading, ported again
(`is_instantiated_module_preserving_const_enums`). The fold-in that §8 does
for `node_can_be_decorated` applies here once both are in one file.

`check_enum_member_from_another_file(node, result, text)` runs on
`Binder::resolve_name`'s result, which takes `&self` and cannot report.
Upstream's enum arm found the name exactly when `result` is the member that
the first enclosing enum exporting `text` holds. A scope between the
reference and that enum would have answered its own symbol, and an enclosing
enum that does not export the name is passed over, as the walk passes it.

The hook (`r6-modules2-global-script.diff`, main's `check.rs`) has two
calls:

- `check_global_script_namespace` in the `ModuleDeclaration` arm of the
  statement walk, right after `check_exports_on_merged_declarations` and
  inside the same `checkGrammarModuleElementContext` guard. Upstream reaches
  TS1280 right after `checkExportsOnMergedDeclarations` (`:5161`–`:5168`).
- `check_enum_member_from_another_file` in `check_value_identifier`, before
  `report_type_only_alias_used_as_value` (TS1361/TS1362) and r6-isolated's
  TS2866. The resolver reports TS1281 inside its walk, ahead of both success
  arms.

Until the hook lands, both functions carry `#[allow(dead_code, reason = …)]`
naming the diff. The diff adds
`crates/tsr-compiler/tests/r6_modules2_global_script.rs`. It covers TS1280
in a script but not in a module, without the option or under
`moduleDetection: force`, and TS1281 on the two unqualified members but not
the qualified one or the ambient declaration's.

No cache, side table or traversal is added. TS1281's extra work is an
ancestor walk, and only for an identifier that resolved to an enum member
while `isolatedModules` is on. Both functions return on their first read
without the option.

### Measured

Applied on the base:

- diagnostics **+1 row**: `isolatedModulesGlobalNamespacesAndEnums` WRONG →
  RIGHT. Zero losses, types identical, `slowcases` clean on both dumps.
- Native `tsgo` and TSR print identical lines, text included, on the
  fixture under `isolatedModules`, under `verbatimModuleSyntax` (the flag
  name in both messages), and under `moduleDetection: force` (no TS1280 or
  TS1281; the names no longer merge).
- Callgrind Ir: domain-model 1,091,359,216 → 1,091,322,400, generic-imports
  343,059,372 → 343,074,763 (both within ±0.005%). The hot hook is the
  identifier site, which returns on `isolated_modules == false`.
- `cargo test --workspace --release` with the diff applied: 3,510 passed, 0
  failed.

## 3. TS2503 on `import f1 = NonExistent` (item 3): already covered

`conformance/verbatimModuleSyntaxInternalImportEquals` misses only TS2503 at
`(2,13)`. That has nothing to do with `verbatimModuleSyntax`: native `tsgo`
reports the same TS2503 without the option, and TSR reports nothing in either
configuration. The cause is that the identifier arm of `import a = b`
(`getSymbolOfPartOfRightHandSideOfImportEquals`'s case 1) was never checked.

r6-smallcodes4's `r6-smallcodes4-namespace-not-found.diff` (its notes §2.1)
ports that check (`check_import_equals_identifier_reference`). Its table
lists this case among its lossless conversions, so it lands with batch BC.
Nothing here duplicates it.

## 4. TS2450 on an early `const enum` use under `isolatedModules` (item 4, a held diff)

### Classification against native

`blockScopedEnumVariablesUseBeforeDef_isolatedModules` and
`_verbatimModuleSyntax` (`target=es2015`) each miss TS2450 at `(7,12)` and
`(12,8)`. Both are uses of a `const enum` before its declaration.
`checkResolvedBlockScopedVariable` (`checker.go:1888`) picks the message by
flags: block-scoped variable, class, `RegularEnum`, and then a last arm
(`:1911`–`1914`) that reports a `ConstEnum` only under
`GetIsolatedModules()`. TSR's port (`check.rs`, the TS2448/TS2449/TS2450
selection) stopped at `RegularEnum`. That is right without the option (a
const enum is inlined) and missing with it.

### The diff: `r6-modules2-const-enum-tdz.diff`

The diff adds one arm to the selection in main's `check.rs`: a `CONST_ENUM`
(not a class, matching upstream's arm order) answers TS2450 when
`isolated_modules` (`GetIsolatedModules()`), and returns otherwise. The
declaration lookup and the before-use test below it are shared, as upstream
shares them. The diff adds
`crates/tsr-compiler/tests/r6_modules2_const_enum_tdz.rs`. Under
`isolatedModules` it expects all three lines. Without the option, with or
without `preserveConstEnums`, it expects only the regular enum's line, which
is native's answer.

### Measured

Applied on the base:

- diagnostics **+2 rows**: both cases WRONG → RIGHT. Zero losses, types
  identical, `slowcases` clean.
- Native `tsgo` and TSR print identical lines on the case under
  `isolatedModules`, under `verbatimModuleSyntax` and under
  `preserveConstEnums` alone.
- Callgrind Ir: domain-model 1,091,989,234 → 1,091,266,664 (−0.07%, the
  same spread as two runs of the base binary), generic-imports 343,080,966 →
  343,063,120.
- `cargo test --workspace --release` with the diff applied: 3,509 passed, 0
  failed.

## 5. TS1295 through `export = globalThis.console` (item 5, a held diff)

### Classification against native

`isolatedModulesShadowGlobalTypeNotValue`'s `(false,true)` and `(true,true)`
rows miss TS1295 at `good.ts(2,10)`, on `import { Console } from
'node:console'`, where the ambient module ends in `export =
globalThis.console`. Native `tsgo` reports it in both rows. The rule is
round 5's CommonJS arm of `checkAliasSymbol`. It never ran, because
`check_alias_symbol` returns when the import's target is `None`. Probing
each hop turned up three separate breaks, all in main's `symbols.rs`:

1. `declaration_of_alias_symbol` admitted an `ExportAssignment` only for an
   identifier or class expression. Upstream's `IsAliasSymbolDeclaration`
   reads `ExpressionIsAlias` (`ast/utilities.go:1872`), which is any entity
   name expression, so `export = a.b` is an alias with a declaration.
2. `export_assignment_target` resolved only those two shapes.
   `getTargetOfAliasLikeExpression` (`checker.go:14996`) sends an entity
   name to `resolveEntityName(…, Value|Type|Namespace, …, dontResolveAlias)`.
   That resolves the left side as a namespace with aliases resolved, then
   reads `getSymbol(getExportsOfSymbol(left), right, meaning)` (`:15809`).
   The binder has no `globalThis` symbol. Upstream's (`:962`) is a module
   whose exports are `globals`.
3. `get_external_module_member` reads `Console` off the `export =` target's
   type and withholds it, because its type (`ConsoleConstructor`) is not
   site-independent (the printer guard documented there).

### The diff: `r6-modules2-export-equals-entity.diff`

- `declaration_of_alias_symbol` admits a dotted entity name
  (`IsPropertyAccessEntityNameExpression`) on an `export =`.
- `export_assignment_property_access_target` is the property-access arm of
  `resolveEntityName`. It resolves the left side through
  `heritage_entity_symbol(left, Namespace)`, which resolves aliases as
  upstream's left side does. An unbound `globalThis` reads `globals`. The
  member is then accepted by `getSymbol`'s rule: its own meaning, or an
  alias whose chain carries it.
- `check_alias_symbol`'s fallback, which already takes
  `qualified_alias_target` for flags only (§686), also takes
  `export_equals_member_for_flags`. That is the value member
  `getExternalModuleMember` reads off an `export =` target's type, without
  the printer guard. The rule prints the local name and reads only flags,
  so the guard's reason does not reach it. The type and naming paths keep
  the guard.

**Declined: `export default a.b`.** Upstream makes it the same alias. With
it admitted, `exportDefaultProperty2` lost a RIGHT line (`x : B` →
`error`): the default import now reaches `C.B`, a merged `Property|Interface`
symbol, and its use as a type reference loses the interface. The `export =`
twin, `exportEqualsProperty2`, goes from four gaps and one WRONG to all five
RIGHT through the `import = require` path. So the missing piece is the
default-import type path: the declared type of an alias whose target is a
merged `Property|Interface` member. That is main's (or r6-declared's) code.
The decline is commented at the predicate, and the integrator should file
the blocker (listed in §9).

The diff adds `crates/tsr-compiler/tests/r6_modules2_export_equals_entity.rs`.
It covers TS1295 on the reduced `node.d.ts`/`good.ts` (and silence without
the option), and TS2322 on `exportEqualsProperty2` plus a use, which native
reports and the base did not.

### Measured

Applied on the base:

- diagnostics **+2 rows**: both `verbatimmodulesyntax=true` rows of
  `isolatedModulesShadowGlobalTypeNotValue`, WRONG → RIGHT.
- types **+5 lines**, all `exportEqualsProperty2` (4 GAP → RIGHT, 1 WRONG →
  RIGHT). No other line changes text.
- Zero losses on both dumps, `slowcases` clean.
- Native `tsgo` and TSR print the same lines on the case's four option
  combinations.
- Callgrind Ir: domain-model 1,091,982,033 → 1,091,372,630 (−0.06%),
  generic-imports 343,058,775 → 343,073,177 (+0.004%).
- `cargo test --workspace --release` with the diff applied: 3,510 passed, 0
  failed.

The first measurement admitted `export default a.b` too and lost
`exportDefaultProperty2` 1:1, which is the decline above.
