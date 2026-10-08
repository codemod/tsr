# Slice E: TS2345 misses (call resolution) and TS2322 NOTREPORTABLE misses

All 215 rows are classified (182 TS2345 NEVER, 33 TS2322 NOTREPORTABLE). Nothing is left unclassified, but 4 rows rest on a hypothesis (marked **hyp** below).

## Method (all read-only; nothing under /home/user/tsr was modified)

- `diagcase` and `traceone` were run per case. `traceone` gives TSR's type against native's for every expression. It is the evidence for "the call or callee answers `error`/`any`".
- **Ad-hoc diagnostics.** No example takes an ad-hoc file through the diagnostics suite, so I used a private mount namespace (`unshare -m` plus `mount --bind`). Each repro was laid over an existing case file, and over that case's `.errors.txt` for gate probes:
  - `/tmp/box/an/repro/E/run.sh repro.ts` prints the diagnostics TSR emits for the repro.
  - `/tmp/box/an/repro/E/run2.sh repro.ts "L,C,CODE;..."` prints the `r5census` gate for the positions you list.
  - The files on disk do not change; `git status` stays clean.
- **Proxy for "does the relater decline this argument pair?"** Write `const v: <param type> = <arg>` and read the gate: `DECLINED` means `Unknown`, and `NOTREPORTABLE` means a side is any/error/unknown.
- Repro sources are in `/tmp/box/an/repro/E/*.ts`. The row-to-bucket map is `/tmp/box/an/repro/E/map.txt`, and its expansion is `assigned.tsv`.

## 1. Table

