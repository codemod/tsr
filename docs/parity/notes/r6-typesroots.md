# r6-typesroots — fourteen stale `checker_types` root clusters (`tsr-2zk.16.*`)

Round-6 parity box on epic `tsr-2zk`. Its items are fourteen root clusters
from the first types triage (`docs/parity/types-triage.md`, tree
`cfa02cf4`) that were still marked `in_progress` with no assignee. The lane
owns only the files it creates and this note. Every hook in another lane's
file ships as a measured diff under `docs/parity/notes/r6-typesroots-*.diff`,
and the query it calls lands in the box's own file under
`#[expect(dead_code)]` until the diff removes the allowance (the
r5-jsdoc3 pattern). Native source is `vendor/typescript-go` @ `5b1047d`.

## 0. Baseline and setup

Frozen base: `b18aec06` (batch BB had not landed on
`claude/beautiful-shannon-ar5gh0` at dispatch).

- types 549,853 RIGHT / 5,607 WRONG / 843 GAP of 556,303;
- diagnostics: 12,238 cases;
- Ir (`valgrind --tool=callgrind`, release `tsr`, `--singleThreaded true
  --pretty false`): domain-model 1,090,695,983; generic-imports 343,099,428.

Setup as r5-operators3 §4: PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept outside the repo.
The native probe oracle is `scripts/offline-cargo/build-tsgo.sh`'s `tsgo`
(Go 1.26.8, `Version 7.1.0-dev`). Native types were read from its
diagnostics (assign the probed expression to `never`).

## 1. Re-measurement on the frozen base

Each cluster's "finished alone" cases, then its "also blocked" cases, as
non-RIGHT type lines / total on `b18aec06`:

| Cluster | Finished-alone cases | Blocked cases | State |
|---|---|---|---|
| `.16.14` FORIN-VARIABLE-INDEX-TYPE | 6 of 7 RIGHT; keyofAndForIn 2, typeGuardsTypeParameters 4 | isomorphicMappedTypeInference 17, mappedTypes4 8 | §8 |
| `.16.16` APPEND-LOCAL-TYPE-PARAMETERS | 4 of 6 RIGHT; recursiveGenericMethodCall 2, typesWithDuplicateTypeParameters 2 | 3 cases, 11 lines | §8 |
| `.16.21` TYPE-ALIAS-ACCESSIBILITY-GATE | 0 of 5; 29 lines | 3 cases, 31 lines | §8 |
| `.16.28` LATE-BIND-INDEX-SIGNATURE-TYPE-LITERAL | 0 of 5; 6 lines | — | §3 |
| `.16.32` IMPORT-TYPE-NODE-TYPE-MEANING | 0 of 4; 23 lines | importUsedInGenericImportResolves 1 | §8 |
| `.16.34` TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE | **4 of 4 RIGHT** | intersectionTypeInference2 2 | already fixed |
| `.16.35` BINDING-ELEMENT-ALLOW-MISSING-DEFAULT | **4 of 4 RIGHT** | — | already fixed |
| `.16.36` INSTANTIATION-EXPRESSION-TYPE | 1 of 4; 11 lines | — | §8 |
| `.16.40` TYPELIT-INDEX-INFO-KEY-DEDUPE | **4 of 4 RIGHT** | — | already fixed |
| `.16.41` TYPEREF-ARITY-ERRORTYPE-COMPOSES | 0 of 4; 5 lines | — | §2 |
| `.16.43` BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS | 2 of 3 RIGHT; genericObjectRest 2 | renamingDestructuredPropertyInFunctionType 1 | §8 |
| `.16.46` ORIGIN-SLICE-GATE | 1 of 3 RIGHT; 22 lines | literalTypes2 10 | §7 |
| `.16.47` BINDING-PATTERN-IMPLIED-TYPE | 2 of 3 RIGHT; restParameterWithBindingPattern1 3 | renamingDestructuredPropertyInFunctionType 1 | §8 |
| `.16.54` TYPELIT-DUPLICATE-PROPERTY-SYMBOL-MERGE | **3 of 3 RIGHT** | — | already fixed |

`.16.35`, `.16.40` and `.16.54` are fully RIGHT, and `.16.34`'s own four
cases are too; its one remaining blocked case waits on
INFERENCE-REVERSE-MAPPED-INTERSECTION, another cluster. These four can be
closed.

