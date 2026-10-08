# r5-jsdoc3 — JSDoc scope hop and JSDoc diagnostics (round 5)

Lane `tsr-2zk.1046`, epic `tsr-2zk`. Issues `.16.107`, `.16.98`. Native
source is `vendor/typescript-go` @ `5b1047d`. Continues
[r5-jsdoc2](r5-jsdoc2.md) and [r4-jsdoc](r4-jsdoc.md).

Frozen baseline (`bd8ae26`, batch T landed): types **544,047 RIGHT** of
552,533 aligned lines (994 GAP, 7,492 WRONG); diagnostics **5,350 RIGHT /
5,581 EMPTY_RIGHT** of 12,238 rows.

## Ownership as worked

The lane owns `jsdoc_params.rs`, a new `jsdoc_checks.rs`, `check.rs`'s
`source_file_of_for_diagnostics`, and its tests. Every consumer the items
need sits in a file another lane owns (binder, `contextual.rs`,
`node_reuse.rs`, `declared.rs`, `grammar.rs`, `checker.rs`), so as in
r5-jsdoc2 the commit holds the reparse **queries** in `jsdoc_params.rs`
(each `#[expect(dead_code)]` until its consumer lands) and each consumer is a
measured diff under `docs/parity/notes/r5-jsdoc3-*.diff`.

## 1. The JSDoc scope hop, made zero-loss (`.16.107`)