| bucket | root cause (one line) | native anchor | TSR function | #lines | #cases | owning file |
|---|---|---|---|---:|---:|---|
| OVLSURV | Overload set where the arity filter leaves one **generic** survivor. TSR returns it without `isSignatureApplicable`, so no `overload_argument_failures` is published and the diagnostics path finds nothing. | internal/checker/checker.go:9025 chooseOverload (+ :9649 reportCallResolutionErrors) | crates/tsr-checker/src/calls.rs:4030 choose_ordered_overload (survivor arm); calls.rs:1288 report_overload_argument_failure | 21 | 1 | calls.rs |
| INFERR | Generic call where TSR's inference/instantiation answers `error` (native infers, falls back to constraint/`unknown`, then reports). Diagnostics bail when the instantiation is `None`. | internal/checker/checker.go:9390 inferTypeArguments (inference.go:1317 getInferredType) | crates/tsr-checker/src/inference.rs:267 check_generic_call_with; calls.rs:1495 check_single_generic_candidate_arguments | 20 | 12 | inference.rs |
| PARSE | `file_has_parse_errors` returns early from every call/new/tagged-template diagnostic; native still resolves calls in files with parse errors. | internal/checker/checker.go:2196 checkSourceFile → :8843 resolveCall | crates/tsr-checker/src/calls.rs:736 check_call_expression_diagnostics (also :2135, :2179) | 19 | 8 | calls.rs |
| SPREAD | A call with a spread argument goes to `Applicable(None)`. The overload walk refuses spreads, so nothing is published and no spread element is related. | internal/checker/checker.go:30042 getEffectiveCallArguments + :9256 isSignatureApplicable | crates/tsr-checker/src/calls.rs:856 check_candidates_arity; calls.rs:5045 transcribed_generic_set_walk_worker | 18 | 10 | calls.rs |
| CSARG | Argument whose type follows the instantiated contextual type (context-sensitive function, array literal, class expression) is declined in the instantiated-candidate check. | internal/checker/checker.go:9256 isSignatureApplicable (:9291 checkExpressionWithContextualType) | crates/tsr-checker/src/calls.rs:1667 check_instantiated_candidate_arguments | 17 | 10 | calls.rs |
| REST | Non-array rest parameter (variadic/union/`never`/generic tuple). Native relates `getSpreadArgumentType(args)` to the rest type as a whole; TSR relates per position or declines. | internal/checker/checker.go:9256 isSignatureApplicable rest arm (:9308), relater.go:1858 getNonArrayRestType, checker.go:29500 getSpreadArgumentType | crates/tsr-checker/src/calls.rs:990 check_single_candidate_arguments via signature_positions.rs:133 signature_type_at_position; calls.rs:1555 (generic decline) | 17 | 6 | calls.rs |
| NEW | `new` with a generic construct signature: diagnostics route to `report_overload_argument_failure`, but `check_new_expression`'s `[single]` arm never runs the walk that publishes a failure, so no report. | internal/checker/checker.go:8575 resolveNewExpression → :9025 / :9649 | crates/tsr-checker/src/expressions.rs:2361 check_new_expression; calls.rs:2135 check_new_expression_diagnostics | 16 | 7 | calls.rs |
| RELUNK | Relater answers `Unknown` for the (argument, parameter) pair, so `report_argument_failure` stays silent. Arms are listed in §2. | internal/checker/relater.go:2600 isRelatedToEx / structuredTypeRelatedTo | crates/tsr-checker/src/assignreport.rs:1636 report_argument_failure → relater.rs relate_ternary | 9 | 8 | relater.rs |
| PARSERNEW | TSR parses `new <T> expr` as `new (<T>expr)`. Native `parsePrimaryExpression` has no `<` arm, so it reads `(new <missing>) < T > expr` (boolean, TS1109). | internal/parser/parser.go:5746 parseNewExpressionOrNewDotTarget | crates/tsr-parser/src/expression.rs:1103 parse_primary_expression (`<` arm), via :786 parse_new_expression | 8 | 1 | tsr-parser/src/expression.rs |
| TAGGEN | Tagged template with a **generic** tag: `check_tagged_template_resolution` ignores `ApplicableGeneric`/`Applicable(None)`, so no argument check at all. | internal/checker/checker.go:8719 resolveTaggedTemplateExpression → :8843 resolveCall | crates/tsr-checker/src/calls.rs:2238 check_tagged_template_resolution | 8 | 2 | calls.rs |
| OVLTA | Overload set with **written type arguments**. The walk is gated `!has_type_arguments`, or §391 returns the sole generic, so no applicability check and no failure is published. | internal/checker/checker.go:9025 chooseOverload (+ :9222 checkTypeArguments) | crates/tsr-checker/src/calls.rs:4169 choose_ordered_overload (§391) / :3929 choose_construct_overload | 7 | 3 | calls.rs |
| KEYOFMINT | A written `keyof T` / `T[K]` over a type parameter is minted as a placeholder registered in `unresolved_types`. `is_error` then makes the pair not reportable (and an inner relation declines). | internal/checker/checker.go:22960 getTypeFromTypeOperatorNode → :26684 getIndexTypeEx | crates/tsr-checker/src/declared.rs:875 keyof arm (mint at :975, `T[K]` mint at :1392); assignreport.rs:2149 | 6 | 4 | declared.rs |
| ARGSHAPE | A `c ? a : b` argument is declined as "type not upstream's" (no UnionReductionSubtype). | internal/checker/checker.go:10934 checkConditionalExpression | crates/tsr-checker/src/calls.rs:1751 argument_type_is_not_upstreams | 6 | 2 | calls.rs |
| JS | Every call in a JS file is `CallArity::Undecided`. | internal/checker/checker.go:8843 resolveCall (no JS exclusion) | crates/tsr-checker/src/calls.rs:806 check_resolve_call_arity | 5 | 4 | calls.rs |
| IDXGEN | Element access with a generic index (mapped type with an `as` clause, or a for-in key over `T`) answers `error`/`any` instead of a deferred `T[K]`. | internal/checker/checker.go:26927 getIndexedAccessTypeEx | crates/tsr-checker/src/indexed.rs:227 check_element_access_type | 5 | 2 | indexed.rs |
| TAGORDER | Calls inside callback bodies of a **generic** tagged template: the diagnostics walk does not resolve the tag before visiting the bodies, so the callback parameter is typed by the stateless road. | internal/checker/checker.go:10034 checkTaggedTemplateExpression (resolve before spans) | crates/tsr-checker/src/check.rs:723 check_node TaggedTemplateExpression arm (no resolve_call_before_callback_bodies, contextual.rs:2936) | 5 | 1 | check.rs |
| NRANY | Source `any` against target `never`: `assignability_pair_is_reportable` vetoes any `any` side; native's `isSimpleTypeRelatedTo` rejects a `never` target first. | internal/checker/relater.go:215 isSimpleTypeRelatedTo | crates/tsr-checker/src/assignreport.rs:2149 assignability_pair_is_reportable | 4 | 2 | assignreport.rs |
| NARROW | Narrowing gives a different argument type (instanceof keeps/drops a deferred conditional; type-predicate narrowing of a constraint-substituted `T`). | internal/checker/flow.go:811 narrowTypeByInstanceof / :846 getNarrowedType / :316 narrowTypeByTypePredicate | crates/tsr-checker/src/flow.rs:5467 narrow_type_worker (instanceof arm ~5670); flow.rs:6366 narrow_type_by_type_predicate | 3 | 2 | flow.rs |
| KEYOFINTR | `keyof any` / `keyof null` answer `error` (native: `string \| number \| symbol` / `never`). | internal/checker/checker.go:26684 getIndexTypeEx | crates/tsr-checker/src/declared.rs:875 keyof arm (→ `error` at :990) | 3 | 2 | declared.rs |
| BPREST | Callee has a binding-pattern rest (`...[a, b]: T[]`), so the call is `Undecided`. | internal/checker/checker.go:17904 getTypeFromBindingPattern | crates/tsr-checker/src/calls.rs:879 check_candidates_arity (has_binding_pattern_rest :1814) | 3 | 3 | calls.rs |
| BP | Binding-pattern parameter type: no fallback to the pattern type (arrow typed `error`), or an object-pattern implied target that does not report. | internal/checker/checker.go:10404 assignParameterType (unknown → pattern, :10419) / :17904 getTypeFromBindingPattern | crates/tsr-checker/src/binding_patterns.rs:35 binding_pattern_implied_type; contextual.rs:189 contextually_typed_parameter_type | 2 | 2 | binding_patterns.rs |
| TATV | Written type arguments that mention type variables make `check_call_type_argument_constraints` answer `None`, so the call is declined. | internal/checker/checker.go:9222 checkTypeArguments | crates/tsr-checker/src/calls.rs:1589 check_call_type_argument_constraints (:1630) | 2 | 2 | calls.rs |
| OROP | `t \|\| u` over type parameters answers `error` (native `U \| NonNullable<T>`). | internal/checker/checker.go:12509 (BarBarToken result) | crates/tsr-checker/src/binary.rs:154 | 2 | 1 | binary.rs |
| IMPTYPEOF | `typeof import("m")` type node answers `error` (even for a plain relative module). | internal/checker/checker.go:24575 getTypeFromImportTypeNode | crates/tsr-checker/src/declared.rs:5175 get_type_from_import_type_node | 2 | 2 | declared.rs |
| IMPORTANY | A named import resolves to `any`: through `export = alias` of an `import alias = NS`, or the string-literal export name `""`. | internal/checker/checker.go:14667 getExternalModuleMember / :15568 resolveESModuleSymbol | crates/tsr-checker/src/symbols.rs:2065 get_external_module_member | 2 | 2 | symbols.rs |
| INFDIFF | Reverse-mapped inference through a homomorphic recursive mapped type (`Deep<T>`) infers a different `T`. | internal/checker/inference.go:1014 createReverseMappedType / :1066 inferReverseMappedType | crates/tsr-checker/src/inference.rs:3382 reverse_mapped_member_type | 1 | 1 | inference.rs |
| OVLCS | Tagged template over non-generic overloads with a context-sensitive argument: `check_overload_candidates_arguments` declines and tagged templates have no fallback. | internal/checker/checker.go:9025 chooseOverload (SkipContextSensitive) | crates/tsr-checker/src/calls.rs:1170 check_overload_candidates_arguments (:1190); :2238 | 1 | 1 | calls.rs |
| SUPERCTX | Arguments of `super(...)` get no contextual type, so `function(s){…}` is `error`. | internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex | crates/tsr-checker/src/contextual.rs:2798 contextual_type_for_argument | 1 | 1 | contextual.rs |
| GENIIFE | Immediately-invoked `function*` expression under `yield` with a contextual type answers `error`. | internal/checker/checker.go:10952 checkYieldExpression / :10114 checkFunctionExpressionOrObjectLiteralMethod | crates/tsr-checker/src/expressions.rs:3586 check_yield_expression (**hyp**: exact site not isolated) | 1 | 1 | expressions.rs |
| ANYDECL | Mapped type whose template is a function type mentioning the key (`(...args: K extends … ) => unknown`) answers `error`, so the callee is untyped. | internal/checker/checker.go:20894 resolveMappedTypeMembers | crates/tsr-checker/src/mapped.rs:466 resolve_mapped_type_members | 1 | 1 | mapped.rs |
| **total** | | | | **215** | | |

