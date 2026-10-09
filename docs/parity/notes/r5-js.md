# r5-js — JavaScript-file type lines left by the JSDoc lanes (round 5)

Lane `r5-js`, epic `tsr-2zk`. Native source is `vendor/typescript-go` @
`5b1047d`. Inputs: [r5-typetriage](r5-typetriage.md) (the `js:*` causes),
[r5-jsdoc3](r5-jsdoc3.md), [r5-jsdoc4](r5-jsdoc4.md),
[r5-modexports](r5-modexports.md),
[ADR-0046](../../adr/0046-jsdoc-reparse-is-a-checker-query.md).

Frozen baseline (integration head `e20cdd4`, batch AH): types **548,751
RIGHT** of 556,291 aligned lines (900 GAP, 6,640 WRONG); diagnostics
**5,431 RIGHT / 5,590 EMPTY_RIGHT** of 12,238 rows.

## 1. What main has done and is doing on JS

`git log --since=96.hours origin/main` (fetched at session start, tip
`a6b8a68`) carries one JS-touching commit, `de26f58` (r4-operators2:
operator arms run in JS files). The rest of main's window is owner-migration
documentation, the shared-contract alias ports (`tsr-2zk.16.2`, `.16.57`),
oracle work (`.47.3`) and one contextual arm (`.16.385`). The JS producers
main owns already live on the integration branch: `SetValueDeclaration`
(`binder.rs`), the assignment-declaration value worker
(`assignment_declarations.rs`), `isCommonJSRequire` / the require alias
target (`calls.rs`, `symbols.rs`), and the late-bound and
`Object.defineProperty` expando binding (`binder.rs`). This lane changes
none of them except through the measured diffs below, and leaves the
expando sub-cluster (§2, S1) to main's `tsr-2zk.5`.

## 2. The 100 cases solely blocked by `js:*` causes, by producer

Regenerated at `e20cdd4` with the typetriage scripts (`classify.py`,
`report.py --claims`): **100** cases are solely blocked by JS-file causes
(the triage counted 113 at `ccb48e7`; 13 converted in between). Each case
was read against its source and assigned the producer of its *first* wrong
line; a case is listed once. "Not JS-specific" means the TypeScript twin of
the witness fails the same way (probed with `probefile`).

