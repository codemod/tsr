# r4-rwfix — real-world parity fixes (`tsr-2zk.923`, `.930`, `.933`)

Round-4 lane under `tsr-2zk.914`. Ports three of the root causes that
[r4-realworld.md](r4-realworld.md) found on TypeScript's `src/jsTyping`
(causes 2, 12 and 15). Native source is `vendor/typescript-go` @ `5b1047d`.

## Method

As in r4-realworld.md: an untracked `jsTyping/tsconfig.scratch.json`
(`extends ./tsconfig.json`, `types: []`, `outDir` outside the submodule,
`pretty: false`), native `tsgo` built by `scripts/offline-cargo/build-tsgo.sh`,
both tools run from `src/`, `outDir` deleted before every run. Keys are
`(file, line, column, code)` of each `error TS` line.

The baseline reproduces r4-realworld.md's table exactly: TSR 874, native 163,
common 56, TSR-only 818, native-only 107 (TS6307 77, TS2724 17, TS2591 11,
TS7006 1, TS7031 1).

## Cause 2 — logical assignment narrows its target (`tsr-2zk.923`)

**Forcing constraint.** `a ??= b; a.length` reported TS18048 on `a`. The
binder already builds the flow graph native builds for `??=`/`||=`/`&&=`
(`bindLogicalLikeExpression`: an assignment flow node for the left operand on
the branch that evaluates the right, merged with the short-circuit branch),
and `assignment_target_kind` already calls these operators `Definite`, so
`get_type_at_flow_assignment` reaches the reduction. Only
`get_initial_or_assigned_type` (`getAssignedType`, `flow.go:2288` →
`getAssignedTypeOfBinaryExpression`, `flow.go:2314`) declined anything but
`=`, and a declined assigned type keeps the declared type — `undefined`
included.

**Port.** Accept the three logical-assignment operators beside `=`; upstream
answers `getTypeOfExpression(right)` for every binary operator that reaches
`getAssignedTypeOfBinaryExpression`. Compound operators never reach it
(`getTypeAtFlowAssignment` returns the antecedent first for
`AssignmentKindCompound`). `is_in_compound_like_assignment` needed no change:
upstream's `IsAssignmentExpression(target, excludeCompoundAssignment=true)`
admits only `=`, which TSR already tests.

**Measured.** jsTyping TSR-only 818 → 722 (−96: TS18048 79 → 18, TS2345
190 → 171, TS2322 55 → 41, TS2769 91 → 89); common and native-only
unchanged. Corpus: `conformance/logicalAssignment11` EMPTY_WRONG →
EMPTY_RIGHT, no diagnostic or type-line loss. r4-realworld.md's 97 was taken
after its exp1 counterfactual; on the real code base it is 96. The remaining
18 TS18048 are other causes (destructuring and closure narrowing, causes 9
and 10, among them).

**Falsifier.** A `??=` target whose declared union keeps `undefined` after
assignment where native narrows, or a TS18048 that native reports after
`&&=` and TSR drops.

### Re-measured after merging the integration head `5ad60b1`

The merged head alone gives jsTyping TSR 898 (TSR-only 842): +24 TS7053
(`expression of type 'string' can't be used to index type 'CompilerOptions'`
/ `'OptionsBase'`, e.g. `compiler/builder.ts(1449,21)`), none of which native
reports. They arrived with the merge, not with this lane, and are reported to
the integrator. Cause 2 on that head: 898 → 802 (TSR-only 842 → 746), the
same −96; the corpus result is unchanged (`logicalAssignment11`, +1 type
line, no loss).

## Cause 12 — missing-import suggestion sees `export *` members (`tsr-2zk.930`)

**Forcing constraint.** `import { Diagnostics } from "./_namespaces/ts.js"`
where the barrel re-exports `Diagnostic` through `export *`. Native reports
TS2724 `… Did you mean 'Diagnostic'?`; TSR reported TS2305 because
`report_missing_module_export` spelled against the target's **own** export
table only. Native `getSuggestedSymbolForNonexistentModule`
(`checker.go:15909`) spells against `getExportsOfModule(targetSymbol)`, whose
worker (`getExportsOfModuleWorker`, `checker.go:16148`) folds in every
`export *` target recursively.

**Port.** Two new functions in `symbols.rs`:
`module_exports_with_stars` mirrors the worker's `visit` — own exports first,
then each `export *` target's table (recursively, a visited module
contributing nothing), merged with `extendExportSymbols`' rules (`default`
never re-exported, the first name wins).
`module_member_spelling_candidates` applies `getSpellingSuggestionForName`'s
candidate filter (`checker.go:1800`): names that are empty, quoted or
internal (`export=`, `__export`, upstream's `\xFE` names) are dropped, and the
symbol must carry `SymbolFlagsModuleMember`. That mask contains `Alias`, so
upstream's `tryResolveAlias` fallback never decides here and is not written.

**Not ported, deliberately.** The worker's TS2308 collision diagnostics and
type-only bookkeeping (they do not change which names exist); the CommonJS
`export =` typedef arm (the `export =` path keeps declining as before); and
`compareSymbols` as the distance tie-break, which `spelling_suggestion` has
never ported. The table is rebuilt per missing specifier, uncached: it runs on
the error path only, once per reported name.

**Alternative rejected.** Reusing `get_export_from_star` (a by-name lookup)
cannot enumerate candidates; building a cached `getExportsOfModule` table on
the module symbol would be the faithful home for every caller, but it is a
new side table with publication rules (export-star resolution can run while
module resolution is open) for one error-path consumer. If a second consumer
needs the full table, that cache is the right move.

**Measured.** jsTyping on the merged head: 802 → 802 diagnostics, common
56 → 73, TSR-only 746 → 729, native-only 107 → 90 (all 17 TS2724
converted).
