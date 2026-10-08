# r5-funcdecl — a function declaration's type answered `error`

Lane box on epic `tsr-2zk`, pinned vendor `5b1047d`. Items `tsr-2zk.1062`
(this section, §1–§4) and `tsr-2zk.1067` (§5).

## 1. `tsr-2zk.1062`: where the `error` was minted

r5-typetriage's cause `any:function-declaration-error` is 65 WRONG lines in
34 cases (`r5-typetriage.md` §5 item 4). Every one prints `any` where native
prints a function type. The port's `getTypeOfFuncClassEnumModule`
(`symbols.rs`, not owned here) answers `error` whenever
`get_signatures_of_symbol_for_type` answers `None`, and that answers `None`
whenever `get_signature_from_declaration` (`signatures.rs`) declines any one
part of any one declaration.

### Method

A throwaway build (not committed) printed a tag at every decline inside
`get_signature_from_declaration`, `parameter_of` and the generator arm of
`return_type_from_body`, and the 34 cases were run one at a time with that
build. Tags per case, measured at `df42dc4`:

| producer | cases | of the 65, converted |
|---|---|---|
| `render_binding_pattern` declined a hole, a literal key or a computed key (§1.1, §1.2) | arrayBindingPatternOmittedExpressions, computedPropertiesInDestructuring1 (+`_ES6`), isolatedDeclarationErrorsExpressions, renamingDestructuredPropertyInFunctionType (+`3`), restParameterWithBindingPattern3, unusedParametersWithUnderscore, contextuallyTypedBindingInitializerNegative, parameterInitializersForwardReferencing1_es6:0:37; destructuringParameterDeclaration1ES5 (+`iterable`, `ES6`) render now but stay WRONG on the pattern's implied type (§4) | 22 |
| an unresolvable `typeof x` annotation declined its parameter at §929's gate (§1.3) | arguments | 1 (+2 outside the 65) |
| generator NEXT slot: a yield in a contextual position other than the written forms declined (§1.4) | generatorImplicitAny:0:43, generatorTypeCheck39 | 2 |
| another producer, not in this box's files (§4) | the remaining 40 lines | 0 |

Across the corpus the same ports convert 113 lines (§3), because the shapes
recur outside the 65: pattern names in arrows and source-map cases, and
expansion labels in `spreadParameterTupleType`.

### 1.1 Binding-pattern names: `cloneBindingName`

Native prints a pattern parameter's name with
`parameterToParameterDeclarationName` → `cloneBindingName`
(`nodebuilderimpl.go:1691`, `:1713`). The clone visits every child with
itself, drops each binding element's initializer and deep-clones the rest
(`ast/deepclone.go`). Nothing else is removed. So the printed name keeps:

- **holes** — `parseArrayBindingElement`'s all-nil element prints as nothing
  between its delimiters: `[, a, , b]`;
- **every property-name kind** — identifier, string literal with its written
  quote (`TokenFlagsSingleQuote` survives the clone), numeric literal and
  computed name: `{ ["a"]: x, 'b': y, 2: z, [foo2()]: w }`;
- **the source list's trailing comma**. The deep clone marks it (`-2` end on
  the last element, `deepclone.go:41`) and `Printer.hasTrailingComma`
  (`printer.go:4770`) reads the original list for binding patterns. Both
  `LFArrayBindingPatternElements` and `LFObjectBindingPatternElements` allow
  it. `[, a, , b, , , , s, , , ]` prints `[, a, , b, , , , s, , ,]`
  (`arrayBindingPatternOmittedExpressions`).

§48/§71 had rendered the plain shapes and declined the rest, which answered
the whole function `error`. The renderer now prints all of them. One decline
is left, and it is deliberate: a computed key whose expression is not a name,
a literal, a property access or a call. Printing an arbitrary expression needs
the emitter, which the checker crate does not depend on (`tsr-printer`).
Adding that dependency would change `Cargo.lock`, which a box may not commit.
No corpus line needs a wider form today. If one does, the fix is to depend on
the printer, not to grow this list.