`r4-jsdoc-scope-hop.diff` (binder: `JSDoc → host` side table and
`resolve_name`'s `scope_parent`) still applies at `bd8ae26` with one trivial
conflict against r5-binperf's `name_nodes` change. Re-measured alone on the
frozen base it reproduces r4-jsdoc §2.4's four losses exactly. Each was a
coincidence: a JSDoc name that did not resolve before the hop made some
other unported piece unobservable. Root causes and ports:

### 1.1 `getContextualType`'s `KindExportAssignment` arm (checkJsdocTypeTagOnExportAssignment8)

**Native.** `checker.go:29398`: `case ast.KindExportAssignment: return
c.tryGetTypeFromTypeNode(parent)`. An export assignment's `Type()` is only
ever set by `reparseHosted`'s `KindJSDocTypeTag` arm
(`parser/reparser.go:356`). TSR had no such arm, so `export default { b: 'b' }`
under `/** @type {Foo} */` widened `'b'` to `string` once `Foo` resolved, and
the existing `check_jsdoc_annotated_initializer` reported TS2322.

**Port.** Owned query `jsdoc_export_assignment_type(node)` (the reparsed
type: JS file, the node's `@type`), read by a new arm in
`get_contextual_type` (`contextual.rs`, diff).

### 1.2 `getContextualTypeForBinaryOperand` in JS (expandoFunctionContextualTypesJs)

Two native pieces were missing for JS assignments:

- **`binary.Type`** (`checker.go:29811`, first line). `reparseHosted`'s
  `KindExpressionStatement` arm (`reparser.go:369`) moves a statement's
  `@type` onto an assignment declaration that is the statement's
  expression. Owned query `jsdoc_binary_type(binary)`; the diff reads it
  where `binary.r#type` was read. This covers `/** @type {T} */ F.p = {…}`,
  `this.p = {…}` and `module.exports = {…}`.
- **The assignment-declaration arm** of `getContextualTypeForAssignmentExpression`
  (`checker.go:29843`): `F.id = expr` with a binder symbol answers the
  annotated variable's property type, else nil. TSR declined every JS
  assignment outright (`in_js_file(operand)` at the top of the `=` arm).
  The diff narrows that decline: a JS assignment **with** a binder symbol
  takes the existing ported arm; one without still declines (those are
  `getTypeOfExpression(left)` natively, a wider change not measured here).
  Inside the arm, `symbol.ValueDeclaration.Type()` now also reads the
  variable's reparsed `@type` (`jsdoc_type_annotation`), and the arm's
  TS-only class exception ("TypeScript classes are not expando
  initializers") no longer applies in JS, where a class receiver's
  assignment declaration answers nil like any other non-variable receiver.

  The class exception was found by the narrowing's own first measurement:
  `C.blah2 = 456` (classFieldSuperAccessibleJs1, jsDeclarationsClassStatic2)
  fell through to the left operand's type and lost 6 lines and a case.

### 1.3 `getAnyImportSyntax` for a JSDoc `@import` (importTag24:1:18)

**Native.** `reparseUnhosted`'s `KindJSDocImportTag` arm
(`reparser.go:119`) makes a `JSImportDeclaration` statement, and
`parseListIndex` (`parser.go:613`) propagates it out to the nearest source
file or block statement list. `hasVisibleDeclarations` reaches it through
`getAnyImportSyntax` (`checker/utilities.go:1602`, specifier →
parent³) and asks `isDeclarationVisible(import.Parent)`.

**Gap.** Node reuse's walk (`node_reuse.rs` `has_visible_declarations`)
stopped at the `JSDocImportTag`, so `@returns {Foo}` naming an `@import`ed
alias was judged invisible once the hop resolved `Foo`, and the signature
printed `() => string`. Before the hop `Foo` did not resolve at all and the
reuse visitor accepted the unresolved name.

**Port.** Owned query `jsdoc_import_declaration_parent(tag)`: the nearest
`SourceFile`, `Block` or `ModuleBlock` enclosing the comment's host. The
diff treats `JSDocImportTag` as the import statement and asks that parent.
`symbol_access.rs` has the same walk for other printers (`parent³` there
lands on the tag, whose parent is the comment); not changed, no measured
case needs it.

### 1.4 A `require` alias in type position (jsdocImportType:0:8)

**Native.** `const D = require("./mod1")` binds an alias
(`IsVariableDeclarationInitializedToRequire`), and `/** @type {D} */`
resolves it through `resolveEntityName` (alias resolved fully) to class
`Chunk`, printed `D` through the symbol chain. `getTypeFromJSDocValueReference`
named in r4-jsdoc §2.4 does not exist at the pin (`getTypeReferenceType`
ends in `// !!! Resolving values as types for JS`); the alias is the road.

**Gap.** `get_type_from_type_reference` (`declared.rs`) handed the alias
symbol itself to `get_type_reference_type` → `any`.

**Port.** The diff routes a require-alias declaration through the same
road `ImportEqualsDeclaration` takes (§157: mint the alias spelling over the
merged target) — native's `getTargetOfImportEqualsDeclaration` covers both
— resolving the require alias fully (`resolve_alias` stops at the module's
`export=` alias; `resolveEntityName` does not).

**Not fixed.** The rest of jsdocImportType stays WRONG: TSR does not admit
`require("./mod1")` as a CommonJS require when `require` is a user-declared
ambient function (`isCommonJSRequire`'s ambient-declaration arm), so
`require(...)` and `c` remain `any`.

### 1.5 Landing order and measurements

**Landing order.** On this lane's commit, apply
`r5-jsdoc3-contextual-js-assignments.diff` (`contextual.rs`),
`r5-jsdoc3-node-reuse-jsdoc-import.diff` (`node_reuse.rs`),
`r5-jsdoc3-require-alias-type-reference.diff` (`declared.rs`), then
`r5-jsdoc3-scope-hop.diff` (binder; supersedes `r4-jsdoc-scope-hop.diff`,
rebased on r5-binperf and with the inlining below). Each consumer diff
removes the `#[expect(dead_code)]` its queries carry. The four applied in
that order reproduce byte-for-byte the tree measured here. The first three
are independent of one another.

Measured unfiltered against the frozen base (`bd8ae26`):

| Set | types RIGHT | diagnostics RIGHT / EMPTY_RIGHT | losses |
|---|---|---|---|
| base | 544,047 | 5,350 / 5,581 | — |
| hop alone (r4 diff) | — | — | reproduces r4-jsdoc §2.4's four (filtered run) |
| three fix diffs, no hop | 544,047 | 5,350 / 5,581 | none; no verdict moves |
| all four | **544,093 (+46)** | **5,354 / 5,581 (+4)** | **none** |

Type lines converted (all four): expandoFunctionContextualTypesJs 15,
inferThis 6, callbackOnConstructor 5, importTag24 4,
typeTagOnFunctionReferencesGeneric 3, checkJsdocTypeTagOnExportAssignment8 2,
assertionTypePredicates2 2, jsDeclarationsClasses(target=es2015) 2,
paramTagTypeResolution 2, varRequireFromJavascript 2,
varRequireFromTypescript 2, jsDeclarationsComputedNames(target=es2015) 1.
Diagnostics WRONG → RIGHT: checkJsdocTypeTagOnExportAssignment1/4/6,
checkJsdocSatisfiesTag9. checkJsdocTypeTagOnExportAssignment8 and
expandoFunctionContextualTypesJs stay EMPTY_RIGHT (they were the r4 losses).

**Perf.** Callgrind Ir (`--singleThreaded --pretty false`), base → all four:
domain-model 1,199,538,490 → 1,200,502,078 (+0.08%), generic-imports
343,374,181 → 343,385,798 (+0.003%); run-to-run spread of one binary is
about 0.05% on domain-model. Median child CPU (21 samples, new/old):
domain-model 1.001, generic-imports 1.005, `diagnostics_match` true.

The r4 hop as written cost **+0.47% Ir** on domain-model: its
`scope_parent` was an out-of-line call on every step of every
name-resolution walk (5.2 M Ir in callgrind's function diff). The diff now
inlines the parent edge and keeps the comment crossing in a `#[cold]`
function, since only a walk that runs off a parentless node reaches it.

**Convention record (hop).** As r4-jsdoc §2.1: `jsdoc_hosts: NodeId(JSDoc)
→ NodeId(host)`, owned by the binder, filled once per file in
`bind_jsdoc_declarations`, merged at publication like `computed_names`,
immutable after; no receiver/alias context; one hash probe per walk that
runs off a comment. The checker's own `Checker::jsdoc_hosts` is the same
map built from `set_jsdoc`; unifying them is left to the integrator.

The three queries (`jsdoc_binary_type`, `jsdoc_export_assignment_type`,
`jsdoc_import_declaration_parent`) add no cache or table: each is a few
parent/kind reads and one `jsdoc_entries` or `jsdoc_hosts` probe.

**Falsifiers.** A JS assignment declaration whose native contextual type is
`getTypeOfExpression(left)` (a nested access receiver without a binder
symbol) would still decline here; a case whose `@import` sits in a
function body and is printed from outside it would expose the container
rule.

### 1.6 Still open in `.16.107`

- **typedefScope1**: every typedef is declared at file scope
  (`binder.rs` `declare_jsdoc_symbol(root, …)`); native binds the reparsed
  alias in the host's block. Binder.
- **jsdocImportType**: §1.4's `isCommonJSRequire` ambient arm.
- A JSDoc reference to an undeclared name in a module now resolves nothing
  and reports nothing until §3 lets diagnostics out of comments.

## 2. A declaration's own `@type` (`.16.98`)

**Native.** `reparseHosted`'s `KindJSDocTypeTag` arm (`parser/reparser.go:356`)
sets `Type` on a `PropertyDeclaration`, `PropertyAssignment` or
`VariableDeclaration` from the first typed `@type` of that node's own last
comment. TSR read hosted `@type` only through variable statements
(`jsdoc_type_annotation`'s statement walk), so three hosts were untyped:

- **class properties** (r4-jsdoc §2.3's diff, `#private` and computed names
  included);
- **catch-clause variables** (r4-jsdoc §5): `catch (/** @type {unknown} */ e)`;
- **object-literal property assignments** (`typeTagOnPropertyAssignment`).

**Port.** One owned query, `jsdoc_self_hosted_type(declaration)`, answers
all three. Consumers (`r5-jsdoc3-hosted-declaration-types.diff`, applies on
top of §1's four):

| Site | Native | Change |
|---|---|---|
| `symbols.rs` `jsdoc_type_annotation` | `declaration.Type()` | non-statement hosts (property, catch variable) ask the query instead of returning `None` |
| `symbols.rs` `get_widened_type_for_variable_like_declaration` | `getTypeForVariableLikeDeclaration`'s catch arm (`checker.go:16678`) | an annotated catch variable (written or `@type`) is its type when `any`/`unknown`, else `errorType`; only the unannotated case was ported |
| `symbols.rs` `PropertyAssignment` arm | `checkPropertyAssignment`'s `node.Type()` arm (`checker.go:13681`) | the reparsed type is the member's type, after checking the initializer |
| `objects.rs` member loop | same | the literal's member takes the reparsed type |
| `contextual.rs` `contextual_type_for_object_literal_element` | `element.Type()` (`checker.go:29921`) | reads the reparsed type |

The catch arm's `errorType` for a non-`any`/`unknown` annotation is
native's for TypeScript too: catchClauseWithTypeAnnotation (4 lines) and
parserCatchClauseWithTypeAnnotation1 (1) convert with it.

**Not ported here.** `checkPropertyAssignment`'s assignability check of the
initializer against the reparsed type (no measured case reports it), and
TS1196 for a JSDoc-typed catch variable (`grammar.rs`
`check_catch_clause_declaration` reads the written type only; its
diagnostic is anchored inside the comment, so it waits on §3).

### 2.1 Measured

All five diffs (§1's four and this one) against the frozen base:

- **types 544,047 → 544,145 RIGHT (+98; +52 over §1)**, wrong 7,492 → 7,394.
- **diagnostics RIGHT 5,350 → 5,355** (§1's four plus
  jsDeclarationsInheritedTypes); EMPTY_RIGHT unchanged.
- **Zero** RIGHT→non-RIGHT lines, zero RIGHT/EMPTY_RIGHT rows moved.
- Lines converted by this item: typeTagOnPropertyAssignment 11 (case fully
  RIGHT), jsdocCatchClauseWithTypeAnnotation 10,
  typeFromPrivatePropertyAssignmentJs 9, lateBoundAssignmentCandidateJS1 8,
  jsDeclarationsClasses(target=es2015) 5, catchClauseWithTypeAnnotation 4,
  jsDeclarationsInheritedTypes 3, jsdocPrivateName1 1,
  parserCatchClauseWithTypeAnnotation1 1.
- typedefOnSemicolonClassElement (r4's reason to hold the property diff)
  stays RIGHT: the hop resolves its typedef.
- **Perf.** Ir domain-model 1,200,511,535 (+0.08% over base, +0.001% over
  §1), generic-imports 343,391,784 (+0.005%). Median child CPU, 41 samples:
  domain-model 1.006, generic-imports 0.997; `diagnostics_match` true. (A
  21-sample run read 1.051 / 1.038 while the base binary's own median moved
  0.369 → 0.378 s; Ir is the deterministic check and did not move.)
- `cargo test --workspace --release` passes with all five applied.

The object-literal consumer adds one `jsdoc_entries` probe per member of
every object literal, TypeScript included; the Ir above includes it.

## 3. Diagnostics anchored inside JSDoc, and the JSDoc type walk

**Forcing fact** (r5-jsdoc2 §2): `source_file_of_for_diagnostics` climbed
parents to the `SourceFile` and dead-ended at the parentless JSDoc root, so
every diagnostic anchored in a comment was silently dropped, and
`r5-jsdoc2-jsdoc-type-walk.diff` (native's `checkSourceElement` over
reparsed type nodes) could report nothing. With the crossing it lost 36
rows; with §1's hop in place every TS2304/TS2503 of those is gone, and the
rest root-cause to six more places that walk parents without crossing the
comment, or that miss a reparsed position:

| Loss (case) | Native | Port (file) |
|---|---|---|
| TS6133 importTag25 — the crossing **alone** | `@import`ed `T` is referenced from a `@type` once names resolve through the host (§1) | none: the crossing lands after the hop |
| TS2503 importTag2/9 | `@import * as ns` reparses to a namespace import and binds `ns` | bind it like the named form (`binder.rs`); this lifts the `tsr-e2u` fence, which no type line now needs |
| TS1228 returnTagTypeGuard | `getTypePredicateParent` (`checker.go:3099`): the reparsed `@returns` is the function's `Type` | `type_predicate_parent` also asks the owned `jsdoc_reparsed_return_owner` (`grammar.rs`) |
| TS17020 checkJsdocTypeTag3 | `IsInJSFile` reads `NodeFlagsJavaScriptFile`, which native's parser puts on every node of a JS file, reparsed JSDoc included | the loader stamps the flag on each JSDoc root of a JS file as well as on the file root (`tsr-compiler` `loader.rs`, `lib.rs`) |
| TS2526 jsDeclarationsThisTypes | `getThisContainer` from a reparsed `@returns {this}` reaches the method | `this_container` crosses the comment (`this_expression.rs`) |
| TS2315 checkJsdocSatisfiesTag2 | `getIntendedTypeFromJSDocTypeReference`'s `Object.<K, V>` arm answers before `getTypeFromClassOrInterfaceReference` | the arity rule skips that reference (owned `is_jsdoc_record_object_reference`; `type_argument_arity.rs`) |
| TS2315 overloadTag3 | a JS class's `@template` tags are its type parameters (`reparser.go:459`) | the arity rule counts them (owned `jsdoc_class_template_parameters`; `type_argument_arity.rs`) |

Also in the set: `check_catch_clause_declaration` (`grammar.rs`) reads the
reparsed `@type` (`checkCatchClause`, `checker.go:4247`) for TS1196, and
the walk visits a class property's own `@type` (§2's host).

**Where the JS-file bit comes from (measured).** Seven more rules ask
`in_js_file` about JSDoc nodes once the walk reaches them (TS2304 in
jsdocTypeDefAtStartOfFile and importTypeResolutionJSDocEOF, TS2749 in
commonJSImportClassTypeReference, TS2583 in checkJsdocTypeTag8, …: each a
JS-relaxed rule that took the comment for TypeScript). Three placements were
measured:

| Placement | domain-model Ir vs §2 | Losses |
|---|---|---|
| `Checker::in_js_file` crosses a JSDoc root to its host | +2.6 M (+0.22%) | 0 |
| only the TS17019/17020 rule crosses | +0.2 M | 7 |
| the loader stamps every JSDoc root of a JS file | +0.2 M | 0 |

The first is a `kind` load at the root on every call, which a TypeScript
project pays and never uses. The third is native's own arrangement — the
parser sets `NodeFlagsJavaScriptFile` on every node of a JS file — moved to
where TSR stamps the file root (the parser never sees the file name,
ADR-0016): one `add_flags` per comment, in JS files only, so the
TypeScript path does no new work. Its falsifier is a JSDoc root reached
other than through a stamped table (a harness that parses a JS file and
stamps only the root, like `dts_emit_suite`).

The same measurement moved `source_file_of_for_diagnostics`' crossing into a
`#[cold]` helper: written inline, its body became identical to
`source_file_of`'s and the two were folded into one out-of-line function
(+1.3 M Ir at `source_file_of`'s new callers). The `check_node` hook is gated
on a per-file `file_is_js` set once in `check_source_file` (+1.8 M Ir
ungated: a `jsdoc_entries` probe on every checked node of every file).

**Ownership.** `jsdoc_checks.rs` (r5-jsdoc2's walk, now this lane's file)
and the queries land as code with `#[expect(dead_code)]`. Because the
crossing in `source_file_of_for_diagnostics` cannot land alone (importTag25),
it ships inside `r5-jsdoc3-jsdoc-diagnostics.diff` with the `check_node`
hook and the consumers above, applied after §1 and §2's five diffs.

**Convention record.** No cache or table. The crossings are one
`jsdoc_hosts` probe, reached only when a walk runs off a parentless node;
the walk is r5-jsdoc2's (one `jsdoc_entries` probe per checked node, the
replay answering from hash lookups when no comment applies).

### 3.1 Landing order and measurements

Apply `r5-jsdoc3-jsdoc-diagnostics.diff` after §1's four and §2's diff
(binder, `check.rs`, `checker.rs`, `grammar.rs`, `this_expression.rs`,
`type_argument_arity.rs`, `tsr-compiler` `lib.rs`/`loader.rs`, and the
expectation removals in the two owned files), then §4's
`r5-jsdoc3-template-constraint.diff`. The seven diffs applied in order
reproduce byte-for-byte the tree measured below; `cargo test --workspace
--release` passes with all seven applied.

Unfiltered, against the frozen base:

| Set | types RIGHT | diagnostics RIGHT / EMPTY_RIGHT | losses |
|---|---|---|---|
| §1–§2 | 544,145 | 5,355 / 5,581 | none |
| r5-jsdoc2's walk + crossing on §1–§2, as first written | — | — | 3 (from r5-jsdoc2's 36) |
| §1–§3 | 544,145 | **5,358 / 5,581** | **none** |
| §1–§4 | **544,157** | **5,359 / 5,581** | **none** |

Diagnostics WRONG → RIGHT for §3: jsdocClassMissingTypeArguments,
importTag23, jsDeclarationsClasses(target=es2015); for §4:
checkJsdocTypeTag4. Type lines for §4: jsdocTemplateTag3 12.

**Perf** (§1–§4 against base). Ir: domain-model 1,199,538,490 →
1,200,752,905 (+0.10%; §3–§4 add +0.02% over §2), generic-imports
343,374,181 → 343,382,468 (+0.002%). Median child CPU, 41 samples:
domain-model 0.984, generic-imports 1.016; `diagnostics_match` true.

## 4. JSDoc `@template` constraints (item 4)

**Native.** `gatherTypeParameters` (`parser/reparser.go:293`) clones an
`@template {C} T, U` tag's first parameter with `{C}` as its constraint.
TSR read constraints only from `TypeParameterDeclaration.constraint`
(`members.rs` `type_parameter_constraint`, `getConstraintDeclaration`), so
every JSDoc `@template` constraint — typedef, function and class alike — was
lost. Owned query `jsdoc_template_constraint(parameter)`; the consumer is
`r5-jsdoc3-template-constraint.diff` (`members.rs`).

Converts checkJsdocTypeTag4's second TS2344 (`B<number>` against
`@template {string} U`) and jsdocTemplateTag3's constrained-parameter lines.

**Not done.** unmetTypeConstraintInJSDocImportCall: `check_type_argument_constraints`
covers `TypeReferenceNode` only, and its argument is the unconstrained
type parameter `T`, which the constraints lane's
`relation_undecidable_for_constraint` declines regardless. extendsTag5:
the walk does not visit `@augments`/`@extends` heritage, and the check
would need `ExpressionWithTypeArguments` in `check_type_argument_constraints`.
Both stay with the constraints lane.

### 4.1 Measured

On top of §1–§3: types +12 (jsdocTemplateTag3), diagnostics +1
(checkJsdocTypeTag4), zero losses; the table in §3.1 has the totals.

## 5. Remaining in the lane

| Issue / case | Hypothesis | Site (owner) |
|---|---|---|
| `.16.107` typedefScope1 | typedefs declared at file scope; native binds the reparsed alias in the host's block | binder |
| `.16.98` jsdocImportType | `isCommonJSRequire`'s ambient-declaration arm: a user-declared ambient `require` still makes `require("./m")` a require call | `calls.rs` |
| jsdocCatchClauseWithTypeAnnotation (diag) | TS18046/TS2339 on `unknown` catch variables, TS2492 (catch redeclaration, not ported) | `grammar.rs`, property access |
| unmetTypeConstraintInJSDocImportCall | `check_type_argument_constraints` covers `TypeReferenceNode` only; and the argument is a type parameter the constraints lane declines | `constraints.rs` |
| extendsTag5 | the walk skips `@augments`/`@extends`; the constraint check needs `ExpressionWithTypeArguments` | `jsdoc_checks.rs`, `constraints.rs` |
| walk coverage | casts, `@satisfies`, `@this`, full-signature `@type`, `@callback`, `@overload`, `@import`, export/accessor `@type` are not visited (`jsdoc_checks.rs` module doc) | `jsdoc_checks.rs` |
| `symbol_access.rs` `has_visible_declarations` | same `getAnyImportSyntax` gap as §1.3 for the other printers | `symbol_access.rs` |
