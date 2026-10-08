# Slice D: TS2322 MISS rows, gate NEVER (220 rows, 109 cases)

Method: I read every row's source line and native message chain. For each row I found the native check site, then the TSR site that should report it, and read that TSR function for its early returns. I also used `diagcase` for the actual diagnostics, `all.tsv` for TSR's EXTRA rows near each MISS, and `probefile` for the types TSR computes. No ad-hoc diagnostics runner exists: `probefile` prints types only, and `diagcase` needs a corpus case. So the TSR-actual column of each repro comes from the corpus case it was cut from.

**Caveat on gate NEVER:** the census matches a probe only at the exact MISS position (`r5census.rs:111`). When native elaborates into a member or element, TSR's attempt at the outer node is not seen at the inner position, so NEVER there covers both "no attempt at all" and "outer attempt that never elaborated inward". Where TSR has an EXTRA at the outer node, I say so.

## 1. Table

| bucket | root cause (one line) | native anchor | TSR function | #lines | #cases | owning file |
|---|---|---|---|---|---|---|
| D1 | Whole-file `file_has_parse_errors` gate silences call, assignment and variable relation checks in files with parser diagnostics. Native has no such gate. | checker.go:8843 resolveCall; checker.go:12757 checkAssignmentOperator; checker.go:5790 checkVariableLikeDeclaration | calls.rs:737 check_call_expression_diagnostics; assignreport.rs:99 check_assignment_operator; assignreport.rs:317 check_variable_like_declaration | 25 | 6 | assignreport.rs / calls.rs |
| D2 | Destructuring-assignment relation is unported. checkReferenceAssignment's relation, the shorthand `{x = d}` default and `for await (ref of ..)` are never related (only the reference-shape checks are ported). | checker.go:12704 checkReferenceAssignment; checker.go:12597 checkObjectLiteralDestructuringPropertyAssignment; checker.go:12663 checkArrayLiteralDestructuringElementAssignment; checker.go:4032 checkForOfStatement | reference_target.rs:83 check_destructuring_assignment_targets (no relation); assignreport.rs:111 (skips `=` with a literal LHS); assignreport.rs:274 (`await_modifier` declined) | 16 | 10 | reference_target.rs / assignreport.rs |
| D3a | Generic call: a context-sensitive function argument (or an object literal containing one) declines. The instantiated-candidate check returns before relating. | checker.go:9256 isSignatureApplicable; checker.go:7484 checkExpressionWithContextualType; relater.go:641 elaborateArrowFunction | calls.rs:1728 check_instantiated_candidate_arguments; calls.rs:1495 check_single_generic_candidate_arguments | 13 | 10 | calls.rs |
| D3b | Generic call: an array-literal argument declines ("tuple-ness follows the instantiated context"). | checker.go:9256 isSignatureApplicable; relater.go:522 elaborateArrayLiteral | calls.rs:1728 check_instantiated_candidate_arguments | 9 | 5 | calls.rs |
| D3c | Generic call: an object-literal argument declines when the target mentions a literal or type variable, or for gate-undetermined reasons. | checker.go:9256 isSignatureApplicable; relater.go:498 elaborateObjectLiteral | calls.rs:1698 check_instantiated_candidate_arguments (type_mentions_literal / head_could_contain_type_variables); calls.rs:1495 | 10 | 7 | calls.rs |
| D4 | elaborateArrayLiteral refuses a UNION target. An optional parameter `T[] \| undefined` or `[..] \| undefined` therefore gets a top-level TS2345 instead of per-element TS2322. | relater.go:522 elaborateArrayLiteral (no union exclusion; getBestMatchIndexedAccessTypeOrUndefined relater.go:620) | assignreport.rs:3349 elaborate_array_literal (`TypeFlags::UNION` bail) | 28 | 6 | assignreport.rs |
| D5 | An object-literal argument against an `{..} \| undefined` (optional parameter) union: the union-literal machinery settles false and nothing is reported. | relater.go:498 elaborateObjectLiteral; relater.go:620 getBestMatchIndexedAccessTypeOrUndefined | assignreport.rs:1959 union_object_literal_failure (called from assignreport.rs:1645 report_argument_failure) | 4 | 2 | assignreport.rs |
| D6 | Elaboration of an object or array literal containing a spread declines (`None` or `false`). TSR reports the outer TS2322 instead, or nothing. | relater.go:498 elaborateObjectLiteral; relater.go:522 elaborateArrayLiteral | assignreport.rs:2497 elaborate_object_literal_members (spread returns None); assignreport.rs:3350 elaborate_array_literal (spread returns false) | 5 | 5 | assignreport.rs |
| D7 | Numeric-literal member names (`2.0:`) are not normalized to the property name (`"2"`). The elaborated member is never found. | relater.go:498 elaborateObjectLiteral (getLiteralTypeFromProperty) | assignreport.rs:2531 elaborate_object_literal_members via check.rs:13615 identifier_text (raw `NumericLiteral.text`) | 3 | 3 | assignreport.rs |
| D8 | assignment_target_type declines on: an element access with a unique-symbol key, an element access in JS, and the `arguments` identifier (no resolvable symbol). | checker.go:12757 checkAssignmentOperator; checker.go:8146 checkElementAccessExpression | assignreport.rs:1166 assignment_target_type (:1212, :1216, :1226) | 5 | 5 | assignreport.rs |
| D9 | The return-statement check declines generators and unannotated getters with a computed (non-bindable) name paired with an annotated setter. | checker.go:4086 checkReturnStatement; checker.go:4131 checkReturnExpression | assignreport.rs:982 return_type_from_annotation (`if generator { return None }`; set_accessor_parameter_annotation bindable-name requirement) | 5 | 4 | assignreport.rs |
| D10 | The yield check declines async generators and every `yield*`. | checker.go:10952 checkYieldExpression | assignreport.rs:821 check_yield_expression_assignability (:826 asterisk, :836 async) | 9 | 1 | assignreport.rs |
| D11 | The generator return-annotation check (`Generator<..>` vs written return type at the type node) has no reporter. | checker.go:2723 checkSignatureDeclaration (:2763) → checker.go:29697 checkGeneratorInstantiationAssignabilityToReturnType | none (iteration.rs:476 generator_instantiation_assignable_to_return_type is a non-reporting predicate) | 3 | 3 | iteration.rs / check.rs |
| D12 | Parameter-initializer check skips unannotated parameters (contextual type) and `?` parameters. | checker.go:5790 checkVariableLikeDeclaration | assignreport.rs:705 check_annotated_initializer (:713 `question_token` return, :719 no annotation) | 2 | 2 | assignreport.rs |
| D13a | JS-file gate on return and arrow-body checks: JSDoc `@type`/`@returns` contracts are never related in .js. | checker.go:4086 checkReturnStatement; checker.go:10206 checkFunctionExpressionOrObjectLiteralMethodDeferred | assignreport.rs:888 check_return_statement; assignreport.rs:1039 check_arrow_expression_body (`in_js_file`) | 4 | 3 | assignreport.rs |
| D13b | JS/JSDoc-typed declarations, export-default `@type` and `@satisfies` in JS: no report. Exact gate unconfirmed (see section 2). | checker.go:5790 checkVariableLikeDeclaration; checker.go:5583 checkExportAssignment; checker.go:10741 checkSatisfiesExpression | assignreport.rs:310 check_variable_like_declaration (jsdoc_type_annotation) / satisfies.rs | 8 | 8 | assignreport.rs / jsdoc_annotations.rs |
| D14 | checkMappedType's constraint ⊆ `string\|number\|symbol` check is unported. | checker.go:3385 checkMappedType | none (no `stringNumberSymbolType` relation in crates/tsr-checker) | 2 | 2 | mapped.rs |
| D15 | Static `import … with {…}` attributes are not related to `ImportAttributes`. | checker.go:5408 checkImportAttributes | none (check.rs:15178 handles only the resolution-mode override) | 5 | 2 | check.rs (or import_call.rs) |
| D16 | The argument error path does not apply getEffectiveCheckNode (skip `satisfies`) before elaboration, so TS2345 lands at the top level. | checker.go:9381 getEffectiveCheckNode (used by checker.go:9256 isSignatureApplicable) | assignreport.rs:1636 report_argument_failure → assignreport.rs:2237 elaborate_error (no Satisfies arm, no skip) | 2 | 1 | assignreport.rs / calls.rs |
| D17 | TSR runs the excess-property check before elaborateError. Native elaborates first: the member's target is read through the apparent type, so `toString` resolves to `Object.toString`. TSR reports TS2353 instead. | relater.go:430 checkTypeRelatedToAndOptionallyElaborate; relater.go:498 elaborateObjectLiteral | calls.rs:1023 check_single_candidate_arguments (check_excess_properties first) | 3 | 1 | calls.rs |
| D18 | Object-literal elaboration does not look through a `NoInfer<…>` (substitution) target, so the top-level TS2345 is emitted. | relater.go:620 getBestMatchIndexedAccessTypeOrUndefined (getIndexedAccessType unwraps substitution) | assignreport.rs:2481 elaborate_object_literal_members (get_type_of_property_of_type on NoInfer) | 1 | 1 | assignreport.rs |
| J1 | A JSX element whose tag is a JsxNamespacedName (`<a:b …>`, `<ns:element …>`) is not attribute-checked. | jsx.go:130 checkJsxOpeningLikeElementOrOpeningFragment | jsx_component.rs:62 check_jsx_component_bound (`JsxNamespacedName` return) | 4 | 2 | jsx_component.rs |
| J2 | Any hyphenated attribute name makes the whole element decline. | relater.go:719 isKnownProperty (hyphen = known); jsx.go:295 elaborateJsxComponents (skips it) | jsx_component.rs:285 check_jsx_attributes_assignable | 1 | 1 | jsx_component.rs |
| J3 | The attributes target is built through `JSX.LibraryManagedAttributes` (conditional + Defaultize mapped intersections). The context or relation declines, so no report. | jsx.go:1020 getJsxManagedAttributesFromLocatedAttributes | jsx_component.rs:258 check_jsx_attributes_assignable (jsx_attributes_context / relate_ternary != NotRelated) | 15 | 3 | jsx_component.rs / jsx_intrinsic.rs |
| J4 | Generic props: a spread of a type-parameter-typed value or an SFC/class with props `P`/`T`. The relation or context declines. | jsx.go:130; relater.go (type-parameter source vs `IntrinsicAttributes & P`) | jsx_component.rs:258 check_jsx_attributes_assignable | 8 | 6 | jsx_component.rs |
| J5 | JSX children: fragments are never attribute-checked, and the children elaboration (TS2322 at the child) declines. | jsx.go:110 checkJsxFragment / jsx.go:130; jsx.go:295 elaborateJsxComponents (children half) | check.rs:954 (no fragment arm); jsx_component.rs:444 elaborate_jsx_children (returns None) | 10 | 5 | jsx_component.rs |
| J6 | Excess or mismatched attribute against intrinsic `DetailedHTMLProps<…>` or `IntrinsicAttributes & (class/intersection)` targets: no report. Gate unconfirmed (jsx_excess_attribute or relation Unknown). | relater.go:2714 hasExcessProperties; jsx.go:295 elaborateJsxComponents | jsx_component.rs:258 check_jsx_attributes_assignable (jsx_excess_attribute / relate_ternary) | 14 | 8 | jsx_component.rs |
| U | Unclassified (see section 3). | – | – | 6 | 6 | – |

