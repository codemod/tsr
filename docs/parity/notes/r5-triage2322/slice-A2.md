# A2 — TSR-only TS2322 false positives (no paired native diagnostic)

73 rows, 27 root-cause buckets. Every bucket has a minimal repro. Each repro ran through the real diagnostics
pipeline (`diagcase` + `r5census`) inside a private mount namespace. That namespace bind-mounted
`/tmp/box/an/repro/A2/cases` over `tests/cases/compiler`, so nothing under /home/user/tsr was modified.
The wrapper is `/tmp/box/an/repro/A2/bin/diag.sh <name>`, and all repro files are in `/tmp/box/an/repro/A2/cases/`.
Line numbers in the repros follow the census convention, so `// @` directive lines are not counted.
Confidence: **C** = confirmed by repro plus reading the code at the named line. **R** = the repro isolates the
shape and the cause, but not the exact failing predicate.

## 1. Bucket table

| id | root cause (one line) | native anchor | TSR function | #lines | #cases | owning file |
|---|---|---|---|---|---|---|
| D | `typeRelatedToDiscriminatedType` not ported: an object/tuple source whose discriminant is a union (`{type: "A"\|"B"}`, `["a"\|"b", number]`) is not related to a discriminated-union target by enumerating combinations (C) | vendor/typescript-go/internal/checker/relater.go:3989 typeRelatedToDiscriminatedType (call site :3893 in structuredTypeRelatedToWorker) | crates/tsr-checker/src/relater.rs:2885 structured_type_related_to_worker (tail, no such arm) | 10 | 2 | relater.rs |
| DA | Flow `getAssignedType` destructuring arms unported: the target of `[{x: b}] = …` / `({x: d} = o)` / `[, g] = t` is never narrowed, so it keeps its declared union (C) | flow.go:2288 getAssignedType (+ :2361 getAssignedTypeOfPropertyAssignment, :2365 getTypeOfDestructuredProperty) | crates/tsr-checker/src/flow.rs:2543 get_initial_or_assigned_type (the comment there says this arm "stays unported") | 12 | 1 | flow.rs |
| TH | An inherited member's polymorphic `this` (`m(this: this)`, `m(): this`) is not re-bound to the derived receiver during the structural relation, so `Derived1` and `Derived2` fail to relate. This makes the bivariant `this`-parameter check fail (C for the effect, R for the exact site) | checker.go:19573 getTypeWithThisArgument (base types instantiated with the derived `this`) | crates/tsr-checker/src/members.rs:1333 get_type_of_property_of_type, as used by relater.rs:3922 properties_related_to_with_optionals | 6 | 1 | members.rs / relater.rs |
| CD | `@ts-ignore`/`@ts-expect-error` inside a JSX expression container (`{/* @ts-ignore */}`) is never collected, so the diagnostic is not suppressed (C for the effect, R for the mechanism) | compiler/program.go:1386 getDiagnosticsWithPrecedingDirectives; scanner/scanner.go:674 processCommentDirective (collected while the parser scans in JSX mode) | crates/tsr-compiler/src/comment_directives.rs:57 directives_in (rescans with the plain TS scanner) | 5 | 1 | tsr-compiler/comment_directives.rs |
| VT | The `isSingleElementGenericTupleType` arm is missing: `[...T]` should relate to `T` through its single type argument (C) | relater.go:3410 structuredTypeRelatedToWorker | relater.rs:2885 structured_type_related_to_worker (no arm; no port of the predicate exists) | 5 | 1 | relater.rs |
| U | `isUnknownLikeUnionType` is missing from `isSimpleTypeRelatedTo`: under strictNullChecks, anything (including `unknown`) is assignable to a union with `undefined`, `null` and `{}` (C) | relater.go:275 isSimpleTypeRelatedTo → checker.go:27803 isUnknownLikeUnionType | relater.rs:2522 is_simple_type_related_to | 4 | 2 | relater.rs |
| AL | Aliased-condition narrowing (`const isS = typeof o.x === 'string'; if (isS) o.x`) never fires when the reference is an access expression. Identifier references work. For `obj[0]`, `is_constant_reference`'s element-access arm accepts only string-literal keys (C). For property access, the failing predicate was not isolated (R) | flow.go:386 narrowType identifier arm; flow.go:1814 isConstantReference | crates/tsr-checker/src/flow.rs:5535 narrow_type_worker (alias inlining) and flow.rs:1620 is_constant_reference (element arm at :1685) | 4 | 1 | flow.rs |
| P0 | `object_against_primitive` override: when the relater answers Unknown for primitive → object, the reporter still reports if the target has any required property. It never checks the primitive's apparent type (`number`→`{toString():string}`, `string`→`{length:number}`) (C) | relater.go:3814 structuredTypeRelatedToWorker (apparent-type source) | crates/tsr-checker/src/assignreport.rs:2214 object_against_primitive (called at :1858 in report_relation_failure) | 3 | 3 | assignreport.rs |
| V | Variance measurement for a recursive generic whose recursion goes through a type alias (`type A<T>=B<T>; interface B<T>{prop:A<T>}`; `CalcValue<O>=CalcObj<O>`) gives a wrong (non-independent / non-contravariant) answer, so the variance check rejects (C for the effect, R for the site) | relater.go:1341 getVariancesWorker / :1332 getAliasVariances | crates/tsr-checker/src/variances.rs:26 inference_variances (alias with a reference body → `None`; in-progress → empty vec) | 2 | 1 | variances.rs |
| CRET | Return-mapper contextual type of an arrow inside a generic call is lost when the contextual return is `R \| PromiseLike<R>` (`then`) or the arrow is `async`, so literals and tuples widen (`{name:string}`, `(A\|B)[]`) (C for the effect) | checker.go:29621 getContextualTypeForReturnExpression (async → awaited) and checker.go:30817 instantiateContextualType | crates/tsr-checker/src/contextual.rs:1541 (concise-arrow-body arm → contextual_return_expression_slot) | 2 | 2 | contextual.rs |
| K | Arguments of `new X(...)` are not contextually typed when X's construct signature comes from a type literal or var/const (`{ new(o: Opts): T }`, `Intl.DisplayNames`). Class constructors work. `'region'` widens to `string` (C) | checker.go:29762 getContextualTypeForArgument (New ≡ Call) | crates/tsr-checker/src/contextual.rs:1376 NewExpression arm (falls to `sole_constructor_parameters` = class only) | 2 | 1 | contextual.rs |
| ENUM | Object-literal elaboration does not match a non-canonical numeric property name (`2.0:`, `3.0:`) against a number index signature, so it reports the outer TS2322 at the declaration instead of at the member (C) | relater.go:498 elaborateObjectLiteral | crates/tsr-checker/src/assignreport.rs:2466 elaborate_object_literal / :2481 _members | 2 | 2 | assignreport.rs |
| EXP | `export { A }` where local `A` merges an imported type-only alias with a `const` is missing from the module namespace (`types.A` → TS2339), so `{A:any,…}` vs `typeof import` fails (C for the effect, R for the site) | checker.go:16131 getExportsOfModule | export-table resolution (tsr-binder / symbols.rs); not isolated | 2 | 2 | symbols.rs (probable) |
| EOPT | Under `exactOptionalPropertyTypes`, a definite-assignment target `obj.b = …` is still flow-narrowed. `obj.b = 'hello'; obj.b = undefined` sees target `string`. Deliberate legacy compensation (C) | checker.go:11397 getFlowTypeOfAccessExpression (definite target → removeMissingType(propType), no flow) | crates/tsr-checker/src/members.rs:964 (`!exact_optional_property_types \|\| jsdoc_property` guard) | 1 | 1 | members.rs |
| W | `typeof undefined` under strictNullChecks:false: `getWidenedType` is not applied. The special case compares against `intrinsics.undefined`, but the symbol's type is `loose_undefined_widening` (a different id), so the type stays `undefined` (C) | checker.go:24102 getTypeFromTypeQueryNode | crates/tsr-checker/src/declared.rs:1474 get_type_from_type_query_node (:1516 guard) | 1 | 1 | declared.rs |
| IX | `isObjectTypeWithInferableIndex` lacks the intersection arm (`every` constituent), so `{x:A}&{x:B}` → `{[k:string]:A&B}` is NotRelated (C) | relater.go:4624 isObjectTypeWithInferableIndex | crates/tsr-checker/src/relater.rs:2450 object_type_has_inferable_index | 1 | 1 | relater.rs |
| GMI | `indexSignaturesRelatedTo`'s arm "generic mapped source vs string-index target → template related to value" is missing (`{[K in keyof T]: number}` → `{[k:string]:number}` fails) (C) | relater.go:4590 indexSignaturesRelatedTo | crates/tsr-checker/src/relater.rs:2377 related_index_signatures | 1 | 1 | relater.rs |
| PN | `nominal_class_pair_verdict` answers NotRelated when only the SOURCE class has a private/#private member. Native fails only on target private/protected props (C) | relater.go:4270 propertyRelatedTo (private arm :4278) | crates/tsr-checker/src/unions.rs:1959 nominal_class_pair_verdict (called from relater.rs ~1230) | 1 | 1 | unions.rs |
| DK | Object binding element with a computed key of union-literal type over a tuple (`{[a]: b} = [9,0,5]`, `a: 0\|1`) uses the number index (`0\|5\|9`) instead of distributing the indexed access (`9\|0`) (C) | checker.go:17707 getBindingElementTypeFromParentType → checker.go:26935 getIndexedAccessTypeOrUndefined | crates/tsr-checker/src/destructure.rs:182 (computed_key → get_applicable_index_info fallback) | 1 | 1 | destructure.rs |
| MP | A mapped type over overlapping string-enum keys (`T.CAT="cat"`, `A.CAT="cat"`) does not merge into one property with a union key type. `m.cat` is `error`, so the contextual type is lost and the enum literal widens (C for the effect) | checker.go:20894 resolveMappedTypeMembers (addMemberForKeyTypeWorker unions keyType/nameType) | crates/tsr-checker/src/mapped.rs:653 resolve_mapped_type_members_worker | 1 | 1 | mapped.rs |
| RA | Self-reference of a recursive alias inside its own type argument (`type Box2 = Box<Box2 \| number>`) resolves to a member-less image: `v.value` on the inner `Box2` is `error`, so depth ≥ 2 literals fail (C for the effect) | checker.go:23580 getTypeFromTypeAliasReference / :23641 getTypeAliasInstantiation | crates/tsr-checker/src/declared.rs (alias reference resolution); not isolated | 1 | 1 | declared.rs |
| SM | A generic alias whose body is a string mapping or a template intersection, instantiated with concrete args, stays an unevaluated alias image: `T5<"F">` where `T5<S>=Lowercase<S&string>` is not assignable to `"f"`. The conditional in `G<"">` therefore picks the false branch (C for the effect) | checker.go:29223 getStringMappingType (via instantiateType) | crates/tsr-checker/src/string_mapping.rs:44 get_string_mapping_type / instantiation | 1 | 1 | string_mapping.rs |
| OP | `s += ""` on a `symbol`: after TS2469, native returns before `checkAssignmentOperator`. TSR's `+=` goes through the generic assignment arm and also reports TS2322 (C) | checker.go:12443 checkBinaryLikeExpressionWorker (`!checkForDisallowedESSymbolOperand → return`) | crates/tsr-checker/src/check.rs:577 (assignment-operator arm calls check_assignment_operator unconditionally) | 1 | 1 | check.rs |
| GM | `typeof r === 'function'` on `(() => void) \| Record<keyof S, () => void>` narrows the Record constituent to `Function`. The image `Function` can only come from narrowTypeByTypeFacts' `isTypeSubtypeOf(implied, t)` branch, so TSR answers `Function <: Record<keyof S,…>` true. Native: false (keyof S not related to keyof Function) (C for the effect, R for the arm) | flow.go:685 narrowTypeByTypeFacts; relater.go:3593 generic-mapped-target arm | crates/tsr-checker/src/flow.rs:8149 narrow_type_by_type_facts → relater.rs:1587 generic_mapped_target_related_to | 1 | 1 | relater.rs |
| CND | Type-predicate narrowing `value is Extract<T, Function>` on `x: T` yields `T` (native `Extract<T,Function>`). Separately, `Extract<T,Function>` → `Function` is NotRelated in TSR (C for both effects, R for the arm) | flow.go:859 getNarrowedTypeWorker; relater.go:3721 conditional-source arm | crates/tsr-checker/src/flow.rs:7506 narrowed_type_worker; relater.rs:2885 conditional arm | 1 | 1 | flow.rs / relater.rs |
| CTUP | Contextual tuple detection fails for `test1 & {length: 2}` where `test1 = [...number[]]` is an alias (it works when written inline), so `[0,0]` is typed `number[]` (C for the effect, R for the site) | checker.go:23544 isTupleLikeType | crates/tsr-checker/src/tuples.rs:737 is_tuple_like_type (→ is_array_like_type "undecided → false") | 1 | 1 | tuples.rs |
| EJSX | JSX children elaboration (`elaborateJsxComponents`) is not applied to two function-expression children of a class component, so TSR reports the outer TS2322 at the opening tag (native reports per child) (R) | jsx.go:295 elaborateJsxComponents | crates/tsr-checker/src/jsx_component.rs:352 elaborate_jsx_components (/ :582 elementwise) | 1 | 1 | jsx_component.rs |

