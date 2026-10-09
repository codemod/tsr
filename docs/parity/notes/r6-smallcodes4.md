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

Every hook this lane needs lands in a file it does not own: `check.rs`,
`nullable_operand.rs`, `declared.rs`, and the parser's `lib.rs` and
`parsed_file.rs`. So the logic lives in new files this lane creates, in one of
three ways:

- **A checker file**: committed with its `mod` line in `tsr-checker`'s
  `lib.rs`, which is a hub file where adding is allowed. Its `impl` block
  carries `#[expect(dead_code)]` until its hook diff is applied, and the diff
  removes the attribute (the r5-jsdoc2/3 precedent).
- **A parser file**: committed but not compiled. The parser's `lib.rs` is
  main's, so its `mod` line ships in the hook diff.
- **A held port that needs another file's private function**
  (`import_type_node.rs` needs `get_symbol_of_exports` made `pub(crate)`):
  ships whole in its diff.

A hook diff also carries its unit test, because the test fails without the
hook.

Apply order and status:

| # | Diff | New file | Status |
|---|---|---|---|
| 1 | `r6-smallcodes4-namespace-not-found.diff` | `namespace_not_found.rs` | lossless, §2.1 |
| 2 | `r6-smallcodes4-pragma-diagnostics.diff` | `tsr-parser/src/pragma_diagnostics.rs` | lossless, §2.2 |
| 3 | `r6-smallcodes4-import-call-trailing-comma.diff` | `import_call_grammar.rs` | lossless, §2.3 |
| 4 | `r6-smallcodes4-unknown-operand.diff` | `unknown_operand.rs` | **held**: four losses from inference producers, §3.1 |
| 5 | `r6-smallcodes4-import-type-node.diff` | `import_type_node.rs` (in the diff) | **held**: +38/−8 type lines, all losses printer-side, §3.2 |

Diffs 1, 3 and 4 touch disjoint hunks of `check.rs`. Every diff applies to the
base alone, and each applies on top of the ones before it.

## 2. Lossless

### 2.1 TS2503 / TS2833 — `namespace_not_found.rs`

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

### 2.2 TS1453 / TS1084 — `tsr-parser/src/pragma_diagnostics.rs`

Cases on the base: `nodeModulesTripleSlashReferenceModeOverrideModeError`
(`module=node16`, `node18`, `node20`, `nodenext`). Each is missing TS1453 at
`/// <reference types="pkg" resolution-mode="esm"/>` (1,45).

Native: `processPragmasIntoFields` (`parser.go:6581`) runs at the end of
`parseSourceFile` and reports two parse errors:

- `parseResolutionMode` (`:6641`): TS1453 at the value's range for anything
  but `import` or `require`;
- `parseErrorAtRange(pragma.TextRange, Invalid_reference_directive_syntax)`
  (`:6623`): TS1084 for a `reference` pragma naming none of `types`, `lib`
  and `path`.

TSR: `pragma::parse_file_references` already collects both shapes
(`invalid_resolution_modes`, `invalid_reference_directives`), and its own
unit test checks them. No caller reported them.

The new file appends both, pragma by pragma in source order, after every other
parse error, with `parseErrorAt`'s same-position guard (`parser.go:327`). The
diff adds the `mod` line and calls it from the two places a file's parse
diagnostics are assembled: `parse_into` (`lib.rs`) and
`ParsedFile::parse_with_options` (`parsed_file.rs`).

Probe (`resolution-mode="esm"`, `<reference foo="x" />`): native and TSR both
give `TS1453` at (1,45) and `TS1084` at (2,1).

**Measured** unfiltered against the frozen base:

- Diagnostics: +5 cases, zero losses. The four `…ModeOverrideModeError`
  cases convert, and `invalidReferenceSyntax1` (TS1084) converts too. 5 rows
  matched, none unmatched.