A string literal's spelling repeats `node_reuse::quoted_literal`, which is
private to that module (r5-mapped4's file). The two should become one
`pub(crate)` helper when that file is free.

### 1.2 Rest expansion names: `getTupleElementLabelFromBindingElement`

Once a rest pattern rendered, `restParameterWithBindingPattern3` printed the
expansion as `[a, , , d]_0: boolean`, because both signature printers named
an unlabelled element `{parameter.name}_{index}`. Native names it in
`expandSignatureParametersWithTupleMembers` (`checker.go:27732`) through
`getUniqAssociatedNamesFromTupleType` (`:27759`) and `getTupleElementLabel`
(`relater.go:1943`). With a parameter rest symbol, that is
`getTupleElementLabelFromBindingElement` (`relater.go:1959`): an identifier
written with `...` labels `name_i`, an array pattern labels each position
from its element (a hole or a nested pattern is `arg_i`), and an object
pattern is `arg_i`.

Both printers (`signature_to_string` and `signature_to_string_at_worker`) now
ask one helper, `expanded_rest_names`. The variable-element arms (`name`,
`name_n`) are not reachable: `tuple_element_lists` holds only fixed
(required or optional) elements. This is written down at the helper. The
duplicate-label rule (`_1`, `_2`) is ported too. Before this change, a tuple
with two equal labels printed both unsuffixed.

The contextual copy of the same expansion (`contextual.rs`
`expand_contextual_tuple_rest`) still uses `restName_i`. That file is not
owned here; it is listed in §4.

### 1.3 `typeof x` written annotations

§929 keeps a parameter whose annotation answers `errorType`. Native's
`pseudoTypeEquivalentToType` answers true for it and the written node is
reused. The port gates that charity on `written_type_text` being able to spell
the annotation. The gate exists because `get_type_from_type_node`'s
mapped/conditional arm *mints* `error` when its bounded renderer refuses, and
that is not upstream's `errorType`. `written_type_text` has no `TypeQuery`
arm, so `m(args: typeof arguments)` declined the parameter and the method's
type answered `error`.

A type query resolves by name lookup. Its failure is upstream's
`errorType`, never a port-minted one, so the gate now admits a type query
without type arguments. The reuse printer already spells `typeof a.b`. Adding
the arm to `written_type_text` itself was measured and rejected (§3).

### 1.4 Generator NEXT slot: `getContextualType` at every yield

`checkAndAggregateYieldOperandTypes` (`checker.go:20322`) appends
`getContextualType(yieldExpression, ContextFlagsNone)` to the next aggregate
for every yield that has one. The port read only the written forms: an
annotated variable, an assertion, an assignment target. Every other
contextual position declined the whole signature. The arm now asks the
general `get_contextual_type` at the outermost parenthesis (the node native's
parenthesis delegation asks about) and records a non-error answer. A `None`
still declines, because the general road's `None` does not tell native's nil
apart from a position the port does not model.

Known divergence, not hidden: for a yield passed to a *generic* call
(`f2(yield 1)` with `f2<T>(x: T): T`), the port's argument context answers
the uninstantiated parameter. The signature now prints
`Generator<number, void, unknown>`. Native resolves the call, infers
`T = any` from the yield's `any`, and prints `Generator<number, void, any>`.
That line moved from WRONG (`any`) to WRONG (the wrong slot). The producer is
`contextual_type_for_argument` (`contextual.rs`, not owned), §4.

## 2. Tests

`crates/tsr-checker/tests/funcdecl_signatures.rs`: holes and the trailing
comma, literal and computed keys, pattern-rest expansion labels, and an
unresolved `typeof` annotation.

Two existing unit tests used `{ "value": value }`, a string-literal key, as
their example of a parameter the port *cannot* render. Native renders it, and
now so does the port, so those tests' premise moved and not their subject.
The fixture becomes `{ [a + b]: value }`, a computed key that
`cloned_expression_text` still declines:

- `signatures.rs` `unreadable_interface_signatures_are_not_empty_or_partial_candidates`
  is changed in this commit;
- `members.rs` `class_module_copy_members_filter_after_source_shadowing` is
  in a file this box may not edit. The one-line change is
  `r5-funcdecl-members-fixture.diff`. Until it lands, that one test fails, by
  design. With it applied, `cargo test --workspace --release` passes (312 test
  binaries).

## 3. Measurement (at the landing commit, base `df42dc4`)

Both dumps unfiltered, compared with `cut -f1,2`:

- types: 544,661 → 544,774 RIGHT (+113: 67 WRONG→RIGHT, 46 GAP→RIGHT),
  2 GAP→WRONG, 0 RIGHT→non-RIGHT;
- diagnostics: no verdict changed;
- the 2 GAP→WRONG are `declarationEmitComputedNameCausesImportToBePainted:1:7/8`.
  The pattern `({ [Key]: value }: Context)` now renders, and it shows that the
  binding element `value` (a `unique symbol` computed key) types `error`. The
  arrow's inferred return is that `error`, and it prints `any` where native has
  `string`. The renderer's old decline was hiding a producer in binding-element
  typing. That producer is not in this box's files (§4).

Rejected on measurement: a `TypeQuery` arm in the shared `written_type_text`
walk. It fixed `compiler/arguments` too, but it also changed written-form
reuse at that walk's other callers: `declarationEmitNestedGenerics` went GAP
to WRONG (`typeof x extends …`, where native serializes the query as `T`), and
WRONG text moved in four more cases. The landed version admits a type query
only at `parameter_of`'s §929 gate, which is the decision this cause needed.

Rejected on measurement: the checker's `getUniqAssociatedNamesFromTupleType`
(`checker.go:27759`) for printed names. It suffixes every duplicate
(`s_1, s_2`). The printer's copy (`nodebuilderimpl.go:1917`) keeps the first
occurrence (`s, s_1`). `spreadParameterTupleType` records the printer's
spelling.

Performance against the frozen base binary: median child CPU new/old is
domain-model 1.002 (41 samples; 1.07 at 21) and generic-imports 0.97
(21 samples), with `diagnostics_match` true. Callgrind Ir
(`--singleThreaded`): domain-model 1,193,706,276 → 1,193,395,692, and
generic-imports 342,896,776 → 342,903,978.

## 4. Remaining in the cause, and changes outside owned files

Producers that were not in this box's files, from the same tagged run:

- **`parameter_of` symbol road, no annotation, symbol type `error`**:
  `isolatedDeclarationsAddUndefined` (2), `parameterInitializersForwardReferencing1_es6:0:8`.
  `get_type_of_symbol` for the parameter answers `error` (`symbols.rs`).
- **pattern parameter with a contextual type that cannot be shown absent**:
  `contextuallyTypedParametersWithInitializers1:0:188`;
  `expressionsForbiddenInParameterInitializers` (`typeof import(...)`
  annotation answers `error`).
- **type parameter with a variance modifier** (`<in T>`, `<out T>`):
  `varianceAnnotations` (2). `type_parameter_of` declines modifiers, because
  `getTypeParameterModifiers` reads every declaration of the symbol.
- **async / async-generator / import-type returns**:
  `crashInYieldStarInAsyncFunction`, `asyncFunctionDeclaration15_es6`,
  `importTypeGeneric` (2), `varianceAnnotations:0:142`.
- **generator NEXT slot from a binding pattern** (`const [a = 1, b = 2] = yield`):
  `getContextualTypeForInitializerExpression`'s pattern arm
  (`getTypeFromBindingPattern(..., includePatternInType=true)`) is not in
  `contextual.rs`'s `get_contextual_type`. Cases: generatorReturnTypeInference(+NonStrict):117.
- **overloaded callee argument context** (`f1(yield 1)` with two overloads):
  `contextual_type_for_argument` answers `None`. Cases: generatorReturnTypeInference(+NonStrict):53.
- **generic callee argument context** (`f2(yield 1)`): answers the
  uninstantiated `T`'s constraint, not `any` (§1.4). Cases: generatorReturnTypeInference(+NonStrict):61,
  generatorImplicitAny:39.
- **not signature construction at all** (class/namespace `typeof` and
  `complexRecursiveCollections`' overload sets): `defaultDeclarationEmitNamedCorrectly`,
  `decoratorOnClass7.es6`, `esmModuleExports1/3`, `multipleExportDefault5`,
  `typeofAnExportedType`, `functionAndInterfaceWithSeparateErrors`
  (a function merged with an interface: `symbols.rs`' members test),
  `thisPredicateInObjectLiteral`, `complexRecursiveCollections` (8).
- **binding element typed through a `unique symbol` computed key**
  (`({ [Key]: value }: Context) => value`): `value` types `error`, so the
  arrow's inferred return prints `any` where native has `string`.
  `declarationEmitComputedNameCausesImportToBePainted:1:7/8`, now GAP→WRONG,
  because the pattern renders (§3). Producer: binding-element typing
  (`binding_patterns.rs` / `getTypeOfDestructuredProperty`).
- **a pattern parameter's implied type with holes**
  (`function f(z, y, [, a, b], { p, m: { q, r } })`): the implied type is
  `any` where native builds `[any, any, any]` and
  `{ m: { q: any; r: any; }; p: any; }`
  (`getTypeFromBindingPattern`, holes are `anyType` elements).
  destructuringParameterDeclaration1ES5 (+`iterable`, `ES6`):72.
- **`contextual.rs` `expand_contextual_tuple_rest`** names expanded
  elements `restName_i`. It should call the same
  `getTupleElementLabel` port (§1.2) so contextual signatures name pattern
  rests as native does.

### 4.1 Correction to the landing commit's message

`4c9d6b66`'s message lists cases as "converted (all lines RIGHT)". That holds
for arrayBindingPatternOmittedExpressions, spreadParameterTupleType,
arguments, unusedParametersWithUnderscore, isolatedDeclarationErrorsExpressions,
contextuallyTypedBindingInitializer(+Negative), generatorTypeCheck39/61 and
declarationEmitDestructuring5. It does **not** hold for
computedPropertiesInDestructuring1(+`_ES6`) (10 other lines each),
renamingDestructuredPropertyInFunctionType(+`3`) (1 each),
restParameterWithBindingPattern3 (1: the `Array` member list at `:0:21`) or
coAndContraVariantInferences3 (5). In those cases this lane's target lines
converted and other lines did not. The line counts in §3 are measured and
stand.

## 5. `tsr-2zk.1067`: union reduction matched literals by full relation

`removeStringLiteralsMatchedByTemplateLiterals` (`checker.go:25857`) drops a
string literal from a union when a pattern template beside it matches it. The
template arm is `isTypeMatchedByTemplateLiteralOrStringMapping` (`:25874`) →
`isTypeMatchedByTemplateLiteralType` (`relater.go:2332`) with
`compareTypesAssignable`. That is the direct matcher: infer the
placeholders from the template's literal parts, then check each placeholder
on its own (`isValidTypeForTemplateLiteralPlaceholder`, `relater.go:2476`,
whose `string` target short-circuits). The port called
`is_type_assignable_to(literal, template)` instead. That reaches the same
matcher through the relater's template-target arm, but it pays a top-level
relation setup per pair. r5-harness measured 11.7 M of them in
`templateLiteralTypes1` (`r5-harness.md` §4).

Ported as `Checker::is_type_matched_by_template_literal_type` in
`templates.rs`, next to `isValidTypeForTemplateLiteralPlaceholder` with the
assignable comparer. The relater keeps its own copy (`relater.rs`
`valid_template_placeholder`), because inside an open relation native passes
`r.isRelatedToWorker` and not a fresh top-level check.

**Cache/work boundary** (`docs/conventions.md`, checker ports). No cache is
added. The pinned operation is `isTypeMatchedByTemplateLiteralType`. It has
no memo natively; per pair it runs one `inferFromLiteralPartsToTemplateLiteral`
and one comparison per placeholder. The union reduction reads each
template's parts once and matches every literal against the borrowed parts
(`is_type_matched_by_template_literal_parts`). Native passes the template by
pointer. The first draft copied the parts out of `template_literal_parts` per
pair, and that copy was the next hotspot (gdb stack samples: all in
`TemplateLiteralParts` clone/drop under the matcher).

Measured: `templateLiteralTypes1` with `--strict --declaration --noEmit
--singleThreaded true --target es2015`, three runs each, release builds:

| binary | wall |
|---|---|
| base `df42dc4` | 34.3 / 35.2 / 36.0 s |
| item 1 (`4c9d6b66`) | 34.3 / 33.8 / 33.9 s |
| matcher, parts copied per pair | 4.6 / 4.8 / 4.9 s |
| matcher, parts read once (landed) | 2.0 / 2.0 / 2.0 s |

The diagnostics output is byte-identical across all four. Both dumps skip this
case as a known divergence (`r5-harness.md` §4), so the zero-loss gate does
not see it. The other corpus users of this reduction are measured by the
dumps in the commit message.

How this would be wrong: a union whose literal is matched by the relater's
template arm but not by the direct matcher, or the reverse. That would show
as a union-printing transition in the types dump. The landing commit records
none.

Gates at the landing commit, against item 1's dumps and the frozen base:
both dumps are identical to item 1's (verdicts and printed text), so they are
0 losses with no transitions. Callgrind Ir (`--singleThreaded`) against base
`df42dc4`: domain-model 1,193,706,276 → 1,193,328,321, and generic-imports
342,896,776 → 342,886,620. Both are lower. Median child CPU against the base
binary read 1.031 (21 samples) and 1.037 (41) on domain-model, and 1.024 on
generic-imports. Two controls on the same box, 41 samples each, show this is
slot noise and not the change. The base binary against a copy of itself reads
**0.975**. Item 2 against item 1 directly reads **0.967**. The per-slot bias
on this container is about ±3%, and the deterministic Ir is the tie-breaker.

## 6. `tsr-2zk.16.70`: GROUNDED refused type parameters native never re-binds

Added by the integrator after §1–§5, from r5-shapes' split
(`r5-shapes.md` §2.5, branch `claude/beautiful-shannon-ar5gh0-r5-shapes`). It
covers all 8 `any:function-expression-error` cases.
`get_type_of_function_expression` answers `error` for a function expression
with an unannotated parameter unless its contextual signature is
**grounded**: no contextual parameter type may mention a type parameter
(§192, §137). The gate is this port's own. Native
`checkFunctionExpressionOrObjectLiteralMethod` (`checker.go:9077`) always
answers. The gate was added because an uninstantiated contextual signature
typed arrows confidently wrong (generatedContextualTyping's 48 G→W).

The hazard is narrower than the test. `instantiateContextualSignature` maps a
contextual signature through the call's inference context, and that mapper
binds only the *called signature's own* type parameters. A type parameter
declared by a declaration enclosing the arrow (the generic function or class
it is written in) is a fixed type there, and native prints it:

- `(x) => f(g(x))` under `(r: U) => S` inside a generic arrow:
  `(x: U) => S` (`contextualSignatureInstantiation2`);
- `a.forEach(x => …)` in `map<A, B>`: `(x: A) => void`
  (`inferFromGenericFunctionReturnTypes1` ×3);
- `xs.forEach(x => ys.push(f(x)))` in `map<T, U>`: `(x: T) => number`
  (`mismatchedExplicitTypeParameterAndArgumentType`);
- `return function*(state) {…}` under `(a: T) => …`:
  `(state: T) => Generator<…>` (`generatorTypeCheck62/63`).

GROUNDED now asks `mentions_type_parameter_out_of_scope`. It is the same
walk as `mentions_any_type_parameter` (`contextual.rs`, not owned), but it
exempts a type parameter whose declaring node encloses the arrow, *unless*
that type parameter belongs to a signature of the callee at the arrow's
argument position, because the call re-binds it (a recursive call to the
enclosing function, for one). An unresolvable callee keeps the decline.

Measured against item 2's dumps: types +18 RIGHT (15 WRONG→RIGHT,
3 GAP→RIGHT), 0 RIGHT→non-RIGHT; diagnostics `restTuplesFromContextualTypes`
WRONG→RIGHT and nothing else moved. Tests pass (with the §2 fixture diff).
Ir: domain-model 1,193,328,321 → 1,193,338,729, generic-imports
342,886,620 → 342,904,733. Median CPU against item 2: 1.010 and 0.989.

One GAP→WRONG: `esDecorators-contextualTypes.2:0:56`. There
`return function (this, ...args) {…}` under
`(this: This, ...args: Args) => Return`, with both type parameters in scope,
prints `(this: any, ...args: any[]) => Return`. The gate was hiding a
contextual *parameter* typing producer: an unannotated `this` parameter and a
rest parameter whose contextual type is a type parameter both type `any`
(`assignContextualParameterTypes`, in `contextual.rs`/`symbols.rs`, not
owned). `restTuplesFromContextualTypes:0:250` shows the same producer as
WRONG→WRONG (`(...x: [x: number, ...args: T])`).

Still declined, as the brief's mechanism predicts (callee type parameters,
which need the call's inference mapper at the arrow):
`genericCallAtYieldExpressionInGenericCall1`,
`partiallyAnnotatedFunctionInferenceError` ×3. `tsxInArrowFunction` is a
different producer: its context is a JSX child, which mentions no type
parameter.
