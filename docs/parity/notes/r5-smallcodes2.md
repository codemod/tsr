# r5-smallcodes2: eight more small sole-code clusters

Lane on epic `tsr-2zk`, vendor `5b1047d`, run alongside r5-smallcodes and
using its triage-then-port method (`r5-smallcodes.md`). Target: the
diagnostics cases whose **only** wrong code is one of TS2300, TS2540, TS2352,
TS2695, TS2403, TS1156, TS2454 or TS7010.

Frozen baseline: branch head `e20cdd4` (batch AH snapshots). Diagnostics dump
12,238 rows, 11,021 RIGHT or EMPTY_RIGHT (5,431 + 5,590), 1,160 WRONG, 57
EMPTY_WRONG; types dump 556,291 lines, 548,751 RIGHT, 900 GAP, 6,640 WRONG.

Cases were listed with a throwaway script over `diagverdictdump`: WRONG and
EMPTY_WRONG rows whose differing `(file, line, column, code)` entries all carry
one target code. That is stricter than a code-set comparison, so it also lists
the cases where the code is present on both sides but at a different position
or count (marked *pos* below). 32 cases. Probes use a native `tsgo` built from
the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

## 1. Triage

| Code | Cases | Native site | Root cause in TSR | Where the fix lives |
|---|---|---|---|---|
| TS1156 (extra) | `constDeclarations-invalidContexts(alwaysstrict=true)`, `letDeclarations-invalidContexts` (*pos*: the extra line is the `const`/`let` inside `with`) | `checkWithStatement` (`checker.go:4156`) checks the expression only; the statement is never handed to `checkSourceElement` | the generic walk descends into a `with` body and the TS1156 rule fires there | §2.1, `check.rs` TS1156 functions |
| TS1156 (extra) | `exportNonInitializedVariablesInIfThenStatementNoCrash1` (commonjs, esnext) | `checkVariableStatement` (`checker.go:5767`): `!checkGrammarModifiers(node) && !checkGrammarVariableDeclarationList(…)` gates `checkGrammarForDisallowedBlockScopedVariableStatement` | the TS1184 `findFirstIllegalModifier` arm (`check_modifier_on_nested_statement`) does not stop TS1156 | §2.1 |
| TS2695 (extra) | `jsxInvalidEsprimaTestSuite`, `tsxErrorRecovery2`, `tsxErrorRecovery3` | `checkBinaryLikeExpressionWorker`'s comma arm (`checker.go:12534`): no report when the left operand's start lies inside a TS2657 parse diagnostic | the checker cannot see parse diagnostics | §2.2: a `ModuleHost` query |
| TS2695 (*pos*) | `commaOperatorWithoutOperand` (`( , )`: col 3 for 2) | `GetErrorRangeForNode` (`scanner.go:2649`): a **missing** node keeps its full start, no `SkipTrivia` | `error_span` on a zero-width node answers the token start | shared `error_span` — measured diff, §3.1 |
| TS7010 (*pos*) | `MemberFunctionDeclaration5_es6`, `parserEqualsGreaterThanAfterFunction1`, `…2` | same `GetErrorRangeForNode` arm, through the declaration's missing name | `error_span` falls back to the whole declaration when the name is missing | same diff, §3.1 |
| TS7010 (missing) | `wideningTuples7` | `reportErrorsFromWidening` with `WideningKindFunctionReturn` (`'bar' … implicitly has an '[any]' return type`) | no widening report on a function expression's inferred return | tsr-2zk.11 (implicit-any widening, main's); not taken |
| TS2300 (missing) | `symbolProperty44` | `lateBindMember` (`checker.go:16035`): a late-bound name whose flags conflict (`getExcludedSymbolFlags`) reports on every declaration | no late-bound conflict report | §2.3 |
| TS2300 (missing) | `checkerInitializationCrash` | global-augmentation merge (`mergeSymbolTable` → `reportMergeSymbolError`) of `type VNode` with `export import VNode` from two files | — | §3.4 |
| TS2300 (missing) | `importTag4`, `jsDeclarationsDefaultsErr` | JSDoc `@import` / `@typedef` declarations in the binder | JSDoc | r5-jsdoc4 |
| TS2540 (missing) | `readonlyMembers(target=es2015)` (`this.c = 1` on a getter-only `c`, in the constructor and in an IIFE inside it) | `isAssignmentToReadonlyEntity` (`checker.go:27296`): the constructor exemption needs `symbol.Flags&SymbolFlagsProperty` | the exemption is asked of any symbol | §2.4, `readonly_target.rs` |
| TS2540 (missing) | `globalThisReadonlyProperties` | `globalThisSymbol` is minted with `CheckFlagsReadonly` (`checker.go:962`) | this port has no `globalThis` symbol (`binder.rs`, the global-augmentation merge), so `globalThis.globalThis` finds nothing to ask | §2.4 |
| TS2540 (missing) | `omitTypeHelperModifiers01`, `readonlyAssignmentInSubclassOfClassExpression` | `CheckFlagsReadonly` on a mapped member (`Omit`'s `Pick`, an inherited `Readonly<…>`) | the reused member owner does not carry the mapped modifier off the receiver itself | r5-mapped5 (`mapped.rs`) |
| TS2352 (extra) | `asOperatorASI` | `parseBinaryExpressionRest`: `as`/`satisfies` after a line break ends the expression | TSR's parser takes `10\nas \`…\`` as an as-expression | parser (main's), §3.2 |
| TS2352 (extra) | `genericTypeReferenceWithoutTypeArgument` (`<M.E>null`) | `getTypeFromClassOrInterfaceReference`: wrong type-argument count → `errorType` | a **qualified** generic reference without arguments is not `errorType` (the unqualified `<C>null` is) | errorType producers, r5-errorsplit5 §3.3 |
| TS2454 (extra) | `genericCloduleInModule2` (`var b: A.B; b.foo()`) | `checkIdentifier`: `assumeInitialized` for an `any` declared type | the same `A.B` producer: the type lines print `A.B` where upstream prints `any` | r5-errorsplit5 §3.3 |
| TS2352 (missing) | `checkJsTypeDefNoUnusedLocalMarked`, `jsDeclarationsDefault` | JSDoc `@type` casts | JSDoc | r5-jsdoc4 |
| TS2352 (missing) | `parenthesisDoesNotBlockAliasSymbolCreation` (*pos*) | comparability of `null` to an intersection with an alias-wrapped `InvalidKeys` | relater | r5-relater7 |
| TS2403 (missing) | `typeOfEnumAndVarRedeclarations`, `FunctionAndModuleWithSameNameAndCommonRoot`, `parserCastVersusArrowFunction1` | `checkVariableLikeDeclaration` → `isTypeIdenticalTo` | the unannotated-primary pair takes the assignability road; r5-vardecl §2 measured the structural arm at −5 | relater identity (r5-relater7); `r5-vardecl.md` §2 |
| TS2403 (missing) | `objectLiteralContextualTyping` (`var b = bar({})`) | inference: `T` with no candidate infers `unknown` | TSR infers `any` (type lines `unknown` vs `any`) | `inference.rs` (main's) |
| TS2454 (missing) | `augmentExportEquals5` | module augmentation of an `export =` module | `Request` resolves to `any` (type lines) | r5-modules2 |
| TS2454 (missing) | `moduleResolutionWithSymlinks`, `…_withOutDir` | symlinked package resolution | `MyClass2` resolves to `any` (type lines) | loader / r5-modules2 |

No case needs a `flow.rs` change: every TS2454 difference comes from the
declared type, not from the flow analysis.

TS7010 and main's implicit-any lane (`tsr-2zk.11`): no commit naming that
issue is among `origin/main`'s last 400, and the only TS7010 case that is its
kind (`wideningTuples7`, `reportErrorsFromWidening`) is left to it. The other
three are span cases in the shared `error_span`, not in `implicit_any.rs`.

## 2. Ported here

### 2.1 TS1156

Two causes, both in the variable-statement form.

**A `with` body is never checked.** `checkWithStatement` (`checker.go:4156`)
checks its expression, reports TS2410, and never hands the statement to
`checkSourceElement`; nothing below a `with` is checked at all. This port's
walk visits every child (`check_node`'s doc comment records the consequence:
every rule carries its own position test), so the TS1156 rule fired on the
`const c5 = 0` inside `with (obj)`. Both TS1156 functions now return for a
node inside a `with` statement's body, through the existing
`is_inside_with_statement`.

The alternative is to stop the walk at a `with` body for every rule, which is
upstream's shape. It is a change to `check_node` (the hub), so it would be
measured and shipped separately; this lane needed only its own rule.

**The modifier chain gates it.** `checkVariableStatement`
(`checker.go:5767`) asks `checkGrammarForDisallowedBlockScopedVariableStatement`
only when `!checkGrammarModifiers(node) && !checkGrammarVariableDeclarationList(…)`.
`if (true) export const x: T;` gets TS1184 from `findFirstIllegalModifier`'s
default arm and no TS1156. Two changes:

- `check_modifier_on_nested_statement` (that arm) records the node in
  `modifier_chain_reported`, as every other arm of the chain does (§876);
- the variable form's call moves from the `VariableStatement` arm at the top
  of `check_node` into `check_declaration_statement_container`, which the
  walk calls after the modifier chain, and is skipped for a node the chain
  reported. The type alias and interface forms are not gated
  (`checker.go:6878`, `:4996`) and are unchanged.

`checkGrammarVariableDeclarationList`'s half of the gate (trailing comma,
empty list, `using` placement) is not added: no listed case needs it.