Total: 73 rows. (D 10, DA 12, TH 6, CD 5, VT 5, U 4, AL 4, P0 3, V 2, CRET 2, K 2, ENUM 2, EXP 2, and one row each for EOPT, W, IX, GMI, PN,
DK, MP, RA, SM, OP, GM, CND, CTUP, EJSX.)

Highest-yield single fixes: **DA** (12, flow.rs), **D** (10, relater.rs), **TH** (6), **VT** (5), **CD** (5), **U** (4).
Five of the buckets are missing relater arms with exact native anchors (D, VT, U, IX, GMI). One is a reporter override
(P0) and one a nominal shortcut (PN). Each of these lands in a single function.

## 2. Buckets in detail

### D — discriminated-union relation (typeRelatedToDiscriminatedType) not ported
Rows: unionRelationshipCheckPasses 1:7; assignmentCompatWithDiscriminatedUnion 15:5, 29:5, 104:5, 139:11, 159:9 (return
object literal), 189:15, 200:11 (union of tuples), 222:13, 223:13.
Repro (`rel1.ts`, `dis2.ts`):
```ts
// @strict: true
declare let s: { foo?: number | undefined };
const item: { foo?: undefined } | { foo: number } = s;          // native: ok; TSR: TS2322
type S = { a: 0 | 2, b: 4 };
type T = { a: 0, b: 1 | 4 } | { a: 1, b: 2 } | { a: 2, b: 3 | 4 };
declare let s2: S; declare let t2: T; t2 = s2;                    // native: ok; TSR: TS2322
type A = ["a", number] | ["b", number] | ["c", string];
declare const b: "a" | "b" | "c";
const a: A = b === "a" || b === "b" ? [b, 1] : ["c", ""];       // native: ok; TSR: TS2322
```
Evidence: `grep -i discrimin relater.rs` finds no port. The worker tail (relater.rs ~3490–3524) ends without the
`source Object|Intersection && target Union → typeRelatedToDiscriminatedType(source, extractTypesOfKind(target, Object|Intersection|Substitution))`
fallback at relater.go:3889–3897. Every TSR message names a source whose discriminant property is a union.