Total: 220 lines.

## 2. Per bucket: cases, repros, evidence

### D1: whole-file parse-error gate (25)
Cases: thisTypeInFunctionsNegative 86:5, 107:1, 108:1, 110–117:1, 145:1, 146:1, 148:1 (14); destructuringParameterDeclaration2 7:8, 34:6, 36:6, 37:6, 38:6, 39:11, 40:13; typeAssertions 45:2; parserRealSource7 99:13; destructuringObjectBindingPatternAndAssignment3 3:5; bigintPropertyName g.ts:31:18.

Each of these files carries a parser diagnostic in the native baseline: TS1005/1109/1128 (thisTypeInFunctionsNegative, typeAssertions), TS1005 (destructuringParameterDeclaration2 7:29, destructuringObjectBindingPatternAndAssignment3), TS1011 (parserRealSource7) and TS1434 (bigintPropertyName g.ts). `has_parse_errors = !file.diagnostics().is_empty()` (diagnostics_suite.rs:353).

Repro (native reports both errors; TSR reports only the TS1005):
```ts
// @strict: true
function a0(x: [number, number]) {}
a0([1, "s"]);        // native TS2322 at "s"; TSR silent (calls.rs:737)
let c = {a: 1;       // any parse error in the file
```
```ts
class C { n = 1 } class D { s = "" }
let c = new C; c.n = "x";   // native TS2322; TSR silent (assignreport.rs:99)
var x = (1 2);             // parse error
```
Evidence: `diagcase conformance/destructuringParameterDeclaration2` shows every TS2345/TS2322/TS2741 on calls missing (7:8, 8:4, 23:4, 34–40). TSR still emits TS1005 at 7:29, TS2300 and TS7031, so only the relation sites are dark. thisTypeInFunctionsNegative's rows are all `c.x = …` assignments (gate at :99). 86:5 is a variable declaration that `has_complete_source_variable_initializer` (assignreport.rs:343) did not certify. The variable rows (parserRealSource7, destructuringObjectBindingPatternAndAssignment3, bigintPropertyName) are classified by the same certification exception, not individually traced.

