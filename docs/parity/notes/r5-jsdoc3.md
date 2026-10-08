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
