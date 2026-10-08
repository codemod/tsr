# r4-realworld2 — `src/compiler` and `src/services` (`tsr-2zk.938`)

Round-4 triage lane under `tsr-2zk.914`, continuing
[r4-realworld](r4-realworld.md). No checker source was changed. This file
records what was measured on TypeScript's two largest projects, which
differences belong to causes already filed, the new root causes, and the time
analysis asked for by `tsr-2zk.935`. New repros are at the end of
`crates/tsr-conformance/tests/realworld_repros.rs`, each `#[ignore]`d with
its cause and asserting native's output.

## Method

Same as [r4-realworld](r4-realworld.md#method), on
`vendor/typescript-go/_submodules/TypeScript/src/{compiler,services}`
(`4d4f005c`). Each project got an untracked `tsconfig.scratch.json` next to
its `tsconfig.json`: `{ "extends": "./tsconfig.json", "compilerOptions":
{ "types": [], "outDir": "<scratch>", "pretty": false } }`. `references` is
not inherited through `extends`, so `services` compiles `../compiler` from
source, as jsTyping did.

- **Native.** `tsgo` from `scripts/offline-cargo/build-tsgo.sh` (Go 1.26.8
  built from source; the toolchain download was not needed), `vendor/typescript-go`
  @ `5b1047d`. The `outDir` was deleted before every native run.
- **TSR.** Release `tsr` at `81be45a4` (this branch: r4-realworld `ff7929d` +
  merge of `claude/beautiful-shannon-ar5gh0` @ `0cd6c437`).
- **Diff key.** `(file, line, column, code)` of each `error TS` line, as before.
- **Attribution.** The predecessor's two counterfactual binaries, rebuilt on
  this tree and never committed: *exp1* drops the origin gate's
  `return self.intrinsics.error` in `unions.rs` `build_origin_union`
  (cause 1); *exp2* adds `??=`/`||=`/`&&=` to `flow.rs`
  `get_initial_or_assigned_type` (cause 2). exp1+exp2 and exp2 alone were
  run on `services`; exp1's share is the difference. Survivors were grouped
  by message and reduced by hand or with a line-deletion script that keeps
  a candidate only while TSR reports the key and native reports nothing.

## Both projects now finish

Before the relation cache (`tsr-2zk.902`) both hung on every binary. Now:

| | compiler | services |
|---|---|---|
| TSR diagnostics | 898 | 1404 |
| native diagnostics | 86 | 260 |
| common keys | 56 | 56 |
| TSR-only | 842 | 1348 |
| native-only | 30 | 204 |

**`compiler` adds nothing new beyond jsTyping.** Its TSR-only set is the
jsTyping set key for key (paths normalised), because jsTyping already
type-checks every compiler file through the `_namespaces/ts` barrel. That
set is r4-realworld's 818 (identical per-code counts) **plus 24 TS7053**
that appeared with the round-4 merges (r4-index's new TS7053 arm,
`3d248ae8`); see causes N1 and N2.

`services` = the same 842 compiler-file keys + **506** keys in services
files. Native-only: TS2724 108 (17 compiler + 91 services), TS6307 83,
TS2591 11, TS7006 1, TS7031 1.

## Attribution

### Native-only (all already filed)

| Cause | compiler | services |
|---|---|---|
| 12 `export *` spelling suggestion (`tsr-2zk.930`) | 17 TS2724 | 108 TS2724 |
| 13 TS6307 composite file list (`tsr-2zk.931`) | 0 | 83 |
| 15 node core module TS2591 (`tsr-2zk.933`) | 13 | 13 |
| **total** | **30** | **204** |

### TSR-only, compiler files (842, in both projects)

| Cause | Count |
|---|---|
| r4-realworld causes 1-12, 16 and long tail, as recorded there | 818 |
| **N1** TS7053 arm fires on an access whose index info has an error value (rides cause 1) | 21 |
| **N2** intersection index infos skip a primitive constituent's apparent type | 3 |

N1 was measured with exp1: the 21 vanish with the origin gate removed, the
3 N2 lines stay.

### TSR-only, services files (506)

| Cause | Count | Existing issue |
|---|---|---|
| 1 Union origin slice gate (exp1, given exp2) | 310 | `tsr-2zk.16.46` |
| 12 `export *` suggestion (TS2305 vs native TS2724) | 91 | `tsr-2zk.930` |
| **N4** qualified reference to an alias whose body carries it is a print-only mint | 28 | `tsr-2zk.16.264` (existing; newly counted here) |
| **N3** enum literal widened under an indirect union contextual type | 26 | new |
| 2 Logical assignment does not narrow (exp2 alone: 14 TS18048 + 1 TS2345) | 15 | `tsr-2zk.923` |
| **N5** mapped type over an intersection loses members after an object-literal relation | 15 | new |
| **N6** assignment narrowing rejects a context-typed arrow in an optional property (`compilerHost`) | 7 | new |
| 4 Inference to `T & X` priority (`visitNode(…, isTypeNode)` TS2769) | 2 | `tsr-2zk.924` |
| **N7** check-order-dependent results (`navigationBar.ts` 259, 264) | 2 | new |
| Unclassified long tail | 10 | — |
| **sum** | **506** | |

exp1+exp2 left 97 services-file keys; 7 of those are not baseline keys at
all (lines cause 1 was hiding: `findAllReferences.ts` 2399, `convertImport.ts`
137/138, `completions.ts` 2131, `navigationBar.ts` 258/262, `transpile.ts`
256), so 90 baseline keys survive both counterfactuals. As in r4-realworld,
the counts behind causes 2-N7 are taken with cause 1 removed by a
counterfactual and may move once it lands for real.

## New root causes

### N1 — TS7053 arm reads an error-valued index access as "no index signature" (21)

- **Shape.** `options[name]` with `options: CompilerOptions` / `OptionsBase`,
  whose string index is `CompilerOptionsValue | TsConfigSourceFile |
  undefined`. That union holds the named union `CompilerOptionsValue` beside
  an object, so cause 1 turns the index value into `errorType`; the access
  then answers error and r4-index's arm reports TS7053 against a type that
  has a string index.
- **Native.** `getPropertyTypeForIndexType` (checker.go:27129-27184) reports
  TS7053 only when no index info applies; an applicable info returns its
  type, error or not.
- **TSR.** `crates/tsr-checker/src/index_access_reports.rs`
  `report_implicit_any_element_access` gates on "the access resolved to
  error". It does decline when an index info's value `is_error`, so the
  report is reached through a path where the info value it reads is not the
  error intrinsic itself while the access still answered error (not traced
  further).
- **Owner.** r4-index (`tsr-2zk.905`). Fixing cause 1 hides it; the arm
  should still ask whether an applicable info exists rather than infer it
  from the access's error answer.
- **Repro.** `index_signature_with_error_valued_info_is_not_an_implicit_any_access`.

### N2 — index infos of `string & {…}` miss `String`'s number index (3)

- **Shape.** `type Path = string & { __pathBrand: any }`; `path[i]` with
  `i: number` (`moduleNameResolver.ts:1255`, `resolutionCache.ts:1619`,
  `sys.ts:712`). TSR resolves the access to nothing; before r4-index that
  was a silent `any`, now it is TS7053.
- **Native.** `getIndexInfosOfType` = `getIndexInfosOfStructuredType(
  getReducedApparentType(t))`; for an intersection `getApparentType` →
  `getApparentTypeOfIntersectionType` → `getTypeWithThisArgument(t, t,
  needApparentType=true)`, which maps `string` to `String`, and
  `resolveIntersectionTypeMembers` collects its `[n: number]: string`.
- **TSR.** `crates/tsr-checker/src/index_signatures.rs`
  `get_index_infos_of_type`, intersection arm, recurses on the raw
  constituent; `members.rs` `apparent_type` has no intersection arm.
- **Repro.** `branded_string_intersection_has_string_number_index`
  (native: TS2322 `string` to `number`).

### N3 — enum literal property widened under an indirect union context (26)

- **Shape.** `this.changes.push({ kind: ChangeKind.Text, … })` with
  `changes: Change[]`, `[{ definition: { type: DefinitionKind.Symbol, … } }]`
  returned as `SymbolAndEntries[]`, `cond ? undefined : { kind: … }`. TSR
  prints `{ kind: ChangeKind; … }` and reports TS2322/TS2345. A direct
  `const c: C = { kind: K.R }` and plain string/number literal discriminants
  agree with native; the miss needs an enum member literal and a union
  contextual type reached through an array element, a conditional branch,
  or a rest argument.
- **Native.** `checkObjectLiteral` → `getWidenedLiteralLikeTypeForContextualType`
  with `isLiteralOfContextualType` over the property's contextual type from
  `getContextualTypeForObjectLiteralElement` / `getTypeOfPropertyOfContextualType`
  (and `getContextualTypeForArgumentAtIndex` for the rest case).
- **TSR.** `crates/tsr-checker/src/objects.rs` (the literal-freshness
  decision before `get_widened_literal_type`, ≈:2660-2715, including its
  `in_call_argument` ancestor walk) and `contextual.rs`
  `contextual_type_for_argument` for rest parameters;
  `signatures.rs` `is_literal_of_contextual_type`.
- **Existing issue to check first.** `tsr-2zk.16.150`
  (CHECK-EXPRESSION-FOR-MUTABLE-LOCATION-CONTEXT) names the same function
  and its `in_call_argument` exclusion for a member nested in a call
  argument's object literal. N3's array-element and conditional-branch
  shapes are not call arguments, so it is at least wider than that row;
  the integrator may file it as a sibling or widen `.16.150`.
- **Repro.** `enum_literal_member_keeps_literal_under_indirect_union_context`.

### N4 — qualified reference to a union alias is opaque (28; existing `tsr-2zk.16.264`)

- **Shape.** `textChanges.TypeAnnotatable`, `textChanges.ThisTypeAnnotatable`,
  `codefix.ImportOrRequireAliasDeclaration`, `codefix.AddNode`,
  `FindAllReferences.Entry` (no narrowing to `NodeEntry`). Even
  `namespace a { export type TA = string | number }` makes `a.TA` assignable
  from nothing and to nothing; it prints `a.TA` where native prints `TA`.
- **Native.** `getTypeFromTypeReference` → `getTypeReferenceType` →
  `getTypeFromTypeAliasReference`: the alias's declared type, whatever the
  spelling.
- **TSR.** `crates/tsr-checker/src/declared.rs` `qualified_type_reference`:
  an argument-less alias whose declared type carries the alias keeps the
  written-text OBJECT mint, because routing it to the declared type printed
  `T7` for `N.T7` (36 R→W, NB-SYMBOL-CHAIN). Filed as
  `tsr-2zk.16.264` QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE/ALIAS-CARRYING-BODY;
  not re-filed. Real-world cost: 28 lines here.
- **Repro.** `namespace_qualified_alias_reference_is_the_aliased_type`.

### N5 — mapped type over an intersection loses its members (15)

- **Shape.** `Mutable<ImportsCollection & { useRequire: boolean }>` in
  `importFixes.ts`: `entry.useRequire`, `entry.namedImports`, … report
  TS2339. Reduced: `type M<T> = { [K in keyof T]: T[K] }; const w: M<{ d?:
  string } & { u: boolean }> = { u: true }; w.u`. `declare const v: W; v.u`
  works when checked first; after an object literal has been related to
  `W`, `w.u`, `(w as W).u` and a later `v.u` all fail. So it is not flow
  narrowing but the mapped type's member table, and it is order-dependent.
  r4-realworld's cause 8 (`tsr-2zk.925`, array intersection) fails even on
  the declared type, so this is a separate shape in the same family.
  `tsr-2zk.16.214` (generic `(E<T> & S<T>)[keyof T]`) is another shape.
- **Native.** `resolveMappedTypeMembers`, `instantiateMappedType`
  (`isArrayOrTupleOrIntersection` arm); no order dependence.
- **TSR.** `crates/tsr-checker/src/mapped.rs` member resolution, reached
  from the relater during the object-literal assignment.
- **Repro.** `mapped_intersection_keeps_members_after_object_literal_relation`.

### N6 — assignment narrowing rejects a context-typed arrow in an optional property (7)

- **Shape.** `let compilerHost: CompilerHost | undefined = { …33 members… }`
  in `services.ts` stays `CompilerHost | undefined` at the next line in TSR
  (TS18048 ×5, TS2345 ×2); native narrows to `CompilerHost`. With
  `{} as CompilerHost` as the initializer all seven go. A bisection that
  replaced member values with `null!` (all members kept, so the literal stays
  complete) found that `writeFile: noop`, `directoryExists: d => { … }` and
  `getDirectories: p => { … }` must all be nulled: each of them alone keeps
  the declared type. Reduced: `interface H { d?: (x: string) => boolean }`,
  `let h: H | undefined = { d: x => true }; h.d` — TS18048 in TSR only.
  Annotating `x`, a referenced function value, or a required `d` agree.
  The `writeFile: noop` member reproduced with a function-type alias target
  (`type WF = (f: string, t: string, b: boolean) => void`) but not with the
  same type written inline; not reduced further.
- **Native.** `getAssignmentReducedType` (flow.go): `filterType` with
  `typeMaybeAssignableTo`, then the `isTypeAssignableTo(assignedType,
  reducedType)` guard, with the initializer's type from
  `getInitialOrAssignedType` → `getTypeOfExpression`.
- **TSR.** `crates/tsr-checker/src/flow.rs` `get_assignment_reduced_type`
  (the `reference_assignable_decidable` / `is_type_assignable_to` guard) and
  the type `get_initial_or_assigned_type` gets from `check_expression` for a
  context-sensitive member.
- **Repro.** `assignment_narrowing_accepts_context_typed_arrow_in_optional_property`.

### N7 — check order changes diagnostics (2 in the default run)

`tsr --singleThreaded` and the default 4-checker pool disagree on 1 key in
`compiler` and 5 in `services`, all around `BindableStaticNameExpression`
(a recursive alias pair with `BindableStaticElementAccessExpression`):
`utilities.ts:4258` TS2322 and `navigationBar.ts` 259/264 only in the pool,
`navigationBar.ts` 545/548 only single-threaded. Pooled runs are
deterministic (`--checkers 2` and `4` agree on `compiler`). Native gives
none of these. Each checker in the pool checks a different file sequence,
so some type answer depends on what was resolved first; N5 is one known
order-dependent mechanism, the recursive alias pair is another candidate.
No standalone repro: two same-file orderings of a hand-written recursive
alias pair agree.

### Unclassified tail (10)

`importFixes.ts:1276`, `organizeImports.ts:165,184` (an `unknown`
argument from inference), `formatting.ts:572` TS2454, `goToDefinition.ts:507`
×2, `services.ts:2695,2705` and `pasteEdits.ts:111` (object literal vs
`CodeFixContext`, whose base is `textChanges.TextChangesContext`; a
hand-written qualified-heritage copy agrees), `services.ts:3266`.

## Time

4-core box, release builds, `--extendedDiagnostics`. TSR rows were run
under a per-thread CPU sampler (`/proc/<pid>/task/*/stat`, 0.5 s).

| | Parse | Bind | Check | Emit | Wall | User |
|---|---|---|---|---|---|---|
| compiler, TSR pool (4) | 0.253 s | 0.064 s | 22.25 s | — | 23.2 s | 52.1 s |
| compiler, TSR `--singleThreaded` | | | 43.0 s | — | 43.6 s | 43.1 s |
| compiler, native default | 0.229 s | (in check) | 1.471 s | 0.210 s | 2.22 s | 5.16 s |
| compiler, native `--singleThreaded` | 0.263 s | (in check) | 2.050 s | 0.366 s | 2.99 s | 3.53 s |
| services, TSR pool (4) | 0.230 s | 0.083 s | 38.42 s | — | 39.0 s | 90.3 s |
| services, TSR `--singleThreaded` | | | 69.8 s | — | 70.5 s | 69.6 s |
| services, native default | 0.249 s | (in check) | 1.867 s | 0.200 s | 2.74 s | 7.14 s |
| services, native `--singleThreaded` | 0.362 s | (in check) | 3.178 s | 0.529 s | 4.48 s | 5.33 s |

Single-threaded check: TSR is **21x** native on `compiler` (43.0 / 2.05) and
**22x** on `services` (69.8 / 3.18). Wall: 10.5x and 14.2x.

### user ≈ 2x wall is imbalance, not duplicated checking

- **No file is checked twice.** `Checked files` is 77 / 251, the sum over
  the four checkers, and `checker_pool.rs` assigns file `i` to checker
  `i % 4` exactly as native `checkerpool.go` does.
- **Per-checker CPU** (thread sampler; checker 0-3): compiler 3.5 / 12.7 /
  12.3 / 21.4 s; services 37.2 / 18.1 / 17.0 / 15.4 s. Wall is the slowest
  checker.
- **Per-file check time** (temporary `eprintln!` around `check_source_file`
  in `checker_pool.rs`, separate target dir, not committed): `checker.ts`
  alone is **14.0 s** single-threaded, 16.8 s in the compiler pool and
  19.9 s in the services pool; next are `binder.ts` 3.9 s, `utilities.ts`
  3.5 s, `nodeFactory.ts` 3.1 s, `transformers/jsx.ts` 2.8 s, `parser.ts`
  2.6 s. In services the checker that owns `checker.ts` also owns
  `binder.ts` (9.7 s there): 41.1 s of a 39-41 s wall. The critical path is
  `checker.ts`'s owner.
- **Repeated global work** exists and is native's design: summed per-file
  check time is 56.4 s pooled vs 43.1 s single on compiler (+31%), 97.6 vs
  74.6 s on services (+31%); thread CPU says +19% / +28%. Each checker
  lazily resolves the shared declarations it touches (lib, `types.ts`,
  imported signatures) again. Native shows the same overhead: user 5.16 vs
  3.53 s (+46%) and 7.14 vs 5.33 s (+34%). TSR does not repeat more than
  native does, proportionally.

So the r4-realworld observation (user = 2x wall) is the critical-path file
plus ~30% native-shaped per-checker repetition. The lever is the absolute
cost of checking one large file — `checker.ts` at 14 s against native's
2.05 s for the whole project — which is `tsr-2zk.915`/`tsr-2zk.935`'s
territory (relation call volume from narrowing, first-walk member
resolution). A per-file breakdown is the cheapest next profile target:
`checker.ts` alone reproduces most of the gap.

The repository's `work-trace` feature records every type query; on
`compiler` it wrote 13.7 GB in 45 s before it was stopped, so it is not
usable at this scale without a file-level filter.