### D2: destructuring-assignment relation unported (16)
Cases: declarationsAndAssignments 138:6, 138:9; destructuringAssignmentWithDefault2 11:4; for-of48 4:10; iterableArrayPattern5 17:5; iterableArrayPattern6 17:8; iterableArrayPattern8 17:8; noUncheckedIndexedAccessDestructuring 62:2, 71:8; objectRestNegative 6:10; shorthandPropertyAssignmentsInDestructuring_ES6 38:9, 70:5, 80:5, 80:20; types.forAwait.es2018.2 10:16, 12:16.
```ts
var a: string, b: number;
[a, b] = [b, a];            // native TS2322 at 1:2 and 1:5; TSR none
let y1: string;
({ y1 = 5 } = {});          // native TS2322 at y1; TSR none
```
```ts
declare const it: AsyncIterable<number>; let z: string;
async function f() { for await (z of it) {} }   // native TS2322 at z; TSR none
```
Evidence: assignreport.rs:108–117 returns for `=` with an Object/ArrayLiteral LHS ("checkDestructuringAssignment … relates per element and never reaches this site"). reference_target.rs:83–222 ports only checkReferenceExpression's shape arms; no relation call exists there. `check_for_of_reference_assignment` returns for `await_modifier.is_some()` (:274) and for a literal LHS (:283).

