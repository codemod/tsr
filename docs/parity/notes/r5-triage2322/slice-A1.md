# Slice A1: TSR-only TS2322 false positives (71 rows, 33 cases)

Method. I read every row's TSR message against the pinned native baseline (`.types` / `.errors.txt`), cut a minimal repro, and ran it
through `target/release/examples/probefile`. Repros are in `/tmp/box/an/repro/A1/`. probefile prints types, not diagnostics,
so I checked the relation side with **type-level probes**. `type R = S extends T ? 1 : 0` makes TSR's own conditional
evaluator call the relater. Then I read the TSR and native code paths. All TSR line numbers are at HEAD `f90fcef`. All native
line numbers are at the pinned `5b1047d`.
Unless a bucket says otherwise, every case in it was checked in depth (each one is a repro or a source read). Rows marked
"by pattern" were classified from the message shape only.

## 1. Summary table

| bucket | root cause (one line) | native anchor | TSR function | #lines | #cases | owning file |
|---|---|---|---|---|---|---|
| B1 | The assignment target `x` in `x = x + 1` reads its declared literal type. Native reads the literal's base type (`isInCompoundLikeAssignment`). The identifier arm of the assignment check never applies this. | checker.go:11110 checkIdentifier (`isInCompoundLikeAssignment` → `getBaseTypeOfLiteralType`) | assignreport.rs:1166 `assignment_target_type` (identifier arm returns `get_type_of_symbol` at :1259) | 11 | 1 | crates/tsr-checker/src/assignreport.rs |
| B2 | The test harness ignores the `// @noCheck: true` directive, so checking runs and reports. | testutil/harnessutil/harnessutil.go:309 (`noCheck` harness option) → compiler/program SkipTypeChecking | tsr-conformance/src/trace_case.rs:400 `apply_test_directives` (no `no_check` field; `..base`) | 3 | 3 | crates/tsr-conformance/src/trace_case.rs |
| B3 | `getFlowTypeOfDestructuring` is not ported. A destructured binding gets the declared slice of its parent, not the slice narrowed through a synthetic `aFoo.bar` reference. | checker.go:17849 getFlowTypeOfDestructuring (called :17743, :17771) | destructure.rs:86 `get_type_for_binding_element` (comment at :150–158 notes "unported") | 4 | 1 | crates/tsr-checker/src/destructure.rs |
| B4 | A non-async body with no `return` always infers `void`. Native returns `undefined` when the contextual return type contains `undefined`. The async arm has this logic and the plain arm does not. | checker.go:20177–20183 getReturnTypeFromBody (empty aggregate) | signatures.rs:2649 `return_type_from_body` (unconditional `void` at :3607, :3632, :3641; compare async :3436) | 5 | 1 | crates/tsr-checker/src/signatures.rs |
| B5 | Variance-based relation of same-target references has two gaps. (a) Aliases whose body is a mapped type or a type reference (`Record`, `Partial`, `Pick`, user aliases) are never measured, so they default to Covariant. (b) A failed variance check returns NotRelated with no structural fallback, where native falls back for empty, Unmeasurable or Unreliable variances. | relater.go:3392 alias-variance arm + relater.go:3266 relateVariances (`VarianceFlagsAllowsStructuralFallback`, `len(variances)==0`) | relater.rs:3371–3430 (variance arm; "Until Unmeasurable/Unreliable flags are represented… default covariance") + variances.rs:26 `inference_variances` (:46–56 returns None for non-literal alias bodies) | 7 | 3 | crates/tsr-checker/src/relater.rs (+ variances.rs) |
| B6 | A conditional-type source is never related through its constraint. The relater lacks the conditional-source arm (default constraint, then distributive constraint). The union-target arm also returns before the instantiable-source fallthrough. Separately, the true-branch implied constraint (substitution type `any[] & T`) is not built. | relater.go:3721–3770 structuredTypeRelatedToWorker conditional-source arm; relater.go:3370–3386 (union/intersection arm falls through for instantiable sources); checker.go:24952 getConditionalFlowTypeOfType | relater.rs:2885 `structured_type_related_to_worker` (union-target arm :2936 returns immediately; no CONDITIONAL arm); constraints.rs:51 `default_constraint_of_conditional_type` exists but the relater never calls it | 7 | 2 | crates/tsr-checker/src/relater.rs |
| B7 | A non-distributive conditional whose check type is a non-generic union containing `infer` stays deferred: `Ch extends {type: infer R} ? R : never`. As a constraint it then gives no literal hint, so `T` widens `'text'` to `string`. | checker.go:24300 getConditionalType (inferTypes from union check type) | declared.rs:8245 `evaluate_conditional_inference` (declines `UNION` check at :8252) | 1 | 1 | crates/tsr-checker/src/declared.rs |
| B8 | Contextual element/property type of an **intersection** context reads the raw property (`any & 1` → `any`, index `any`). Native's intersection arm swaps `any` for `unknown`, so the literal survives and `[1]` stays `[1]` / `1[]`. | checker.go:30555 getTypeOfPropertyOfContextualTypeEx (intersection arm) + :30671 appendContextualPropertyTypeConstituent | contextual.rs:2245 `contextual_type_for_element_expression` (plain `get_type_of_property_of_type` / `get_applicable_index_info` at :2351–2361) | 2 | 1 | crates/tsr-checker/src/contextual.rs |
| B9 | An array literal under `number[] & [number, ...number[]]` is not tuple-shaped. `number[] & [number]` and a bare `[number, ...number[]]` both work, so the failure is specific to an intersection containing a variadic tuple. | checker.go:8029 checkArrayLiteral `inTupleContext` (someType isTupleLikeType) + :29972 getContextualTypeForElementExpression | array_literals.rs:631 `array_literal_has_a_tuple_contextual_type` / :679 `check_array_literal` (exact failing step not isolated) | 1 | 1 | crates/tsr-checker/src/array_literals.rs |
| B10 | The write type of a **union** receiver's property (divergent accessors) is never computed. `composite_property_of_type` returns None for unions, so `u1['prop1'] = 42` writes into the getter's type. | checker.go:21452 createUnionOrIntersectionProperty (writeType) / :16414 getWriteTypeOfSymbolWithDeferredType | members.rs:2269 `composite_property_of_type` (`if !Intersection { return None }` at :2275) feeding members.rs:2421 `write_type_of_property_of_type` | 5 | 1 | crates/tsr-checker/src/members.rs |
| B11 | A qualified reference to a namespace type alias whose body carries the alias (`N.T7 = {…}`) becomes a print-only `Named` mint with no member table. Relating it as a source answers NotRelated. | checker.go getTypeFromTypeAliasReference (alias declared type); relater sees the real object | declared.rs:5226 `qualified_type_reference` (written-text mint path after :5300); relater.rs:1798 `is_qualified_alias_mint` | 3 | 1 | crates/tsr-checker/src/declared.rs |
| B12 | `typeRelatedToDiscriminatedType` is not ported. `{x:'x'\|'y', y}` → `{x:'x',…}\|{x:'y',…}` and `{name:"A"\|"B"}` → `{name:"A"}\|{name:"B"}` answer NotRelated. | relater.go:3989 typeRelatedToDiscriminatedType (called :3893) | relater.rs:2885 `structured_type_related_to_worker` (no discriminant decomposition anywhere in relater.rs) | 3 | 2 | crates/tsr-checker/src/relater.rs |
| B13 | In a multi-signature call, a generic candidate after the first loses return-type contextual inference into its arguments. `g<T>(o:T): Readonly<T>` alone gives `readonly [string,number][]`; as the second overload it gives `readonly (string\|number)[][]`. This also makes `Promise.all` pick the Iterable overload, even with the tuple overload first. | checker.go:9025 chooseOverload (inferTypeArguments per candidate with that candidate's return mapper) | calls.rs:3968 `choose_overload` | 3 | 3 | crates/tsr-checker/src/calls.rs |
| B14 | `SkipGenericFunctions` only applies on the overload-failure retry. In the normal first pass, a generic function argument (`box`) is inferred before the context-sensitive arrow fixes `B`, so `C = unknown` and `<V>` leaks. | checker.go:7599/7617 instantiateTypeWithSingleGenericCallSignature (SkipGenericFunctions → anyFunctionType) + chooseOverload :9025 (`argCheckMode\|SkipGenericFunctions`) | inference.rs:752 (gate `if overload_failure && …`) / inference.rs:2340 `overload_failure_skips_generic_argument` | 2 | 1 | crates/tsr-checker/src/inference.rs |
| B15 | Contextual property type over a **union** context returns None under §927's "second guard" (unit leaf + may-discriminate), so `b: false` widens to `boolean`. Native maps over the union and drops members without the property. | checker.go:30551/30555 getTypeOfPropertyOfContextualType (mapType) | contextual.rs:2621 `union_contextual_property_type` (guard at :2706–2711) | 1 | 1 | crates/tsr-checker/src/contextual.rs |
| B16 | A generic homomorphic mapped **source** over a tuple-constrained `T` is not related through its resolved apparent array type. TSR has the apparent type (members resolve), but the relater never consults it. | checker.go:21772 getResolvedApparentTypeOfMappedType (used via getApparentType in the structural arm) | mapped.rs:1265 `apparent_mapped_type` (used by members.rs/index_signatures.rs only; not by relater.rs) | 3 | 1 | crates/tsr-checker/src/relater.rs |
| B17 | A second `declare module './index'` augmentation of a re-exported (`export *`) interface does not reach the re-exported symbol. The first does. | checker.go:1397 mergeModuleAugmentation (:1433–1441 resolvedExports merge + merged-symbol chain) | tsr-binder/src/lib.rs:505 `merge_module_augmentations` (export-star arm :596) | 1 | 1 | crates/tsr-binder/src/lib.rs |
| B18 | Flow reference matching does not strip a **comma** on the source (reference) side. `(f(), value).inner` is never narrowed by a guard on the same spelling. | flow.go:1645–1646 isMatchingReference source-switch BinaryExpression/comma arm | flow.rs:4532 `references_match` (`strip_source` closure :4569–4577 lacks the comma arm) | 2 | 1 | crates/tsr-checker/src/flow.rs |
| B19 | Type-predicate inference only accepts a body that is exactly one `return` statement. Native allows other statements (e.g. a throwing `if`) as long as there is a single return. | checker.go:20535 getTypePredicateFromBody | signatures.rs:1832 `infer_type_predicate_from_body` (`let [statement] = block.statements else { return None }`) | 1 | 1 | crates/tsr-checker/src/signatures.rs |
| B20 | `isTypeDerivedFrom` lacks the Array → ReadonlyArray arm. `x instanceof Array` does not narrow `readonly number[] \| number` (true branch `any[]`, false branch unnarrowed). | relater.go:4962 isTypeDerivedFrom (default arm :4989 `isArrayType(target) && !isReadonlyArrayType(target) && isTypeDerivedFrom(source, globalReadonlyArrayType)`) | flow.rs:1362 `is_derived_from_decidable` | 1 | 1 | crates/tsr-checker/src/flow.rs |
| B21 | The static side of a local class returned from a generic function (`typeof Inner`) is not instantiated by the call's mapper, so `outer(5).y` reads `T`. | checker.go:22304 getObjectTypeInstantiation (outer type parameters of anonymous/class types) | inference.rs:5397 `instantiate_type` (class-constructor `typeof C` not mapped; exact arm not isolated) | 1 | 1 | crates/tsr-checker/src/inference.rs |
| B22 | A binding-element default strips `undefined` with `getTypeWithFacts(NEUndefined)`, not `getNonUndefinedType`. For a generic parent (`Readonly<P>`) the declared type stays `42 \| P["foo"]` and later reads `number \| undefined`. | checker.go:17789 getBindingElementTypeFromParentType tail → :31548 getNonUndefinedType (base-constraint mapping for generic-with-undefined-constraint) | destructure.rs:348 (`get_type_with_facts(element_type, NE_UNDEFINED)`) | 2 | 2 | crates/tsr-checker/src/destructure.rs |
| B23 | Gap cascade: `PropTypes.InferProps<…>` evaluates to `error` in TSR. `ExtractedProps extends … ? true : false` stays deferred, and the relater answers NotRelated (not Unknown) for that unresolved conditional against `true`. | checker.go:24300 getConditionalType (native resolves to `true`) | declared.rs mapped/conditional alias evaluation (InferProps → error); relater.rs conditional-source verdict (see B6) | 1 | 1 | crates/tsr-checker/src/declared.rs |
| U | Unclassified (see §3) | — | — | 1 | 1 | — |
| **total** | | | | **71** | **33** | |

(B13's three cases are objectFromEntries, correctOrderOfPromiseMethod and inferFromGenericFunctionReturnTypes3. The last one
also involves B13b below.)

## 2. Buckets in detail

### B1: compound-like assignment target keeps its literal (11 rows)
Cases: literalWideningWithCompoundLikeAssignments 5:1, 11:1, 15:1, 19:1, 23:1, 27:1, 31:1, 35:1, 39:1, 43:1, 48:1.
```ts
// @strict: true
declare const numLiteral: 0;
let t1 = numLiteral;          // declared 0
t1 = t1 + 42                  // native: no error (target reads `number`); TSR: TS2322 Type 'number' is not assignable to type '0'
```
Evidence: probefile prints `>t1 : number` for the target, because the expression walker (expressions.rs:728) does apply
`is_in_compound_like_assignment`. The **reporter** takes a different road: `check_assignment_operator` → `assignment_target_type`,
whose identifier arm returns `get_type_of_symbol(symbol)` (assignreport.rs:1259) and never applies the base-literal rule
(native checker.go:11110). Fix: in the identifier arm, apply `get_base_type_of_literal_type` when `is_in_compound_like_assignment`.

### B2: `@noCheck` directive dropped by the harness (3 rows)
Cases: noCheckDoesNotReportError 1:14, noCheckNoEmit 1:14, noCheckRequiresEmitDeclarationOnly 1:14 (none of them has a native `.errors.txt`).
Evidence: `diagcase compiler/noCheckDoesNotReportError` reports TS2322. `diagnostics_suite::collect` calls
`program_diagnostics::skip_type_checking`, which honours `options.no_check`. But `apply_test_directives`
(trace_case.rs:400–631) has no `no_check: tristate("nocheck", …)` line and falls through `..base`. Native's harness adds
`noCheck` to its option table (harnessutil.go:309). This is not a checker bug.

### B3: getFlowTypeOfDestructuring unported (4 rows)
Cases: destructuringTypeGuardFlow 15:9, 18:9, 31:9, 34:9.
```ts
// @strictNullChecks: true
type foo = { bar: number | null; nested: { b: string | null } };
declare const aFoo: foo;
if (aFoo.bar && aFoo.nested.b) {
  const { bar, nested: { b: text } } = aFoo;
  const w: number = bar;      // native: bar is number (narrowed via synthetic aFoo.bar); TSR: number | null → TS2322
}
```
Evidence: probefile shows `>bar : number | null` and `>text : string | null` inside the guard. The module doc
(destructure.rs:52) and the comment at :155 both say "`getFlowTypeOfDestructuring` … is unported".

### B4: empty body ignores a contextual `undefined` return (5 rows)
Cases: functionsMissingReturnStatementsAndExpressionsStrictNullChecks 13:7, 17:7, 45:7, 63:7, 71:7.
```ts
// @strict: true
const f20: () => undefined = () => {};                 // native () => undefined; TSR () => void → TS2322
type FN = () => Promise<undefined> | undefined;
const fn3: FN = () => {};                              // same
```
Evidence: probefile gives `>() => {} : () => void`. Native getReturnTypeFromBody (checker.go:20177) checks
`someType(unwrapReturnType(contextualReturnType) ?? void, Undefined)`. TSR's async road does this
(signatures.rs:3436–3445). The plain road returns `self.intrinsics.void` at :3607/:3632/:3641 unconditionally. fn1
(`return;` only) falls in the same arm at :3607.

### B5: variance relation missing alias variances and structural fallback (7 rows)
Cases: consistentAliasVsNonAliasRecordBehavior 10:5, 14:5, 26:5, 30:5; genericIndexedAccessVarianceComparisonResultCorrect 26:1;
nongenericPartialInstantiationsRelatedInBothDirections 11:1, 12:1.
```ts
type Record2<K extends keyof any, T> = { [P in K]: T };
type R1 = Record<string, string> extends Record<"a", string> ? 1 : 0;    // native 1 (K contravariant), TSR 0
type R2 = Record2<string, string> extends Record2<"a", string> ? 1 : 0;  // native 1, TSR 0
```
```ts
class A { x = 'A'; y = 0 }  class B { x = 'B'; z = true }
type T<X extends { x: any }> = Pick<X, 'x'>;
type TA_TB = T<A> extends T<B> ? true : false;                 // native true (Unreliable → structural fallback), TSR false
interface Foo { a: number; b: number; bar: string }
type X4 = Partial<Foo> extends Partial<{ a: number, foo: number }> ? 1 : 0;  // native 1 (Unmeasurable → structural), TSR 0
```
Evidence: these alias instantiations are registered in `type_reference_targets` (declared.rs:6317, :4551), so
relater.rs:3371 `same_target_references` matches them. `inference_variances` returns `None` for any alias body that is not a
function, constructor or type literal (variances.rs:46–56), so the arm defaults to `vec![Covariant; n]`
("Until Unmeasurable/Unreliable flags are represented…"). It then returns the NotRelated result directly. Native
(relater.go:3266–3306) falls back to a structural check when `len(variances)==0` or any variance has
`AllowsStructuralFallback`, and getAliasVariances measures `Record`'s K as contravariant.
The nongenericPartial case is an interface (`ObjectContaining<T>` uses `Partial<T>`). Native measures its T as Unmeasurable
(via mappedTypeRelatedTo's reportUnmeasurableMapper), so it relates structurally in both directions.

### B6: conditional-type source relation (7 rows)
Cases: distributiveConditionalTypeConstraints 32:9, 54:11, 65:11, 76:11, 87:11; inlineConditionalHasSimilarAssignability 8:3, 15:3.
```ts
// @strict: true
interface A { foo(): void } interface B { bar(): void }
function test1<T extends A>(y: T extends B ? number : string) {
  const newY: string | number = y;   // native OK via getDefaultConstraintOfConditionalType = number|string; TSR TS2322
}
function foo<T>(a: T) {
  const c: (any[] extends T ? any[] : never) = 0 as any;
  a = c;                              // native OK: true branch is substitution any[] & T, default constraint relates to T; TSR TS2322
}
```
Evidence: relater.rs has no CONDITIONAL-flag arm (grep finds none). `default_constraint_of_conditional_type`
(constraints.rs:51) exists but is only used by inference.rs and assertions.rs. relater.rs:2936 answers a union target as
`any(parts)` and returns, while native (relater.go:3370–3386) falls through to the source-instantiable arms when the union
check fails. The ZeroOf row (32:9) prints a malformed source (`… : never | null`), which may be a second defect in TSR's
nested-conditional construction. Its fix is still the same arm (distributive constraint). The inlineConditional rows also
need checker.go:24952 getConditionalFlowTypeOfType: `any[]` in the true branch becomes a substitution constrained by `T`.
TSR has only a partial use of this (index_access_reports.rs:304).

### B7: `infer` conditional over a non-generic union is not evaluated (1 row)
Case: complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound 40:7.
```ts
interface TC { type: 'text' } interface EC { type: 'email' }
type CT = (TC | EC) extends { type: infer R } ? R : never;   // native "text"|"email"; TSR prints the deferred conditional
declare function mk3<T extends CT>(t: T): [T];
const m3 = mk3('text');                                       // native ["text"]; TSR [string]
```
Evidence: probefile shows `>CT : Ch extends { type: infer R; } ? R : never` and `>m3 : [string]`. The single-member form
`TC extends {type: infer R}` evaluates to `"text"`. declared.rs:8252 returns None whenever the check type is a UNION.
Downstream, `makeNewChannel('text')` infers `T = string` (no literal-preserving constraint), so
`ChannelOfType<string>` = both channels, which gives the reported `NewChannel<EmailChannel | TextChannel>`.

### B8: contextual type from an intersection context, any → unknown (2 rows)
Cases: contextualTypeBasedOnIntersectionWithAnyInTheMix3 23:7, 24:7.
```ts
const a: [any] & [1] = [1];      // native [1]; TSR [number] → TS2322
const b: any[] & 1[] = [1, 1];   // native 1[]; TSR number[] → TS2322
```
Evidence: probefile `>[1] : [number]`, `>[1, 1] : number[]`. contextual.rs:2351–2361 reads `get_type_of_property_of_type`
or `get_applicable_index_info` on the intersection, which gives `any & 1 = any` / `any`. Native
getTypeOfPropertyOfContextualTypeEx (checker.go:30555) walks intersection constituents and
appendContextualPropertyTypeConstituent (:30671) turns `any` into `unknown`, so the result is `unknown & 1 = 1`.

### B9: tuple context under an intersection with a variadic tuple (1 row)
Case: intersectionsAndOptionalProperties 25:7.
```ts
const yy: number[] & [number, ...number[]] = [1];   // native [number]; TSR number[]
const ww: number[] & [number] = [1];                // TSR [number]  (works)
const zz: [number, ...number[]] = [1];              // TSR [number]  (works)
```
Evidence: those three probes. The seven hand-written arms decline, and §888's generic arm
(array_literals.rs:631, `is_tuple_like_type` per constituent) is where the intersection should qualify. I did not find which
sub-step drops it (property "0" of the intersection, or realizing the tuple once the context is accepted).

### B10: union receiver write type (divergent accessors) (5 rows)
Cases: divergentAccessorsTypes8 65:1, 68:1, 73:1, 75:1, 77:1.
```ts
// @strict: true
class One { get prop1(): string { return "" } set prop1(s: string | number) {} prop3: number = 42 }
class Two { get prop1(): string { return "" } set prop1(s: string | number) {} get prop3(): string { return "" } set prop3(s: string | boolean) {} }
declare const u1: One | Two;
u1['prop1'] = 42;   // native target string|number; TSR string → TS2322
u1['prop3'] = true; // native target string|number|boolean; TSR string|number → TS2322
```
Evidence: probefile left side `>u1['prop1'] : string`, `>u1['prop3'] : string | number`. A single-class receiver
`o['prop1']` gives `string | number`, which is correct. `write_type_of_property_of_type` handles the synthetic case, but
`composite_property_of_type` (members.rs:2275) returns None for any non-intersection ("A union receiver is not yet routed
here"), so it falls back to the read type.

### B11: qualified alias mint has no members (3 rows)
Cases: dynamicNames main.ts:97:19, 97:46, 97:73 (all `t* = t7`).
```ts
namespace N { export type T12 = { a: number }; export interface I12 { a: number } }
type R3 = N.T12 extends N.I12 ? 1 : 0;   // native 1, TSR 0
```
Evidence: R3 probe gives 0. The unqualified `T11` and the interface forms give 1, and `keyof N.T9` prints `error`.
declared.rs:5226 keeps a "written-text mint" for an argument-less qualified alias reference whose declared type carries the
alias (the comment above :5300 cites NB-SYMBOL-CHAIN). relater.rs:1798 recognizes these mints, and the relater answers
NotRelated as a source.

### B12: typeRelatedToDiscriminatedType unported (3 rows)
Cases: discriminableUnionWithIntersectedMembers 7:7, 14:7; relatedViaDiscriminatedTypeNoError2 9:1.
```ts
type Y = { x: 'x', y: number } | { x: 'y', y: number, z?: boolean };
type R1 = { x: 'x' | 'y', y: number } extends Y ? 1 : 0;                       // native 1, TSR 0
type R2 = { name: "A" | "B" } extends { name: "A" } | { name: "B" } ? 1 : 0;   // native 1, TSR 0
```
Evidence: the probes above. relater.rs contains no discriminant decomposition. Native relater.go:3893 → :3989.
Note: discriminableUnionWithIntersectedMembers 7:7 has a source comment "// error", but the pinned native baseline has no
errors file, so native relates it as well.

### B13: overload candidates after the first lose per-candidate return-type context (3 rows)
Cases: objectFromEntries 8:7; correctOrderOfPromiseMethod 20:11; inferFromGenericFunctionReturnTypes3 158:7.
```ts
declare function fr<T>(o: T): Readonly<T>;
declare function g<T extends Function>(f: T): T;
declare function g<T>(o: T): Readonly<T>;
const f3: readonly [string, number][] = fr([['a', 1]]);  // TSR readonly [string, number][] (OK)
const f8: readonly [string, number][] = g([['a', 1]]);   // native same as f3; TSR readonly (string | number)[][] → TS2322
```
```ts
declare function all3<T extends readonly unknown[] | []>(v: T): Promise<{ -readonly [P in keyof T]: Awaited<T[P]> }>;
declare function all3<T>(v: Iterable<T | PromiseLike<T>>): Promise<Awaited<T>[]>;
declare const pa: Promise<A[]>; declare const pb: Promise<B[]>;   // B extends A structurally
const r = all3([pa, pb]);   // TSR Promise<A[][]> even with the tuple overload FIRST; native picks the tuple → [A[], B[]]
```
Evidence: those probes. With only the tuple overload (`all`/`tu`), TSR gives `Promise<[A[], B[]]>`. inferFromGenericFunctionReturnTypes3 is
`Promise.all([...]) : Promise<Player[]>`. There, both overloads in sequence (`both`) fail like this bucket. In addition, the
Iterable overload alone (`it`) loses the literal because the return mapper is not applied to a `T | X<T>` parameter context
(**B13b**, see below).
B13b (part of row 37): `declare function c2<T>(x: T | {q:T}): T; const r6: P = c2({ p: "x" })` gives `{p: string}` in TSR.
Native instantiateContextualType (checker.go:30817) uses only the return mapper when ContextFlagsSignature is absent.
contextual.rs:560 `instantiate_contextual_inference_type` tries the live non-fixing mapper first and returns its image
unless it is any/unknown. This is my best hypothesis for b2/b4/c1/c2.

### B14: SkipGenericFunctions only on the overload-failure retry (2 rows)
Cases: genericFunctionInference1 25:7, 29:7.
```ts
// @strict: true
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
declare function list<T>(a: T): T[];  declare function box<V>(x: V): { value: V };
const g03: <T>(x: T) => { value: T[] } = pipe(x => list(x), box);  // native (x: T) => {value: T[]}; TSR (x: T) => {value: unknown}
const f03 = pipe(x => list(x), box);                                 // native (x: any) => {value: any[]}; TSR <V>(x: any) => {value: V}
```
Evidence: probefile output above. `pipe(x => list(x), x => box(x))` works. inference.rs:752 replaces a generic function
argument with the skipped placeholder only when `overload_failure`. Native first-pass inference always runs with
`argCheckMode|SkipGenericFunctions` (chooseOverload) and returns anyFunctionType at checker.go:7617.

### B15: contextual property type over a union context, §927 second guard (1 row)
Case: contextualTypeIterableUnions 7:7.
```ts
const o1: { a: true } | { b: false } = { b: false };            // TSR {b: false} (OK)
const o2: ({ a: true } | { b: false })[] = [{ b: false }];       // native {b: false}[]; TSR {b: boolean}[]
const i2: Iterable<{ a: true }> | Iterable<{ b: false }> = [{ b: false }];  // the row
```
Evidence: probes. contextual.rs:2706–2711 returns `None` when there is more than one candidate, `member` has a unit leaf and
`object_literal_may_discriminate`. The comment there says this guard was kept on purpose (+6 W→R / −3 R→W in
excessPropertyCheckWithUnions). Native's mapType drops the `{a: true}` constituent and gives `false`.

### B16: generic homomorphic mapped source over a tuple constraint (3 rows)
Cases: mappedTypeUnionConstrainTupleTreatedAsArrayLike 4:9, 5:9, 10:9.
```ts
// @strict: true
type H<T> = { [P in keyof T]: T[P] extends string ? boolean : null };
function t<T extends [number] | [string]>(a: T) { const arr: any[] = [] as H<T>; }  // native OK; TSR TS2322
```
Evidence: in TSR `x.length : 1` and `x.map : …` resolve, so `apparent_mapped_type` (mapped.rs:1265) works for members. It is
called from members.rs and index_signatures.rs, never from relater.rs. Native relates through getApparentType →
getResolvedApparentTypeOfMappedType (checker.go:21772).

### B17: second module augmentation through `export *` (1 row)
Case: mergeMultipleInterfacesReexported test.ts:4:7.
```ts
// @filename: index.ts
export * from './eventList';
// @filename: eventList.ts
export interface EventList { p0: [] }
// @filename: foo.ts
declare module './index' { interface EventList { p1: [] } } export {};
// @filename: bar.ts
declare module './index' { interface EventList { p2: [] } } export {};
// @filename: test.ts
import { EventList } from "./eventList"; declare const e: EventList; const k1 = e.p1; const k2 = e.p2;
// TSR: k1 : [], k2 : any.  If bar.ts augments './eventList' directly, k2 : [].
```
Evidence: repro `/tmp/box/an/repro/A1/mma/m1.ts` vs `m2.ts`. Only the second augmentation of the same `export *` module is
lost (lib.rs:596 arm). After the first merge, `target.exports.get(name)` is Some, so the export-star arm is skipped. Native
reaches the re-exported symbol through the merged-symbol chain of the first augmentation's symbol.

### B18: comma not stripped on the flow source side (2 rows)
Cases: narrowCommaOperatorNestedWithinLHS 10:11, 15:11.
```ts
if (typeof (otherValue(), value).inner === 'number') {
  const b: number = (otherValue(), value).inner;  // native number; TSR string | number
}
```
Evidence: probefile `(otherValue(), value).inner : string | number`, while `value.inner : number` inside the guard. The
`strip_source` closure in references_match (flow.rs:4569) handles Parenthesized, NonNull and Satisfies, but not the comma
BinaryExpression that native has at flow.go:1645.

### B19: predicate inference needs a single-statement body (1 row)
Case: inferTypePredicates 252:7.
```ts
function assertAndPredicate(x: string | number | Date) { if (x instanceof Date) { throw new Error(); } return typeof x === 'string'; }
// native: (x) => x is string; TSR: (x) => boolean
```
Evidence: probefile output. signatures.rs:1857 uses `let [statement] = block.statements else { return None }`.

### B20: instanceof Array does not narrow ReadonlyArray (1 row)
Case: instanceofNarrowReadonlyArray 7:17.
```ts
// @strict: true
function narrow(x: readonly number[] | number): readonly number[] {
  if (x instanceof Array) return x;   // native readonly number[]; TSR any[]
  else return [x];                    // native number; TSR number | readonly number[] → TS2322
}
```
Evidence: probefile output. flow.rs:1362 `is_derived_from_decidable` has no Array/ReadonlyArray arm (relater.go:4989).

### B21: static side of a local class not instantiated (1 row)
Case: staticAnonymousTypeNotReferencingTypeParameter 10:5.
```ts
function outer<T>(x: T) { class Inner { static y: T = x; } return Inner; }
let y: number = outer(5).y;   // native .y : number; TSR .y : T
```
Evidence: probefile output. Native's `typeof Inner` carries outer type parameters and is instantiated by
getObjectTypeInstantiation (checker.go:22304). I did not isolate the exact TSR arm.

### B22: binding default uses getTypeWithFacts, not getNonUndefinedType (2 rows)
Cases: strictNullNotNullIndexTypeShouldWork 22:9, strictNullNotNullIndexTypeNoLib 23:9.
```ts
// @strictNullChecks: true
interface Foo { foo?: number }
function f<P extends Foo>(p: Readonly<P>) { const { foo = 42 } = p; return foo; }
// native foo : number; TSR declared 42 | P["foo"], read number | undefined
```
Evidence: probefile output. `const { foo: f2 = 42 } = q` with `q: P` gives `number`. destructure.rs:348 uses
`get_type_with_facts(…, NE_UNDEFINED)`. Native getNonUndefinedType (checker.go:31548) first maps a
generic-with-undefined-constraint type to its base constraint.

### B23: gap cascade into an unresolved conditional (1 row)
Case: propTypeValidatorInference file.ts:53:7.
Evidence: probefile on the case prints `>ExtractedProps : error`, `>ExtractedPropsWithoutAnnotation : error`, and
`ExtractPropsMatch` as the deferred conditional text. Native resolves it to `true`. Two fixes are possible: make
InferProps evaluate (the primary fix), or make the relater decline (Unknown) on an unresolved non-generic conditional source,
which overlaps B6.

## 3. Unclassified

- **lastPropertyInLiteralWins 13:5** (`test({ thunk: (num: number) => {}, thunk: (str: string) => {} })`, the second call).
  Native reports nothing because the last property wins. In TSR the literal's type is last-wins
  (`{ thunk: (str: string) => void }` in both a declaration and a call argument, checked with probefile), and
  `elaborate_object_literal_members` (assignreport.rs:2530) reads the completed source member, which is related. So the
  TS2322 at the **first** `thunk` comes from some other road that relates each `PropertyAssignment` initializer to the
  contextual property type on its own: a per-member contextual check on duplicate names. Most likely it is in the
  call-argument reporting at calls.rs:1680–1700 or in a contextual-signature check of the arrow function. I could not find
  which site reports it without a diagnostics probe for an ad-hoc file.

## Notes and caveats
- No ad-hoc diagnostics runner exists (the corpus path is compiled in, and vendor/ must not be modified). Relation verdicts
  were therefore checked with `S extends T ? 1 : 0` probes, which go through TSR's conditional evaluator into the same
  relater.
- I am least sure of B9 (failing sub-step not isolated), B13b, B21 (exact arm) and the ZeroOf row inside B6.
- B5 and B6 are relater-wide gaps, and fixing them will very likely move rows in other slices as well.
