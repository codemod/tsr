# r5-jsdoc4 — JSDoc walk coverage, heritage constraints, catch clauses (round 5)

Lane `tsr-2zk.1075`, epic `tsr-2zk`. Native source is `vendor/typescript-go`
@ `5b1047d`. Continues [r5-jsdoc3](r5-jsdoc3.md) (its §5 is this lane's item
list), [r5-jsdoc2](r5-jsdoc2.md) and
[ADR-0046](../../adr/0046-jsdoc-reparse-is-a-checker-query.md).

Frozen baseline (`f5d0291`, batch AB landed with r5-jsdoc3's seven diffs):
types **545,044 RIGHT** of 552,533 aligned lines (906 GAP, 6,583 WRONG);
diagnostics **5,385 RIGHT / 5,584 EMPTY_RIGHT** of 12,238 rows.

Perf is judged by Callgrind Ir (`tsr -p <project> --singleThreaded --pretty
false --noEmit`). Three runs of the base binary on domain-model read
1,196,991,723 / 1,196,992,447 / 1,197,691,013: a 0.06% spread, which is the
noise floor every Ir figure below is read against.

## 1. Walk coverage: every reparsed type node native checks (item 1)

**Forcing fact.** r5-jsdoc3 §3 landed `check_node`'s JSDoc hook
(`jsdoc_reparsed_type_nodes`) for a variable's, class property's and
parameter's own type, a function's `@returns`, and typedef bodies. Its module
doc listed what native's `checkSourceElement` also reaches and the hook did
not: casts, `@satisfies`, `@this`, a full-signature `@type`, `@callback`,
`@overload`, and export/accessor `@type`.

**Port** (`jsdoc_checks.rs`), each node as `reparser.go` exposes it:

| Reparsed node | Native reader | Rule visited |
|---|---|---|
| `@type` on a parenthesized expression or `return` → `AsExpression` (`makeNewCast`, `reparser.go:378`) | `checkAssertion` → `checkSourceElement(type)` (`checker.go:12302`) | each typed `@type` of the host's **last** comment (every one wraps again) |
| `@satisfies` → `SatisfiesExpression` (`reparser.go:396`) | `checkSatisfiesExpression` (`checker.go:10743`) | each typed `@satisfies` of the last comment, on the hosts `jsdoc_satisfies_target` already answers for the relation check |
| `@this` → a prefixed `this` parameter (`reparser.go:487`) | `checkSignatureDeclaration` → `checkParameter` | its type; **and** `checkParameter`'s `this` arm: TS2681/TS2730/TS2784 at the tag name (the parameter's location) |
| full-signature `@type` (`FullSignature`) | `checkFunctionOrMethodDeclaration` (`checker.go:10143`), `checkFunctionExpressionOrObjectLiteralMethod` (`:3441`) | the type node |
| `@callback` → `JSTypeAliasDeclaration` of a function type (`reparser.go:100`) | `checkTypeAliasDeclaration` | the function type the parser already builds (`parse_callback_tag`) |
| `@overload` → a `FunctionDeclaration`/`MethodDeclaration`/`ConstructorDeclaration` per tag (`reparser.go:134`) | `checkFunctionDeclaration` → `checkSignatureDeclaration` | each signature's `@this`, `@param` and `@return` types, for a function/method/constructor host outside every object literal |
| export assignment `@type` (`reparser.go:356`) | `checkExportAssignment` (`checker.go:5662`) | the type node |

An accessor's own `@type` was already covered: the replay's getter arm makes
it the function's `return_type`.

`JSDocReparsedFunction` gained `this_tag` (the first `@this` that prefixes a
parameter), set where the replay already tracked `ThisParameter::Reparsed`.
`jsdoc_satisfies_target` (`jsdoc_annotations.rs`) became `pub(crate)` so the
walk asks the same host rule the relation check uses; no behaviour change.

**Judgment calls.**