### D3a/b/c: generic-call argument declines (13 + 9 + 10)
D3a (context-sensitive function): chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2 7:49, 10:35; promiseChaining1 7:55, 7:84; promiseChaining2 7:50, 7:67; circularResolvedSignature 11:9; objectGroupBy 9:49; reverseMappedTypeContextualTypeNotCircular 10:3; reverseMappedTypeIntersectionConstraint 19:7; genericCallWithGenericSignatureArguments2 37:43; intraExpressionInferences 131:5; mappedTypeInferenceErrors 16:9.

D3b (array literal): inferenceShouldFailOnEvolvingArrays 15:17, 18:22; mismatchedExplicitTypeParameterAndArgumentType 10:34; noInfer 36:14, 37:14; recursiveTypeReferences1 60:7, 66:8, 72:8; variadicTuples1 371:26.

D3c (object literal): coAndContraVariantInferences5 9:9; coAndContraVariantInferences6 34:42; infiniteConstraints 31:43, 31:63; typeArgInference2 9:16; destructuringParameterDeclaration5 49:6, 50:6; noInfer 38:15; objectLiteralNormalization 48:20, 49:14.
```ts
class Chain<T> { then<S extends T>(cb: (x: T) => S): Chain<S> { var t!: T, s!: S;
  (new Chain(s)).then(ss => t);   // native TS2322 'T'→'S' at `t` (elaborateArrowFunction); TSR none
  return null!; } constructor(public value: T) {} }
```
```ts
declare function map<T, U>(xs: T[], f: (x: T) => U): U[];
map<number, string>([1, ""], x => x.toString());   // native TS2322 at "" ; TSR none
```
```ts
declare function f<T>(...a: T[]): T; let data = { a: 1 as const };
f(data, { a: 2 });            // native TS2322 '2'→'1' at a; TSR none
```
Evidence: calls.rs:1728–1734 returns for `ObjectLiteralExpression | ArrayLiteralExpression | ClassExpression | is_context_sensitive_argument`. calls.rs:1698–1706 returns for an object literal whose target `type_mentions_literal` or could contain type variables. calls.rs:1520–1531 requires a fully instantiated resolved signature for context-sensitive calls. `probefile` shows TSR's inference is right on typeArgInference2 (`foo({ name: null }) : Item`, same as native) and on the map repro (`(x: number) => string`), so the gap is the reporter, not inference.

For destructuringParameterDeclaration5 the whole `d3(...)` call family is silent, including `d3({})` TS2741 (47:6, 48:4 missing per diagcase). The decline is therefore earlier than the object-literal arm, somewhere in calls.rs:1495; the exact gate is unconfirmed. typeArgInference2 and coAndContraVariantInferences5 are classified by shape only.

