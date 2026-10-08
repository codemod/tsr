# r4-realworld — why real projects disagree (`tsr-2zk.916`)

Round-4 triage lane under `tsr-2zk.914`. No checker source was changed here;
this file records what was measured, the root causes it found, and a minimal
repro for each. The repros are tests in
`crates/tsr-conformance/tests/realworld_repros.rs`. Each one asserts native's
output and is `#[ignore]`d until its cause is ported.

## Method

- **Projects.** TypeScript's own `src/jsTyping` and `src/typingsInstallerCore`
  (`vendor/typescript-go/_submodules/TypeScript` @ `4d4f005c`). Each was run
  through an untracked scratch `tsconfig.scratch.json` placed next to the
  original: `{ "extends": "./tsconfig.json", "compilerOptions": { "types": [],
  "outDir": "<scratch>", "pretty": false } }`. `types: []` keeps native from
  stopping at TS2688 (no `@types/node`). `outDir` keeps emit out of the
  submodule. **The scratch config must sit in the project directory.** From
  any other directory TSR finds 0 files (cause 14 below).
- **Tools.** Native `tsgo` built by `scripts/offline-cargo/build-tsgo.sh` from
  `vendor/typescript-go` @ `5b1047d`. TSR is the release `tsr` at this branch's
  merge of `claude/beautiful-shannon-ar5gh0` into the relcache lane
  (`682a1ba` + merge). Both were run from `src/` with `--pretty false`.
- **Diff key.** `(file, line, column, code)` of each `error TS` line. Related
  information lines are ignored. Native is composite and incremental:
  **delete the `outDir` before every native run**, or the run replays
  `.tsbuildinfo` (0.3 s, same diagnostics).
- **Attribution.** Two counterfactual binaries, built locally and never
  committed, measured how many diagnostics a cause accounts for:
  - *exp1* replaces the origin gate's `return self.intrinsics.error` in
    `unions.rs` `build_origin_union` with the plain union.
  - *exp2* adds the logical-assignment operators to `flow.rs`
    `get_initial_or_assigned_type`.

  The remaining diagnostics were grouped by message and source line, then
  reduced by hand or with a delta script to a few lines that reproduce the
  difference against native.

## Counts

| | jsTyping | typingsInstallerCore |
|---|---|---|
| TSR diagnostics | 874 | 874 |
| native diagnostics | 163 | 169 |
| common keys | 56 | 56 |
| TSR-only | 818 | 818 |
| native-only | 107 | 113 |

The two projects' TSR-only sets are **identical**: every difference is in
`src/compiler/*.ts`, which both projects import. The native-only sets differ
only in TS6307 (77 vs 83). The brief's 875 is one more than measured here.

TSR-only by code: TS2339 293, TS2345 190, TS2769 91, TS18048 79, TS2322 55,
TS2739 32, TS2740 24, TS2305 17, TS2352 13, TS2540 9, TS2367 6, others 7.
Native-only: TS6307 77/83, TS2724 17, TS2591 11, TS7006 1, TS7031 1.

## Root causes

Accounted counts are TSR-only (or native-only) keys on jsTyping. They are the
same on typingsInstallerCore, except TS6307.

