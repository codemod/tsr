# r6-smallcodes4: eleven small sole-code clusters

Lane on epic `tsr-2zk`, item `tsr-2zk.1132`, vendor `5b1047d`. The method is
the one in `r5-smallcodes.md`, `r5-smallcodes2.md` and `r5-smallcodes3.md`.
List the diagnostics cases that are WRONG on exactly one code. Classify each
against a native `tsgo` built from the pinned submodule
(`scripts/offline-cargo/build-tsgo.sh`). Then port the root causes.

Frozen base: `claude/beautiful-shannon-ar5gh0` `b18aec06` (main `17265fac`
plus bookkeeping).

- Diagnostics dump: 12,238 rows. 5,530 RIGHT, 5,596 EMPTY_RIGHT, 1,063
  WRONG, 49 EMPTY_WRONG.
- Types dump: 556,303 lines. 549,853 RIGHT, 843 GAP, 5,607 WRONG.

Setup: PyPI answers 403 here, so `tomlkit` could not be installed
(r5-operators3 §4). A stdlib-only stand-in for `parse`, `inline_table` and
`dumps`, kept in the session scratchpad and not committed, built the vendored
tree.

## 1. Ownership and how the work ships

Every hook this lane needs lands in a file it does not own (`check.rs`,
`nullable_operand.rs`). So the logic lives in new files this lane creates.
Each new file is committed with its `mod` line in `lib.rs`. Its `impl` block
carries `#[expect(dead_code)]` until its hook diff is applied, and the diff
removes the attribute (the r5-jsdoc2/3 precedent). A hook diff also carries
its unit test, because the test fails without the hook.

Apply order and status:

| # | Diff | New file | Status |
|---|---|---|---|
| 1 | `r6-smallcodes4-namespace-not-found.diff` | `namespace_not_found.rs` | lossless, §2.2 |
| 2 | `r6-smallcodes4-unknown-operand.diff` | `unknown_operand.rs` | **held**: four losses from inference producers, §3.1 |

The diffs touch disjoint hunks of `check.rs`. Each applies to the base alone,
and 2 applies on top of 1.

## 2. Lossless

### 2.2 TS2503 / TS2833 — `namespace_not_found.rs`

Cases on the base, each WRONG on TS2503 alone:

| Case | Shape |
|---|---|
| `strictModeReservedWord` | missing: `var b: public.bar`, where `public` is a local `var` |
| `intrinsicKeyword` | missing: `let intrinsic: intrinsic.intrinsic`, where `intrinsic` is the variable being declared |
| `unknownSymbols2` | missing: `import d = asdf;` |
| `verbatimModuleSyntaxInternalImportEquals` | missing: `import f1 = NonExistent;` |
| `instanceofOperatorWithRHSHasSymbolHasInstance` | extra: `T extends globalThis.Function ?` |

Native, for a name resolved with `SymbolFlagsNamespace`:

- `resolveEntityName` fails into `onFailedToResolveSymbol` (`checker.go:1564`)
  with `Cannot_find_namespace_0`. Under that meaning only three of its arms
  can answer:
  - `checkAndReportErrorForUsingTypeAsNamespace` (`:1608`), for a name that
    resolves with `Type &^ Namespace`;
  - the suggested lib (`:1585`), which keeps TS2503;
  - the spelling suggestion (`:1591`), which is TS2833.

  The other `checkAndReportErrorFor…` arms each need a value or type meaning,
  an `extends` clause or an export specifier. So **a value used as a
  namespace is TS2503**.
- `import a = b` resolves its unqualified right side as a namespace. That is
  "case 1" of `getSymbolOfPartOfRightHandSideOfImportEquals`
  (`checker.go:5020`), reached from `checkImportEqualsDeclaration` →
  `checkImportBinding` → `resolveAlias`.
- `resolveEntityName` returns nil for a missing name (`ast.NodeIsMissing`,
  `checker.go:15773`).
- `globals` holds a `Module` symbol named `globalThis` (`checker.go:962-964`).

TSR, before this diff:

- `check_qualified_type_name_at` (`check.rs`) declined when the left name
  resolved under **any** of `TYPE`, `VALUE` or `NAMESPACE` (its §302). The
  `TYPE` part is right: that is the TS2702 arm, and r5-smallcodes3 §3.2 still
  holds its port. The `NAMESPACE` part is right too, because an enum resolves
  as a namespace. The `VALUE` part has no upstream counterpart.
- The identifier arm of `import a = b` was never checked. Only the qualified
  arm was (§559).
- The binder has no `globalThis` symbol (`binder.rs`,
  `merge_global_augmentation`), so `globalThis.Function` in a type failed the
  namespace lookup.

The new file:

- `resolves_as_namespace`: the `Namespace | Alias` lookup, plus the name
  `globalThis`, spelled as the name as `typeof globalThis` already is
  (`expressions.rs`);
- `report_cannot_find_namespace`: `onFailedToResolveSymbol` for this meaning,
  with the type arm left silent as before;
- `check_import_equals_identifier_reference`: case 1, with the
  `NodeIsMissing` return.

The diff hooks two sites in `check.rs`:

- the `ImportEqualsDeclaration` arm of the check walk calls
  `check_import_equals_identifier_reference`;
- the §302 decline becomes `!resolves_as_namespace` →
  `report_cannot_find_namespace`.

Probes, native and TSR identical:

```text
import d = asdf;                                        TS2503 'asdf'
function f1() { let intrinsic: intrinsic.intrinsic; }   TS2503 'intrinsic'
namespace Outer { export interface T {} } let o: Outr.T; TS2833 … Did you mean 'Outer'?
import mod = globalThis;  type F = globalThis.Function; (none)
import abstract class D {}                              TS1005 only
```

The first measurement without the `NodeIsMissing` return and the `globalThis`
arm lost three cases, and those losses are why both are there:

- `globalThisDeclarationEmit3` (`import mod = globalThis`);
- `classAbstractManyKeywords` (`import abstract class D {}`);
- `importDeferTypeConflict1` (`import type defer * as ns1`, whose module
  reference is a missing identifier).

**Measured** unfiltered against the frozen base:

- Diagnostics: +8 cases, zero losses. Right went from 11,126 to 11,134.
  - Converted: `strictModeReservedWord`, `intrinsicKeyword`, `unknownSymbols2`,
    `verbatimModuleSyntaxInternalImportEquals`,
    `instanceofOperatorWithRHSHasSymbolHasInstance`.
  - TS2833 converted three more: `importedModuleAddToGlobal`,
    `primaryExpressionMods`, `invalidInstantiatedModule`.
  - Rows: 20 changed rows across 18 cases, and every one moves toward the
    baseline. That includes still-WRONG `aliasErrors` ×2,
    `declarationEmitUnknownImport`/`2`, `declareModifierOnImport1`,
    `jsdocInTypeScript`, `reexportedMissingAlias`,
    `invalidImportAliasIdentifiers`, `parser519458`, and the dropped extra
    rows of `reuseTypeAnnotationImportTypeInGlobalThisTypeArgument` and
    `globalThisAmbientModules`.
- Types dump: verdicts unchanged (549,853 RIGHT).
- slowcases: clean on both dumps.
- Ir (callgrind, `--singleThreaded --pretty false --noEmit`): domain-model
  1,090,830,758 → 1,091,248,337 (+0.038%). generic-imports 343,085,575 →
  343,083,681 (−0.0006%). Both are inside the base's own run-to-run spread,
  which r5-smallcodes3 §2.3 measured at about ±0.09%. CLI output is
  byte-identical on both projects.

Unit test: `tests/namespace_not_found.rs`, which ships in the diff.

**Not covered:** `globalThis.X` as a type now reports nothing, but its *type*
is still not `X`. `resolve_entity_name` (`declared.rs`) cannot find the
namespace either. The type dump has no line for that case, so nothing
measures it. The faithful fix is a `globalThis` symbol in the binder (main's).

## 3. Held

### 3.1 TS18046 / TS2571 — `unknown_operand.rs`

Cases on the base:

| Case | Shape |
|---|---|
| `useUnknownInCatchVariables01` | missing: `e.toUpperCase()`, `e++` on a catch variable |
| `privateNameAndAny` | missing: `thing.#foo` ×5 with `thing: unknown` |
| `es2016IntlAPIs` | missing: `err.toString()` |
| `controlFlowAliasingCatchVariables(useunknownincatchvariables=true)` | missing: `e.toUpperCase()` |
| `reverseMappedPartiallyInferableTypes` | missing: `k.length` on a `k` TSR infers differently (not converted) |

Native: `checkNonNullTypeWithReporter` (`checker.go:7413`) reports an
`unknown` operand before asking any nullable fact, under `strictNullChecks`.
The message is `'{0}' is of type 'unknown'` for an entity name expression
shorter than 100 characters, and `Object is of type 'unknown'` otherwise. Its
readers include the property and element access receivers
(`checkNonNullExpression`) and the operator operands.

TSR had the arm only at the call head (`check_non_null_callee`, `calls.rs`).
`check_non_null_type_reporting` (`nullable_operand.rs`) declined it on purpose,
and the receiver check (`check_null_or_undefined_receiver`, `check.rs`) never
asked. The new file holds the arm. The diff calls it from both sites.

Probe (`try {} catch (e) { void e.toUpperCase(); void e++; void e(); }`,
`t.#foo`, `-u`, `u + 1`, `(… as unknown)[0]`): every TS18046/TS2571 row is
identical in native and TSR.

**Measured** with both diffs against the frozen base: +5 cases from this
diff, and **four losses**. Each loss is an extra report on an operand whose
TSR type is wrong upstream of the report, as the types dump shows:

| Loss | Report | TSR type (types dump) |
|---|---|---|
| `badInferenceLowerPriorityThanGoodInference` | `result.BLAH` (receiver) | `result` is `unknown`, native `{ BLAH: number; }` (0:17, 0:19 WRONG) |
| `typeArgumentInferenceWithClassExpression3` | `foo(class { prop = "hello" }).length` (receiver, TS2571) | `T` inferred `unknown` from the class-expression argument, native `string` (0:6 WRONG) |
| `nonInferrableTypePropagation2` | `n > 0` in `exists((n) => n > 0)` (operator) | `(n: unknown) => boolean`, native `(n: number) => boolean` (0:30, 0:31 WRONG) |
| `inferFromGenericFunctionReturnTypes2` | `n > 10` in `wrap(n => n > 10)` (operator) | `Mapper<unknown, any>` (0:139, 0:141–145 WRONG) |

These are the producers `nullable_operand.rs`'s own decline comment named.
Muting the report for those shapes would be the §3a heuristic.

- **Owner:** `inference.rs` and `calls.rs` (main's).
- **Falsifier:** once those four inferences match native, the diff measures
  +5 with no loss. If any of the four still reports then, the diff is wrong.