### D4: elaborateArrayLiteral refuses union targets (28)
Cases: destructuringParameterDeclaration1ES5 29:5/8/11, 32:5/15/18, 33:6/17/22/28; destructuringParameterDeclaration1ES5iterable (same 10); destructuringParameterDeclaration1ES6 32:5/8/11; optionalBindingParameters1 7:6, 7:16; optionalBindingParametersInOverloads1 8:6, 8:16; didYouMeanElaborationsForExpressionsWhichCouldBeCalled 26:5.
```ts
// @strict: true
function b1(z = [undefined, null]) {}
b1([1, 2, 3]);   // native: TS2322 ×3 at 1, 2, 3; TSR: TS2345 at the array, "...'(null | undefined)[] | undefined'"
function foo([x, y, z]?: [string, number, boolean]) {}
foo([false, 0, ""]);   // native TS2322 at false and ""; TSR TS2345 at the array
```
Evidence: TSR's EXTRA rows print exactly these union targets (all.tsv, e.g. destructuringParameterDeclaration1ES5 29:4 "parameter of type '(null | undefined)[] | undefined'", didYouMean 23:14 "'[number, number, number] | undefined'"). assignreport.rs:3349 returns `false` when the target `intersects(PRIMITIVE | NEVER | UNION)`. Native relater.go:523 excludes only Primitive|Never and resolves union members through getBestMatchIndexedAccessTypeOrUndefined. `probefile` confirms TSR types the parameter as `(z?: (null | undefined)[])`.

### D5: object literal vs `{…} | undefined` argument (4)
Cases: optionalBindingParameters2 7:7, 7:23; optionalBindingParametersInOverloads2 8:7, 8:23.
```ts
// @strict: true
function foo({ x, y, z }?: { x: string; y: number; z: boolean }) {}
foo({ x: false, y: 0, z: "" });   // native TS2322 at x and z; TSR nothing at all
```
Evidence: `diagcase` shows TSR emits only TS2463, with no outer TS2345 either. report_argument_failure takes the `union_literal` branch (assignreport.rs:1642) and returns `Settled(false)`. Which `None` inside union_object_literal_failure fires is not traced: excess_properties_verdict, find_matching_discriminant_type with an `undefined` constituent, or best_matching_type_for_object_literal.

### D6: spread inside the literal stops elaboration (5)
Cases: objectSpreadStrictNull 28:26; spreadUnion3 2:14; iteratorSpreadInArray5 14:30; literalFreshnessPropagationOnNarrowing 60:12; intersectionPropertyCheck 17:22.
```ts
// @strict: true
function f(x: { y: string } | undefined): { y: string } {
  return { y: 123, ...x };   // native TS2322 at y; TSR TS2322 at `return` ('{ y: string | number; }')
}
var array: number[] = [0, 1, ...[Symbol()]];   // native at the spread element; TSR at `array`
```
Evidence: TSR EXTRA at the outer node in four of the five cases (spreadUnion3 2:5, objectSpreadStrictNull 28:7, iteratorSpreadInArray5 14:5, literalFreshness 60:5). The code returns `None` at assignreport.rs:2497 for a SpreadAssignment and `false` at :3350 for a SpreadElement. intersectionPropertyCheck has no outer EXTRA either, because its source `T & {hi: boolean}` relation is presumably Unknown. Its attribution to D6 is by shape.

### D7: numeric-literal member names not normalized (3)
Cases: numericIndexerConstrainsPropertyDeclarations 85:5; numericIndexerConstrainsPropertyDeclarations2 43:5; stringIndexerConstrainsPropertyDeclarations 85:5.
```ts
var b: { [x: number]: string } = { 1.0: '', 2.0: 1 };   // native TS2322 at `2.0`; TSR none
```
Evidence: in the same literals, identifier- and string-named members (`b: 1`, `"e": 1`) are not missing; only `2.0:`/`3.0:` are. elaborate_object_literal_members uses `identifier_text`, which returns `NumericLiteral.text` verbatim (check.rs:13615). Source and target lookups then use `"2.0"` instead of `"2"` and `continue`.