| # | sub-cause | producer (file) | owner | cases | lines | witnesses |
|---|---|---|---|---:|---:|---|
| S8 | JSDoc tags: `@param`/`@type`/`@template`/`@overload`/`@callback`/`@satisfies`/typedef reach | `jsdoc_*.rs`, `signatures.rs`, the parser's EOF comment | r5-jsdoc4 and successors | 55 | 248 | `jsdocSignatureOnReturnedFunction`, `overloadTag1`, `typeTagWithGenericSignature`, `checkJsdocTypeTagOnExportAssignment1` |
| S7 | names: `typeof m26` vs `typeof m4`, `default: typeof mod2`, `import(".")` vs `import("./index")`, import qualifiers | the symbol-chain printer | main `.39`, r5-modules2 | 15 | 160 | `nodeModulesAllowJs1` ×4, `nodeModulesAllowJsSynchronousCallErrors` ×4, `reactImportDropped` |
| S1 | expando / property-assignment declarations: late-bound `f[k] = v`, `Object.defineProperty` merged with `module.exports =`, `exports.a.b =`, `x.#p.q =` | `binder.rs` late binding, `members.rs` / `symbols.rs` (`getTypeOfFuncClassEnumModule`'s expando members) | main `tsr-2zk.5` | 5 | 53 | `declarationEmitLateBoundJSAssignments`, `expandoFunctionSymbolPropertyJs`, `ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge` |
| S3 | JS value / require aliases in type positions: `@type {D}` with `const D = require(…)`, `@param {K}` with `const { K } = require(…)`, `ex.Crunch` through `var ex = require(…)`, `typeof import(export= module)` | `declared.rs` (`getTypeFromJSDocValueReference`, the import-type arm) | r5-declared3 | 5 | 22 | `jsdocImportType`, `commonJSImportClassTypeReference`, `varRequireFromTypescript` |
| S9 | not JS-specific: self-referential redeclared `var a = f(a)`, unreachable returns, generic callback inference, `instanceof` a call-only function | `symbols.rs`, `signatures.rs`, `inference.rs`, `flow.rs` | main, r5-printer2 | 6 | 26 | `inferingFromAny`, `unreachableJavascriptUnchecked`, `controlFlowInstanceof` |
| S4 | `this.x =` in constructors, constructor functions and static blocks | `assignment_declarations.rs`, `this_expression.rs` | unowned | 0 | 0 | none survives reading (§2.1) |
| S2c | `import("x")` of a shorthand ambient module (`declare module "x";`) printed `{ default: typeof import("x") }` | `calls.rs` `check_import_call_expression` | main (diff, §3.2) | 4 | 24 | `nodeModulesAllowJsDynamicImport` ×4 |
| S2a | `require("x")` of an ambient module prints through the local alias (`typeof fs`, `typeof _`) | the symbol-chain printer | main `.39` | 4 | 4 | `ambientRequireFunction` ×2, `bundlerSyntaxRestrictions` ×2 |
| S5 | uncontextual parameter: an undocumented JS setter parameter reads the getter | `symbols.rs` `get_type_for_variable_like_declaration` | main (diff, §3.1) | 2 | 4 | `accessorDeclarationEmitJs`, `privateNamesIncompatibleModifiersJs` |
| S6 | `super` in a `static {}` block (TS too) | `expressions.rs` `check_super_expression` | hub (diff, §3.3) | 2 | 21 | `classFieldSuperAccessibleJs1`, `javascriptThisAssignmentInStaticBlock` |
| S2b | `module.exports = { [sym](x = 12) {} }`: a context-sensitive function on an assignment declaration's right gaps | `signatures.rs` `has_no_contextual_type` | r5-printer2 (diff, §3.4) | 1 | 9 | `jsDeclarationsComputedNames` |
| S2d | a module that default-imports itself | `symbols.rs` alias circularity | main | 1 | 3 | `selfReferentialDefaultNoStackOverflow` |

Reading it:

- **Most of the JS remainder is JSDoc (53) and names (15).** Neither is a
  JS-checking producer; they stay with the JSDoc lane and the printer.
- **The five producers the brief named hold 17 cases between them**:
  expando 5 (S1), CommonJS 10 (S2a–d), value references in type
  positions 5 (S3), constructor `this` 0 (S4), uncontextual parameters 2
  (S5). S2a is the printer, not CommonJS: `require("fs")` resolves and the
  alias `fs` already prints `typeof fs`, but the bare call names the module
  through its local alias, which is getSymbolChain's job.
- Owners: S1 is main's binder and member image (`tsr-2zk.5` is in
  progress there); S3 is declared.rs (r5-declared3). This lane ports S5,
  S2c and S6 as measured diffs (§3).

### 2.1 Corrections to the first table (`8696a95`)

The pushed table put four cases under S4 by their `this.x =` lines. Read
line by line, none of them is the constructor-`this` producer:

- `javascriptThisAssignmentInStaticBlock`: every wrong line follows from
  `super.isArray` answering `error` inside `static {}` — S6. §3.3's diff
  converts the whole case.
- `jsDeclarationEmitDoesNotRenameImport`: `options.test` is a `@typedef`
  `@property` read — S8.
- `jsDeclarationsFunctionJSDoc`: `@param {null} b` under `strict: false`
  reads `any` — S8.
- `controlFlowInstanceof` (its `.js` unit): `v instanceof AtTop` with a
  call-only function narrows `any` to `{}` in native (`getInstanceType`'s
  empty-object leg, `flow.go:971`) — `flow.rs`, S9.

So S4 holds no case at `e20cdd4`, and the files this lane claimed for it
(`assignment_declarations.rs`, `this_expression.rs`) are released.

### 2.2 Why S1 and S3 are not ported here

