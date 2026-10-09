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
  *Corrected after §4:* the tests first asserted TS2304 on unresolved names
  and passed only because their harness stamped `JAVASCRIPT_FILE` on the
  file root and not on comment roots, as the loader does. In production
  `check_type_reference_name` declines every node `in_js_file`, comment
  nodes included, so no TS2304 is reported inside JSDoc (§4.2). The tests
  now stamp like the loader and assert TS2344 against a
  `@template {string}` typedef; six still fail without this section's walk.

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

## 4. A typedef's scope (item 4, typedefScope1)

**Native.** `reparseUnhosted` makes each `@typedef`/`@callback` a
`JSTypeAliasDeclaration` in the reparse list, and `parseListIndex`
(`parser/parser.go:610`) keeps it only in a source-element or
block-statement list (`PCSourceElements`, `PCBlockStatements`; a module
block parses its statements as the latter), propagating it outward from
any other list. The binder then files the alias with
`bindBlockScopedDeclaration`, in that list's block-scope container.

**Gap.** `bind_jsdoc_declarations` declared every typedef and callback in
the file root's locals, so `B` declared inside `function B1` resolved at
file scope: typedefScope1's top-level `/** @type {B} */` found a `B`.

**Diff** `r5-jsdoc4-typedef-block-scope.diff` (binder): `jsdoc_alias_scope`
climbs from the comment's host to the nearest `SourceFile` (→ the root),
`ModuleBlock` (→ its namespace) or `Block` (→ the function whose body it
is, else the block itself — `container_flags`' own rule for a `Block`), and
the two arms declare there. `@import` stays at file scope (no measured case;
r5-jsdoc3's `jsdoc_import_declaration_parent` already answers its
statement's parent for visibility).

**Measured** (on §3, unfiltered): types **+1** (typedefScope1 `notOK : B`,
native's unresolved spelling); diagnostics unchanged; zero losses. Ir flat
(the binder arm runs in JS files only).

### 4.1 Not converted: the missing TS2304

typedefScope1 still lacks its TS2304: `B` now fails to resolve, but
`check_type_reference_name` (`check.rs`) returns before reporting for any
node `in_js_file`, and r5-jsdoc3's loader change stamps comment roots as JS.
The decline exists for written TypeScript annotations in a JS file
(TS8010's territory). Narrowing it to nodes outside comments was measured
(with this diff): **+1** (typedefScope1) and **−6**: callbackTag2
RIGHT→WRONG; importTypeResolutionJSDocEOF, checkJsdocTypeTag8,
commonJSImportClassTypeReference, commonJSImportExportedClassExpression and
jsdocTypeDefAtStartOfFile EMPTY_RIGHT→EMPTY_WRONG. Those are r5-jsdoc3
§3's "JS-relaxed rules": a JSDoc reference to a value (a class through a
`require` alias, `Object`, a module) resolves through native's JS arms of
`getTypeFromTypeReference`, which TSR answers as unresolved. Refused here;
the narrowing waits on those arms (`declared.rs`). Number that refused it:
6 losses.

### 4.2 The `@import` arm of `symbol_access.rs`' `has_visible_declarations`

`getAnyImportSyntax` (`checker/utilities.go:1602`) reaches an `@import`'s
reparsed `JSImportDeclaration`, whose parent is a statement list. TSR's
specifier → parent³ walk lands on the `JSDocImportTag`, whose parent is the
comment, so visibility was asked of the comment. The arm now asks
`jsdoc_import_declaration_parent` for a `JSDocImportTag`, as r5-jsdoc3's
node-reuse diff did for `node_reuse.rs`. Measured: no row moves on either
dump, zero losses, Ir flat (domain-model 1,199,737,599, generic-imports
342,888,546). No corpus case prints an `@import`ed name through this
printer path yet.

## 5. jsdocImportType: `isCommonJSRequire`'s ambient arm (item 5; calls.rs diff)

**Finding.** The ambient arm is already ported (`calls.rs`
`is_commonjs_require`, main's) but can never pass: it tests
`combined_node_flags(declaration).contains(NodeFlags::AMBIENT)`, and no
producer in this port sets `NodeFlags::AMBIENT` (the parser does not; see
`unused.rs` `is_in_ambient_context`, `bd tsr-o9tl`). jsdocImportType's
`declare function require` in `types.d.ts` resolves as a global FUNCTION,
fails the test, and `require("./mod1")` stays `any`. (The CLI hides this
for single-file globals: there `require` resolves to nothing and takes the
implicit-`requireSymbol` arm.)

**Diff** `r5-jsdoc4-commonjs-require-ambient.diff` (`calls.rs`): the first
declaration of the kind (`ast.GetDeclarationOfKind`), asked through the
existing any-file stand-in `is_ambient_declaration`
(`merged_export_spaces.rs`: a `.d.ts`, `declare`, or an ambient module).

**Measured** (on §4's commit, unfiltered): types **+5**
(ambientRequireFunction(module=commonjs/preserve), bundlerSyntaxRestrictions
(module=esnext/preserve), jsdocImportType `require("./mod1") : typeof D`);
diagnostics unchanged; zero losses; Ir flat (domain-model 1,199,733,854,
generic-imports 342,899,521). `cargo test --workspace --release` passes
with it applied.

jsdocImportType stays WRONG: `@type {C}` for `C = import("./mod1")` is the
unqualified module-object import type `get_type_from_import_type_node`
declines (`tsr-e2u`), so `c` is `undefined` after flow and reports TS18048
for native's TS2454; `d` (`@type {D}`, the require alias) narrows the same
way.

## 6. Landing order and totals

Diffs, each measured alone on the commit that precedes its section, all
independent of one another (the two binder diffs touch different hunks of
`bind_jsdoc_declarations`; all three were applied together and build):

1. `r5-jsdoc4-augments-binding.diff` (binder) — §2.1;
2. `r5-jsdoc4-typedef-block-scope.diff` (binder) — §4;
3. `r5-jsdoc4-commonjs-require-ambient.diff` (`calls.rs`) — §5.

| Set (vs `f5d0291`) | diagnostics RIGHT | types RIGHT |
|---|---|---|
| base | 5,385 | 545,044 |
| this lane's commits | **5,394 (+9)** | 545,044 |
| + the three diffs (sum of their separate measurements) | 5,395 (+10) | 545,054 (+10) |

EMPTY_RIGHT 5,584 throughout. Zero RIGHT→non-RIGHT and zero
RIGHT/EMPTY_RIGHT verdict moves in every measured step.

### Remaining in the lane

| Case | Blocker | Site (owner) |
|---|---|---|
| unmetTypeConstraintInJSDocImportCall (+ TS twin) | import types with type arguments answer `error` | `declared.rs` (r5-declared2) — §2.3 |
| jsdocCatchClauseWithTypeAnnotation (+ TS twin) | TS18046 on `unknown` declined; TS2339 on destructured `unknown` | `nullable_operand.rs`/`members.rs` (main), `destructure.rs` (r5-shapes) — §3.2 |
| typedefScope1 TS2304, and every TS2304 inside JSDoc | `check_type_reference_name` declines all JS-file nodes; narrowing needs native's JS value-reference arms | `check.rs`, `declared.rs` — §4.1 |
| jsdocImportType | unqualified module-object import type | `declared.rs` (`tsr-e2u`) — §5 |
| jsdocResolveNameFailureInTypedef, reuseTypeAnnotationImportTypeInGlobalThisTypeArgument | end-of-file comment not attached | parser — §1 |
| jsFileMethodOverloads3, overloadTag1/2 | overload signatures not built | `signatures.rs` (`.16.163`) — §1 |
| typedefMultipleTypeParameters | arity rule ignores a typedef's `@template` count | `type_argument_arity.rs` — §1 |
| jsdocTemplateTagDefault, jsdocTemplateTag7/8 | `@template` defaults/modifiers not walked | `jsdoc_checks.rs` + type-parameter checks — §1 |