### D8: assignment-target declines (5)
Cases: symbolProperty46 10:1; symbolProperty47 11:1; noUncheckedIndexedAccess 85:1 (unique-symbol key); lateBoundAssignmentCandidateJS3 5:9 (JS element access); parseClassDeclarationInStrictModeByDefaultInES6 6:9 (`arguments = "hello"`).
```ts
declare const s: unique symbol; declare const m: { [s]: string };
m[s] = undefined;   // native TS2322; TSR none (assignreport.rs:1216)
class C { bar() { arguments = "hello"; } }   // native TS2322 'string'→'IArguments'; TSR none
```
Evidence: assignreport.rs:1196–1218 documents the unique-symbol and JS element-access declines. For `arguments`, `resolve_name` (:1226) has no symbol, so it returns `None` (inferred from code).

### D9: return-statement declines (5)
Cases: generatorExplicitReturnType 5:5; generatorReturnContextualType 29:3, 34:3 (generators); symbolProperty47 3:9; constEnumPropertyAccess1 25:9 (computed getter with setter).
```ts
function* g(): Generator<number, boolean, string> { return 10; }   // native TS2322; TSR none
class C { get [Symbol.hasInstance]() { return ""; } set [Symbol.hasInstance](x: number) {} }  // native TS2322; TSR none
```
Evidence: assignreport.rs:997 `if generator { return None; }` ("Generators are declined"). For the getters, set_accessor_parameter_annotation (:1020) pairs only when `binder.symbol_of(getter)` is bound, which is not the case for `[Symbol.hasInstance]` or `[G.B]`.

### D10: async and `yield*` yields declined (9)
Cases: types.asyncGenerators.es2018.2 38:11, 41:12, 44:12, 47:11, 50:12, 53:12, 56:11, 59:12, 62:12.
```ts
async function* f(): AsyncIterableIterator<number> { yield "a"; }   // native TS2322; TSR none
async function* g(): AsyncIterable<number> { yield* ["a"]; }         // native TS2322 at the array; TSR none
```
Evidence: assignreport.rs:826 (`asterisk_token.is_some() → return`) and :836 (`has_async → return`).

### D11: generator return annotation not checked (3)
Cases: generatorTypeCheck6 1:17; generatorTypeCheck8 2:17; types.asyncGenerators.es2018.2 70:42.
```ts
function* g1(): number { }   // native TS2322 'Generator<any, any, unknown>'→'number' at `number`; TSR none
```
Evidence: grep finds only predicate uses of `generator_instantiation_assignable_to_return_type` (assignreport.rs:845, contextual.rs:1658, iteration.rs:476). No caller reports at the return type node, unlike native checker.go:2763.

### D12: parameter initializer vs contextual or optional type (2)
Cases: defaultArgsInFunctionExpressions 17:41; callSignatureWithOptionalParameterAndInitializer 45:32.
```ts
var f4: (a: number) => void = function (a = "") {};   // native TS2322 at a; TSR none
```
Evidence: assignreport.rs:713 returns when `question_token.is_some()`, and :719 needs a written annotation. The contextual parameter type is never used.

### D13a: JS gate on return and arrow body (4)
Cases: arrowExpressionBodyJSDoc 6:44, 13:44; jsdocBracelessTypeTag1 3:3; importTag24 19:17.
```js
// @checkJs: true  @filename: a.js
/** @returns {string} */ function f4() { return 1; }   // native TS2322; TSR none
```
Evidence: `in_js_file` early return at assignreport.rs:888 and :1039.

### D13b: JSDoc-typed JS declarations (8), gate unconfirmed
Cases: checkJsdocTypeTagOnExportAssignment4 6:16 (`/** @type {Foo} */ export default ""`); jsdocBracelessTypeTag1 20:16 (braceless `@type`); jsdocCallbackAndType 6:5 (`@callback` generic); checkJsdocSatisfiesTag15 9:20; checkJsdocSatisfiesTag8 6:5 (`@satisfies` in JS); jsdocTemplateTagDefault 9:20; jsdocTemplateTagNameResolution 10:7 (`@template` defaults and typedefs); recursiveTypeReferences2 25:7.

Hypothesis: this is a mix of JSDoc-type resolution gaps (braceless, `@callback` and `@template` typedef instantiation) and JS-only declines (export-assignment `@type`; `excess_properties_verdict` returns `None` in JS at assignreport.rs:2620). Each needs a trace. They are kept together only because all are JSDoc-annotation sites.

