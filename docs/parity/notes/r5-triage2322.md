# Round 5 — TS2322 / TS2345 root-cause census (lane r5-triage2322)

Epic `tsr-2zk`. Analysis lane: no checker code. The integrator turns the
ranked buckets below into lanes.

**Status: complete.** Every one of the 863 differing TS2322/TS2345 lines is
bucketed. 27 lines stay unclassified, each with a hypothesis in its slice file.

## Method

- Tree: `f90fcef` (integration head = `405b55c` + a `main` merge; the
  population below differs from the brief's `405b55c` counts by one case on
  each of two rows, see §1).
- `diagverdictdump` unfiltered; rows kept: `WRONG`/`EMPTY_WRONG`, key without
  `(` (configuration variants excluded); expected/actual
  `BaselineDiagnostic` multisets diffed. 351 cases carry at least one TS2322
  or TS2345 difference.
- `examples/r5census.rs` (added by this lane, instrument only) prints one
  row per differing line: side, position, code, the
  `report_assignability_failure` gate under `TSR_ASSIGN_PROBE=1`
  (`NEVER` = no TS2322 report attempted at that position; `DECLINED` = the
  relation answered `Unknown`; `NOTREPORTABLE`; `REPORTED`), the native
  message chain from the pinned reference baseline, TSR's rendered message,
  and the source line. With `CODES=` empty it covers every code, which is how
  a TS2322 extra is paired with the native diagnostic it displaced (e.g.
  TS2353 at the property).
  ```text
  TSR_ASSIGN_PROBE=1 CODES= CASES=cases.txt cargo run --release -p tsr-conformance --example r5census
  ```
- **Native oracle.** Native tsgo could not be built in this container: the
  pinned `go.mod` asks for the Go 1.26 toolchain and `proxy.golang.org` is
  not in the egress allowlist (`403 Host not in allowlist`). The native
  answer per line is therefore the pinned reference baseline
  (`testdata/baselines/reference/submodule/**.errors.txt`, which *is* native
  output at `5b1047d`) and the native function was identified by reading the
  pinned Go source, not by a debug print. Repros were run through TSR with
  `examples/probefile`.

## 1. Population

| | brief (`405b55c`) | this tree (`f90fcef`) |
|---|---|---|
| plain cases failing on TS2322 alone | 162 | 160 (missing in 118, extra in 54) |
| plain cases failing on TS2345 alone | 57 | 56 (missing in 54, extra in 2) |
| cases with any missing TS2322 | 184 | 184 (423 lines) |
| cases with any extra TS2322 | 117 | 116 (224 lines) |
| cases with any missing TS2345 | 83 | 82 (182 lines) |
| cases with any extra TS2345 | 20 | 20 (35 lines) |
| mixed-code cases touching TS2322/TS2345 | ~380 | 135 (of 1,172 plain WRONG/EMPTY_WRONG) |

The brief's "TS2322 alone: 162" uses the per-code set view; here "alone"
means the multiset difference contains no other code.

## 2. Mechanical split (gate × side)

| side | code | gate | lines | cases |
|---|---|---|---|---|
| EXTRA | 2322 | – | 224 | 116 |
| EXTRA | 2345 | – | 35 | 20 |
| MISS | 2322 | DECLINED (relation `Unknown`) | 161 | 62 |
| MISS | 2322 | NEVER (no report attempted) | 220 | 110 |
| MISS | 2322 | NOTREPORTABLE | 33 | 17 |
| MISS | 2322 | REPORTED (elsewhere) | 8 | 7 |
| MISS | 2345 | (call path; probe n/a) | 182 | 82 |

Of the 259 extra lines, 144 TS2322 + 14 TS2345 have **no** native
diagnostic of any code within three lines (TSR's relation or one of its
operand types is wrong); the rest pair with a native TS2353 (28), TS2741 (21),
TS2739/2740 (8), a TS2322/TS2345 at another column (23), TS2820 (4), …
(reporter or excess-property path).