## 2. Per bucket: cases, repros, evidence

Line numbers in the run.sh output are one less than in the source, because the `// @strict` directive line is stripped.

### OVLSURV (21 lines, 1 case)

- **Case:** compiler/promisePermutations2. Every `s*.then(...)` row: 73:70, 81:19, 82:19, 83:19, 90:19, 91:19, 92:19, 99:19, 100:19, 101:19, 119:19, 120:19, 121:19, 131:19, 132:19, 133:19, 136:33, 151:36, 157:21, 158:21, 159:21.
- **How the case triggers it:** the local `interface Promise<T>` merges with lib `Promise`. That gives two generic `then` overloads, and three arguments leave one arity survivor.
- **Repro (`pp.ts`):**
  ```ts
  // @strict: false
  interface Promise<T> { then<U>(success?: (value: T) => U, error?: (error: any) => U, progress?: (progress: any) => void): Promise<U>; }
  declare function testFunction4P(x: number, y?: string): Promise<string>;
  declare var s4: Promise<string>;
  s4.then(testFunction4P, testFunction4P, testFunction4P); // native TS2345; TSR nothing
  interface P2<T> { then<U>(success?: (value: T) => U, error?: (error: any) => U, progress?: (progress: any) => void): P2<U>; }
  declare var q4: P2<string>;
  q4.then(testFunction4P, testFunction4P, testFunction4P); // TSR reports (single generic path)
  ```
  The same call reports on a non-merged interface (single generic) and is silent on the merged one.
- **Code:**
  - calls.rs:4030–4119: "A GENERIC survivor is not argument-checked here", followed by `return Some(survivor)`.
  - The diagnostics side is `Applicable(None)` → `report_overload_argument_failure`, and `overload_argument_failures.get(&node)` is `None` (calls.rs:1310).

### INFERR (20 lines, 12 cases)

- **Cases:**
  - compiler/extractInferenceImprovement 26:26, 28:26, and the 2322 at 28:1. The 2322 is NOTREPORTABLE because the call answered `error`.
  - compiler/indexSignatureOfTypeUnknownStillRequiresIndexSignature 9:3
  - conformance/indexSignatureTypeInference 18:27
  - compiler/inferenceShouldFailOnEvolvingArrays 7:11, 9:15, 17:19
  - conformance/templateLiteralTypes3 20:19
  - compiler/recursiveTupleTypeInference 23:5
  - conformance/restTupleElements1 59:4
  - conformance/variadicTuples2 120:5, 121:5, 126:5, 127:5
  - compiler/thislessFunctionsNotContextSensitive1 25:3
  - compiler/emptyObjectNotSubtypeOfIndexSignatureContainingObject1 41:3 and …2 42:3. Both are 2322 NOTREPORTABLE: `result` is `any` because `mapValues(foos, f => f.foo)` answered any.
  - compiler/dissallowSymbolAsWeakType 18:12, 19:14. The receiver `f = new FinalizationRegistry(() => {})` answers `error`, so `f.register` is untyped.