- *Full signature and export `@type` are visited with the whole walk.*
  Native only `getTypeFromTypeNode`s them, which reports name resolution and
  arity but not `checkTypeReferenceNode`'s TS2344. This port reports
  TS2304 only from the walk (`check_type_reference_name`), so visiting is the
  only road to the resolution errors; the over-report is a constraint error
  inside one of these two positions. No corpus case moved either way.
  Falsifier: a case with a TS2344 inside a function's full-signature `@type`.
- *Casts follow native's hosting, not the typing road.* The walk takes each
  typed `@type` of the last comment; `jsdoc_cast_annotation` (the type the
  paren answers) takes the first typed `@type` of any comment. They differ
  only for two typed `@type` tags, or a non-last comment carrying one.
- *Overload signatures' `@template` tags and every function's `@template`
  constraints are not visited* (nor `@implements`, `@import`, typedef
  `@property` types): no corpus row needs them, and the function-level
  `@template` walk would also need TS2706/TS2744 (`jsdocTemplateTagDefault`),
  which the TypeScript path computes in a different file.
- *The object-literal rule* is `parsingContexts`' `PCObjectLiteralMembers`
  bit, which stays set through every list parsed inside an object literal;
  "any `ObjectLiteralExpression` ancestor" is the same set of hosts.

**Measured** (unfiltered, against `f5d0291`):

- diagnostics **5,385 → 5,386 RIGHT** (thisTag3); EMPTY_RIGHT unchanged;
  checkJsTypeDefNoUnusedLocalMarked stays WRONG but loses its wrong TS6133
  (the cast now references `FooFun`; the remaining TS2352 is the deferred
  cast check, `expressions.rs`);