## 3. Ranked root causes

Slices were classified independently, and the per-slice files under
[`r5-triage2322/`](r5-triage2322/) hold each bucket's full case list, its
minimal repros (native expected vs TSR actual) and the evidence.
[`census.tsv`](r5-triage2322/census.tsv) is the raw row list. Buckets with
the same root cause in several slices are merged here, and the slice bucket
ids are kept (`A1.B5` = slice A1, bucket B5) so every number traces back.

Slices:
- **A1/A2**: extras with no native diagnostic nearby.
- **B**: extras that pair with another native code, plus TS2345 extras.
- **C**: missing lines whose relation answered `Unknown`, or that were
  reported elsewhere.
- **D**: missing TS2322 never attempted.
- **E**: missing TS2345, plus NOTREPORTABLE.

**Counting.** *Lines* are differing diagnostic lines. *Cases* sum the
per-slice case counts, so a case that appears in two slices of one bucket is
counted twice; treat it as an upper bound.

**Owner.** The file where the faithful fix lands. Neither the ownership table
(`round5.md`) nor `docs/parity/lanes/*` existed on the integration branch or
`main` at `ac56208`, so ownership is checked against `main`'s activity
instead. "main-active" marks the files the brief names (`members.rs`,
`contextual.rs`, `flow.rs`, `calls.rs`, `inference.rs`). `relater.rs` (17
non-merge commits since 2026-10-01), `assignreport.rs` and `declared.rs` are
also busy on `main`.

**Confidence.** C means confirmed by a repro run through TSR's diagnostics
pipeline plus a reading of both sides at the named lines. R means the effect
is confirmed, but the exact failing predicate is a reading, not a trace. The
slice files mark this per bucket.

### 3a. False positives (TSR reports, native does not) — ranked first

Each line is a confident wrong answer: 224 TS2322 and 35 TS2345 lines in 136
cases.