### D14: checkMappedType constraint (2)
Cases: bigintPropertyName q.ts:2:19; mappedTypeErrors2 15:47.
```ts
type Q = bigint; type T = { [t in Q]: string };   // native TS2322 at Q; TSR none
```
Evidence: no `checkMappedType` port and no relation against `string | number | symbol` anywhere in crates/tsr-checker (grep).

### D15: static import attributes (5)
Cases: importAssertionNonstring mod.mts 1:37, 5:37, 7:37, 9:37; importAttributes9 b.ts:7:27.
```ts
import * as m from "./mod.mjs" with { field: 0 };   // native TS2322 vs ImportAttributes; TSR none
```
Evidence: the only ImportAttributes reader is check.rs:15178 (resolution-mode). Dynamic `import()` options are related (import_call.rs:73–92), static ones are not.

### D16: `satisfies` argument not skipped (2)
Cases: typeSatisfaction_errorLocations1 5:7, 12:12.
```ts
const fn1 = (s: { a: true }) => {};
fn1({ a: 1 } satisfies unknown);   // native TS2322 at a; TSR TS2345 at the argument
```
Evidence: TSR EXTRA TS2345 at 5:5 and 12:10. Native passes `getEffectiveCheckNode(arg)` (checker.go:9381, skips satisfies and parentheses) as the elaboration node. TSR's elaborate_error (assignreport.rs:2262) has no SatisfiesExpression arm and report_argument_failure passes the raw argument.

### D17: excess check runs before elaboration (3)
Cases: objectLiteralFunctionArgContextualTyping2 11:6, 12:6, 13:17.
```ts
interface I2 { value: string; doStuff: (t: string) => string }
declare function f2(a: I2): void;
f2({ toString: (s) => s });   // native TS2322 '(s: any) => any'→'() => string' at toString; TSR TS2353
```
Evidence: TSR EXTRA TS2353 at the same three positions. calls.rs:1023 calls `check_excess_properties` before `report_argument_failure`. Native's checkTypeRelatedToAndOptionallyElaborate (relater.go:430) runs elaborateError first. getIndexedAccessTypeOrUndefined(I2, "toString") then finds the apparent `Object.toString`, the elaboration reports, and the excess-property error is never produced.

### D18: NoInfer target in elaboration (1)
Case: noInfer 39:15.
```ts
declare function foo5<T extends string>(a: T, b: NoInfer<{ x: T }>): void;
foo5('foo', { x: 'bar' });   // native TS2322 at x; TSR TS2345 "...NoInfer<{ x: \"foo\"; }>" at the literal
```
Evidence: TSR EXTRA at 39:13 printing the NoInfer target. elaborate_object_literal_members reads `get_type_of_property_of_type(target, name)` without unwrapping the substitution, unlike report_argument_failure's own `no_infer_base_type` at assignreport.rs:1692.

### J1: namespaced tag names (4)
Cases: jsxElementType 110:6, 111:19; jsxNamespacePrefixIntrinsics 16:30, 17:30. Evidence: jsx_component.rs:62–64 `if matches!(tag, JsxNamespacedName) { return; }`.
```tsx
declare namespace JSX { interface IntrinsicElements { "ns:element": { "ns:attribute": string } } }
<ns:element attribute="nope" />;   // native TS2322; TSR none
```

### J2: hyphenated attribute (1)
Case: tsxAttributeResolution7 9:2. Evidence: jsx_component.rs:285–292 returns for any attribute name containing `-`.
```tsx
<test1 data-foo={32} />   // with IntrinsicElements { test1: { "data-foo"?: string } }; native TS2322; TSR none
```

### J3: LibraryManagedAttributes targets (15)
Cases: tsxLibraryManagedAttributes 55:12, 57:41, 59:42, 69:26, 71:35, 80:38, 81:29, 98:12, 100:56, 102:57, 113:57, 122:58, 123:49; jsxCallElaborationCheckNoCrash1 10:29; jsxElementType 95:11. Every native target prints as `Defaultize<…>`, `LibraryManagedAttributes<…>` or a conditional result. TSR emits nothing for any element of tsxLibraryManagedAttributes.tsx, while other JSX cases in this slice do produce TSR attribute reports. That points to the target construction (jsx_attributes_context) or the relation over the conditional/mapped intersection answering not-NotRelated (jsx_component.rs:310 or :323). Which of the two is not traced.