- **S1, late-bound expandos.** `foo[k] = v` with a non-literal `k` binds
  through `bindDeferredExpandoAssignment`'s `HasDynamicName` arm
  (`binder.go:1057`): an anonymous `__computed` property plus the target's
  `exports["__assignment"]` list (`addLateBoundAssignmentDeclarationToSymbol`,
  `binder.go:1000`), which `getResolvedMembersOrExportsOfSymbol` late-binds
  for the static side (`checker.go:15963`). TSR's binder has no
  `__assignment` table, so the port is a binder change plus a members.rs
  change plus the function-type printer reading the new members. That is
  main's `tsr-2zk.5`, not a small diff.
- **S3, require aliases in type positions.** `declared.rs`'s alias road
  (`get_type_from_type_reference`) admits `ImportSpecifier`,
  `ImportClause`, `ImportEquals` and a require-initialised
  `VariableDeclaration`; a destructured `const { K } = require(…)` binding
  element is none of them. Whether it should take the name-agreement road
  of the import specifier is r5-declared3's call (§158's naming wall); it
  is reported, not made (§4).

## 3. Ports

All three are measured diffs against the frozen base (`e20cdd4`),
unfiltered, each applied alone; each carries its test. Ir is Callgrind's
`tsr -p <project> --singleThreaded --pretty false --noEmit`, base
domain-model 1,155,945,043 and generic-imports 342,949,755.

| diff | types RIGHT | cases converted | diagnostics | losses | Ir dm | Ir gi | slowcases |
|---|---:|---|---|---|---:|---:|---|
| [`r5-js-super-static-block.diff`](r5-js-super-static-block.diff) | +45 (548,796) | 6: `classFieldSuperAccessible`, `classFieldSuperAccessibleJs1`, `javascriptThisAssignmentInStaticBlock`, `classStaticBlock5` ×3 | unchanged | none | 1,155,899,389 (−0.004%) | 342,928,996 (−0.006%) | clean |
| [`r5-js-setter-parameter.diff`](r5-js-setter-parameter.diff) | +4 (548,755) | 2: `accessorDeclarationEmitJs`, `privateNamesIncompatibleModifiersJs` | unchanged | none | 1,155,964,693 (+0.002%) | 342,922,580 (−0.008%) | clean |
| [`r5-js-shorthand-dynamic-import.diff`](r5-js-shorthand-dynamic-import.diff) | +48 (548,799) | 8: `nodeModulesAllowJsDynamicImport` ×4, `nodeModulesDynamicImport` ×4 | unchanged | none | 1,155,878,692 (−0.006%) | 342,913,942 (−0.010%) | clean |
| [`r5-js-assignment-context.diff`](r5-js-assignment-context.diff) | +9 (548,760) | 1: `jsDeclarationsComputedNames` | unchanged | none; no non-RIGHT line changed text | 1,155,961,326 (+0.001%) | 342,926,441 (−0.007%) | clean |
| [`r5-js-full-signature-generic.diff`](r5-js-full-signature-generic.diff) | +17 (548,768) | 2: `typeTagWithGenericSignature`, `checkJsdocTypeTag7` | unchanged | none; no non-RIGHT line changed text | 1,155,974,568 (+0.003%) | 342,910,431 (−0.012%) | clean |
| [`r5-js-jsdoc-cast-context.diff`](r5-js-jsdoc-cast-context.diff) | +28 (548,779) | 1: `jsdocSignatureOnReturnedFunction` | unchanged | none; no non-RIGHT line changed text | 1,155,987,415 (+0.004%) | 342,914,191 (−0.010%) | clean |
| [`r5-js-jsdoc-cast-overlap.diff`](r5-js-jsdoc-cast-overlap.diff) | unchanged | diagnostics 2: `checkJsTypeDefNoUnusedLocalMarked`, `jsDeclarationsDefault(target=es2015)` | 5,431 → 5,433 RIGHT | none | 1,155,974,041 (+0.003%) | 342,918,828 (−0.009%) | clean |