| # | bucket (slice ids) | root cause | native anchor (`internal/checker/`) | TSR function (`crates/tsr-checker/src/`) | lines | cases | owner |
|---|---|---|---|---|---|---|---|
| X1 | **Missing-property code choice** (B.B2, C.REP) | TS2741/2739/2740 is chosen only when certified member tables exist. Otherwise TSR prints a plain TS2322 head: alias/mapped types, generic instantiations, intersection source or target (every JSX row), `types.A`, lib `Function`/`TemplateStringsArray`. The reverse also occurs (C.REP: TSR emits TS2741 where native keeps a TS2322 head because `chainArgsMatch` differs). | relater.go:4345 `reportUnmatchedProperty`, :4705 `reportErrorResults`, :4751 `reportRelationError`, :4805 `chainArgsMatch` | assignreport.rs:1510 `missing_required_property`; relater.rs:797 `unmatched_property_report` | 30+7 | 18+6 | assignreport.rs |
| X2 | **Excess-property report path** (B.B1, C.EPC) | The relater's excess arm fails the relation silently. The TS2353/TS2561 pre-pass declines on intersection/mapped/alias targets, index signatures, spreads, shorthand, computed names, nested literals, `\|\|` initializers and namespace targets, so the fallback prints a TS2322 head where native prints TS2353 at the property. | relater.go:2714 `hasExcessProperties` | assignreport.rs:1320 `check_excess_properties`; silent arm relater.rs:1189; fallback assignreport.rs:1756 | 26+1 | 17+1 | assignreport.rs |
| X3 | **Conditional-type relation arms** (A1.B6, B.B9, B.B12, A2.CND, A1.B23; misses C.CT, C.CS, C.CC) | Several arms are missing: conditional **source** (default constraint, then distributive constraint), conditional **target**, and conditional↔conditional. The union-target arm also returns before the type-variable constraint fallthrough. There is no true-branch substitution (`getConditionalFlowTypeOfType`: `Extract<T,U>`'s true branch is `T & U`). Extras show the arm answering NotRelated; misses show it answering Unknown. | relater.go:3721-3770, :3540, :3727, :3370-3386; checker.go:24952 `getConditionalFlowTypeOfType` | relater.rs:2885 `structured_type_related_to_worker` (:2936 union arm, end-of-worker Unknown :1567); declared.rs:7357 `capture_conditional_alias_branches` | 16 extra + 29 miss | 8+15 | relater.rs (+declared.rs) |
| X4 | **`typeRelatedToDiscriminatedType` unported** (A2.D, A1.B12, B.B8) | A source whose discriminant is a union (object or tuple) is not related to a discriminated-union target, and the union-target arm answers NotRelated. | relater.go:3989 (called at :3893) | relater.rs:2885 `structured_type_related_to_worker` (no arm) | 16 | 5 | relater.rs |
| X5 | **Array/object-literal elaboration declines** (B.B3, B.B4, D.D4, D.D6; numeric names A2.ENUM, D.D7) | `elaborate_array_literal` bails on a **union** target (an optional parameter's `T[] \| undefined`, `Book \| Book[]`) and on spreads; object elaboration bails on spreads and skips computed names. Numeric names (`2.0:`) are not canonicalized. TSR then reports the outer TS2322/TS2345 (extra) instead of the per-element TS2322 (missing). | relater.go:522 `elaborateArrayLiteral`, :498 `elaborateObjectLiteral`, :620 `getBestMatchIndexedAccessTypeOrUndefined` | assignreport.rs:3344/3349 `elaborate_array_literal`, :2481/2497/2531 `elaborate_object_literal_members` | 19 extra + 36 miss | 15+14 | assignreport.rs |
| X6 | **Generic mapped relation arms** (B.B13, A1.B16, A2.GMI, A2.GM; misses C.M1, C.M2, C.M3, C.IAM) | Several gaps. A generic mapped type reached through an alias (`Record<K,T>`) is not recognized. The relater never uses the resolved apparent type of a homomorphic mapped source. The "generic mapped source vs string index → template" arm is missing. On the miss side: a mapped target never reaches native's default false, a mapped source is excluded from "object → T is false", and only the modifier gate of `mappedTypeRelatedTo` is ported. | relater.go:3593, :3805-3812, :3423-3433, :3972 `mappedTypeRelatedTo`, :4590; checker.go:21772 | relater.rs:1587 `generic_mapped_target_related_to`, :1788 `is_generic_mapped_target`, :1431-1440, :1721 `mapped_modifiers_reject`, :2377 `related_index_signatures`; mapped.rs:1265 `apparent_mapped_type` | 8 extra + 34 miss | 5+14 | relater.rs |
| X7 | **Variance: alias/unmeasured/unreliable** (A1.B5, A2.V, B.B14) | Mapped- or reference-bodied alias variances are never measured (they default to Covariant), and Unreliable/Unmeasurable are absent, so a failed variance check never falls back to the structural walk. | relater.go:3392 `getAliasVariances`, :3266 `relateVariances`, :1341 `getVariancesWorker`, :1493 | relater.rs:3371-3430; variances.rs:26 `inference_variances` | 10 | 5 | relater.rs / variances.rs |
| X8 | **Flow: destructuring narrowing** (A2.DA, A1.B3, A1.B22, A2.DK) | `getAssignedType`'s destructuring arms (`({x:d}=o)`, `[,g]=t`) and `getFlowTypeOfDestructuring` are unported, so the declared rather than narrowed slice is used. A binding default uses NEUndefined facts instead of `getNonUndefinedType`. | flow.go:2288 `getAssignedType` (:2361, :2365); checker.go:17849, :17789 | flow.rs:2543 `get_initial_or_assigned_type`; destructure.rs:86 `get_type_for_binding_element`, :348, :182 | 19 | 5 | flow.rs (main-active) / destructure.rs |
| X9 | **Compound-like assignment literal base** (A1.B1, A2.OP) | For `x = x + 1`, the reporter reads the target's declared literal type and skips `isInCompoundLikeAssignment`'s base-literal rule (the expression walker applies it). After TS2469, `s += ""` still reports. | checker.go:11110 `checkIdentifier`; :12443 | assignreport.rs:1166 `assignment_target_type` (identifier arm :1259); check.rs:577 | 12 | 2 | assignreport.rs |
| X10 | **Qualified type reference minted as an empty print-only object** (A1.B11; misses C.Q) | `N.T7` and `First.E` mint an OBJECT `Named` image instead of the alias or enum type. The relater walks a fake object: NotRelated (extra) or Unknown (miss). Same-named enums also need `isEnumTypeRelatedTo`. | `getTypeFromTypeAliasReference`; relater.go:236, :282 `isEnumTypeRelatedTo` | declared.rs:5226/5384 `qualified_type_reference`; relater.rs:1798, :2557 | 3 extra + 21 miss | 1+4 | declared.rs |
| X11 | **Single-element generic tuple / variadic** (A2.VT, B.B15; misses C.VT) | The `[...T]` → `T` arm and the generic variadic element relation are missing (`tuples_related_to` returns Unknown). | relater.go:3410, :4178 | relater.rs:2885; :3528 `tuples_related_to` (:3566-3571) | 6 extra + 11 miss | 2+3 | relater.rs (tuples) |
| X12 | **Contextual type lost** (A1.B8, A1.B13, A1.B14, A1.B15, A2.CRET, A2.K, B.B25) | Several distinct gaps: in an intersection context, `any` does not become `unknown`; overload candidates after the first lose per-candidate return context (Object.freeze, Promise.all); SkipGenericFunctions applies only on retry; the §927 union property guard; the return mapper for `R \| PromiseLike<R>`; `new X()` args with a type-literal construct signature; context after a failed argument. | checker.go:30555/30671, :9025, :7617, :30551, :29621, :30817, :29762 | contextual.rs:2245/2351, :2621/2706, :1541, :1376; calls.rs:3968; inference.rs:752 | 15 | 11 | contextual.rs / calls.rs / inference.rs (all main-active) |
| X13 | **Empty body returns `void`** (A1.B4, B.B18) | A non-async body with no return always infers `void`, ignoring a contextual return type that contains `undefined`. | checker.go:20177 `getReturnTypeFromBody` | signatures.rs:2649 `return_type_from_body` (:3607/:3632/:3641) | 6 | 2 | signatures.rs |
| X14 | **`@ts-ignore` in a JSX `{/* */}`** (A2.CD, B.B22) | The comment directive inside a JSX expression container is never collected. | scanner/scanner.go:674/972; compiler/program.go:1386 | tsr-compiler/src/comment_directives.rs:57 `directives_in` | 6 | 2 | tsr-compiler comment_directives.rs |
| X15 | **Reporter/argument normalization** (B.B5, B.B6, B.B7; misses D.D16, D.D18, D.D17) | `NoInfer<T>` is not normalized, and `satisfies` is not skipped (`getEffectiveCheckNode`), before argument elaboration. TS2820 ("Did you mean") and TS2719 are unported. EPC runs before `elaborateError`. | checker.go:27865 `getNormalizedType`, :9381 `getEffectiveCheckNode`; relater.go:4785/4790, :430 | assignreport.rs:1636 `report_argument_failure`, :1909 `relation_diagnostic`; calls.rs:1023 | 10 extra + 6 miss | 6+3 | assignreport.rs |
| X16 | **`this`-type rebinding of inherited members** (A2.TH) | An inherited `this: this` / `(): this` member is not instantiated with the derived receiver. | checker.go:19573 `getTypeWithThisArgument` | members.rs:1333 via relater.rs:3922 | 6 | 1 | members.rs (main-active) |
| X17 | **Flow reference/narrowing gaps** (A2.AL, A1.B18, A1.B20, B.B10, A2.EOPT) | Several gaps: aliased-condition narrowing for element/property access references; a comma not stripped on the reference side; `isTypeDerivedFrom`'s Array→ReadonlyArray arm (instanceof); a predicate `Extract<T,Function>` not narrowing T; a definite write target still flow-narrowed under exactOptionalPropertyTypes. | flow.go:386, :1814, :1645; relater.go:4989; flow.go:859; checker.go:11397 | flow.rs:5535, :1620/1685, :4532, :1362, :7506; members.rs:964 | 10 | 5 | flow.rs (main-active) |
| X18 | **Union-receiver write type** (A1.B10) | A property write through a union receiver (divergent accessors) never computes the write type. | checker.go:21452 `createUnionOrIntersectionProperty` | members.rs:2269 `composite_property_of_type` (:2275) | 5 | 1 | members.rs (main-active) |
| X19 | **Simple-type relation arms** (A2.U, A2.P0, A2.IX, A2.PN) | `isUnknownLikeUnionType` is missing. `object_against_primitive` turns a relater Unknown into a report while ignoring the primitive's apparent type. The intersection arm of `isObjectTypeWithInferableIndex` is missing. The nominal private-member shortcut is one-sided. | relater.go:275, :3814, :4624, :4270 | relater.rs:2522, :2450; assignreport.rs:2214 `object_against_primitive`; unions.rs:1959 | 9 | 7 | relater.rs / assignreport.rs |
| X20 | **Harness drops `// @noCheck`** (A1.B2) | `apply_test_directives` never applies `noCheck`, so `SkipTypeChecking` does not skip the file. Not a checker defect. | testrunner harnessutil.go:309 → `SkipTypeChecking` | tsr-conformance/src/trace_case.rs:400 `apply_test_directives` | 3 | 3 | tsr-conformance |
| X21 | One-offs (A1.B7, B9, B17, B19, B21; A2.W, CTUP, MP, RA, SM, EXP, EJSX; B.B16, B17a/b, B19-B21, B23, B24) | Each is one or two lines with its own native function; see the slice files. | — | — | ≈25 | ≈22 | various |

### 3b. Missing lines (native reports, TSR is silent)

| # | bucket (slice ids) | root cause | native anchor | TSR function | lines | cases | owner |
|---|---|---|---|---|---|---|---|
| M1 | **Call resolution declines without a report** (E.OVLSURV, SPREAD, CSARG, NEW, TAGGEN, TAGORDER, OVLTA, ARGSHAPE, BPREST, TATV, OVLCS; D.D3a/b/c) | The applicability walk is skipped in several shapes, so no failure is published and no TS2345 is emitted: a sole generic overload survivor; a spread argument (`Applicable(None)`); a context-sensitive function, array literal or object literal in a generic candidate; generic `new` (`[single]` arm); a generic tag; written type arguments (§391); a `c ? a : b` argument; a binding-pattern rest. | checker.go:9025 `chooseOverload`, :9256 `isSignatureApplicable`, :9649 `reportCallResolutionErrors`, :8575, :8719, :30042 | calls.rs:4030/4169 `choose_ordered_overload`, :856, :1667/1698/1728 `check_instantiated_candidate_arguments`, :1495, :2238, :2135; expressions.rs:2361 | 122 + 32 (D3) | 47 + 22 | calls.rs (main-active) |
| M2 | **JSX attribute checks** (D.J1-J6, A2.EJSX) | Each of these declines: namespaced tags, hyphenated attributes, `LibraryManagedAttributes`/`Defaultize` targets, generic props, fragments/children, intrinsic `DetailedHTMLProps` excess. | jsx.go:130, :110, :295 `elaborateJsxComponents`, :1020; relater.go:719 | jsx_component.rs:62, :258/285 `check_jsx_attributes_assignable`, :444 | 52 | 25 | jsx_component.rs |
| M3 | **Whole-file `file_has_parse_errors` gate** (D.D1, E.PARSE) | Every assignment, variable, return, call, new and tagged-template report returns early in a file with any parse error. Native has no such gate. | checker.go:2196 `checkSourceFile`, :8843, :12757, :5790 | assignreport.rs:99/198/271/317/708/749/822/888/1039; calls.rs:736/2135/2179; call_arity.rs:36 | 44 | 14 | assignreport.rs + calls.rs |
| M4 | **Relater `Unknown` on decidable pairs** (C.IA, TERM, PI, IAW, US, TPC, SM, SIG, NS; E.RELUNK) | `S[K]`→`T[J]` and primitive→`keyof U` fall off the worker as Unknown where native returns false at relater.go:3900. The site there is labelled `CompositeShape`, and in this population that label is wrong. Also: primitive → index-signature-only target, `unique symbol` missing from `FLAG_DECIDABLE`, circular `T extends T`, `Capitalize<string>` → template, signatures, module namespace objects. | relater.go:3443-3539, :3900, :4603, :206, :3653, :3572, :4441 | relater.rs:3519-3522, :1567, :1259-1279, :1496, :211, :3178, :1343, :1907 | 55 | 30 | relater.rs |
| M5 | **Generic call inference answers `error`** (E.INFERR, E.INFDIFF) | When instantiation fails, the diagnostics path bails. Native falls back to the constraint or `unknown` and reports. | checker.go:9390 `inferTypeArguments`; inference.go:1317 `getInferredType` | inference.rs:267 `check_generic_call_with`; calls.rs:1495 | 21 | 13 | inference.rs (main-active) |
| M6 | **Non-array rest parameter** (E.REST, B.B16) | Native relates the gathered spread-argument tuple to the rest type as a whole. TSR relates per position or declines. | checker.go:9308, :29500 `getSpreadArgumentType`; relater.go:1858 | calls.rs:990 `check_single_candidate_arguments`; signature_positions.rs:133; calls.rs:1555 | 18 | 7 | calls.rs (main-active) |
| M7 | **Destructuring-assignment relation unported** (D.D2) | `checkReferenceAssignment`, the shorthand `{x = d}` default and `for await (ref of …)` are missing; only the reference-shape checks exist. | checker.go:12704, :12597, :12663, :4032 | reference_target.rs:83; assignreport.rs:111/274 | 16 | 10 | reference_target.rs / assignreport.rs |
| M8 | **Generators: return/yield/annotation** (D.D9-D11, E.GENIIFE) | The return check declines in generators, the yield check declines in async generators and on every `yield*`, and `checkGeneratorInstantiationAssignabilityToReturnType` has no reporter. | checker.go:4086, :10952, :29697 | assignreport.rs:982, :821; iteration.rs:476 | 18 | 9 | assignreport.rs / iteration.rs |
| M9 | **`keyof`/`T[K]` written over a type parameter minted as a placeholder** (E.KEYOFMINT, E.KEYOFINTR, E.IDXGEN) | `unresolved_types` marks the placeholder as an error, which makes the pair not reportable. `keyof any` and `keyof null` answer error. A generic element access answers error instead of a deferred `T[K]`. | checker.go:22960, :26684 `getIndexTypeEx`, :26927 | declared.rs:875 (:975, :1392); indexed.rs:227; assignreport.rs:2149 | 14 | 8 | declared.rs / indexed.rs |
| M10 | **JS / JSDoc gates** (E.JS, D.D13a, D.D13b) | Every call in a JS file is `Undecided`, return and arrow-body checks are gated by `in_js_file`, and JSDoc `@type`/`@satisfies` declarations report nothing. | checker.go:8843, :4086, :10206, :5790, :10741 | calls.rs:806; assignreport.rs:888/1039/310 | 17 | 15 | calls.rs / assignreport.rs |
| M11 | **Parser: `new <T> expr`** (E.PARSERNEW) | TSR parses it as `new (<T>expr)`. Native has no `<` arm in `parsePrimaryExpression`, so it reads a comparison and reports TS1109. | parser.go:5746 | tsr-parser/src/expression.rs:1103 | 8 | 1 | tsr-parser |
| M12 | Smaller miss buckets (D.D8, D12, D14, D15; E.NRANY, NARROW, OROP, IMPTYPEOF, IMPORTANY, BP, SUPERCTX, ANYDECL) | See the slice files. | — | — | ≈38 | ≈25 | various |
| — | Unclassified | 27 lines (A1: 1, C: 20, D: 6), each with a hypothesis in its slice file. | | | 27 | | |

### 3c. Cross-cutting observations

1. **The `CompositeShape` Unknown label is misattributed** (slice C). The
   end-of-worker Unknown at relater.rs:3522 is reached by `S[K]`→`T[J]` and
   primitive→`keyof U` pairs, where native's worker returns false
   (relater.go:3900). Any histogram keyed on that label (`undecidable_split`)
   over-counts "composite" shapes.
2. **The NEVER gate under-reports attempts for elaborated lines** (slice D).
   The probe matches only the exact native position. When native elaborates
   to a member and TSR reported at the outer node, the miss reads NEVER and
   the outer report shows up as an EXTRA. X5 and X15 are those pairs; they
   are one defect, not two.
3. **One fix, both sides.** X3, X5, X6, X10, X11 and X15 each convert extras
   *and* misses, because a missing arm answers NotRelated in one shape and
   Unknown in another.
4. The two gates confirmed directly in this lane's session, beyond the
   slices: `file_has_parse_errors` is read at the nine `assignreport.rs`
   sites and the `call_arity.rs` site listed in M3, and
   `crates/tsr-conformance/src/` has no `noCheck` handling at all, while the
   pinned `noCheckDoesNotReportError` sets `// @noCheck: true` and has no
   native `.errors.txt`.

## 4. Proposed issues (top 10, for the integrator)

Ranked by false positives first, then by lines converted per fix:

1. `X1` — port reportUnmatchedProperty code choice without certified tables (TS2741/2739/2740 vs TS2322): 37 lines / ≤24 cases — assignreport.rs
2. `X2` — port hasExcessProperties reporting for intersection/mapped/alias/index-signature targets (TS2353 vs TS2322 head): 27 / ≤18 — assignreport.rs
3. `X5` — elaborateArrayLiteral/ObjectLiteral on union targets, spreads, numeric names: 55 / ≤29 — assignreport.rs
4. `X3` — conditional source/target/conditional↔conditional arms + getConditionalFlowTypeOfType: 45 / ≤23 — relater.rs (+declared.rs)
5. `X6` — generic mapped arms (mappedTypeRelatedTo, alias-reached generic mapped, apparent type): 42 / ≤19 — relater.rs
6. `X4` — port typeRelatedToDiscriminatedType: 16 / 5 — relater.rs
7. `X10` — qualified alias/enum type references minted as empty objects (+ isEnumTypeRelatedTo): 24 / 5 — declared.rs
8. `M1` — chooseOverload applicability for generic survivors, spreads, context-sensitive args, generic new/tag, written type args: 154 / ≤69 — calls.rs (main-active)
9. `M3` — remove the whole-file `file_has_parse_errors` report gate (native has none): 44 / ≤14 — assignreport.rs + calls.rs
10. `M2` — JSX attribute check declines (namespaced, hyphenated, LibraryManagedAttributes, generic props, fragments): 52 / ≤25 — jsx_component.rs

Next in line: X8 flow destructuring narrowing (19, flow.rs), M4 relater
decidable-pair Unknowns (55, relater.rs), X7 variance fallback (10), X20
harness `@noCheck` (3, tsr-conformance, trivial).