## 2. `.16.41`: an arity error is `errorType`, and `errorType` composes (diff)

**Forcing constraint.** `getTypeFromClassOrInterfaceReference`
(`checker.go:23169`) and `getTypeFromTypeAliasReference`
(`checker.go:23596`) test the written argument count against
`[getMinTypeArgumentCount, len(typeParameters)]` (`checker.go:21938`) and,
outside it, report TS2314/TS2707 and return `c.errorType`. That is a
computed answer, an ordinary `any`-flagged type, and the types that
contain it are built normally: `IFoo[]` is `any[]`, `C<I>` is `C<any>`.
`get_instantiated_type_reference` (`declared.rs`) answered the port's gap
(`intrinsics.error`, ADR-0038) for every count it could not fill, inside the
window or outside it. Its per-argument guard (`if resolved == error {
return error }`) then declined the enclosing reference, so
`>c2 : C<any>` recorded `any` (the top-level gap is spelled `any` by the
variable's line, and the composed line is lost).

**Port.** `crate::reference_arity::reference_arity_answers_error_type` is
the window test: one past the last parameter without a default is the
minimum, the parameter count is the maximum. The JS leg of the class arm
never answers `errorType` (`if !isJs { return c.errorType }`), so a class
or interface referenced from a `.js` file stays on the existing fill road.
The alias arm has no JS exception. The hook
(`r6-typesroots-arity-error-type.diff`, `declared.rs`) answers
`intrinsics.native_error` (ADR-0048) before the fill decision. A count
inside the window that this port cannot fill keeps the gap, because there
upstream computes a real reference.

**Alternative rejected.** Dropping the per-argument guard instead would
make every gap argument compose into a printed reference (`C<error>`). That
is the wrong line the guard exists to prevent. The fault is the producer
answering the gap for a decided outcome, not the guard.

**Measured** (diff applied on `b18aec06`): types +31 WRONG→RIGHT, zero
losses, diagnostics unchanged. The four cluster cases go fully RIGHT
(genericArrayWithoutTypeAnnotation, genericInterfacesWithoutTypeArguments,
genericTypeReferencesRequireTypeArgs, missingTypeArguments2), plus
returnTypeTypeArguments 12, missingTypeArguments1 6,
genericsWithoutTypeParameters1 4, and the four
genericTypeReferenceWithoutTypeArgument cases 1 each. slowcases is clean.
Ir: domain-model ×1.00051, generic-imports ×1.00000.

**Falsifier.** A reference outside the window whose native line is not
`any`-composed (for example a JS class reference with too many arguments
under `noImplicitAny`) would show as a new R→W here. The unit tests in
`crates/tsr-checker/tests/reference_arity_error_type.rs` (shipped in the
diff) pin both directions, and the JS leg.

**Ships as** `r6-typesroots-arity-error-type.diff`: the `declared.rs` hook,
the removal of the query's `#[expect(dead_code)]`, the new unit tests, and
six existing assertions that recorded the gap where native answers
`errorType` (`tests/types.rs` ×5, `tests/yield_expression.rs` ×1; each
asserted `"error"` with a comment saying upstream answers `errorType`, and
`errorType` prints `any`, ADR-0048). The yield assertion's comment called
`G<number>` for `G<T, R, N>` "a slot this port cannot fill"; it is outside
the window, so native's annotation is `errorType` and the yield is `any`.

## 3. `.16.28`: a computed type-literal member that does not late-bind (diff)

**Forcing constraint.** The binder gives a member with a dynamic name an
anonymous symbol (`bindPropertyOrMethodOrAccessor`, `binder.go:978`), so it
is in no members table. `getResolvedMembersOrExportsOfSymbol` then gives it
one of three fates: it late-binds as a real member (`isLateBindableName`,
`checker.go:19961`); or it joins the `__index` symbol when its name is an
entity-name expression (`isLateBindableAST`, `checker.go:19990`) whose type
is assignable to `string | number | symbol` (`isLateBindableIndexSignature`,
`checker.go:19976`), keyed `number`, else `symbol`, else `string`
(`getIndexInfosOfIndexSymbol`, `checker.go:19676-19690`); or it is dropped.
`build_type_literal` (`declared.rs`) handled only the property half of fate
2. A METHOD whose name did not late-bind declined the whole literal
(`var v: { [e](): number }` printed `any`, `parserComputedPropertyName14`).
An unannotated property did the same (`{ [index]; }`, `propertyAssignment`).
The property half also took the object-literal key dispatch
(`computed_member_index_key`), which has no entity-name gate, so
`["" + ""]` would have keyed a `string` index where native drops it
(`computedPropertyNamesDeclarationEmit4` records `{}`).

**Port.** `crate::type_literal_index_symbols` holds fates 2 and 3
(`type_literal_computed_name`) and the method's `getTypeOfSymbol`: its
function type, plus `undefined` when optional under `strictNullChecks`
(`type_literal_computed_method_type`). The hook routes both arms of
`build_type_literal` through it. An unannotated property is the implicit
`any`, as §355 already had for named properties.

**Known deviation, kept.** Aggregation stays the literal's existing one: a
single key kind, and the union of the computed members' values only.
Native's `getObjectLiteralIndexInfo` (`checker.go:19720`) also unions the
literal's other properties into a `string` index, and numerically named
ones into a `number` index. No case in this cluster reaches that.

**Measured** (on top of §2's diff): types +7 WRONG→RIGHT, zero losses,
diagnostics unchanged. Converts propertyAssignment (2 lines),
computedPropertyNamesDeclarationEmit4_ES6 and its ES5 twin,
parserComputedPropertyName14/18/19. slowcases clean. Ir ×1.00049 /
×0.99993 against the base binary (§2's share included).

**Falsifier.** A type literal whose computed member native keeps as a real
member while this port drops it would show as a loss. The unit tests
(`tests/type_literal_index_symbols.rs`, in the diff) pin all three fates.

## 4. `.16.36`: the node builder reuses an instantiation expression's `typeof` node (diff)

**Re-measured.** Of the cluster's 11 lines, 7 are the self-referential
queries in circularInstantiationExpression and selfReferentialFunctionType
(`declare function h<T>(): typeof h<T>`). r5-instexpr §2.5 declines these
on purpose: native resolves signature returns lazily and never re-enters
`h`, while this port completes returns eagerly. They wait on lazy signature
returns, which this lane does not own. The other lines are
arrayTypeOfTypeOf's `>xs3 : typeof Array<number>`, with the instantiated
`ArrayConstructor` members printed in their place.

**Forcing constraint.** `getInstantiationExpressionType`
(`checker.go:10660`) records its node on the minted type. When that node is
a `TypeQueryNode` and `getTypeFromTypeNode(existing) == t`,
`createAnonymousTypeNodeEx` (`nodebuilderimpl.go:2816`) reuses the written
node. So a type computed from `typeof f<A>` prints as written wherever it
prints.

**Port.** `crate::instantiation_type_query_reuse` re-mints the result of a
`TypeQueryNode`'s computation with its written spelling. It copies the
property table, signatures and index infos, and marks the new type in
`alias_named_signature_types` so the site re-render keeps the text. The
identity test is kept exactly: only a result minted by this node's own
computation qualifies (not the expression type, not a pre-existing
constraint, not a union or intersection constituent), and only with no
alias frame open. An alias instantiation is a mapped copy upstream, not the
node's type. Arguments print as the builder reuses them: a nested
argument-free `typeof` as written, anything else as its type prints.

**Approximation, stated.** Upstream re-checks each entity name's
accessibility at the print site and falls back to the structural form. Text
baked at creation cannot. Every corpus print of these types is in the
declaring file. A site where the name is inaccessible would print the
written text where native expands it.

**Measured** (on top of §2-§3): types +4 WRONG→RIGHT, zero losses,
diagnostics unchanged: arrayTypeOfTypeOf 2, and
aliasInstantiationExpressionGenericIntersectionNoCrash2 2 (the alias
declared types `typeof Class<T>`, `typeof fn<T>`). slowcases clean. Ir
×1.00114 / ×0.99992 against the base binary, cumulative with §2-§3.
`tests/type_query.rs`'s
`an_instantiation_expression_instantiates_while_plain_typeof_answers`
asserted the structural `(y: string) => string` for
`var a: typeof identity<string>`. Native prints `typeof identity<string>`
(the `FnAlias` line above is the same shape), so the diff flips it.

## 5. `.16.16`: duplicate type parameters of a signature (diff)

**Re-measured.** Four of six finished-alone cases are already RIGHT. The
class and interface half (`local_type_parameters_of`'s merged-symbol dedupe)
landed in an earlier round. typesWithDuplicateTypeParameters still printed
`<T, T>() => void` for `function f<T, T>() { }`. recursiveGenericMethodCall's
2 lines (`Generator<T, any, any>` against `Generator<T>`) are the
return-annotation printing of a defaulted reference, which belongs to the
printer lane, not this cluster's root cause.

**Forcing constraint.** `SymbolFlagsTypeParameterExcludes` is
`SymbolFlagsType &^ SymbolFlagsTypeParameter` (`ast/symbolflags.go:72`). Two
same-named type parameters of one declaration therefore do not conflict in
`declareSymbolEx` (`binder.go:152`): they merge into one symbol, and the
checker reports the duplicate. `getTypeParametersFromDeclaration`
(`checker.go:19912`) appends `getDeclaredTypeOfTypeParameter(node.Symbol())`
with `core.AppendIfUnique`, so the merged parameter is listed once.
`get_signature_from_declaration` (`signatures.rs`, r6-printer) built one
type parameter per written node.

**Port.** `crate::signature_type_parameters::unique_type_parameter_declarations`
keeps the first node of each merged symbol, in order. The hook is one line
where the signature takes its type-parameter nodes.

**Measured** (on top of §2-§4): types +17 WRONG→RIGHT, zero losses,
diagnostics unchanged: typesWithDuplicateTypeParameters 2,
genericsWithDuplicateTypeParameters1 14, duplicateTypeParameters1 1.
slowcases clean. Ir ×1.00038 / ×1.00001 cumulative.

## 6. `.16.47`: a rest parameter's binding pattern is its implied type (diff)

**Re-measured.** noImplicitAnyDestructuringVarDeclaration and
declarationInAmbientContext are already RIGHT. restParameterWithBindingPattern1
printed `(...{ a, b }: any[]) => void`, and its elements were gaps.

**Forcing constraint.** `getTypeForVariableLikeDeclaration`'s final arm
(`checker.go:16790`) answers `getTypeFromBindingPattern` for any
unannotated declaration with a pattern name. Only when it answers nothing
does `getWidenedTypeForVariableLikeDeclaration` fall back, and its rest
parameter arm (`anyArrayType`) comes after that. TSR's port of the
implied-pattern arm (`symbols.rs`, §429) excluded rest parameters, so the
rest fallback won.

**Port.** The diff removes the `dot_dot_dot_token.is_none()` condition. The
admission gates for the container (declaration, or an expression shown
uncontextual) are unchanged. An array pattern's implied tuple then spreads
into the signature's parameters. Native prints `function f(...[a, b])` as
`(a: any, b: any) => void` (probed with the pinned `tsgo`), and TSR already
did, through the same tuple-rest expansion.

**Measured** (on top of §2-§5): types +6 (3 GAP→RIGHT, 3 WRONG→RIGHT), zero
losses, diagnostics unchanged: restParameterWithBindingPattern1 3,
restParameterWithBindingPattern2 1, iterableArrayPattern25 2. slowcases
clean. Ir ×1.00037 / ×1.00000 cumulative.

renamingDestructuredPropertyInFunctionType's line
(`({ a: string }: { a: any; }) => any`) is a call-signature parameter of a
type literal, and its own blocker is CLONE-BINDING-NAME. It is unchanged.

## Diffs, in apply order

Every diff applies to `b18aec06` plus this branch's commits and the diffs
above it.

1. `r6-typesroots-arity-error-type.diff` (§2): `declared.rs`,
   `reference_arity.rs`, tests. +31 types, zero losses.
2. `r6-typesroots-late-bound-index.diff` (§3): `declared.rs`,
   `type_literal_index_symbols.rs`, tests. +7 types, zero losses.
3. `r6-typesroots-typeof-reuse.diff` (§4): `instantiation_expressions.rs`
   (r6-declared), `instantiation_type_query_reuse.rs`, tests. +4 types,
   zero losses. Independent of 1-2.
4. `r6-typesroots-duplicate-type-parameters.diff` (§5): `signatures.rs`
   (r6-printer), `signature_type_parameters.rs`, tests. +17 types, zero
   losses. Independent of 1-3.
5. `r6-typesroots-rest-binding-pattern.diff` (§6): `symbols.rs` (main),
   tests. +6 types, zero losses. Independent of 1-4.