The seven touch disjoint functions and apply in any order. Stacked (all four applied, unfiltered against `e20cdd4`): types **548,751 → 548,857 RIGHT (+106)**, GAP 900 → 865, WRONG 6,640 → 6,569, **17 cases converted** — exactly the sum of the four rows — diagnostics unchanged (5,431 RIGHT / 5,590 EMPTY_RIGHT), zero losses on both dumps.

### 3.1 S5: the JS setter parameter (`symbols.rs`)

**Native.** `getTypeForVariableLikeDeclaration` (`checker.go:16712`):
after `tryGetTypeFromEffectiveTypeNode` has found no annotation, a
parameter of a set accessor with a bindable name takes the getter's return
type (`:16719`), before `getParameterTypeOfFullSignature` and the
contextual type.

**What TSR had.** The §441 arm read the accessor pair for TypeScript only:
`!self.in_js_file(declaration)`. The gate was added because the ungated
arm lost `declarationEmitClassAccessorsJs1`, whose setter carries
`@param {URL | string} path`. That loss was the order, not JS: in JS the
reparsed `@param` *is* the effective type node, so native never reaches the
getter arm for a documented parameter.

**Port.** The gate becomes native's condition: in JS, the arm runs when
the parameter has neither a `@param` (`jsdoc_parameter_annotation`) nor a
parameter `@type` (`jsdoc_type_annotation`). A tag whose type does not
compute still counts as present, as native's annotation does. The TS path
runs the same single `in_js_file` test it ran before.

**Falsifier.** A JS setter whose only typing is a full-signature `@type`
on the setter: native's getter arm wins over `getParameterTypeOfFullSignature`
there too, and this port agrees; a case wanting the full signature's
parameter type would show the order is wrong.

### 3.2 S2c: `import()` of a shorthand ambient module (`calls.rs`)