| # | Cause | Accounts for | Native function | TSR owner |
|---|---|---|---|---|
| 1 | Union origin slice gate answers `errorType` | **485** TSR-only | `getUnionTypeWorker` (checker.go:25653, origin :25705-25728) | `crates/tsr-checker/src/unions.rs` `build_origin_union` gate (≈:1012-1058); existing issue `tsr-2zk.16.46` |
| 2 | Logical assignment is not an assignment for flow | **97** TSR-only | `getAssignedType` (flow.go:2288) → `getAssignedTypeOfBinaryExpression` (flow.go:2314); `getTypeAtFlowAssignment` (flow.go:220) incl. `isInCompoundLikeAssignment` | `crates/tsr-checker/src/flow.rs` `get_initial_or_assigned_type` (`=` only) |
| 4 | Inference to `T & X` not demoted to `NakedTypeVariable` priority | **79** TSR-only (all `visitNode(x, visitor, isFoo)` TS2769) | `inferToMultipleTypes` (inference.go:448, intersection arm :523) | `crates/tsr-checker/src/inference.rs` (intersection arms of `infer_from_types_within` / `intersection_inference_source`) |
| 8 | Homomorphic mapped type over an array intersection not distributed | 18 | `instantiateMappedType` (`isArrayOrTupleOrIntersection` arm) | `crates/tsr-checker/src/mapped.rs` |
| 5 | Homomorphic mapped type over a union loses generic-base inherited members | 17 (`Mutable<HasContainerFlags>` TS2345 + TS2540) | `instantiateMappedType` / `resolveMappedTypeMembers` | `crates/tsr-checker/src/mapped.rs` |
| 12 | Missing-import spelling suggestion ignores `export *` members | 17 (TS2305 here, TS2724 native) | `getSuggestedSymbolForNonexistentModule` via `getExportsOfModule` | `crates/tsr-checker/src/symbols.rs` `report_missing_module_export` |
| 6 | Generic return keeps the constraint (`createModifier` → `ModifierToken<ModifierSyntaxKind>`) | 15 | `chooseOverload`/`inferTypeArguments` through `NodeFactory.createToken` overloads | `crates/tsr-checker/src/calls.rs` / `inference.rs` (not reduced, see below) |
| 7 | Alias type-argument variance applied to a **union** alias | 12 (`toSearchResult(undefined)`) | `structuredTypeRelatedToWorker` alias arm, gated on `Object\|Conditional` sources | `crates/tsr-checker/src/relater.rs` / `variances.rs` |
| 9 | Destructuring does not read the narrowed property | 7 (`const { cache } = state` after `if (!state.cache)`) | `getFlowTypeOfDestructuring` | `crates/tsr-checker/src/destructure.rs` / `binding_patterns.rs` |
| 11 | Comparable relation rejects weak targets / optional source members | 5 TS2352 (`as TracingNode`, `as EmitNode`) | `isRelatedTo` weak check skipped for comparable (relater.go:2675); optionality ignored under comparable | `crates/tsr-checker/src/relater.rs` (`assertion_overlap.rs` calls it correctly) |
| 10 | Closure narrowing of a never-reassigned destructured `let` | 3 (`let { oldProgram } = …`) | `isSymbolAssigned` / constant-reference rule in `getNarrowedTypeOfSymbol` | `crates/tsr-checker/src/flow.rs` (`symbol_has_any_assignment` and friends) |
| 3 | Overload resolution loses a nested generic call's contextual tuple | 2 (`new Map(xs.map(x => [x, i]))`), likely ≈5 more of the long tail | `chooseOverload` re-checks arguments per candidate | `crates/tsr-checker/src/calls.rs` |
| 13 | TS6307 composite file-list check missing | **77 / 83** native-only | `Program.verifyCompilerOptions` (compiler/program.go:938-956) | not ported; `crates/tsr-compiler` has no `verifyCompilerOptions` |
| 15 | Node core module reports nothing instead of TS2591 | 11 native-only TS2591 + 2 TS7006/TS7031 | `getCannotResolveModuleNameErrorForSpecificModule` (checker.go:15110) | `crates/tsr-checker/src/check.rs` `module_specifier_unfindable` (declines every core module) |
| 16 | `any` source vs object target under Subtype/StrictSubtype answers `Unknown` | 0 on these projects (found while reducing #1) | `isSimpleTypeRelatedTo` (relater.go:209-262) → False | `crates/tsr-checker/src/relater.rs` (final `flag_decidable` arm, ≈:1495) |
| 14 | Inherited `include` resolved against the extending config's directory | 0 diagnostics, but **0 files** found | `parseConfig` `relativeDifference` (tsoptions/tsconfigparsing.go:1105-1128) | `crates/tsr-tsoptions` |
| — | Unclassified long tail | 61 TSR-only | — | see below |

Sum of TSR-only attribution: 485 + 97 + 79 + 18 + 17 + 17 + 15 + 12 + 7 + 5 + 3
+ 2 + 61 = 818. Native-only: 77 + 17 + 11 + 2 = 107.

### Ordering and coupling

Cause 1 dominates and masks the rest. Its `errorType` acts as `any`, so checks
on it are silent, and relations through it answer `Unknown`. Narrowing by a
type predicate then keeps the declared `Node`. That is where most of the
TS2339 `'X' does not exist on type 'Node'` (202) and `Declaration` (44) lines
come from. Fix it first, then re-measure: exp1 alone introduces 23 new
TSR-only keys of its own (it prints the plain union without origin). Those are
artifacts of the counterfactual, not predictions. The counts for causes 2-12
were taken *after* exp1/exp2 and could shift once cause 1 lands for real.

## Repros

Test names refer to `realworld_repros.rs`.

1. `named_union_alias_with_object_constituent_is_a_real_union`
   `type XY = A | B; declare const md: XY | C; const t1: number = md;`.
   Native gives TS2322; TSR is silent (`errorType`). The jsTyping shape is
   `predicate_narrowing_through_named_union_parent_chain`:
   `ModuleDeclaration.parent: ModuleBody | SourceFile` with
   `ModuleBody = NamespaceBody | JSDocNamespaceBody`. In TSR,
   `isModuleBlock(p)` leaves `p: Node`.
2. `logical_assignment_narrows_its_target`: `a ??= b; a.length` (also `||=`,
   and optional parameter `a ??= ""`). TSR gives TS18048.
3. `overloaded_generic_call_gives_nested_map_callback_a_tuple_context`: two
   generic overloads taking `readonly [K, V]` entries, and `new Map`. TSR
   gives TS2769.
4. `intersection_target_inference_has_naked_type_variable_priority`:
   `g<T>(a: T, cb: (n: T & F) => void)` with `string | undefined` and a
   `(n: string)` callback. Native infers `string | undefined`; TSR infers
   `string` and adds TS2345.
5. `mapped_type_over_union_with_generic_base_member`:
   `Mutable<OLE | Node>` where `OLE extends Base<number>`.
6. No standalone repro. With the real types, the following gives
   `ModifierToken<ModifierSyntaxKind>` where native gives
   `ModifierToken<SyntaxKind.ExportKeyword>`:
   `function cm<T extends ModifierSyntaxKind>(k: T) { return f.createToken(k); }`
   with `f: NodeFactory`. Hand-written copies of the overload set and token
   interfaces (enum `K`, three overloads, interface inheritance) do not
   reproduce. The next reduction should keep the real `SyntaxKind` enum
   (about 400 members) and `KeywordTypeNode`'s default type argument.
7. `union_alias_relates_structurally_not_by_variance`:
   `SearchResult<undefined> -> SearchResult<string>`. The object-alias
   control `R<T> = { value: T | undefined }` errors in both tools.
8. `mapped_type_over_array_intersection_maps_each_constituent`:
   `Readonly<string[] & { __brand: any }>`, then `.length`.
9. `destructured_property_keeps_reference_narrowing`.
10. `destructured_let_narrowing_reaches_closures`. The `let p = o.p` control
    already agrees.
11. `assertion_to_weak_or_optional_target_is_comparable`.
12. `missing_import_suggests_re_exported_name`. Three files: a barrel
    `export *`, then `import { Diagnostics }`.
15. `unresolved_node_core_module_reports_install_types_hint`:
    `typeof import("fs")` with no node types.
16. `any_property_source_is_not_a_subtype_of_an_object_property`:
    `[sf, n]` where `SourceFile.parent: any`. TSR declines subtype reduction
    and is silent.

13 (TS6307) and 14 (extends/include) are program/config-level, and the
conformance harness cannot express them. Repro for 14: `base/tsconfig.json`
with `"include": ["*.ts"]` and `ext/tsconfig.json` with
`"extends": "../base/tsconfig.json"`. Native lists `base/a.ts`; TSR reports
TS18003. Repro for 13: a `composite` project that imports a file outside its
`include`.

### Smaller findings seen on the way (not counted above)

- TS2739's missing-property list is ordered differently
  (`statements, _statementBrand, …` vs native
  `statements, _jsdocContainerBrand, _localsContainerBrand, _statementBrand`).
  The key matches, the message does not.
- A script (non-module) file declaring a global `interface Node` makes TSR
  report TS2320/TS2344 inside `lib.dom.d.ts` when lib is the default. Native
  reports nothing there.
- TSR emits no related-information/elaboration lines (`Type 'undefined' is
  not assignable to …`). The diff key ignores them.

### Unclassified long tail (61)

These were not reduced. Visible sub-shapes:

- array-literal callback returns that should be tuples via generic return
  context (`append(result, [a, b])` ×3, `map(…, f => [sourceFile, f])`,
  `commentDirectives.map(… => [line, d])` and its TS2339/TS2345 echoes),
  probably cause 3's family;
- `originalExtension === Extension.Ts` compared as `""` (TS2367 ×6);
- `Debug.assertNotNode(node, isIdentifier)` (TS2769);
- TS2394 overload/implementation compatibility ×2;
- `PrivateIdentifier*Declaration` parameter mismatches in esDecorators.ts ×4;
- TS2352 to `MappedType`/`DynamicNamedDeclaration`/`PragmaDefinition`/
  `DiagnosticMessageChain` ×8.

## Time split (`--extendedDiagnostics`, for `tsr-2zk.915`)

4-core box. Native was measured with the `outDir` cleared before each run.

| | Parse | Bind | Check | Emit | Total / wall |
|---|---|---|---|---|---|
| jsTyping, TSR | 0.126 s | 0.076 s | 21.90 s | — | 22.19 s (real 20.0 s, user 40.0 s) |
| jsTyping, native default | 0.184 s | (in check) | 1.231 s | 0.220 s | 1.717 s (wall 1.91 s) |
| jsTyping, native `--singleThreaded` | 0.220 s | (in check) | 1.861 s | 0.314 s | 2.501 s |
| typingsInstallerCore, TSR | 0.148 s | 0.062 s | 23.99 s | — | 24.32 s (real 20.1 s, user 40.2 s) |
| typingsInstallerCore, native default | 0.169 s | (in check) | 1.262 s | 0.143 s | 1.645 s |
| typingsInstallerCore, native `--singleThreaded` | 0.195 s | (in check) | 1.907 s | 0.371 s | 2.577 s |

Parse and bind are at parity. All of the gap is in check: about 11.8x native
single-threaded check, about 18x native default. TSR's user time is twice its
wall time, so two threads each do about 20 s of checking. Whether that is
duplicated work is a question for `tsr-2zk.915`. The exp1 counterfactual
(cause 1 removed) did not change wall time (24.3 s), so the error-type
cascade is not what makes TSR slow.