### 2.2 TS2695 inside a TS2657 range

`checkBinaryLikeExpressionWorker`'s comma arm (`checker.go:12534`) skips the
report when the left operand's first token lies inside a TS2657 parse
diagnostic (`isInDiag2657`, reading `sourceFile.Diagnostics()`). The parser
builds `<div/><div/>` as a synthetic comma expression and reports TS2657 over
it, so without the exemption every adjacent-JSX recovery also reported
TS2695.

The checker had no way to read parse diagnostics: `FileContext` carries only
`has_parse_errors`. `ModuleHost` gains `parse_diagnostics(file, nodes)`,
witnessed against the node table as `source_text` is, defaulting to none;
`Program` answers its `ProgramFile`'s list. Rejected: adding the list to
`FileContext`, which every one of its ten constructors would have to fill, and
re-deriving the TS2657 range from the tree, which would be a second copy of
the parser's recovery.

### 2.3 TS2300 from `lateBindMember`

`lateBindMember` (`checker.go:16005`) is where a late-bound member's conflict
is reported: when the late-bound symbol of its name already has flags that
`getExcludedSymbolFlags` of the new declaration excludes, every declaration of
that symbol, the early-bound member of the same name if any, and the new
declaration get TS2300. The new declaration then goes into a fresh symbol
that is not stored. Two getters `get [Symbol.hasInstance]()`
(`symbolProperty44`) are the case: `GetAccessorExcludes` contains
`GetAccessor`, while `checkObjectTypeForDuplicateDeclarations` (state 2, kind
2) says nothing.

