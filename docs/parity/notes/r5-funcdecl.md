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