- **Evidence:** `traceone` shows the call itself `GOT … : any`/`error` while native has a type, for every case. The shapes below are confirmed with `probefile` on `i1.ts`, `vt.ts`, `tl4.ts`/`tl5.ts` and `fr.ts`.
- **Repro shapes:**
  ```ts
  // @strict: true
  declare function g<T>(x: { [k: string]: T }): T[];
  declare var stooges: { name: string; age: number }[];
  g(stooges);                       // TSR: call `error`; native 2345 (index signature missing)
  function logLength<T extends string, U extends string>(arg: { [K in U]: T }[U]): T { return arg; }
  logLength(42);                    // TSR `error`
  declare function foo1<V extends string>(arg: `*${V}*`): V;
  foo1('hello');                    // TSR `error`
  declare function fz<T>(x: [T, T]): T;
  fz([1]);                          // TSR `error` (tuple arity mismatch during inference)
  declare function T3<C>(c: C, t: C extends string ? true : false): C extends string ? true : false;
  T3("s", false);                   // inline (non-alias) conditional → `error`; the aliased form resolves and reports
  declare function mk<T>(cb: (h: T) => void): T[];
  mk(() => {});                     // `error` (native T = unknown) — FinalizationRegistry rows
  ```
- **Code:**
  - `check_single_generic_candidate_arguments` returns `false` when `check_generic_call_with` answers error or leaves `instantiated` `None` (calls.rs:1536–1546). Nothing else reports.
  - The sub-shapes are distinct inference declines inside inference.rs, but they share one terminal.

### PARSE (19 lines, 8 cases)

- **Cases:**
  - compiler/parseBigInt 69:15, 70:15, 70:34, 70:53, 70:72
  - compiler/objectCreationExpressionInFunctionParameter 5:24
  - compiler/superWithTypeArgument3 11:22. TS2754 is a parser diagnostic in tsgo (parser.go:5222).
  - compiler/taggedTemplatesWithIncompleteTemplateExpressions3 5:18 and …6 5:18
  - conformance/destructuringParameterDeclaration2 8:4, 23:4
  - conformance/taggedTemplatesWithTypeArguments2 13:30, 17:30, 17:59
  - conformance/thisTypeInFunctionsNegative 66:6, 73:13, 76:16, 79:16, 82:20
- **Repro:**
  ```ts
  // @strict: true
  declare function f(x: string): void;
  f(1);              // TSR TS2345 (2,3) …
  const bad = 0bn;   // … add this parse error and TSR's TS2345 disappears (p1.ts vs p2.ts)
  ```
  Native reports both diagnostics, as the baselines show.
- **Evidence:**
  - Each case file has a parser-emitted code in TSR's output: 1005, 1109, 1128, 1034, 1359, 2754 or 1125. I cross-checked them against codes referenced in `internal/parser/*.go`.
  - `sw4.ts` vs `sw5.ts`: with `super<T>()` in the file, the `super.bar<T>(null)` TS2345 vanishes; with `super()` it is reported.
- **Caveat:** a few of these rows may hit another decline once the gate is lifted. Examples are `new tag<number>\`…\`(…)` (NEW or TAGGEN) and parseBigInt's literal-union parameters (fine, single candidate).

### SPREAD (18 lines, 10 cases)

- **Cases:**
  - conformance/callWithSpread2 27:5, 28:5, 29:13, 30:13, 31:11, 32:11, 35:8
  - callWithSpread3 21:6, 25:7
  - callWithSpread5 6:4
  - iteratorSpreadInCall6 28:28, iteratorSpreadInCall7 28:28
  - iteratorSpreadInCall8 31:32, iteratorSpreadInCall9 31:32. These are `new Foo<T>(...)`, so they are also NEW.
  - typeSatisfaction_errorLocations1 34:9, 36:9
  - variadicTuples1 53:17
  - variadicTuples2 117:6
- **Repro (`s1.ts`):**
  ```ts
  // @strict: true
  declare function all(a?: number, b?: number): void;
  declare const tuple: [number, string];
  all(...tuple);            // native TS2345 at ...tuple; TSR nothing
  declare function one(a: number, ...r: number[]): void;
  one(1, ...tuple);         // same
  declare function fs(a: string, b: string): void;
  fs("a", 1);               // control: TSR reports
  ```
- **Code:**
  - calls.rs:946–985: the single-non-generic and single-generic arms require `!argument.spread`, so the call is `Applicable(None)`.
  - `report_overload_argument_failure` needs a published failure, but `transcribed_generic_set_walk_worker` returns `None` for any spread (calls.rs:5047), and the walk never runs for a non-generic single candidate.

### CSARG (17 lines, 10 cases)

- **Cases:**
  - compiler/contextualTypingOfGenericFunctionTypedArguments1 17:32
  - compiler/genericCombinators2 15:43. Type arguments leave one generic candidate.
  - compiler/fallbackToBindingPatternForTypeInference 2:7, 3:7, 4:7, 5:7, 6:7. A secondary defect is also present: TSR types `({a})` as `unknown` instead of falling back to `{ a: any }` (checker.go:10419). Native would only report after both are fixed.
  - conformance/genericCallWithFunctionTypedArguments 26:18, 35:23
  - genericClassWithFunctionTypedMemberArguments 62:30
  - partiallyAnnotatedFunctionInferenceError 12:11, 13:11, 14:11. The arrows are also typed `any` (trace).
  - partiallyAnnotatedFunctionInferenceWithTypeParameter 33:10
  - restTuplesFromContextualTypes 56:7
  - typeParameterAsTypeParameterConstraint2 18:10 (array literal)
  - typeArgumentInferenceWithClassExpression2 6:5 (class expression)
