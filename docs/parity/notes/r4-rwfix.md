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

Re-measured on integration head `59c76e7` (which already carries cause 2 and
much of cause 1; jsTyping there is TSR 505, common 133, native-only 30):
cause 12 takes common 133 → 150 and native-only 30 → 13, TSR-only
unchanged at 355, corpus unchanged, both loss checks empty.

## Cause 15 — unresolved Node core module reports TS2591 (`tsr-2zk.933`)

**Delivered as a measured patch, not as code:**
[r4-rwfix-ts2591.diff](r4-rwfix-ts2591.diff) (applies to this branch's head).
The lane owns `module_specifier_unfindable`; the port also needs the message
substitution in `report_module_not_found`, the three TS2307 reporters'
call sites (`check.rs`), and a `uses_wildcard_types` field read in
`Checker::apply_compiler_options` (`checker.rs`), none of which this lane
owns.

**Forcing constraint.** `typeof import("fs")` without `@types/node` reported
nothing. `module_specifier_unfindable` declined every Node core module, a
refusal list chosen because the right code was not TS2307. Native resolves a
core module like any other specifier and substitutes the message
(`resolveExternalModuleName`, `checker.go:15101` →
`getCannotResolveModuleNameErrorForSpecificModule`, `checker.go:15110`):
TS2580 when `types` contains `"*"`, else TS2591. A side-effect import keeps
its own message (`checkImportDeclaration`, `checker.go:5325`).

**Port (in the patch).**
- `module_specifier_unfindable_worker(specifier, admit_node_core)`; the
  existing `module_specifier_unfindable` passes `false` (unchanged answers
  for every caller), and the new `module_specifier_unfindable_for_diagnostics`
  passes `!uses_wildcard_types` and is called by the three TS2307 reporters.
- `cannot_resolve_module_name_error_for_specific_module` answers TS2591 for a
  core module; `report_module_not_found` uses it only when handed the TS2307
  default message, which is exactly the `resolveExternalModuleName` route.
- `is_node_core_module` is made exact (`core.NodeCoreModules()`: the
  unprefixed names bare and with `node:`, plus the five `node:`-only names).
  It used to admit every `node:` prefix, harmless only while it was a
  refusal list.

**Two declines kept, each measured.** A first version admitted core modules
for every caller and lost two corpus results:
- `compiler/localRequireFunction`, 4 type lines RIGHT → WRONG: the type
  callers (`get_type_of_alias`'s calibrated unfindable-import `any`) answer
  `any` for `const fs = require("fs")` in a JS file where native prints
  `error`. The type callers therefore keep the decline. Whether that `any`
  calibration is right for the JS `require` arm is a question for
  `symbols.rs`' owner, not settled here.
- `compiler/referenceTypesPreferedToPathIfPossible`, EMPTY_RIGHT →
  EMPTY_WRONG: under `@types: *` native loads `@types/node`, whose
  `declare module "url"` resolves the import. The conformance harness does
  not load wildcard `@types` (the CLI does, through `tsr-compiler`'s
  automatic type directive task, and agrees with native on a copy of the
  case), so TS2580 there was a false report. Core modules stay declined for
  diagnostics under a wildcard until the harness loads them; that is the
  only route to TS2580, so the patch does not spell it.

**Measured (patch on `c46e060`).** Corpus: `compiler/importTypeWithUnparenthesizedGenericFunctionParsed`
and `compiler/undeclaredModuleError` WRONG → RIGHT; both loss checks empty;
perf (41 samples, new/old median child CPU) domain-model 0.980,
generic-imports 1.000. jsTyping: TSR 505 → 516, common 150 → 161,
native-only 13 → 2 (all 11 TS2591), TSR-only unchanged.

**Remaining native-only (2).** TS7006/TS7031 at `compiler/sys.ts(1696)`,
`activeSession.post("Profiler.stop", (err, { profile }) => …)` with
`activeSession: import("inspector").Session | "stopping" | undefined`.
Native types the unresolved import type as `errorType`, so the callback's
parameters are implicitly `any`. That is `getTypeFromImportTypeNode`'s
answer for an unresolved module (`check.rs`'s import-type arm / the type
reference path), outside this lane.

## Perf note on cause 12

At 41 samples generic-imports measured 1.033, then 1.025, against a
domain-model 1.000. The code runs only on a missing-import report, and
generic-imports reports only one TS2322. Controls on the same box: the cause-2
binary vs the merged baseline 1.002, the baseline against a copy of itself
0.996, and the cause-12 binary in the *old* slot 1.020 (i.e. faster).
Callgrind on generic-imports: baseline 434,789,788 instructions, cause 12
434,651,470. Recorded as slot noise, not a regression.

Cause 2's callgrind on `generate_perf_project.py --modules 100`: 4,108,492,223
→ 4,104,520,214 instructions.