- types unchanged (545,044); zero RIGHT→non-RIGHT on both dumps;
- Ir: domain-model 1,197,819,796 / 1,197,767,845 (base 1,196.99–1,197.69 M,
  +0.01–0.07%, inside the base's own spread), generic-imports 342,898,976
  (base 342,914,464). Nothing new runs on the TypeScript path: the hook is
  gated on `file_is_js`.
- `crates/tsr-checker/tests/jsdoc_reparsed_checks.rs`: nine tests; six fail
  without the change.

**Not covered, with the piece they wait on.**

- `jsdocResolveNameFailureInTypedef`, `reuseTypeAnnotationImportTypeInGlobalThisTypeArgument`:
  a typedef in the comment before end of file. The parser parses that
  comment but does not attach it (`parse_source_file`, "not yet attached");
  native's `withJSDoc(eof, …)` hosts it on the end-of-file token. Parser.
- `jsFileMethodOverloads3`, `overloadTag1/2`: TS7012/TS2394 come from the
  overload *signatures*, which `signatures.rs` does not build (`.16.163`).
- `typedefMultipleTypeParameters` TS2314: the arity rule
  (`type_argument_arity.rs`) does not count a typedef's `@template`
  parameters.

## 2. Heritage type-argument constraints (item 2, extendsTag5)

**Forcing fact.** `check_type_argument_constraints` (`constraints.rs`) took
`TypeReferenceNode` only, so TS2344 never fired on a heritage clause — in
TypeScript as well as JS. Native reaches `checkTypeArgumentConstraints` from
three heritage sites:

- `checkClassLikeDeclaration` (`checker.go:4327`): the class base's
  arguments, against each constructor `getConstructorsForTypeArguments`
  keeps;
- `checkTypeReferenceNode` on each `implements` element (`:4371`) and each
  interface `extends` element (`:5022`).

In JS the base's arguments may come from `@augments`/`@extends`:
`reparseHosted`'s `KindJSDocAugmentsTag` arm (`reparser.go:589`) clones them
onto an `extends` element that writes none, which
`jsdoc_augments_type_arguments` (`type_argument_arity.rs`) already answers.

**Port** (`constraints.rs`). `check_type_argument_constraints` dispatches an
`ExpressionWithTypeArguments` to `check_heritage_type_argument_constraints`;
the argument-against-constraint loop moved unchanged into
`check_type_argument_constraints_of(symbol, arguments)`, which both arms call.
`constraint_check_type_parameters` also reads a JS class's `@template`
parameters (`jsdoc_class_template_parameters`, `reparser.go:459`), which a
JS class declares in place of a written list.

**Judgment calls.**

- *A class base is checked against the class's own type parameters.* Every
  construct signature of a class carries the class's type parameters, so
  `getConstructorsForTypeArguments` over a class base checks the same list
  once per signature and stops at the first failure. A base that is not a
  class (a `var` with construct signatures) declines: its signatures are not
  modelled here. The base must resolve as a **value** to a class, the same
  test `type_argument_arity.rs` makes (`class_extends_entry_names_a_class`).
- *Only identifier expressions.* A qualified base (`extends ns.A<X>`)
  declines, as the arity rule's value test does.
- *No new decline in the relation.* The heritage arm reuses every gate the
  type-reference arm has (generic arguments, undecidable sides, reportable
  pairs).

**Measured** (on §1, unfiltered):

- diagnostics **5,386 → 5,393 RIGHT** (+7, all TypeScript):
  genericTypeConstraints, subtypingWithNumericIndexer2/3/4,
  subtypingWithStringIndexer2/3/4. Types unchanged. Zero losses.
- extendsTag5 does **not** convert on this commit: the `@augments` type
  literal's members are unbound (§2.1).
- **Ir: domain-model +1.9 M (+0.16%)**, 1,199,725,707 vs §1's
  1,197.8 M; generic-imports flat (342,897,414). The whole difference is
  inside `check_type_argument_constraints` (inclusive 56.31 M → 58.34 M):
  domain-model declares 40 classes `extends Service<ModelNN, "modelNN">` over
  `Service<T extends Entity<B>, B extends string>`, and each now relates
  `ModelNN` to `Entity<"modelNN">` — about 50 k Ir per class, work tsgo's
  `checkClassLikeDeclaration` does for the same declarations. Median child
  CPU: domain-model 1.032 at 21 samples, **1.018 at 41**; generic-imports
  1.009; `diagnostics_match` true.
- `crates/tsr-checker/tests/heritage_type_argument_constraints.rs`: class
  base, interface base, `implements`, and a satisfying base.

**Falsifier.** A TypeScript case whose base is a constructor-typed `var`
with several construct signatures of different type-parameter counts: native
checks each kept signature, this arm declines.

### 2.1 The `@augments` arguments are unbound (binder; diff)

**Finding.** With §2 in place extendsTag5 still reported nothing: the
relater answered `Unknown` for `{ a: string; b: string }` against `Foo`. The
binder binds the type expressions of `@type`, `@param`, `@returns`, `@this`,
`@satisfies` and typedefs (`bind_jsdoc_declarations`), but not
`@augments`/`@implements`, so the argument's type-literal members carry no
symbols. Native's arguments are deep clones placed in the class's heritage
clause (`reparser.go:563`, `:589`) and bind like written ones.

**Diff** `r5-jsdoc4-augments-binding.diff` (binder): bind each type argument
of an `@augments`/`@implements` tag's class name, plus the walk's arm
(`jsdoc_checks.rs`, `checkSourceElements(baseTypeNode.TypeArguments())`,
`checker.go:4316`) that visits the `@augments` arguments the `extends`
element takes. Measured in §2.2.

### 2.2 Measured

On §2's commit, unfiltered:

| Set | diagnostics RIGHT | types RIGHT | losses |
|---|---|---|---|
| §2 | 5,393 | 545,044 | — |
| walk arm alone (committed) | 5,393 | 545,044 | none; no row changes |
| walk arm + `r5-jsdoc4-augments-binding.diff` | **5,394** (extendsTag5) | **545,048** (extendsTag5 ×4) | none |

Ir with the walk arm: domain-model 1,199,698,811, generic-imports
342,891,775 (§2: 1,199,725,707 / 342,897,414); the arm runs only behind the
hook's `file_is_js`. The binder diff binds JS comments only.

**Landing.** Apply `r5-jsdoc4-augments-binding.diff` on this lane's commit;
it is independent of everything else here.

### 2.3 Not done: `unmetTypeConstraintInJSDocImportCall`

Native reaches TS2344 through `checkTypeReferenceOrImport` (`checker.go:2998`)
for an `ImportType` with type arguments, gated on
`!isErrorType(getTypeFromTypeNode(node))`. TSR's
`get_type_from_import_type_node` (`declared.rs`, r5-declared2's) answers
`error` for **every** import type that writes type arguments ("written type
arguments … the instantiated print is its own row"), so the gate can never
pass; the TS twin `unmetTypeConstraintInImportCall` is WRONG for the same
reason. Reaching the check without the type would be a guess at
`getTypeFromImportTypeNode`'s answer, which §3a of the box protocol rules
out. Needs: `getTypeReferenceType` over the import type's symbol with its
written arguments in `declared.rs`; then an `ImportType` arm in
`check_type_argument_constraints` (the qualifier's symbol, the written
arguments, `check_type_argument_constraints_of`). Its argument `T` is a bare
type parameter, which `bare_type_parameter_argument_is_decidable` already
admits for a `TypeReferenceNode` argument.

## 3. Catch clauses (item 3, jsdocCatchClauseWithTypeAnnotation)

The case wants TS18046 ×2, TS2492, TS2339 ×2 beyond the TS1196 rows
r5-jsdoc3 landed. Its TypeScript twin `catchClauseWithTypeAnnotation` is
WRONG on exactly the same rows, so none of them is a JSDoc question.

### 3.1 TS2492 (ported)

**Native.** `checkCatchClause` (`checker.go:4247`): for an unannotated catch
variable without an initializer, each name the clause declares
(`node.Locals()`) that the catch block's locals hold as a block-scoped
variable is TS2492 at that variable's value declaration
(`grammarErrorOnNode`, the declaration's name). TSR had no TS2492 at all.

**Port.** `check_catch_clause_block_redeclarations` (`grammar.rs`), called
from `check_catch_clause_declaration`'s new `else` arm — the third branch of
the same `if typeNode / else if initializer / else` native writes. The two
locals tables are the binder's (`locals(catch_clause)`,
`locals(block)`); no new table.

**Measured** (on §2.2's committed walk arm, unfiltered): diagnostics
**+1 RIGHT** (redeclareParameterInCatchBlock); catchClauseWithTypeAnnotation
and jsdocCatchClauseWithTypeAnnotation gain their TS2492 row and stay WRONG
on §3.2's rows. Types unchanged; zero losses. Ir domain-model 1,199,769,056, generic-imports
342,898,629 (flat against §2.2).
`crates/tsr-checker/tests/catch_clause_redeclaration.rs`: four tests.

### 3.2 Not done: TS18046 and TS2339 on `unknown`

- **TS18046** (`err.foo` with `err: unknown`): `checkNonNullTypeWithReporter`
  (`checker.go:7413`). TSR's `check_non_null_type_reporting`
  (`nullable_operand.rs`) deliberately does not report it: a
  context-sensitive arrow parameter is read as `unknown` by the
  diagnostics walk while inference answers `number`, and reporting cost two
  EMPTY_RIGHT cases (`mapGroupBy`, `nonInferrableTypePropagation2`). The
  property-access path (`members.rs`, main) reports nothing on an `unknown`
  receiver either. Unblocking it is the inference cache, not this lane.
- **TS2339** (`catch ({ x }: unknown)`): `getTypeOfDestructuredProperty`'s
  missing-property report on an `unknown` parent — destructuring
  (`destructure.rs`, r5-shapes).