- **Repro (`c1.ts`, `tc.ts`):**
  ```ts
  // @strict: true
  function foo3<T, U>(x: T, cb: (a: T) => U, y: U) { return cb(x); }
  foo3(1, function (a) { return '' }, 1);   // native 2345; TSR nothing
  foo3(1, (a: number) => '', 1);            // annotated → TSR reports (control)
  declare function forEach<T>(c: T[], f: (x: T) => Date): void;
  declare const c2: number[];
  forEach<number>(c2, (x) => x.toFixed());  // TSR nothing
  ```
- **Evidence:** `traceone` has 0 diffs for these cases, so the type road already matches native. The block is the diagnostics rule at calls.rs:1728–1740, which returns on ArrayLiteral, ClassExpression or `is_context_sensitive_argument`.

### REST (17 lines, 6 cases)

- **Cases:**
  - compiler/topFunctionTypeNotCallable 4:1 (`...args: never`)
  - conformance/contextualTypeTupleEnd 8:1
  - genericRestParameters3 17:11, 18:1, 59:5
  - typeSatisfaction_errorLocations1 16:5, 18:5 (generic `...args: T`)
  - variadicTuples1 62:5
  - variadicTuples2 53:5, 66:1, 68:8, 71:8, 73:8, 116:16, 137:16, 138:16, 139:25
- **Repro (`rt.ts`):**
  ```ts
  // @strict: true
  declare let f1: (x: string, ...args: [string] | [number, boolean]) => void;
  f1("foo", 10); f1("foo");                         // native 2345 ×2; TSR nothing
  declare function hmm<A extends [] | [number, string]>(...args: A): void;
  hmm("what");                                      // nothing
  declare let foo: (...args: never) => void; foo(); // nothing
  declare function g2(...args: [string, number]): void;
  g2("a", "b");                                     // fixed tuple rest: TSR reports (control)
  ```
- **Code:**
  - Native builds one tuple from the trailing arguments and relates it to the rest type (checker.go:9308–9325). That is why the message reads "Argument of type '[10]' …" or "Source has 0 element(s)".
  - TSR's `check_single_candidate_arguments` relates each written argument to `signature_type_at_position`. That function indexes the rest type per position (signature_positions.rs:155–167), which is meaningless for union/variadic rests.
  - The generic path returns `false` on `signature_non_array_rest_type` (calls.rs:1572).

### NEW (16 lines, 7 cases)

- **Cases:**
  - compiler/classTypeParametersInStatics 20:52, 27:52
  - genericClassWithStaticFactory 112:52
  - conformance/overloadResolutionClassConstructors 73:25, 79:9, 80:9, 87:9, 88:15
  - typeArgumentInferenceConstructSignatures 25:35, 61:39, 71:39, 81:45, 106:33
  - exportAssignmentConstrainedGenericType 2:17
  - dissallowSymbolAsWeakType 14:24 (`new WeakRef(s)`)
  - dataViewConstructor 1:14 (generic `DataViewConstructor`)
- **Repro (`n1.ts`):**
  ```ts
  // @strict: true
  class List<T> { constructor(isHead: boolean, data: T) {} }
  function mk<T>() { return new List<T>(true, null); } // native 2345; TSR nothing
  class C2<T extends string> { constructor(n: T) {} }
  new C2(3); new C2<string>(3);                          // nothing, nothing
  class C3 { constructor(n: string) {} }
  new C3(3);                                             // TSR reports (non-generic control)
  declare function g<T extends string>(n: T): void;
  g(3); g<string>(3);                                    // TSR reports (call control)
  ```
- **Evidence:**
  - `traceone` shows the `new` types already match native, so resolution succeeds.
  - On the diagnostics side, check_new_expression_diagnostics (calls.rs:2155–2160) sends `ApplicableGeneric`/`Applicable(None)` to `report_overload_argument_failure`.
  - The failure is published only by `transcribed_generic_set_walk`, which `check_new_expression` never calls for `[single]` (expressions.rs:2361). With written type arguments, `choose_construct_overload` skips it as well (calls.rs:3929).

### RELUNK (9 lines, 8 cases)

Each pair was proxied with `const v: P = arg` through run2.sh, and every one gated `DECLINED`.