- Types dump: verdicts unchanged.
- slowcases: clean on both dumps.
- Ir: domain-model 1,091,398,238 → 1,091,517,113 (+0.011%), generic-imports
  343,069,926 → 343,073,172 (+0.001%). This base run read 0.05% above the one
  in §2.1, which is the box's own spread. CLI output is byte-identical.
- Cost: `ParsedFile::parse_with_options` now reads the pragmas at parse time
  as well as at publication. The read only scans the leading comments.

Unit test: `tsr-parser/tests/pragma_diagnostics.rs`, which ships in the diff.

### 2.3 TS1009 on `import(x,)` — `import_call_grammar.rs`

Cases on the base, each missing one TS1009:

- `dynamicImportTrailingComma` (`import(path,)`, `commonjs`);
- `importAssertion1(module=commonjs)` and `importAttributes1(module=commonjs)`
  (`3.ts(10,50)`, the comma after the attributes argument).

Native: `checkGrammarImportCallExpression` (`grammarchecks.go:2162`)
reports `checkGrammarForDisallowedTrailingComma(arguments)` on the comma
(`list.End() - 1`, `:671`) when the module kind is none of
`node16`..`nodenext`, `esnext` and `preserve` (`:2182`). It does not return,
so TS1324 for a second argument is still reported. The arms before it return
first: `verbatimModuleSyntax` with `commonjs`, `es2015`, and type arguments.
`import.defer(…)` cannot reach the arm. Its own test returns for every module
kind but `esnext` and `preserve`, and those two skip the arm.