This port has no member-resolution step to hang it on, so
`late_bound_member_conflicts` replays the table build over the members
`check_object_type_for_duplicate_declarations` already gathers, in source
order, instance and static separately, and its reports join that function's
(deduplicated by node and name, as upstream's diagnostic collection does).
Accepted limitation: upstream binds across all declarations of a merged
symbol; a conflict between two merged interface declarations is not seen.

The same function now also does `combineSymbolTables(earlySymbols,
lateSymbols)` (`checker.go:15974`): a late-bound property whose name an
early-bound member of the same table already has merges into that symbol, so
`[c0]: number; 1: number` with `const c0 = "1"` is one symbol of two
declarations and both are TS2300 `'1'` (`dynamicNamesErrors`, still WRONG on
a missing TS2717).

### 2.4 TS2540

- `isAssignmentToReadonlyEntity` (`checker.go:27296`) grants the constructor
  exemption only when `symbol.Flags&SymbolFlagsProperty != 0`. A getter-only
  accessor is readonly in its own constructor too (`readonlyMembers`'s
  `this.c = 1`, directly and inside an IIFE). The exemption in
  `assignment_is_inside_the_declaring_constructor` was asked of any symbol.
- `globalThisSymbol` is minted with `CheckFlagsReadonly` (`checker.go:962`)
  and is the `globalThis` entry of the globals table it exports, so
  `globalThis.globalThis = …` is TS2540. This port has no such symbol (the
  global-augmentation merge comment in `binder.rs` explains why), and the
  diagnostic road answers the one name on the `typeof globalThis` receiver
  directly. The type road already printed `any` there.

### 2.5 Measured

§2.1–§2.4 together, four commits, both dumps unfiltered against the frozen
base, both loss checks empty:

- diagnostics: 11,021 → 11,031 RIGHT or EMPTY_RIGHT of 12,238. Converted:
  `constDeclarations-invalidContexts(alwaysstrict=true)`,
  `letDeclarations-invalidContexts`,
  `exportNonInitializedVariablesInIfThenStatementNoCrash1` (commonjs,
  esnext), `jsxInvalidEsprimaTestSuite`, `tsxErrorRecovery2`,
  `tsxErrorRecovery3`, `symbolProperty44`, `readonlyMembers(target=es2015)`,
  `globalThisReadonlyProperties`. No other row changed verdict.
- type lines: 548,751 RIGHT of 556,291, verdict columns unchanged.
- slowcases: only the KNOWN_SLOW cases, on both dumps.
- Ir (callgrind, release binaries, `--singleThreaded --pretty false`):
  domain-model 1,156,062,302 → 1,155,964,115 (−0.008%), generic-imports
  342,945,542 → 342,943,020 (−0.001%); CLI output `cmp`-identical.
- `cargo test --workspace --release` passes; clippy reports nothing in the
  touched files (stable 1.97 flags pre-existing code elsewhere).

## 3. Measured diffs and reports for other files

### 3.1 `error_span` on a missing node

*In progress.*

### 3.2 `as` after a line break (parser)

### 3.3 Qualified generic reference without type arguments

### 3.4 `checkerInitializationCrash`