- **conditionalTypeAssignabilityWhenDeferred 95:23 (`cd2.ts`).** `"hi"` against `Unwrap<this["prop"]>` declines; the arm is a deferred conditional target.
- **typeAssignabilityErrorMessage 42:5 (`ra2.ts`).** `{someProp: Foo<string>}` against `{someProp: Bar<number>}` declines, where `Bar` is a generic union alias. The same members written inline are reported, so the arm is a nested property typed by a union alias instantiation.
- **recursiveConditionalTypes 117:9 (`ra.ts`).** `Grow2<[],T>` against `Grow1<[],T>` declines (recursive conditional types).
- **quickIntersectionCheckCorrectlyCachesErrors 9:15 (`ra.ts`).** `F<CP>` against `F<unknown>` declines (generic interface with a call signature).
- **esModuleInteropPrettyErrorRelatedInformation 3:8 (`es2.ts`).** The synthetic `import * as foo` namespace `{ default: () => void }` against `() => void` declines; the same type written as a literal is reported.
- **recursiveComplicatedClasses 13:27 (`rcc2.ts`).** A class extending `Symbol` (TS2507, unresolvable base) against the class type declines. This is likely the "unfollowable base" site.
- **paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized 27:23, 28:23 (`po2.ts`).**
  - `foo` against `{ [x in "y"]?: Lower<number>[] }` declines, while `{ y?: Lower<number>[] }` reports. The arm is a mapped-type target with the `?` modifier.
  - TSR also infers `T = 12` where native has `number`, but the relation decline is what blocks the report.
- **mixinWithBaseDependingOnSelfNoCrash1 11:48 (`mx3.ts`, hyp).** `typeof BaseItem` against `new (...args: any[]) => any` declines.
  - Native reports only because circular base resolution leaves `typeof BaseItem` without a construct signature.
  - Fixing the decline alone may not reproduce native's answer.

### PARSERNEW (8 lines, 1 case)

- **Rows:** compiler/intTypeCheck 106:5, 120:5, 134:5, 148:5, 162:5, 176:5, 190:5, 204:5.
- **Evidence:** `traceone` shows WANT `new <i1> anyVar : boolean` with `new <i1 : boolean` and `new : any`, against GOT `new <i1> anyVar : any` with `<i1> anyVar : i1`. The native baseline has TS1109 at the `<`.
- **Code:** TSR's `parse_primary_expression` has a `SyntaxKind::LessThanToken => parse_type_assertion()` arm (expression.rs:1103). Its own comment notes that upstream's `parsePrimaryExpression` "has no `<` arm".
- **Downstream effect:** once this is fixed the file has parse errors, so check assignreport.rs:317's parse-error guard for variable initializers.

### TAGGEN (8 lines, 2 cases)

- **Cases:** taggedTemplateStringsTypeArgumentInference and …ES6, rows 33:28, 39:25, 56:6, 62:36 in each.
- **Repro (`tg.ts`):**
  ```ts
  // @strict: true
  function someGenerics4<T, U>(strs: TemplateStringsArray, n: T, f: (x: U) => void) { }
  someGenerics4 `${ null }${ null }`;     // native 2345; TSR nothing
  declare function g4<T, U>(n: T, f: (x: U) => void): void;
  g4(null, null);                          // TSR reports (call control)
  function ng(strs: TemplateStringsArray, n: number, f: (x: string) => void) { }
  ng `${ 1 }${ null }`;                    // TSR reports (non-generic tag control)
  ```
- **Code:** calls.rs:2247–2250 makes `ApplicableGeneric | Applicable(None) | Undecided` a no-op ("A generic tag reports no argument error yet").

### OVLTA (7 lines, 3 cases)

- **Cases:**
  - compiler/typeArgumentConstraintResolution1 4:12, 11:12
  - conformance/overloadResolution 70:21, 71:21
  - conformance/overloadResolutionConstructors 43:15, 77:25, 78:25 (`new`)
- **Repro (`ot.ts`, `c1.ts`):**
  ```ts
  // @strict: true
  function fn4<T extends string, U extends number>(n: T, m: U): void;
  function fn4<T extends number, U extends string>(n: T, m: U): void;
  function fn4() { }
  fn4<string, number>(3, '');         // native 2345; TSR nothing
  ```
- **Code:**
  - Written type arguments skip every `transcribed_generic_set_walk` call (`!has_type_arguments` at calls.rs:3929, :4001, :4176, :4196).
  - §391 returns the sole generic as-is (calls.rs:4169). `traceone` confirms it: `new fn2<Date>('', 0)` GOT `Date`, WANT `number` (failure candidate).

### KEYOFMINT (6 lines, 4 cases)

- **Cases:**
  - conformance/conditionalTypes1 114:5, 116:5 (source `keyof T`)
  - keyofAndIndexedAccessErrors 122:5, 123:5 (target `keyof T`)
  - compiler/keyRemappingKeyofResult 69:5 (target `keyof Remapped` → `any`)
  - compiler/recursiveTypeRelations 27:55 (callback parameter `key: keyof S`)
- **Repro (`r2.ts`, `rd2.ts`):**
  ```ts
  // @strict: true
  function f5<T>(k: keyof T) { let b: keyof T = 42; }       // native 2322; TSR gate NOTREPORTABLE
  declare const keys: string[];
  type O = { [key: string]: string };
  function g<S>(k: S) { keys.reduce<O>((obj: O, key: keyof S) => obj, {}); }       // TSR nothing
  function f<S extends number>(k: S) { keys.reduce<O>((obj: O, key: S) => obj, {}); } // TSR reports
  ```
- **Code:**
  - declared.rs:968–990 mints `keyof T` as `new_named(TypeFlags::OBJECT, …)` and registers it in `unresolved_types`; `T[K]` is minted the same way at :1392.
  - `is_error` (checker.rs:4434) reads `unresolved_types`, so `assignability_pair_is_reportable` returns false.

### ARGSHAPE (6 lines, 2 cases)

