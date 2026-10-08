# Slice B: TSR TS2322/TS2345 false positives (115 rows, all EXTRA)

Pinned native: vendor/typescript-go @ 5b1047d. Every row was bucketed by root cause. At least two cases per large bucket were checked in depth: I read the code and ran a minimal repro through the real diagnostics pipeline. Rows I assigned by pattern are marked "(pattern)".

**How the repros were run.** `diagcase` and `r5census` only find cases under the corpus root, and that path is compiled into the binary. To run new files without editing the repo, I overlaid a scratch directory on `tests/cases/compiler` in a private mount namespace (`unshare -m` plus overlayfs). Nothing under /home/user/tsr was modified.
- `/tmp/box/an/repro/B/runm.sh <name>` runs `/tmp/box/an/repro/B/upper/<name>.ts(x)` through `r5census` and prints TSR's code, position and message.
- `/tmp/box/an/repro/B/run.sh <name>` does the same through `diagcase`.
- All repro files are `/tmp/box/an/repro/B/upper/zzB_*.ts(x)`.
- TSR line numbers in the outputs below omit the leading `// @` directive lines, the same convention the baselines use.

## 1. Bucket table

| id | root cause (one line) | native anchor | TSR function | #lines | #cases | owning file |
|---|---|---|---|---|---|---|
| B1 EPC-REPORT | The relater's excess-property arm fails the relation but never reports. The syntactic TS2353/TS2561 pre-pass declines (intersection, mapped or alias target; index-signature target; spread; shorthand; computed name; nested literal; `\|\|` initializer; module-namespace target), so the reporter falls back to a TS2322 head at the declaration. | relater.go:2714 hasExcessProperties (reportErrors) | assignreport.rs:1320 check_excess_properties; the silent arm is relater.rs:1189 (is_related_to_with_flags → unions.rs:1874 fresh_literal_has_excess_property); fallback at assignreport.rs:1756 report_relation_failure | 26 | 17 | assignreport.rs |
| B2 MISSPROP-CODE | Native suppresses the head and reports TS2741/2739/2740 (`overrideNextErrorInfo` → reportErrorResults/reportRelationError). TSR rebuilds the missing list from certified member tables and gives up on: alias/mapped types (Record, Gen2, Objectish), generic instantiations (`A<boolean>`, `G<boolean>`), intersection source or target (every JSX `IntrinsicAttributes & P`), qualified namespace-import types (`types.A`), and the lib `Function` / merged `TemplateStringsArray`. | relater.go:4345 reportUnmatchedProperty; relater.go:4705 reportErrorResults; relater.go:4751 reportRelationError | assignreport.rs:1510 missing_required_property; relater.rs:797 unmatched_property_report | 30 | 18 | assignreport.rs (+ relater.rs:797) |
| B3 EL-ARRAY | elaborateArrayLiteral declines when the target is a union (e.g. an optional parameter's `X \| undefined`, `Book \| Book[]`, `Style`) or when there is a spread element. Native elaborates both, so its error sits on the element (TS2322/TS2353) while TSR's is TS2322/TS2345 on the whole array. | relater.go:522 elaborateArrayLiteral | assignreport.rs:3344 elaborate_array_literal | 14 | 10 | assignreport.rs |
| B4 EL-OBJECT | elaborateObjectLiteral returns None when the literal has a spread, and skips computed names. Native elaborates the remaining members and uses TS2418 for computed names. | relater.go:498 elaborateObjectLiteral (TS2418 at :515) | assignreport.rs:2481 elaborate_object_literal_members | 3 | 3 | assignreport.rs |
| B5 NOINFER-ARG | A `NoInfer<T>` parameter type is not normalized before argument elaboration or EPC. The substitution type defeats the union-literal and object-literal elaboration arms, so TSR reports a TS2345 head instead of TS2322 at the property or TS2353. | checker.go:27865 getNormalizedType (via elaborateError relater.go:440) | assignreport.rs:1636 report_argument_failure | 3 | 2 | assignreport.rs |
| B6 SATISFIES-ARG | Argument elaboration does not skip `satisfies` (getEffectiveCheckNode). Native elaborates into the literal and reports TS2322 at the property. | checker.go:9381 getEffectiveCheckNode, used by getSignatureApplicabilityError checker.go:~9302 | assignreport.rs:1636 report_argument_failure (assignreport.rs:1125 effective_check_node exists but is not used here) | 2 | 1 | assignreport.rs |
| B7 RELERR-VARIANTS | reportRelationError's message variants are not ported: TS2820 "Did you mean" (getSuggestedTypeForNonexistentStringLiteralType) and TS2719 "two different types with this name". | relater.go:4785 (TS2719), relater.go:4790 (TS2820) | assignreport.rs:1909 relation_diagnostic / :1756 report_relation_failure | 5 | 3 | assignreport.rs |
| B8 DISCRIM-UNION | typeRelatedToDiscriminatedType is not ported. An object such as `{type: 'a'\|'b'}` against `{type:'a'}\|{type:'b'}` gets NotRelated from the union-target SOME arm. | relater.go:3893 → relater.go:3989 typeRelatedToDiscriminatedType | relater.rs:2936 (union-target arm in structured_type_related_to_worker, relater.rs:2885) | 3 | 1 | relater.rs |
| B9 COND-FLOW-SUBST | getConditionalFlowTypeOfType is not ported, so there are no substitution types in a conditional's true branch. `Extract<T,U>`'s true branch is plain T, not T & U; `number` in `number extends T ? (cb:(n:number)=>void)…` is not `number & T`. | checker.go:24952 getConditionalFlowTypeOfType / :24989 getImpliedConstraint | declared.rs:7357 capture_conditional_alias_branches (true branch via get_type_from_type_node); type-node resolution in general | 4 | 2 | declared.rs |
| B10 PRED-NARROW-GENERIC | `value is Extract<T, Function>` does not narrow an unconstrained `T`. narrowed_type_worker answers None (undecidable), so `value` stays `T`. | flow.go:859 getNarrowedTypeWorker | flow.rs:7506 narrowed_type_worker | 2 | 1 | flow.rs |
| B11 IA-SOURCE-CONSTRAINT | The indexed-access source arm uses the deep base constraint first. For `{[K in U]: T}[U]` vs `T` with T constrained, it compares `string[]` to `T` instead of getConstraintOfType (→ T via substituteIndexedMappedType). | relater.go:3664 (TypeVariable source arm, getConstraintOfType) | relater.rs:3147 (in structured_type_related_to_worker) | 2 | 1 | relater.rs |
| B12 UNION-TARGET-PREEMPTS-TYPEVAR | The union-target SOME arm returns before the source type-variable constraint arm. Native falls through for Instantiable sources, so `E[K]` (constraint `A\|B`) relates to `A\|B`. | relater.go:3373-3386 (fallthrough guard at :3383) then :3664 | relater.rs:2936 (returns `any(parts)`) | 3 | 1 | relater.rs |
| B13 GENERIC-MAPPED-VIA-ALIAS | A generic mapped type reached through an alias reference (`Record<K,T>`, `FunctionProperties<T>` = `Pick<…>`) is not seen as generic mapped. Its `mapped_types` side-table entry is missing for that TypeId. Both the target arm (`T` → `{[P in K]:T[P]}`) and the source→index-signature arm miss it. | relater.go:3593 (generic mapped target), relater.go:4590 (indexSignaturesRelatedTo generic-mapped source) | relater.rs:1788 is_generic_mapped_target / :1587 generic_mapped_target_related_to; relater.rs:2377 related_index_signatures | 3 | 2 | relater.rs (+ mapped.rs registration) |
| B14 VARIANCE-UNRELIABLE | Unreliable/Unmeasurable variance flags are not represented. With a rest type param (`(...a: T) => void`) native marks the variance unreliable and falls back to the structural check; TSR answers from the variance shortcut. | relater.go:1493 (compareSignaturesRelated reportUnreliableMarkers), relater.go:3261-3300 (AllowsStructuralFallback) | relater.rs:3378 (generic-reference variance arm), variances.rs | 1 | 1 | relater.rs / variances.rs |
| B15 SINGLE-ELEM-GENERIC-TUPLE | The `[...U]` → `T` arm (isSingleElementGenericTupleType) is not ported. | relater.go:3410 | relater.rs:2885 structured_type_related_to_worker (absent) | 1 | 1 | relater.rs |
| B16 REST-TUPLE-ARGS | For a non-array rest parameter (`...rest: [...string[], number]`), native gathers the rest arguments with getSpreadArgumentType and reports once at the first rest argument. TSR checks each position against signature_type_at_position. | checker.go:9280-9309 getSignatureApplicabilityError | calls.rs:990 check_single_candidate_arguments | 1 | 1 | calls.rs |
| B17a EOPT-INFER | inferFromMatchingTypes matches by TypeId equality, so `missing` (exactOptional) does not match `undefined`. `e(input.bar)` infers `T = number \| undefined`. | inference.go:110 + :1214 isTypeOrBaseIdenticalTo | inference.rs:4979 infer_to_union | 1 | 1 | inference.rs |
| B17b EOPT-MAPPED | Mapped (`Partial`) optional properties carry plain `undefined`, not `missingType` (getTypeOfMappedSymbol → getOptionalType(isProperty)). The TS2412 variant is not chosen (`obj.a`), and the write type strips an explicit `undefined` (`obj.b`). | checker.go:20984 getTypeOfMappedSymbol; checker.go:29092 removeMissingType | mapped.rs:653 resolve_mapped_type_members_worker (~:800); assignreport.rs:2023 exact_optional_property_assignment_mismatch | 2 | 1 | mapped.rs |
| B18 EMPTY-BODY-RETURN | A body with no return statements returns `void` even when the contextual return type contains `undefined`. Native returns `undefined`. | checker.go:20179 (getReturnTypeFromBody, contextual undefined arm) | signatures.rs:2649 return_type_from_body (void at :3632) | 1 | 1 | signatures.rs |
| B19 NAMERES-FN-LOCALS | A function-scoped `var` declared in the body is visible from the return-type annotation (`(): typeof b`). Native hides it (TS2304 → error type). Only the type-parameter-list arm is ported. | binder/nameresolver.go:76 | tsr-binder/src/lib.rs:973 parameter_hidden_from_type_parameter_list (resolve_name lib.rs:824) | 1 | 1 | crates/tsr-binder/src/lib.rs |
| B20 MIXIN-TYPEVAR-BASE | A named class expression extending a type variable is typed `typeof C`, not `typeof C & TBase`. This is a documented limit. | checker.go:16924 getTypeOfFuncClassEnumModuleWorker → :16936 getBaseTypeVariableOfClass | symbols.rs:3661 get_type_of_func_class_enum_module_worker | 1 | 1 | symbols.rs |
| B21 INSTANTIATION-DEPTH | TS2589 (instantiation depth or count limit → error type) is not ported. `Circular<tup>` stays a real type and the return check fails. | checker.go:22118 / :24311 | (absent; instantiation in declared.rs/mapped.rs) | 1 | 1 | declared.rs/mapped.rs |
| B22 JSX-COMMENT-DIRECTIVE | `@ts-ignore`/`@ts-expect-error` inside a JSX expression container `{/*…*/}` is not collected, so the error is not suppressed. | scanner/scanner.go:972 processCommentDirective | crates/tsr-compiler/src/comment_directives.rs:57 directives_in | 1 | 1 | tsr-compiler/src/comment_directives.rs |
| B23 EXPORT-DEFAULT-FRESHNESS | The type of `export default {…}` keeps object-literal freshness. It only regularizes literals, where native applies widenTypeForVariableLikeDeclaration. Freshness then blocks TS2741 (the fresh-literal guard). | checker.go:16617 (ExportAssignment arm) | symbols.rs:4977 in get_type_of_variable_or_parameter_or_property_worker (:4833) | 1 | 1 | symbols.rs |
| B24 EMPTY-BINDING-PATTERN | `const {}: undefined = 1` under strict: native runs only checkNonNullNonVoidType (TS2532), with no assignability check. | checker.go:5861-5867 checkVariableLikeDeclaration | assignreport.rs:705 check_annotated_initializer / :310 check_variable_like_declaration | 1 | 1 | assignreport.rs |
| B25 CTX-AFTER-FAILED-ARG | When an earlier argument fails applicability, native never contextually types later context-sensitive arguments (`x` is implicit any: 7006, 2347). TSR gives `x` the candidate's parameter type and checks its body. | checker.go:8843 resolveCall / :9025 chooseOverload (context-sensitive args are excluded in the first pass; error reporting stops at the first failing argument) | calls.rs overload/contextual path (contextual.rs) | 2 | 1 | calls.rs |

Total: 115 lines.

## 2. Per-bucket cases, repros, evidence

### B1 EPC-REPORT (26)
**Cases:**
- excessPropertyCheckIntersectionWithIndexSignature 7:7
- excessPropertyCheckIntersectionWithRecursiveType 50:7
- excessPropertyCheckWithEmptyObject 9:5, 14:5
- excessPropertyChecksWithNestedIntersections 19:14, 31:14, 40:5, 68:1
- nestedFreshLiteral 11:5
- objectLiteralExcessProperties 15:5, 17:5, 19:5, 49:11
- objectLiteralFreshnessWithSpread 2:5
- typeArgumentDefaultUsesConstraintOnCircularDefault 3:5
- indexSignatures1 53:1, 281:7
- logicalOrExpressionIsContextuallyTyped 6:5
- namespaceImportTypeQuery /b.ts:5:5
- namespaceImportTypeQuery4 /b.ts:2:5
- nonPrimitiveUnionIntersection 8:7
- objectLiteralShorthandPropertiesAssignmentError 4:5
- objectLiteralShorthandPropertiesAssignmentErrorFromMissingIdentifier 4:5
- propertyAccess 11:5
- usingDeclarationsWithObjectLiterals1 15:9, 33:17

**Evidence.**
- relater.rs:1189 returns NotRelated when `fresh_literal_has_excess_property`, with no report.
- The only TS2353 emitter is assignreport.rs:1320 `check_excess_properties`. It returns silently when:
  - `relation_members_are_complete(target)` fails (intersections, mapped types, aliases, index signatures)
  - the literal has a spread, shorthand or computed name
  - the initializer is not an ObjectLiteralExpression (`a || b`)
- A known member is checked by `check_object_literal_member` → `report_assignability_failure`, which never runs the excess pre-pass on a nested literal.
- `report_relation_failure` then emits a TS2322 head.

**Repro** `zzB_epc2.ts` / `zzB_epc4.ts` (`// @strict: true`):
```ts
let i: object & { x: string } = { z: 'abc' };                 // native TS2353 @z ; TSR TS2322 @i
let d: { [k: `data${string}`]: string } = { date123: 'x' };   // native TS2353 ; TSR TS2322 @d
let g: { a: number } = { a: 1, ...sp, z: 3 };                 // spread: TSR TS2322 @g
let f: { b: string; id: number } = { name, id: 1 };           // shorthand: TSR TS2322 @f
let k: { m(): void; v: number } = { ["m"]() {}, v: 1, extra: "" }; // computed: TSR TS2322
let e: { a: string; id: number } = { b: '', id: 1 } || { a: '', id: 2 }; // TSR TS2322 @e
interface A { x: string } interface B { a: A }
let c: B = { a: { x: 'hello', y: 2 } };   // native TS2353 @y ; TSR TS2322 @a ("{x,y} not assignable to A")
```
**Controls:** the same shape against a plain interface or object-literal target (`let c: A = {x:'hello', y:2}`, `let j: {m();v} = {m(){}, v:1, extra:""}`) gives TS2353 correctly. `zzB_ns.ts` (typeof namespace-import target) reproduces namespaceImportTypeQuery: TS2322 '... typeof /a'. Its printed target also differs from native `typeof import("/a")`.

### B2 MISSPROP-CODE (30)
**Cases:**
- assignmentToObjectAndFunction 8:5
- consistentAliasVsNonAliasRecordBehavior 18:5, 22:5, 34:5, 38:5
- jsxElementType 34:2, 40:2, 46:2, 52:2, 59:2
- mappedTypeNotMistakenlyHomomorphic 33:1, 34:1
- templateStringsArrayTypeRedefinedInES6Mode 7:3
- chained2 /d.ts:4:7, 5:7
- checkJsxChildrenProperty2 16:10
- checkJsxChildrenProperty5 22:10
- generic /b.ts:5:5
- importClause_namespaceImport /b.ts:5:7, 6:7
- mappedTypeWithAny 45:5, 46:5
- objectTypeWithStringAndNumberIndexSignatureToAny 69:5
- tsxIntrinsicAttributeErrors 29:2
- tsxReactComponentWithDefaultTypeParameter3 15:11
- tsxSpreadAttributesResolution16 13:10
- tsxSpreadAttributesResolution2 22:10, 23:10
- tsxUnionElementType3 37:10
- tsxUnionElementType6 22:10

**Evidence.**
- `report_relation_failure` and `report_argument_failure` emit TS2741/2739/2740 only when `missing_required_property` or `unmatched_property_report` returns Some.
- `missing_required_property` needs `relation_property_table` on both sides. That is a certified declared table or a regular object-literal table, which an intersection, a mapped/alias image or a generic instantiation does not have.
- `unmatched_property_report` returns None for:
  - fresh literals
  - any TYPE_ALIAS-owned side (relater.rs:829-848)
  - non-object or intersection sides (relater.rs:807)

**Repro** `zzB_mp1.ts`:
```ts
var errFun: Function = {};                    // native TS2740 ; TSR TS2322
declare let r1: Record<"a", string>; declare let r2: Record<string, string>;
r1 = r2;                                      // native TS2741 ; TSR TS2322
interface G<T> { a: T }  let g: G<boolean> = {};   // native TS2741 ; TSR TS2322
interface I {a:string;b:number} interface J {c:string}
declare let ij: I & J; declare let k: {a:string;b:number;c:string;d:string};
k = ij;                                       // native TS2741 ('d') ; TSR TS2322
let h: I = {};                                // control: TSR TS2739 (correct)
```
The JSX rows all have target `IntrinsicAttributes & …`. Native's message names the constituent (e.g. `'{ title: string; }'`): typeRelatedToEachType reports against the failing constituent (pattern).

### B3 EL-ARRAY (14)
**Cases:**
- didYouMeanElaborationsForExpressionsWhichCouldBeCalled 23:14
- literalFreshnessPropagationOnNarrowing 60:5 (spread)
- nestedRecursiveArraysOrObjectsError01 7:7 (union `Style`)
- objectLiteralExcessProperties 13:5 (`Book | Book[]`)
- destructuringParameterDeclaration1ES5 29:4, 32:4, 33:4
- destructuringParameterDeclaration1ES5iterable 29:4, 32:4, 33:4
- destructuringParameterDeclaration1ES6 32:4
- iteratorSpreadInArray5 14:5 (spread)
- optionalBindingParameters1 7:5
- optionalBindingParametersInOverloads1 8:5

**Evidence.** assignreport.rs:3349-3356 returns false if `target_flags.intersects(PRIMITIVE|NEVER|UNION)` or any element is a SpreadElement. Native relater.go:522 excludes only primitive/never targets, and it force-tuples the literal (spreads included).

**Repros:**
```ts
// @strict: true
declare function b1(x: [string, number, boolean] = ["", 0, true]): void;  // param type: [...] | undefined
b1([false, 0, ""]);   // native TS2322 at `false` and `""` ; TSR TS2345 at the array
type XY = 'x' | 'y'; declare let arr: XY[];
arr = [...['y']];      // native TS2322 at the spread element ; TSR TS2322 at `arr`
```
The messages in the census confirm the TSR target is a union (`'[string, number, boolean] | undefined'`, `'Book[] | Book'`) or the literal has a spread.

### B4 EL-OBJECT (3)
**Cases:**
- uniqueSymbolAllowsIndexInObjectWithIndexSignature 10:5 (computed `[SYM]`; native TS2418 @10:13)
- objectSpreadStrictNull 28:7 (spread; native TS2322 @title)
- spreadUnion3 2:5 (spread; native TS2322 @`y: 123`)

**Evidence.** assignreport.rs:2496-2501 returns `None` on any SpreadAssignment. A computed name fails `identifier_text` and is skipped (:2531), so the TS2418 path does not exist.

**Repro:**
```ts
// @strict: true
declare const m: { title: string; year: number };
const x: { title: string; year: number } = { ...m, title: undefined };  // native TS2322 @title ; TSR @x
```

### B5 NOINFER-ARG (3)
**Cases:**
- noInferUnionExcessPropertyCheck1 23:21
- noInfer 39:13, 58:14

**Repro** `zzB_ni.ts`:
```ts
declare function f1<T>(a: T, b: NoInfer<T>): void;
f1({ x: "foo" as const }, { x: 'bar' });   // TSR TS2345 '... NoInfer<{ x: "foo"; }>' ; native TS2322 @x
declare function f2(b: { x: "foo" }): void;
f2({ x: 'bar' });                          // control: TSR TS2322 @x (correct)
declare function g1<T>(a: T, b: NoInfer<{ x: number }>): void;
g1(1, { x: 1, y: 2 });                     // TSR TS2345 ; native TS2353 @y
```
**Evidence.**
- `report_argument_failure` tests `type_of(target).flags.contains(UNION)` and calls `elaborate_error(at, source, target)` on the raw substitution type.
- `no_infer_base_type` is applied only for the missing-property branch (:1704).

### B6 SATISFIES-ARG (2)
**Cases:** typeSatisfaction_errorLocations1 5:5, 12:10.

**Repro** (`zzB_ni.ts`):
- `h({ a: 1 } satisfies unknown)` → TSR TS2345 at the argument; native TS2322 at `a`.
- `h(({ a: 1 }))` → TSR TS2322 at `a` (parentheses are handled).

**Evidence.** `report_argument_failure(at, …)` passes `at` (the satisfies node) to `elaborate_error`. Native getSignatureApplicabilityError passes `getEffectiveCheckNode(arg)`.

### B7 RELERR-VARIANTS (5)
**Cases:**
- didYouMeanStringLiteral 5:7, 7:7 (TS2820)
- errorsForCallAndAssignmentAreSimilar 11:11, 16:11 (TS2820; TSR's source also prints `string` instead of `"hdpvd"`)
- incompatibleAssignmentOfIdenticallyNamedTypes 6:9 (TS2719)

**Evidence.** `grep` finds neither message in crates/tsr-checker. Native picks them at relater.go:4785/4790.

**Repro:**
```ts
type T1 = "string" | "boolean";
const t1: T1 = "strong";   // native TS2820 ; TSR TS2322
```

### B8 DISCRIM-UNION (3)
**Cases:** assignmentCompatWithDiscriminatedUnion 118:21, 175:20, 216:19.

**Repro** `zzB_disc.ts`:
```ts
// @strict: true
type Action = { type: 'a' } | { type: 'b' };
declare function d(a: Action): void; declare const t: 'a' | 'b';
d({ type: t });                       // TSR TS2345 ; native OK
declare let o: { type: 'a' | 'b' }; let w: Action = o;   // TSR TS2322 ; native OK
```
**Evidence.**
- relater.rs:2936: the union target returns `RelationResult::any(per-constituent)`, with no discriminated fallback.
- No port of typeRelatedToDiscriminatedType exists: grep finds no match.

### B9 COND-FLOW-SUBST (4)
**Cases:**
- callOfConditionalTypeWithConcreteBranches 25:19
- conditionalTypes2 67:12, 68:12, 69:12

**Repro** `zzB_ct2.ts`:
```ts
type Foo = { foo: string }; type Bar = { bar: string };
declare function fooBar(x: { foo: string, bar: string }): void;
function f20<T>(y: Extract<T, Foo & Bar>) { fooBar(y); }   // TSR TS2345 ; native OK (true branch T is T & (Foo&Bar))
```
**Evidence.**
- declared.rs:7408-7409 builds the true branch with `get_type_from_type_node(yes)`, which has no implied-constraint substitution.
- The only reference to getConditionalFlowTypeOfType is a report guard (index_access_reports.rs:304).
- Row 3 is the non-type-variable form: `(n: number)` inside `number extends T ? …` is substituted as `number & T` natively, at the covariant position after two parameter flips (checker.go:24964).

### B10 PRED-NARROW-GENERIC (2)
**Cases:** conditionalTypes2 117:19, 118:19.

**Evidence.**
- probefile on `zzB_ct2.ts` shows `value : T` inside `if (isFunction(value))`. The native .types (line 299) shows `Extract<T, Function>`.
- narrowed_type_worker returns `None` (preserved as undecidable) when the subtype relation of `Extract<T,Function>` to `T` is Unknown.
- Hypothesis: this follows from B9, because without the substitution the conditional source relation does not decide.

### B11 IA-SOURCE-CONSTRAINT (2)
**Cases:** inferenceShouldFailOnEvolvingArrays 5:5, 13:5.

**Repro** `zzB_mia.ts`:
```ts
function f<T extends string[], U extends string>(arg: { [K in U]: T }[U]): T { return arg; }  // TSR TS2322
function h<T, U extends string>(arg: { [K in U]: T }[U]): T { return arg; }                    // control: OK
```
**Evidence.** relater.rs:3147 calls `base_constraint_of_type(source)` first (deep: `string[]`), relates it to `T` and returns. Native relater.go:3666 uses `getConstraintOfType` (→ T).

### B12 UNION-TARGET-PREEMPTS-TYPEVAR (3)
**Cases:** quickinfoTypeAtReturnPositionsInaccurate 35:24, 63:24, 80:24.

**Repro** `zzB_qi2.ts`:
```ts
declare function isP<Item extends { a: number } | { b: string }>(item: Item): void;
function g<E extends { [i: string]: { a: number } | { b: string } }, K extends keyof E>(e: E, k: K) {
  isP(e[k]);   // TSR TS2345 "E[K] ... '{ a: number; } | { b: string; }'" ; native OK
}
```
**Evidence.** relater.rs:2936 returns the union SOME result before the indexed-access/type-variable arm at :3147. Native relater.go:3383 lets Instantiable sources fall through to :3664.

### B13 GENERIC-MAPPED-VIA-ALIAS (3)
**Cases:**
- indexSignatureAndMappedType 5:5
- conditionalTypes1 105:5, 107:5

**Repros** `zzB_gms.ts` and `zzB_gm2.ts`:
```ts
function f1<T, K extends string>(x: { [key: string]: T }, y: Record<K, T>, z: { [P in K]: T }) {
  x = y;  // TSR TS2322
  x = z;  // OK
}
type FPN<T> = { [K in keyof T]: T[K] extends Function ? K : never }[keyof T];
type FunctionProperties<T> = Pick<T, FPN<T>>;
function f7<T>(x: T, y: FunctionProperties<T>, z: Pick<T, FPN<T>>) {
  y = x;  // TSR TS2322
  z = x;  // OK
}
```
**Evidence.** relater.rs:1788 `is_generic_mapped_target` reads `mapped_types.get(&id)`. Only the direct-written image is keyed there. That this lookup is the cause is inferred from the alias-vs-direct difference, not traced.

### B14 VARIANCE-UNRELIABLE (1)
**Cases:** lambdaParameterWithTupleArgsHasCorrectAssignability 17:14.

**Repro** `zzB_var.ts`:
```ts
type GF<T extends any[]> = (...a: T) => void;
class GC<T extends any[]> { from: GF<T> | undefined; }
declare let a: GC<[str: string]>; let b: GC<[string, boolean]> = a;   // TSR TS2322 ; native OK
declare let fa: GF<[str: string]>; let fb: GF<[string, boolean]> = fa; // control OK
```
**Evidence.**
- relater.rs:3396 comments: "Until Unmeasurable/Unreliable flags are represented…".
- Inferred types match native (`GenericClass<[str: string]>`, per probefile and the .types baseline).

### B15 (1) variadicTuples1 178:5
Repro `zzB_vt.ts`: `function f<T extends unknown[]>(t0: T, t1: [...T]) { t0 = t1; }` gives TSR TS2322; native relates through relater.go:3410. There is no port: grep for isSingleElementGenericTupleType finds nothing.

### B16 (1) variadicTuples2 73:22
- `ft2(0,'abc','def',true)` with `...rest: [...strs: string[], n2: number]`.
- Native reports TS2345 at 73:8 against the gathered tuple `["abc","def",true]`.
- TSR reports at `true` against the position type `string | number` (calls.rs:1009 `signature_type_at_position`).

### B17a (1) strictOptionalProperties1 177:8
probefile on `/tmp/box/an/repro/B/p/eo.ts` (`@exactOptionalPropertyTypes`):
- `e(input.bar)` → `number | undefined` in TSR; native `number` (.types:657).
- `e(u)` with a plain `number|undefined` → `number` in TSR.

Cause: inference.rs:5015 matches by TypeId and regular literal only.

### B17b (2) strictOptionalProperties1 53:5, 54:5
- `obj: Partial<{a: string, b: string|undefined}>`.
- `obj.a = undefined`: native TS2412; TSR plain TS2322. The check at assignreport.rs:2044 requires `intrinsics.missing` in the property type, but the mapped property holds plain `undefined`.
- `obj.b = undefined`: native OK (explicit undefined is kept); TSR's target is `string`.

### B18 (1) functionsMissingReturnStatementsAndExpressionsStrictNullChecks 41:3
Repro `zzB_ret.ts`: `declare function f(x: () => undefined): void; f(() => { });` gives TSR TS2345 "'() => void'"; native is OK. signatures.rs:3632 returns `void` without consulting the contextual return.

### B19 (1) functionVariableInReturnTypeAnnotation 3:5
`function bar(): typeof b { var b = 1; return undefined; }`. Native reports TS2304 at `b`, the return type is an error type, and there is no TS2322. TSR resolves `b` (diagcase shows no 2304). The binder ports only the type-parameter-list arm of useResult (lib.rs:963 doc).

### B20 (1) baseConstraintOfDecorator 12:5
- `return class decoratorFunc extends superClass {…}` with `TFunction extends new (...args: string[]) => MyClass`.
- Native type is `typeof decoratorFunc & TFunction`, which is assignable.
- symbols.rs:3638-3643 documents the limit for named classes.

### B21 (1) recursiveMappedTypes 21:3
Native TS2589 at `Circular<tup>` makes the parameter type an error type. TSR has no TS2589 (grep EXCESSIVELY_DEEP finds nothing).

### B22 (1) multiline b.tsx:30:18
Repro `zzB_cd.tsx`: `// @ts-expect-error` on a TS line suppresses correctly, but `{/*@ts-ignore*/}` before `<C foo={100} />` does not (TSR TS2322). diagcase on the case shows TSR found no b.tsx directives (no TS2578 either).

### B23 (1) exportDefaultStripsFreshness index.ts:12:6
- The census row shows source `'{ foob: string; }'`, which stays fresh: `nFoo(q)` on the comparable `export const q` is correct.
- symbols.rs:4977 applies only `get_regular_type_of_literal_type`. The fresh source then hits the fresh guards in missing_required_property and unmatched_property_report, so TS2741 is lost.

### B24 (1) destructuringAssignabilityCheck 13:7
`const { }: undefined = 1` gets native TS2532 only, from checker.go:5867 (strictNullChecks with needCheckWidenedType means checkNonNullNonVoidType, not assignability).

### B25 (2) parenthesizedContexualTyping2 28:117, 29:124
Repro `zzB_cs.ts`:
```ts
type F = (x: <T>(p: T) => T) => typeof x;
declare function one<T>(f: F, g: F, x: T): T;
one(1 as any as string, x => { x<number>(undefined); return x; }, 10);
```
TSR reports TS2345 inside the second arrow because it contextually types `x`. Native has `x` implicit any (7006/2347 at the same columns, which TSR misses).

## 3. Unclassified rows / weaker attributions

None of the 115 is unclassified. Weaker attributions:
- **B10 (2 rows).** The mechanism (narrowed_type_worker answering None) is shown by probefile. The link to B9 is a hypothesis.
- **B13 (3 rows).** The behavior was shown by repro (alias fails, direct form passes). That the `mapped_types` TypeId lookup is the culprit was inferred, not traced.
- **B21 (1 row).** Native's TS2589 is certain. Whether TSR could even reach a depth limit there was not checked.
- **B22 (1 row).** It is unverified whether the miss comes from the `{`-prefixed line not counting as a "comment line" in `filter`, or from `directives_in` never seeing the comment.
- **B1 namespaceImportTypeQuery(4).** Hypothesis: the pre-pass table counts the type-only export `A` as known, while the relater's value-property lookup does not.