### DA — destructuring-assignment targets are never narrowed
Rows: controlFlowAssignmentPatternOrder 7, 13, 19, 25, 32, 38, 44, 50, 57, 63, 69, 75 (all col 11).
Repro (`cfp3.ts`):
```ts
declare const o: { x: 0, 1: 0 };
let d: 0 | 1; ({ x: d } = o);  const dd: 0 = d;     // native ok; TSR TS2322 '0 | 1' → '0'
declare const t: readonly [9, 0, 5];
let g: 0 | 9 | 5; [, g] = t;   const gg: 0 = g;     // native ok; TSR TS2322
```
Evidence: flow.rs:2543 `get_initial_or_assigned_type` handles binding elements, variable declarations, for-in/of,
delete and `x = e`. Its doc comment says the destructuring arms "stay unported and answer `None` — the declared
type". Native flow.go:2288 `getAssignedType` covers ArrayLiteral, ObjectLiteral (getAssignedTypeOfPropertyAssignment),
ShorthandPropertyAssignment, SpreadElement and the default `= e` arm. The probefile output for the case shows `b : 0 | 1 | 9`
throughout, where native has `b : 0`.

### TH — inherited polymorphic `this` not re-bound to the derived receiver
Rows: thisTypeInFunctions 163, 164, 167, 168, 169, 170 (col 1). The case is `@strict: false`.
Repro (`thisf3.ts`, `thisf4.ts`):
```ts
// @strict: false
class E1 { x: number; m(this: this): number { return 1; } }
class F1 extends E1 { y: number }
class E2 { x: number; y: number; m(this: this): number { return 1; } }
declare let f1: F1; let o2: E2 = f1;                // native ok; TSR TS2322 'F1' → 'E2'
class R1 { x: number; m(this: this): number { return 1; } }
class R2 { x: number; m(this: this): number { return 1; } }
declare let r1: R1; let o5: R2 = r1;                // passes in both (non-inherited member)
```
Evidence: the same shape passes when the member is declared on the class itself (R1→R2) and fails when it is
inherited (F1→E2, and `m(): this` variants too). Plain `(this: A)=>n` vs `(this: B)=>n` bivariance works
(`thisf.ts`), and relater.rs:2198 does compare `this` parameters bivariantly. So the failing piece is the inherited member's
`this` type, which stays the declaring class's `this` and does not become the receiver.
Native rebinds through `getTypeWithThisArgument(base, derived.thisType)` (checker.go:19573) when it resolves base members.