**Native.** `checkImportCallExpression` (`checker.go:8301`) types the
promise's argument as `getTypeWithSyntheticDefaultOnly(getTypeOfSymbol(esModuleSymbol), …)
?? getTypeWithSyntheticDefaultImportType(…)`. For a shorthand ambient
module `getTypeOfSymbol` is `anyType` (`getTypeOfFuncClassEnumModule`'s
`isShorthandAmbientModuleSymbol` arm). The default-only road is not taken
(not JSON); the synthetic-import road either returns `any` unchanged or
spreads the wrapper with it, and `getSpreadType` answers `any` for an
`any` side. So the call is `Promise<any>`.

**What TSR had.** `check_import_call_expression` mints the
`typeof import("x")` namespace object for every resolved module, then
wraps it in `{ default: … }` under node16+ ESM. A shorthand module printed
`Promise<{ default: typeof import("fs"); }>`.

**Port.** After the default-only test, a shorthand ambient module answers
`Promise<any>` — the one value every remaining native road produces for
it. This is not JS-specific: the four `nodeModulesDynamicImport` TS cases
convert with the four JS ones.

### 3.3 S6: `super` in a static block (`expressions.rs`)

**Native.** `GetSuperContainer` (`ast/utilities.go:1835`) stops at
`ClassStaticBlockDeclaration` as at any member, and `ast.IsStatic` is true
for a static block, so `checkSuperExpression` answers
`getBaseConstructorTypeOfClass` there (`checker.go:7946`); a `super(...)`
call in one fails `isLegalUsageOfSuperExpression`'s constructor test and
is the error-any.

**What TSR had.** `check_super_expression`'s container walk listed every
member kind but the static block, so `super` in `static {}` walked past it
to the class with no member found and answered `error`.

**Port.** The static block joins the member arm with `is_static = true`.
It is a TypeScript fix as much as a JS one: `classStaticBlock5` and
`classFieldSuperAccessible` are `.ts`. `expressions.rs` is a hub file and
the function is not this lane's, hence a diff.

### 3.4 S2b: no contextual type for an assignment declaration (`signatures.rs`)

**Forcing fact.** `G.z = (a = 1) => a` answered `error` — in TypeScript
too — and so did `module.exports = { f: (a = 1) => a }` and
`exports.x = { … }`: the function is context-sensitive, and
`get_type_of_function_expression`'s guard refuses one unless
`has_no_contextual_type` proves the context absent, and that walk answered
"may have context" for every `=`.

**Native.** `getContextualTypeForAssignmentExpression` (`checker.go:29843`)
answers nil, from the shape and one symbol lookup, for the right operand of
`module.exports = expr` (the receiver resolves to the `ModuleExports`
symbol), and of an assignment declaration (`binary.Symbol != nil`) whose
receiver is an identifier not declared by an annotated variable, or is
itself a property or element access.

**Port.** `assignment_context.rs` (new, registered in `lib.rs`):
`assignment_has_no_contextual_type`, the nil arms only, mirroring the
contextual arm in `contextual.rs` (`contextual_type_for_binary_operand`),
including its TypeScript-class exception. The walk's assignment arm asks
it for the right operand and keeps its refusal otherwise. No cache; one
`resolve_name` per context-sensitive assignment RHS.

**Measured.** +9 lines, +1 case (`jsDeclarationsComputedNames`); no
non-RIGHT line changed its text, so nothing turned gap→wrong.
`exportNestedNamespaces2` does not convert: its `exports = require(…)`
rebinding leaves the binary without a symbol in this binder.

**Falsifier.** A case where an expando's receiver is an annotated `const`
(`const o: T = …; o.f = (a) => …`) and the arrow still gaps: this arm
leaves that shape to the contextual path, so the gap would be there, not
here.

### 3.5 S8: a generic full-signature `@type` (`signatures.rs`, `jsdoc_full_signature.rs`)

Taken after the integrator released r5-jsdoc4's files to this lane.

**Native.** `getSignaturesOfSymbol` (`checker.go:19827`) asks
`getSignatureOfFullSignatureType(decl)` *first*, and uses the tag's
`getSingleCallSignature` in place of the declaration's own signature; that
signature keeps the tag's type parameters
(`getTypeParametersFromDeclaration`, `:19913`, reads the same).
`getParameterTypeOfFullSignature` and `getReturnTypeOfFullSignature` read
the same signature.

**What TSR had.** The parameter and return readers went through the
contextual reader `single_call_signature` (`contextual.rs`), which declines
any generic signature, so `/** @type {<T>(param?: T) => T | undefined} */
function typed(param)` printed `(param: any) => any`. The symbol's
signature was always built from the declaration, so even a non-generic
`@type` lost its optional markers on the function line.

**Port.** `jsdoc_full_signature.rs` gets its own `getSingleCallSignature`
(`full_signature_call_signature`, generic signatures admitted) for all three
readers, and `jsdoc_full_signature_of_declaration`, which
`get_signatures_of_symbol_for_type` asks before
`get_signature_from_declaration`. The contextual reader is unchanged.
JS only: `jsdoc_full_signature_node` returns at its `in_js_file` test.

**Measured.** +17 lines, +2 cases; `assertionsAndNonReturningFunctions`
(+5) and `typeTagOnFunctionReferencesGeneric` (+5) gain lines but keep
other causes (an `asserts` predicate from a `@typedef`; a contextual
generic signature on an arrow, `contextual.rs`).

**Falsifier.** A JS function with both a full-signature `@type` and a body
whose return type disagrees: native reports against the tag's signature;
a case printing the body's return type on the function line would mean the
signature substitution is too early.

### 3.6 S8: JSDoc hosted on a cast or a `return` (`contextual.rs`, `symbols.rs`, `signatures.rs`)

**Native.** `reparseHosted` turns `/** @type {T} */ (e)` and
`/** @type {T} */ return e` into `AsExpression(e, T)` (`makeNewCast`,
`reparser.go:378`), and `getContextualType`'s assertion arm answers `T` for
its operand unless it is `const` (`checker.go:29368`).
`getFunctionLikeHost` takes a `return`'s expression as the function a
`@param`/`@returns` comment on the `return` documents (`reparser.go:662`).

**What TSR had.** The cast typed the paren (`expressions.rs`) but gave its
operand no context, so `/** @type {(a: number) => number} */ ((a) => a)`
printed the arrow as `(a: any) => any`; a `return`-hosted comment typed
nothing.

**Port.** Three one-arm changes, all behind `in_js_file`:
- `contextual.rs` `get_contextual_type`: a JS cast paren, or a `return`
  carrying a `@type`, answers the tag's type (non-`const`);
- `signatures.rs` `has_no_contextual_type`: such a paren shows context
  rather than climbing past it;
- `symbols.rs` `jsdoc_parameter_annotation`: `ReturnStatement` joins the
  host walk, as `jsdoc_function_host` already had it for signatures.

**Measured.** +28 lines, +1 case.

### 3.7 Refused: `Object.<K, V>` as `Record<K, V>` (`declared.rs`)

`getIntendedTypeFromJSDocTypeReference`'s `Object` arm (`checker.go:23059`)
answers `getTypeAliasInstantiation(Record, [K, V])` for a valid index key,
else `any`; TSR declines it (`/** @type {Object.<string, number>} */ var o`
prints `any`). Ported as a `Record` type reference (the way `flow.rs`'s `in`
narrowing builds one) and measured against `e20cdd4`: **types +0, and one
diagnostics loss** — `checkJsdocSatisfiesTag2` EMPTY_RIGHT → EMPTY_WRONG,
a spurious TS2353 at `isEven`. The excess-property check reads a `Record`
reference reached through the `@typedef` as having no string index. Refused
on that number. It is worth re-measuring once the excess check resolves
`Record`'s index signature through an alias.

### 3.8 TS2352 on a JSDoc cast (`assertion_overlap.rs`, `check.rs`)

Routed by r5-smallcodes2. **Native.** `/** @type {T} */ (e)` is a reparsed
`AsExpression`, so `checkAssertionDeferred` runs on it and, because the type
node is `Reparsed`, reports at the type node (`checker.go:12323`).
**What TSR had.** `check_assertion_overlap` returned at `in_js_file`, and no
check visited the cast paren. **Port.** The comparison moves into
`check_assertion_overlap_parts(node, expression, annotation, error_node)`.
`check_jsdoc_cast_overlap` feeds it the paren's expression and
`jsdoc_cast_annotation`, with the type node as `error_node`. `check_node`
gets one JS-guarded `ParenthesizedExpression` arm. The written-assertion
path is unchanged. **Measured.** Diagnostics +2 cases, types unchanged, no
losses.

## 4. Needed outside this lane's files

| function (file) | change | cases | owner |
|---|---|---:|---|
| `check_super_expression` (`expressions.rs`) | §3.3's diff | 6 | hub |
| `get_type_for_variable_like_declaration` (`symbols.rs`) | §3.1's diff | 2 | main |
| `check_import_call_expression` (`calls.rs`) | §3.2's diff | 8 | main |
| `has_no_contextual_type` (`signatures.rs`) + new `assignment_context.rs` | §3.4's diff | 1 | r5-printer2 |
| unmapped type parameter after inference from an `any` argument (`inference.rs` / `calls.rs`) | native gets no candidate from `any` and takes the default (`unknown`, or `any` in JS under `InferenceFlagsAnyDefault`); TSR gaps the call | 1 (`inferingFromAny`, 17 lines) | main `.9` |
| require-destructuring binding elements as aliases (`binder.rs`, `symbols.rs` `declaration_of_alias_symbol`) | `const { K } = require(…)` binds `K` as an alias in native; needed before declared.rs can resolve `@param {K}` | 2 | main `.5` |
| late-bound expando members (`binder.rs`, `members.rs`) | `exports["__assignment"]` and its late binding (§2.2) | 2 (`declarationEmitLateBoundJSAssignments`, `expandoFunctionSymbolPropertyJs`) | main `tsr-2zk.5` |
| `get_type_from_type_reference` (`declared.rs`) | admit an unrenamed require-destructuring `BindingElement` on the alias road (§2.2) | up to 2 (`commonJSImportClassTypeReference`, `commonJSImportExportedClassExpression`), unmeasured | r5-declared3 |
| getSymbolChain through a local require alias (printer) | `require("fs")` names the module by the alias in scope (S2a) | 4 | main `.39` |