- **Cases:**
  - conformance/parenthesizedContexualTyping2 26:14, 27:14, 28:14, 29:15
  - keyofAndIndexedAccessErrors 64:33, 67:24
- **Repro (`ov.ts`):**
  ```ts
  // @strict: true
  declare function gp(a: string, b: number): void; declare const cond: boolean;
  gp(cond ? 1 : 2, 3);   // native 2345; TSR nothing
  gp(1, 3);              // TSR reports
  ```
- **Code:** calls.rs:1751 `argument_type_is_not_upstreams` (`ConditionalExpression` → true), checked in all three argument loops.

### JS (5 lines, 4 cases)

- **Cases:**
  - compiler/argumentsReferenceInFunction1_Js 13:29
  - contextuallyTypedParametersOptionalInJSDoc 17:15, 28:15. The JSDoc `[b]` optionality is also lost (trace: `b : number`).
  - conformance/jsdocPostfixEqualsAddsOptionality 8:3
  - typeTagNoErasure 7:6. `@type` template erasure is also present.
- **Repro (`js2.ts`):**
  ```ts
  // @checkJs, @filename a.js
  /** @param {number} a */ function f(a) {}
  f("s");
  ```
  TSR reports nothing, while TS2322 in the same file is reported.
- **Code:** calls.rs:806 `if self.in_js_file(node) { return CallArity::Undecided; }`.

### IDXGEN (5 lines, 2 cases)

- **Cases:**
  - conformance/mappedTypeConstraints2 10:11, 16:11, 59:57, 90:9
  - keyofAndIndexedAccessErrors 105:9. Here the source `t[key]` is `any` and the target `T[K]` is also a KEYOFMINT placeholder.
- **Repro (`sp.ts`):**
  ```ts
  type Mapped2<K extends string> = { [P in K as `get${P}`]: { a: P } };
  function f2<K extends string>(obj: Mapped2<K>, key: `get${K}`) { const x: { a: K } = obj[key]; }
  // probefile: obj[key] : error (native Mapped2<K>[`get${K}`]) → NOTREPORTABLE
  ```

### TAGORDER (5 lines, 1 case)

- **Rows:** conformance/taggedTemplateContextualTyping1 13:31, 14:31, 14:94, 15:31, 16:94.
- **Repro (`tt2.ts`):**
  ```ts
  // @strict: true
  type FuncType = (x: <T>(p: T) => T) => typeof x;
  function tag1<T>(s: TemplateStringsArray, f: FuncType, x: T): T { return x; }
  tag1 `${ x => { x<number>(undefined); return x; } }${ 10 }`;  // native 2345 inside; TSR nothing
  function tag2(s: TemplateStringsArray, f: FuncType, x: number): number { return x; }
  tag2 `${ x => { x<number>(undefined); return x; } }${ 10 }`;  // TSR reports
  function call1<T>(f: FuncType, x: T): T { return x; }
  call1(x => { x<number>(undefined); return x; }, 10);          // TSR reports
  ```
- **Evidence:**
  - `traceone` has 0 diffs: the walker, which types the outer tagged template first, gets `x : <T>(p: T) => T`.
  - The diagnostics walk is different. check.rs:713/719 call `resolve_call_before_callback_bodies` for Call/New but not for TaggedTemplate (check.rs:723), so the generic tag's callback parameter is typed through the stateless road before resolution.
  - This mechanism is inferred from the control pair. The parameter's diagnostics-time type is not directly observable.

### NRANY (4 lines, 2 cases)

- **Cases:** conformance/intersectionReduction 80:1, 81:1; intersectionReductionStrict 69:1, 70:1.
- **Repro (`r1.ts`):**
  ```ts
  declare let nv: never;
  nv = 'bar' as any;
  ```
  Native TS2322 "Type 'any' is not assignable to type 'never'"; TSR gate NOTREPORTABLE.
- **Code:** assignreport.rs:2172 returns false for `side == any`. Native rejects a `never` target before the `any` arm (relater.go:215, then :262).

### NARROW (3 lines, 2 cases)

- **compiler/awaitedTypeNoLib 18:27.**
  - `traceone`: `result` WANT `Thenable<NotPromise<TResult>> | NotPromise<TResult>`, GOT `Thenable<NotPromise<TResult>>`.
  - TSR's instanceof narrowing drops the deferred-conditional member that native keeps, and TSR then relates the narrowed type.
- **conformance/controlFlowGenericTypes 49:15, 55:15 (`cf.ts`).**
  - For `T extends Box<T> | undefined` under `!isBox(x)` / `isUndefined(x)`, TSR types `x` as `Box<T> | undefined` and `unbox(x)` answers `error`.
  - Native narrows to `undefined` and reports.

### KEYOFINTR (3 lines, 2 cases)

- **Cases:** compiler/bigintIndex 15:1 (`keyof any`); conformance/unknownControlFlow 290:11, 291:5 (`keyof T` with `T = null`).
- **Evidence (`ka.ts`, `kn.ts`):**
  - probefile prints `keyof any : error` and `keyof null : error`. Native gives `string | number | symbol` and `never`.
  - `ff1(null, 'foo')` reports nothing, while `ff3<T>(t: T & {})` with `ff3(null)` is reported.

### BPREST (3 lines, 3 cases)

- **Cases:** conformance/iterableArrayPattern16 2:5, 26 2:21, 29 2:21. All declare `(...[a, b]: T[])`.
- **Code:** calls.rs:879 returns `Undecided` before argument checks. 16 and 29 also spread.