### CD — comment directives inside JSX containers
Rows: multiline b.tsx 12:18, 15:18, 19:18, 23:18, 27:18.
Repro (`ml.tsx`):
```tsx
// @jsx: preserve
declare namespace JSX { interface Element {} interface IntrinsicElements { div: any } }
function MyComponent(props: { foo: string }) { return <div />; }
let x = (<div>
    {/*@ts-ignore*/}
    <MyComponent foo={100} />          // native suppressed; TSR TS2322
    {/*@ts-expect-error*/}
    <MyComponent foo={100} />          // native suppressed; TSR TS2322 (and no TS2578)
  </div>);
// @ts-ignore
let y: string = 1;                     // suppressed in both
```
Evidence: `directive_of` already implements `lastLineStart` (comment_directives.rs:94–140), so the multi-line form is
not the cause. Every directive inside a JSX `{…}` is missed, including single-line ones, while a top-level `// @ts-ignore` works.
`directives_in` re-scans the file with the plain TS `Scanner`. Native records directives while the parser scans the
file in JSX-aware mode (scanner.go:631/674).
Most likely the TS-mode rescan desynchronises on the JSX children text. This is not a relation problem.

### VT — `[...T]` → `T` (single-element generic tuple)
Rows: variadicTuples1 159:5, 168:5, 177:5, 186:5, 187:5.
Repro (`vt.ts`):
```ts
function f11<T extends unknown[]>(t: T, m: [...T]) { t = m; /* native ok; TSR TS2322 */ m = t; }
```
Evidence: relater.go:3410–3421 `isSingleElementGenericTupleType(source) && !readonly → isRelatedTo(typeArgs[0], target)`.
There is no TSR counterpart (grep returns nothing).

