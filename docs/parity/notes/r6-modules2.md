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