TSR: STATUS §999 declined TS1009 because this AST records no
`NodeList.HasTrailingComma` for call arguments (`parse_arguments`,
`tsr-parser/src/expression.rs`, drops `parse_delimited_list`'s flag).
Recording it would change the parser's six `parse_arguments` call sites
(main's) and still leave no comma position. The comma is instead found where
the parser found it: the first token after the last argument, past whitespace
and comments, read from the file's source text (`ModuleHost::source_text`, as
`export_star_conflicts.rs` reads it). The two answers differ only on an
argument list the parser did not end at that comma, and such a list is a
parse error, which the grammar arm never reports into.

- **Rejected: the parser flag.** It is the faithful data model, but it costs
  six call-site edits in main's parser for one reader. Revisit it if a second
  argument-list reader (TS1009 on `new`, or the printer) needs the flag.

The diff calls `check_import_call_trailing_comma` from the `CallExpression`
arm of the check walk (`check.rs`), before `check_import_call_specifier`.

Probe (`import(path,)`, `import(path /* x */ , )`, `import(path)` under
`commonjs` and `esnext`): native and TSR identical, TS1009 at (2,12) and
(3,21) under `commonjs` only.

**Measured** unfiltered against the frozen base:

- Diagnostics: +3 cases, zero losses, 3 rows matched.
- Types dump: verdicts unchanged.
- slowcases: clean on both dumps.
- Ir: domain-model 1,091,397,560 → 1,090,978,107 (−0.04%), generic-imports
  343,083,976 → 343,069,416 (−0.004%). Both are noise. CLI output is
  byte-identical.

Unit test: `comma_after` in the file itself.

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
Muting the report for those shapes would be the heuristic `box-protocol.md` §3a rejects.

- **Owner:** `inference.rs` and `calls.rs` (main's).
- **Falsifier:** once those four inferences match native, the diff measures
  +5 with no loss. If any of the four still reports then, the diff is wrong.

### 3.2 `import("…").T` as `T`'s declared type — `import_type_node.rs`

Found while classifying TS2883. In `declarationEmitUsingTypeAlias1`, TSR
types `thing.arg` as `any`, where `thing: SomeType` and
`SomeType = import('./inner').SomeType`. Native types it `Other`. The cause
is `get_type_from_import_type_node` (`declared.rs`, r6-declared's file). It
does not resolve `import("…").T`. It mints an opaque named object type whose
text is the written node, so every member access on it is a gap.

Native: `getTypeFromImportTypeNode` (`checker.go:24575`) works in three
steps:

1. resolve the module and follow `export =`;
2. walk the qualifier through each namespace's exports (`Namespace` meaning,
   then `Type` for the last segment);
3. `resolveImportSymbolType` (`:24657`), which resolves the alias and calls
   `getTypeReferenceType`.

The port answers the non-generic, argument-free target, where every arm of
`getTypeReferenceType` is the declared type in regular form. Anything else
keeps the minted name. The diff:

- calls it first from `get_type_from_import_type_node`;
- makes `get_symbol_of_exports` `pub(crate)`.

Probe: `declare const o: import("./inner").Other; const e: number = o.other;`
and the same through an alias. TSR's TS2322 rows (`'string'`, `'Other'`,
`'SomeType'`) now match native's.

**Measured** unfiltered against the frozen base:

- Diagnostics: unchanged. No TS2883 case converts. Each still prints a
  different type or specifier (§4).
- Types dump: +38 RIGHT (38 WRONG → RIGHT), from
  `nodeModulesImportTypeModeDeclarationEmit1`,
  `nodeModulesImportAttributesTypeModeDeclarationEmit` and `…Errors` (3 lines
  each, over node16..nodenext) and `allowsImportingTsExtension` (2).
- **Losses: 6 RIGHT → WRONG and 2 GAP → WRONG.** Every one is a print where
  the minted written text was right only by coincidence:

| Lines | Native | TSR with the diff | Cause |
|---|---|---|---|
| `declarationEmitUsingTypeAlias1` 1:0, 1:1 | `import("./inner").Other` | `Other` | the alias name prints bare: `symbol_chain` (`checker.rs`) is never asked for it |
| `declarationEmitCrossFileCopiedGeneratedImportType` 2:2 | `import("../projA").Foo` | `Foo` | same |
| `declarationEmitForGlobalishSpecifierSymlink2` 3:0 | `import("typescript-fsa").A` | `A` | same |
| `declarationEmitForGlobalishSpecifierSymlink` 5:0 (GAP) | `import("typescript-fsa").A` | `import("../p1/node_modules/typescript-fsa/src/impl").A` | symlinked package specifier (r5-modules §5.2, not ported) |
| `symbolLinkDeclarationEmitModuleNamesImportRef` 0:0–0:2 | `import("styled-components").InterpolationValue[]` | `import("../../../folder/node_modules/…").InterpolationValue[]` | same symlink cause |

- **Owners:** the alias-name qualification is `checker.rs`/`printing.rs`
  (main's, r6-printer). The symlink specifiers are `module_specifiers.rs` and
  the loader (`tsr-2zk.1098`).
- **Falsifier:** once a type alias's name goes through the same
  `getSymbolChain` as a class name, and symlinked packages generate their
  package specifier, the diff measures +38 with no loss. A loss outside those
  rows means the port is wrong.

## 4. TS2883 — not converted

The four cases. Each needs a print that TSR does not produce yet. The
r5-modules §6 sink reports what the printer generates.

| Case | What blocks it |
|---|---|
| `declarationEmitUsingTypeAlias1` | §3.2's import-type resolution, then the alias-name chain printing `import("../node_modules/some-dep/dist/inner").SomeType` from `src/index.ts`. TSR prints `import("./inner")…`, the written text, today. |
| `declarationEmitCommonJsModuleReferencedType` | a tuple-returning signature's members print bare (`[SomeProps, …]` against `[import("foo").SomeProps, …, import("foo/node_modules/nested").NestedProps]`), so no `node_modules` specifier reaches the sink |
| `declarationEmitObjectAssignedDefaultExport` | the default export's intersection prints `NonReactStatics<"div">` bare, where native prints `import("styled-components/node_modules/hoist-non-react-statics").NonReactStatics<"div">` |
| `declarationEmitReexportedSymlinkReference3` | symlinked package path (r5-modules §5.2) |

All four blockers are printer qualification inside composite types
(`checker.rs`/`printing.rs`) or symlink specifiers. None of them is in this
lane's files.