### U — unknown-like union target
Rows: unknownControlFlow 19:9, 20:9, 21:9; unknownType1 114:9.
Repro (`unk.ts`):
```ts
// @strict: true
declare let u: unknown;
let x2: {} | null | undefined = u;                 // native ok; TSR TS2322
let x3: {} | { x: string } | null | undefined = u; // native ok; TSR TS2322
```
Evidence: relater.go:274–277 has "Anything is assignable to a union containing undefined, null, and {}" (`isUnknownLikeUnionType`,
checker.go:27803). relater.rs:2522 `is_simple_type_related_to` has no such arm, and there is no port of the predicate.

### AL — aliased-condition narrowing of access-expression references
Rows: controlFlowAliasing 51:13 (f15 `obj.x`), 66:13 (f17 `obj[0]`), 219:13 and 232:13 (`this.x` in C10/C11).
Repro (`alias.ts`, `alias2.ts`, `alias4.ts`):
```ts
// @strict: true
function g(v: string | number) { const isS = typeof v === 'string'; if (isS) { let s: string = v; } }   // ok in TSR
declare const cobj: { readonly x: string | number };
function h() { const isS = typeof cobj.x === 'string'; if (isS) { let s: string = cobj.x; } }        // native ok; TSR TS2322
function f(o: { readonly x: string | number }) { if (typeof o.x === 'string') { let s: string = o.x; } } // ok in TSR
```
Evidence: the inlining arm at flow.rs:5535 matches native flow.go:386. Identifier references and discriminant references
work, and direct (non-aliased) narrowing of `o.x` works. The aliased form fails for `cobj.x` (const receiver),
`obj.x` (parameter), `obj["x"]` and `this.x`.
For f17, `is_constant_reference`'s ElementAccess arm (flow.rs:1685) accepts only a `StringLiteral` key, so `obj[0]` is
non-constant. That cause is certain. For property access the arm looks equivalent to native. Hypothesis: the re-entrant
`check_expression(receiver)` inside `is_constant_reference` during the flow walk returns a type for which
`is_readonly_property_of_type` is false. This was not verified.

### P0 — `object_against_primitive` reporter override
Rows: assignFromBooleanInterface2 17:1, assignFromNumberInterface2 22:1, invalidBooleanAssignments 18:5.
Repro (`prim7.ts`, `prim8.ts`):
```ts
// @target: es2015
declare var n: number; declare var s: string;
var a1: { toString(): string } = n;   // native ok; TSR TS2322
var a3: { length: number } = s;       // native ok; TSR TS2322
var a5: { length: number } = s as String; // ok in TSR (no primitive source)
```
Evidence: assignreport.rs:1857–1861 reports when `!not_related && !object_against_primitive(..)` is false. In other
words, an **Unknown** relater answer is still reported when `object_against_primitive` is true. assignreport.rs:2225 makes
that true for any primitive source and any target with a required property ("a number has no `a`"), without looking at the
apparent type's members. So the relater declines (Unknown) and this override turns the decline into a false TS2322.
The underlying Unknown from the apparent-type arm (relater.rs:1252) remains a separate gap.

### V — variance through an alias-mediated recursion
Rows: unwitnessedTypeParameterVariance 11:11, 23:1.
Repro (`unw3.ts`):
```ts
// @strict: true
interface CalcObj<O> { read: (origin: O) => CalcValue<O>; }
type CalcValue<O> = CalcObj<O>;
declare let cs: CalcObj<string>; declare let cu: CalcObj<unknown>;
cs = cu;                                  // native ok (contravariant); TSR TS2322
interface C2<O> { read: (origin: O) => C2<O>; }   // same without the alias: TSR matches native
type A<T> = B<T>; interface B<T> { prop: A<T>; }
declare let a: B<number>; declare let b: B<3>; b = a;   // native ok (independent); TSR TS2322
```
Evidence: when the recursion is direct (`interface B2<T>{prop:B2<T>}`, `C2`), TSR matches native. It fails only when the
recursion passes through a type alias. variances.rs:26 caches `None` for an alias whose body is not a literal/function
type, and answers an empty vector for an in-progress symbol. Native measures alias variance
(`getAliasVariances`, relater.go:1332) and records Independent/Contravariant here.