### BP (2 lines, 2 cases)

- **compiler/crashInEmitTokenWithComment 5:4 (`c2.ts`).**
  - `fn(({[foo.bar]: c}) => undefined)`, where the parameter is `string` and no contextual signature applies.
  - probefile: the arrow is `error` (native `({ [foo.bar]: c }: {}) => any`), so the pair is not reportable.
  - `fn((x) => undefined)` is reported.
- **conformance/argumentExpressionContextualTyping 18:5 (`bp3.ts`, hyp on site).**
  - `function f1({ a }) {}` with `f1(1)` reports nothing.
  - `function f2([a, b]) {}` with `f2([1])` reports, and the annotated `({ a }: { a: any })` reports.
  - Conclusion: an object-binding-pattern implied parameter type does not report. The exact decline site (relater vs reporter) is not isolated.

### TATV (2 lines, 2 cases)

- **Cases:** conformance/genericCallWithObjectTypeArgsAndConstraints4 30:24, …5 22:24.
- **Repro (`c1.ts`):**
  ```ts
  function other<T, U extends T>(c: C, d: D) { foo<T, U>(c, d); }
  ```
  Native 2345; TSR nothing.
- **Code:** calls.rs:1630–1636: `head_could_contain_type_variables` → `None` → the call is declined.

### OROP (2 lines, 1 case)

- **Rows:** conformance/logicalOrOperatorWithTypeParameters 5:9, 14:9.
- **Evidence:** probefile `ct1.ts`: `t || u : error`. Native gives `U | NonNullable<T>`, which is not assignable to `{}`.

### IMPTYPEOF (2 lines, 2 cases)

- **Cases:** compiler/declarationEmitWithInvalidPackageJsonTypings 8:58; compiler/exportAssignmentExpressionIsExpressionNode 7:7. In the second, `typeof import("./lib/index.js")` sits inside the `.d.cts`.
- **Repro (`it.ts`):**
  ```ts
  // a.ts: export const v = 1;   b.ts:
  declare const q: typeof import("./a");   // TSR q : error; `import * as A` resolves fine
  ```

### IMPORTANY (2 lines, 2 cases)

- **compiler/aliasDoesNotDuplicateSignatures 4:5.** `import { f } from 'demoModule'`, where the module's `export =` targets `import alias = demoNS`. `traceone`: `f` GOT `any`.
- **conformance/arbitraryModuleNamespaceIdentifiers_exportEmpty 6:7.** `import { "" as foo }` resolves to `any`.

### INFDIFF (1 line)

- **Row:** compiler/mappedTypeRecursiveInference 19:18.
- **Evidence:** `traceone` for `foo(xhr)` with `Deep<T>`: TSR's inferred object differs from native's reverse-mapped type (member order and nested `readyState: {toString: any…}`).

### OVLCS (1 line)

- **Row:** conformance/taggedTemplateContextualTyping2 16:67.
- **Mechanism:** non-generic overloaded tag plus a context-sensitive arrow. calls.rs:1190 returns `false`, and calls.rs:2246 has no `report_overload_argument_failure` fallback for tagged templates.
- **Repro:** `ov.ts` line 5.

### SUPERCTX (1 line)

- **Row:** compiler/targetTypeBaseCalls 17:61.
- **Repro (`sp.ts`):** `new Foo(function(s){ s = 5 })` reports, while `super(function(s){ s = 5 })` types the function as `error`.

### GENIIFE (1 line)

- **Row:** conformance/generatorTypeCheck31 2:11.
- **Evidence:** probefile `gt.ts`: `function* () { yield x => x.length; } ()` answers `error`, and the yield operand is `any`. The non-invoked form types fine.

### ANYDECL (1 line)

- **Row:** compiler/signatureCombiningRestParameters1 17:6.
- **Evidence:** probefile `sc.ts`: `map : error` for `{ [K in T1 | keyof T2]: (...args: K extends keyof T2 ? T2[K] : []) => unknown }`, and `m3` with `(...args: [K]) => unknown` is also `error`.
- **Consequence:** the callee `fn` is untyped. The call is also a spread (SPREAD would block next).

## 3. Unclassified / hypotheses

- **No row is unclassified.**
- **Hypothesis-level attributions (bucket kept, site not isolated):**
  - mixinWithBaseDependingOnSelfNoCrash1 (RELUNK): native's answer comes from circular base resolution.
  - argumentExpressionContextualTyping (BP): object-pattern implied target, decline site unknown.
  - generatorTypeCheck31 (GENIIFE): the exact site inside function-expression/yield typing is not isolated.
  - taggedTemplateContextualTyping1 (TAGORDER): the mechanism is inferred from controls; the diagnostics-time parameter type is not observable.
- **Follow-on blockers to expect once the first blocker is fixed:**
  - fallbackToBindingPattern: CSARG, then the BP fallback.
  - paramsOnlyHaveLiteral…: RELUNK, then inference `T = 12`.
  - iteratorSpreadInCall8/9: SPREAD, then NEW.
  - signatureCombining: ANYDECL, then SPREAD.
  - JS rows: the JS gate, then JSDoc optionality/template.
  - PARSE rows in taggedTemplatesWithTypeArguments2: PARSE, then NEW/TAGGEN.
