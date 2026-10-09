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