### CRET — return-mapper contextual type lost (union `R | PromiseLike<R>`, async arrow)
Rows: unionAndIntersectionInference1 80:47; contextuallyTypeAsyncFunctionReturnType 78:9.
Repro (`uii.ts`, `acr2.ts`):
```ts
interface ITest { name: 'test' }
declare function then1<R>(f: () => R | PromiseLike<R>): Promise<R>;
declare function then2<R>(f: () => R): Promise<R>;
const b = (): Promise<ITest> => then1(() => ({ name: 'test' }));   // native ok; TSR Promise<{name:string}>
const c = (): Promise<ITest> => then2(() => ({ name: 'test' }));   // ok in TSR (Promise<{name:"test"}>)
interface L { a: boolean }
declare function id<U>(f: (e: L) => U): U[];
const h: Promise<[L, number]>[] = id(async (e) => [e, 1]);        // native ok; TSR Promise<(number|L)[]>[]
const k: [L, number][] = id((e) => [e, 1]);                         // ok in TSR
```
Evidence: a plain `R` contextual return works. `R | PromiseLike<R>`, and an async arrow (whose contextual return
must be awaited per checker.go:29640–29647), do not get the instantiated return-mapper type, so the literal or tuple widens.
For `p.then(...)` TSR also folds `ITest` into the inferred `TResult1`, giving `Promise<ITest | {name:string}>`.
Native uses return-type inferences only as contextual mapping.

### K — `new` arguments without contextual type for non-class construct signatures
Rows: es2020IntlAPIs 32:62, 33:78.
Repro (`intl4.ts`):
```ts
type DT = "language" | "region"; interface Opts { type: DT }
declare const C1: { new (options: Opts): object };
new C1({ type: 'region' });        // native ok; TSR TS2322 "Type 'string' is not assignable to type 'DT'"
class K { constructor(o: Opts) {} }
new K({ type: 'region' });         // ok in TSR
declare function f(o: Opts): void; f({ type: 'region' });   // ok in TSR
```
Evidence: contextual.rs:1376–1470 NewExpression arm. It uses active inference contexts, recorded
`call_inference_signatures`/`resolved_call_signatures`, a single *generic* construct signature, or
`sole_constructor_parameters` (class). A non-generic type-literal construct signature (lib `Intl.DisplayNames`) reaches
none of these, so the object literal is checked without context. Native getContextualTypeForArgument resolves `new`
exactly as a call.

### ENUM — elaboration does not canonicalise numeric property names
Rows: numericIndexerConstrainsPropertyDeclarations 78:5, numericIndexerConstrainsPropertyDeclarations2 39:5. The native
member-level line (85:5 / 43:5) is TSR-missing.
Repro (`nie.ts`):
```ts
var b1: { [x: number]: string; } = { 2.0: 1 };  // native TS2322 at `2.0`; TSR outer TS2322 at `b1`
var b2: { [x: number]: string; } = { 2: 1 };    // TSR elaborates correctly (at the member)
```
Evidence: only the `2.0` spelling fails, and the source type prints `{ 2: number; }`. The elaboration lookup
(assignreport.rs:2481) keys the member by its written text and not by `getPropertyNameForPropertyNameNode`'s
numeric-literal canonical form.

### EXP — merged import+const re-export missing from the namespace
Rows: namespaceImportTypeQuery2 /b.ts 2:5, namespaceImportTypeQuery3 /b.ts 2:5.
Repro (`nsq3.ts`):
```ts
// @Filename: /z.ts
interface A {} export type { A };
// @Filename: /a.ts
import { A } from './z'; const A = 0; export { A };
// @Filename: /b.ts
import * as types from './a';
let n: number = types.A;     // TSR: TS2339 "Property 'A' does not exist on type 'typeof /a'"; native ok
```
Evidence: TSR has the TS2339 and probefile prints `types.A : error`. So `typeof types` lacks the value export `A`, and the
object literal then fails against it. The exact function was not isolated (export-specifier resolution of a local
symbol that merges a type-only import alias with a value).

### EOPT — exactOptionalPropertyTypes write target still flow-narrowed
Rows: strictOptionalProperties1 7:5.
Repro (`sop2.ts`):
```ts
// @strict: true
// @exactOptionalPropertyTypes: true
function f2(obj: { b: string | undefined }) {
    obj.b = 'hello';
    obj.b = undefined;   // native ok; TSR TS2322 'undefined' → 'string'
}
```
Evidence: probefile shows the second write target `obj.b : string`. members.rs:964 deliberately keeps flow narrowing for
definite assignment targets when `exact_optional_property_types` is on ("that mode keeps the earlier narrowed answer").
Native checker.go:11397 never flow-narrows a definite target. Without the option, TSR matches native (`sop3.ts`).

