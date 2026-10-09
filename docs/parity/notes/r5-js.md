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
| S8 | JSDoc tags: `@param`/`@type`/`@template`/`@overload`/`@callback`/`@satisfies`/typedef reach | `jsdoc_*.rs`, `signatures.rs`, the parser's EOF comment | r5-jsdoc4 and successors | 53 | 239 | `jsdocSignatureOnReturnedFunction`, `overloadTag1`, `typeTagWithGenericSignature`, `checkJsdocTypeTagOnExportAssignment1` |
| S7 | names: `typeof m26` vs `typeof m4`, `default: typeof mod2`, `import(".")` vs `import("./index")`, import qualifiers | the symbol-chain printer | main `.39`, r5-modules2 | 15 | 160 | `nodeModulesAllowJs1` ×4, `nodeModulesAllowJsSynchronousCallErrors` ×4, `reactImportDropped` |
| S1 | expando / property-assignment declarations: late-bound `f[k] = v`, `Object.defineProperty` merged with `module.exports =`, `exports.a.b =`, `x.#p.q =` | `binder.rs` late binding, `members.rs` / `symbols.rs` (`getTypeOfFuncClassEnumModule`'s expando members) | main `tsr-2zk.5` | 5 | 53 | `declarationEmitLateBoundJSAssignments`, `expandoFunctionSymbolPropertyJs`, `ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge` |
| S3 | JS value / require aliases in type positions: `@type {D}` with `const D = require(…)`, `@param {K}` with `const { K } = require(…)`, `ex.Crunch` through `var ex = require(…)`, `typeof import(export= module)` | `declared.rs` (`getTypeFromJSDocValueReference`, the import-type arm) | r5-declared3 | 5 | 22 | `jsdocImportType`, `commonJSImportClassTypeReference`, `varRequireFromTypescript` |
| S9 | not JS-specific: self-referential redeclared `var a = f(a)`, unreachable returns, generic callback inference | `symbols.rs`, `signatures.rs`, `inference.rs` | main, r5-printer2 | 5 | 25 | `inferingFromAny`, `unreachableJavascriptUnchecked` |
| S4 | `this.x =` in constructors, constructor functions and static blocks | `assignment_declarations.rs`, `this_expression.rs` | unowned | 4 | 25 | `javascriptThisAssignmentInStaticBlock`, `jsDeclarationEmitDoesNotRenameImport`, `controlFlowInstanceof` |
| S2c | `import("x")` of a shorthand ambient module (`declare module "x";`) printed `{ default: typeof import("x") }` | `calls.rs` `check_import_call_expression` | main (diff, §3.2) | 4 | 24 | `nodeModulesAllowJsDynamicImport` ×4 |
| S2a | `require("x")` of an ambient module prints through the local alias (`typeof fs`, `typeof _`) | the symbol-chain printer | main `.39` | 4 | 4 | `ambientRequireFunction` ×2, `bundlerSyntaxRestrictions` ×2 |
| S5 | uncontextual parameter: an undocumented JS setter parameter reads the getter | `symbols.rs` `get_type_for_variable_like_declaration` | main (diff, §3.1) | 2 | 4 | `accessorDeclarationEmitJs`, `privateNamesIncompatibleModifiersJs` |
| S6 | `super` in a `static {}` block (TS too) | `expressions.rs` `check_super_expression` | hub (diff, §3.3) | 1 | 6 | `classFieldSuperAccessibleJs1` |
| S2b | `module.exports = { [sym]() {} }` gaps | `symbols.rs` export= literal | main | 1 | 9 | `jsDeclarationsComputedNames` |
| S2d | a module that default-imports itself | `symbols.rs` alias circularity | main | 1 | 3 | `selfReferentialDefaultNoStackOverflow` |

Reading it:

- **Most of the JS remainder is JSDoc (53) and names (15).** Neither is a
  JS-checking producer; they stay with the JSDoc lane and the printer.
- **The five producers the brief named hold 20 cases between them**:
  expando 5 (S1), CommonJS 10 (S2a–d), value references in type
  positions 5 (S3), constructor `this` 4 (S4), uncontextual parameters 2
  (S5). S2a is the printer, not CommonJS: `require("fs")` resolves and the
  alias `fs` already prints `typeof fs`, but the bare call names the module
  through its local alias, which is getSymbolChain's job.
- Owners: S1 is main's binder and member image (`tsr-2zk.5` is in
  progress there); S3 is declared.rs (r5-declared3). This lane ports S5,
  S2c and S6 as measured diffs, and S4 directly (its producer files are
  unowned).

## 3. Ports

### 3.1 S5: the JS setter parameter (diff, `symbols.rs`)

Measurement pending.

### 3.2 S2c: dynamic import of a shorthand ambient module (diff, `calls.rs`)

Measurement pending.

### 3.3 S6: `super` in a static block (diff, `expressions.rs`)

Measurement pending.