### J4: generic props (8)
Cases: tsxAttributeResolution5 21:10, 25:10; tsxGenericAttributesType7 7:13; tsxGenericAttributesType8 7:13; tsxNotUsingApparentTypeOfSFC 14:14, 17:14; jsxExcessPropsAndAssignability 14:6; jsxClassAttributeResolution 2:19. In every case source or target is a type parameter (`T`, `U`, `P`) or `IntrinsicClassAttributes<T>`. Hypothesis: the relation answers Unknown, or `resolved_call_signatures` is absent for the generic tag (jsx_component.rs:311).
```tsx
function f<T extends { x: number }>(obj: T) { return <test1 {...obj} />; }   // native TS2322 'T'→'Attribs1'; TSR none
```

### J5: children and fragments (10)
Cases: jsxChildrenIndividualErrorElaborations 63:3, 73:9, 74:9; checkJsxChildrenProperty4 38:15, 41:15; jsxChildWrongType 6:5; jsxFragmentWrongType 6:28, 7:47; checkJsxChildrenProperty15 13:13, 14:13.
```tsx
const test = () => "asd";
const a = <>{test}</>;   // native TS2322 '{ children: () => string; }' at the fragment; TSR none
```
Evidence: check.rs:950–956 dispatches only opening/self-closing elements (`check_jsx_component_bound`), so fragments get no attribute check. The others reach elaborate_jsx_children, which returns `None` on any undecidable member or relation (jsx_component.rs:444, documented), and the caller then reports nothing (:330). Which `None` fires is not traced per row.

### J6: excess or mismatch vs intrinsic or intersection targets (14), gate unconfirmed
Cases: spellingSuggestionJSXAttribute 8:4, 9:4, 10:8, 11:8 (`<a class="">` vs DetailedHTMLProps from react16.d.ts); excessiveStackDepthFlatArray 35:13; contextuallyTypedStringLiteralsInJsxAttributes02 37:57, 40:44; tsxSpreadAttributesResolution14 13:38; tsxSpreadAttributesResolution6 15:10; tsxUnionElementType4 36:22; tsxAttributeResolution15 13:21; tsxStatelessFunctionComponents1 29:15, 31:15, 40:24. In tsxStatelessFunctionComponents1 the missing rows are all the `Meet` SFC, whose parameter is a binding pattern `{name = 'world'}`, plus a context-sensitive `ref={x => …}`. Its `Greet` rows are reported, so the parse-error file is not the cause there. Hypothesis: jsx_excess_attribute (jsx_component.rs:326) returns `None` against large or intersection targets.

## 3. Unclassified (6)

| row | best hypothesis |
|---|---|
| keyofIsLiteralContexualType 5:37 `let b: (keyof T)[] = ["a","b","c"]` | `probefile` shows TSR types the literal as `string[]`; native keeps `"c"` because `keyof T` is a literal contextual type (checker.go:25522 isLiteralOfContextualType). This is a contextual-literal widening gap; the outer relation then presumably declines on generic `keyof T`. |
| lastPropertyInLiteralWins 9:5 (duplicate `thunk`) | `probefile` shows TSR's literal type is correct (`{ thunk: (num: number) => void }`), yet TSR reports nothing, not even an outer TS2345. Likely the elaboration or excess path meets two same-named members and declines. Untraced. |
| indexedAccessRelation 16:25 `this.setState({ a: a })` | Native target member is `(S & State<T>)["a"] \| undefined` (a union). TSR's member type is probably the bare IndexedAccess, which elaborate_element skips (assignreport.rs:2449), and the outer pair is generic (not reportable). |
| invariantGenericErrorElaboration 4:19 `const wat: Runtype<any> = Num` | Native fails via variance or deep structural comparison of a recursive generic. TSR presumably relates it or answers Unknown with no probe at 4:19, contradicting gate NEVER for a plain variable declaration. Needs a probe (possibly a certification decline in the variable path). |
| mappedTypeProperties 40:11 `[P in 'a' \| 'b']: any` in a class *expression* | Parses as a computed name containing `P in 'a'`. TSR's check_in_expression (assignreport.rs:197) would probe if reached, and the same construct in a class declaration (line 37) is not missing. Likely the node walk does not reach computed names of class-expression members. |
| importAttributes9 b.ts:11:25 `import("./a", { with: { type: "not-json" } })` | The dynamic-import options check exists (import_call.rs:73) but did not report. The target `ImportCallOptions \| undefined` is a union with an object literal source. Either the relation answers Related/Unknown (a `declare global` augmentation of ImportAttributes in this case not merged), or the call never reaches report_relation_failure's probe. |