### W — `typeof undefined` not widened under strictNullChecks:false
Rows: widenedTypes 12:5.
Repro (`wid2.ts`):
```ts
// @strict: false
var x: typeof undefined = 3;   // native ok (any); TSR TS2322 '3' → 'undefined'
```
Evidence: declared.rs:1516 compares the checked type to `intrinsics.undefined`. In non-strict mode the symbol's type is
`intrinsics.undefined_widening` = `loose_undefined_widening` (intrinsics.rs:163/241, checker.rs:1796), which is a distinct id,
so the guard never fires and no general `getWidenedType` is applied. (probefile prints `x : any` through another
road, but the annotation check uses `get_type_from_type_node`.)

### IX — intersection source has no inferable index
Rows: intersectionWithIndexSignatures 10:1.
Repro (`iws.ts`):
```ts
type A = { a: string }; type B = { b: string };
declare let sa2: { x: A } & { x: B }; declare let ta1: { [key: string]: A & B };
ta1 = sa2;   // native ok; TSR TS2322
```
Evidence: relater.rs:2450 handles only Named/Anonymous. Native relater.go:4625–4627 handles intersection as
`every(constituents, isObjectTypeWithInferableIndex)`.

### GMI — generic mapped source vs string index target
Rows: unionTypeInference 62:11.
Repro (`uti2.ts`):
```ts
type N2<T> = { [K in keyof T]: number };
function h<T>(m: N2<T>) { const d: { [name: string]: number } = m; }  // native ok; TSR TS2322
```
Evidence: relater.go:4590 has `case isGenericMappedType(source) && targetHasStringIndex: isRelatedTo(template, value)`.
relater.rs:2377 has no such arm. The case itself (`DeepPromised<T>`) additionally needs bucket U, because its template
`T[K]` must relate to `{} | null | undefined`.

### PN — nominal class shortcut wrong when only the source has privates
Rows: privateNameDeclarationMerging 8:22.
Repro (`pn.ts`):
```ts
class D { y = 1 }
class C { #x = 1; y = 2 }       const d: D = new C();   // native ok; TSR TS2322
class E { private z = 1; y = 2 } const d2: D = new E(); // native ok; TSR TS2322
```
Evidence: unions.rs:1959 returns `Some(false)` when `source_private || target_private`. Native rejects only on a
*target* property that is private/protected and does not come from the same declaration (relater.go:4270–4290).

### DK — binding element with a union-literal computed key
Rows: controlFlowBindingPatternOrder 15:11.
Repro (`cfp2.ts`):
```ts
declare const t: readonly [9, 0, 5]; declare let k: 0 | 1;
const { [k]: b1 } = t;      // native b1: 9 | 0; TSR b1: 0 | 5 | 9
```
Evidence: destructure.rs:182 takes the exact-name path only for a single literal (`property_name_from_index`). A union
key falls to `get_applicable_index_info` (the tuple's number index). Native indexes with the key type, which distributes over the union.

### MP — mapped type over overlapping enum keys
Rows: mappedTypeOverlappingStringEnumKeys 30:7.
Repro (`mto2.ts`):
```ts
enum T { CAT = "cat", DOG = "dog" } enum A { CAT = "cat" }
type M = { [V in T | A]: { type: V } };
declare const m: M; const c = m.cat;   // TSR: c : error (native { type: T.CAT | A })
```
Evidence: probefile shows `m.cat : error`. Native resolveMappedTypeMembers (checker.go:20894) merges a second key with
the same property name into the existing symbol (keyType = union). Without that property TSR has no contextual type
for `cat`, so `TerrestrialAnimalTypes.CAT` widens to `TerrestrialAnimalTypes` (visible in the TSR message).

### RA — recursive alias self-reference in its own type argument
Rows: recursiveTypeReferences1 47:7.
Repro (`rtr.ts`, `rtr2.ts`):
```ts
// @strict: true
interface Box<T> { value: T };
type Box2 = Box<Box2 | number>;
const b22: Box2 = { value: { value: { value: 42 }}};   // native ok; TSR TS2322
declare const b: Box2; const v2 = b.value;
if (typeof v2 !== "number") { const v3 = v2.value; }   // TSR: v3 : error
```
Evidence: the inner `Box2` (the self-reference inside `Box<Box2|number>`) has no resolvable `value`, so relation fails
at depth 2. Native resolves the reference lazily to the same instantiated `Box<Box2|number>`.

### SM — string-mapping / template-intersection alias instantiation not evaluated
Rows: stringMappingDeferralInConditionalTypes 18:5.
Repro (`sm3.ts`, `smd2.ts`):
```ts
type T5<S> = Lowercase<S & string>;
let a: "f" = null as any as T5<"F">;          // native ok; TSR TS2322 'T5<"F">' → '"f"'
type T3<S> = Lowercase<`f${S & string}`>;
type C3 = T3<""> extends "f" ? 1 : 0; let a8: C3 = 1;   // native ok; TSR picks 0
```
Evidence: the direct (non-generic) form `Lowercase<`f${""}` & `${""}f`>` prints `"f"`. The generic alias instantiated
with a concrete argument is kept as an alias image that relates to nothing, so the conditional takes the false branch.
`T1<S> = `f${S & string}`` alone instantiates correctly, so the gap is the string-mapping / intersection instantiation.

### OP — compound `+=` keeps checking assignment after TS2469
Rows: symbolType12 11:1.
Repro: case line `s += ""` with `s: symbol`. TSR emits TS2469 **and** TS2322 at (11,1) (diagcase); native emits only TS2469.
Evidence: check.rs:533 routes only `is_numeric_binary_operator` through the gated arm. `+=` reaches the generic
assignment arm at check.rs:577, which calls `check_assignment_operator` with no ES-symbol short-circuit (native
checker.go:12443–12444 returns before checkAssignmentOperator).

### GM — `typeof === 'function'` on a generic mapped constituent
Rows: typeGuardOfFormTypeOfFunction 79:9.
Repro (`tgf.ts`):
```ts
function cs<S extends object>(reducer: (() => void) | Record<keyof S, () => void>) {
  let r: () => void; if (typeof reducer === 'function') { r = reducer; }  // native narrowed: () => void; TSR: Function | (() => void)
}
```
Evidence: flow.rs:8149 mirrors flow.go:685. A constituent image of exactly `Function` (the implied type) comes only
from the `is_type_subtype_of(implied, constituent)` branch, so TSR decides `Function <: Record<keyof S, () => void>`.
Native's generic-mapped-target arm needs `keyof S` related to `keyof Function`, which is false. `gm.ts` confirms TSR
also does not reject `Function` → `Record<keyof S, …>` for assignability.

### CND — Extract<T, Function> narrowing and relation
Rows: conditionalTypes2 42:15.
Repro (`ext.ts`):
```ts
function f<T>(e: Extract<T, Function>) { const g: Function = e; }          // native ok; TSR TS2322
declare function isFunction<T>(value: T): value is Extract<T, Function>;
function h<T>(x: T) { if (isFunction(x)) { const y = x; } }               // native y: Extract<T,Function>; TSR y: T
```
Evidence: two independent gaps. (a) narrowed_type_worker returns `T` (native getNarrowedTypeWorker returns the candidate
because it is a strict subtype of `T`). (b) Even with the right narrowed type, the conditional-source arm does not relate
`Extract<T,Function>` to `Function` through its constraint (relater.go:3721).

### CTUP — contextual tuple through an aliased variadic
Rows: contextualTypeWithTuple 30:5.
Repro (`ctt.ts`):
```ts
type test1 = [...number[]];
type fixed1 = test1 & { length: 2 };
let var1: fixed1 = [0, 0];  // native [number, number] ok; TSR number[] → TS2322
```
(The inline `[...number[]] & {length: 2}` works in TSR.) Evidence: probefile prints `[0, 0] : number[]`. The literal is not
in tuple context, and tuples.rs:737 depends on `is_array_like_type`, which answers false when the relation is undecided.

### EJSX — JSX children elaboration
Rows: checkJsxChildrenProperty4 file.tsx 34:10. Native reports TS2322 at each child arrow (38:15, 41:15), and TSR reports
once at `<FetchUser>`.
Evidence: only the case diff. The children are two arrow functions, the component is a class with
`IntrinsicClassAttributes`, and the native target per child is `boolean | any[] | ReactChild`. Hypothesis:
jsx_component.rs:582 `elaborate_jsx_children_elementwise` declines for function-expression children or for this target
(not verified).

## 3. Rows not fully classified
The table attributes all 73 rows to a bucket. The rows below have a confirmed effect but a hypothetical mechanism:
- controlFlowAliasing 51:13, 219:13, 232:13 (AL, property-access half). Hypothesis: `is_constant_reference`'s re-entrant
  `check_expression(receiver)` (flow.rs:1669) during the flow walk yields a type for which `is_readonly_property_of_type` is false.
- namespaceImportTypeQuery2/3 (EXP). The export-table owner was not isolated.
- checkJsxChildrenProperty4 (EJSX). The decline point inside the elaborator was not isolated.
- thisTypeInFunctions (TH). It is not established whether the receiver rebinding is missing in `get_type_of_property_of_type` or
  in the base-member image (`getTypeWithThisArgument`).
- multiline (CD). The exact desync point of the TS-mode rescan over JSX was not isolated. The effect (no directive collected
  inside JSX braces) is confirmed.
