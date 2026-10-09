# r5-spans: position-only diagnostics, and TS2589's currentNode

Lane on epic `tsr-2zk`, vendor `5b1047d`. Two items:

- `tsr-2zk.1104`: the diagnostics cases that are WRONG **only** on positions;
- `tsr-2zk.1103`: TS2589, which this port never reports (§2).

Frozen base: branch head `1301779` (batch AJ snapshots). Diagnostics dump
12,238 rows: 5,439 RIGHT, 5,595 EMPTY_RIGHT, 1,152 WRONG, 52 EMPTY_WRONG.
Types dump 556,291 lines: 548,897 RIGHT, 879 GAP, 6,515 WRONG.

Setup note: PyPI answers 403 here, as r5-operators3 §4 recorded. A
stdlib-only stand-in for `tomlkit`'s `parse`, `inline_table` and `dumps`,
kept in the session scratchpad and not committed, built the vendored tree.

## 1. Position-only cases (`tsr-2zk.1104`)

### 1.1 Listing

These are the WRONG rows of `diagverdictdump` whose expected and actual code
multisets are equal. A throwaway script found 20, the brief's list. For each
one, the differing `(file, line, column, code)` entries are compared with the
file name included. Leaving the file out misreads `bigintArbirtraryIdentifier`
(§1.2).

### 1.2 Classification

| Case | Misplaced diagnostic | Native rule | Owner / fix |
|---|---|---|---|
| `es5-oldStyleOctalLiteralInEnums`, `literals`, `objectTypeWithStringNamedNumericProperty`, `scannerNumericLiteral8` | TS1121 one column late on `-01` | `scanNumber`'s `withMinus` (`scanner.go:1966`): after a `-` token the span starts one character earlier and the suggestion gains the sign | scanner (main's), §1.3 diff A |
| `parserKeywordsAsIdentifierName2`, `scannerS7.4_A2_T2` | TS1010 at the comment opener | `s.error(Asterisk_Slash_expected)` (`scanner.go:677`) is `errorAt(s.pos, 0)`: zero width at the end of the text | scanner, §1.3 diff A |
| `decoratorOnClassMethodThisParameter` | TS1433 one column late (`, @dec this`) | the **parser** reports it at `modifiers.Nodes[0].Loc` (`parser.go:3335`), which starts at the full start; the checker's `grammarErrorOnFirstToken` is then silent, because the file has parse diagnostics | parser + `grammar.rs`, §1.3 diff B |
| `commaOperatorWithoutOperand`, `MemberFunctionDeclaration5_es6`, `parserEqualsGreaterThanAfterFunction1`, `…2` | one column late, or the whole declaration | `GetErrorRangeForNode` (`scanner.go:2649`): a missing node keeps its full start, with no `SkipTrivia` | `error_span`'s missing-node arm: r5-smallcodes2 §3.1 |
| `deleteOperatorInvalidOperations` (`delete ;`), `jsFileCompilationBindMultipleDefaultExports` (`export default var`) | TS1102/TS2703 and TS2528, one column late | the same missing-node arm: the operand or expression is missing | §1.3a, a diff stacked on r5-smallcodes2 §3.1 (its reporters read raw spans) |
| `awaitUsingDeclarationsWithImportHelpers` | TS2354 at `using`, not `await` | the report is on the declaration list, and `parseVariableDeclarationList` (`parser.go:1563`) consumes the `await` itself, so the list starts at `await` | parser, §1.4 (held) |
| `deleteExpressionMustBeOptional_exactOptionalPropertyTypes` | TS2790 on `f.b`, `f.e` missing; extra on `g.a`, `g.c` | `checkDeleteExpressionMustBeOptional`'s `exactOptionalPropertyTypes` arm (`checker.go:10829`) is not ported; `Partial<Foo>`'s members resolve to `Foo`'s own symbols | §1.5 (held) |
| `bigintArbirtraryIdentifier` | not a span. `export { foo as 0n }` misses TS2304 on `foo`; `import { 0n as foo }` reports an extra TS2304 on `foo` | recovery in the specifier checks | not taken; module/specifier owners |
| `complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound` | TS2322 at a different line (33 vs 40) | relation of `{ type: T }` to `Pick<ChannelOfType<…>, "type">` | relater (r5-relater7) |
| `inferTypePredicates` | TS2322 at a different line | inferred type predicates | `inference.rs` / flow (main's) |
| `keyRemappingKeyofResult` | TS2322 at a different line | `keyof` of a key-remapped mapped type | `mapped.rs` (r5-mapped5) |
| `generatorReturnTypeInference` | TS7057 at a different line | the contextual type of a `yield` in an unannotated generator | main's |

Seven of the 20 are fixed by the two diffs in §1.3. Four are fixed by
r5-smallcodes2's missing-node arm, and two more by §1.3a's diff stacked on
it. Two are held (§1.4, §1.5). Five are semantic, not span
rules, and are routed above.

### 1.3 Shipped diffs (main's files)

Neither diff edits a file this lane owns, so both ship as patches:

- **A**: `r5-spans-scanner-octal-minus-and-unterminated-comment.diff`
  (`crates/tsr-scanner/src/lib.rs`).
  - `withMinus` reads the previous token. This scanner still holds it in
    `self.token` while it scans the next one, so the old comment ("this
    scanner does not keep one here … never a wrong position") was wrong on
    both counts. The span now starts one character left and the suggestion
    gains the `-`.
  - The unterminated comment reports zero width at the end of the text.
- **B**: `r5-spans-parser-this-parameter-modifiers.diff`
  (`crates/tsr-parser/src/expression.rs`, `crates/tsr-checker/src/grammar.rs`).
  - `parseParameterWorker`'s `this` arm reports TS1433 at the first
    modifier's `Loc`. Its start is the previous token's end (`node_end()`
    read before `parse_modifiers`); its end is the modifier node's.
  - `check_grammar_decorator_target`'s report now follows
    `grammarErrorOnFirstToken`: silent in a file with parse diagnostics. It
    still returns `true`, so the rest of the modifier chain stays quiet, as
    upstream's `checkGrammarModifiers` return does.

Measured together, with both dumps unfiltered against the frozen base. The
rows the two diffs change are disjoint by case:

- diagnostics: RIGHT 5,439 → 5,446. Converted: the four TS1121 cases, the two
  TS1010 cases, and `decoratorOnClassMethodThisParameter`.
  `thisTypeInFunctionsNegative` gains its two expected TS1433 rows and stays
  WRONG. No other row's actual diagnostics changed, and both loss checks are
  empty;
- type lines: unchanged (548,897 RIGHT);
- slowcases: only the KNOWN_SLOW cases, on both dumps;
- Ir (callgrind, `--singleThreaded --pretty false --noEmit`): domain-model
  1,156,271,836 → 1,157,123,251 (+0.074%), generic-imports
  342,948,063 → 343,052,896 (+0.031%). The only hot-path addition is one
  token-kind test per parameter, so the difference is code layout. CLI output
  is `cmp`-identical on both projects;
- `tsr-scanner` and `tsr-parser` tests pass.

### 1.3a The missing-node reporters that read raw spans (stacked diff)

r5-smallcodes2's `error_span` missing-node arm
(`r5-smallcodes2-error-span-missing-node.diff`) does not reach
`deleteOperatorInvalidOperations` or
`jsFileCompilationBindMultipleDefaultExports`. Their reporters read the node's
raw span instead of going through `GetErrorRangeForNode`'s port (that box
measured this and handed the cases back). Upstream reports all three
through `GetErrorRangeForNode`:

- TS1102: the binder's `errorOnNode` (`binder.go:1402`), in this port
  `check_strict_mode_delete_expression` (`strict_mode.rs`);
- TS2703/TS2364: `checkReferenceExpression`'s `c.error(expr, …)`
  (`checker.go:13134`), in this port the operand-reference report in
  `check.rs`;
- TS2528: the binder's `createDiagnosticForNode` on the declaration name
  (`binder.go:265`), in this port `declaration_name_span` (`binder.rs`).

`r5-spans-missing-node-raw-span-sites.diff` applies **on top of** r5-smallcodes2's
diff. The two checker sites call `error_span`. The binder, which cannot
reach the checker, takes the same step for a zero-width name: back over the
whitespace before it, the same approximation of `Pos()` that diff makes
(comments are not stepped over).

The stack (r5-smallcodes2's diff plus this one), measured unfiltered against
the frozen base:

- diagnostics: +6 cases, r5-smallcodes2's four plus `deleteOperatorInvalidOperations`
  and `jsFileCompilationBindMultipleDefaultExports`. Seven WRONG rows change
  and none gets worse: `reservedWords2` 31 → 37 matched and 10 → 4 extra;
  `reservedWords3`, `parametersSyntaxErrorNoCrash2`/`3` and
  `varianceAnnotationsWithCircularlyReferencesError` each gain one matched
  row and lose one extra; the two `esDecorators-decoratorExpression.1` rows
  are level. Both loss checks are empty;
- type lines: unchanged;
- slowcases: only the KNOWN_SLOW cases;
- Ir: domain-model +0.103%, generic-imports −0.006%, for the whole stack. All
  of the new code is on error paths, so this is layout. CLI output is
  `cmp`-identical;
- `tsr-binder` and `tsr-checker` tests pass.

### 1.4 Held: the `await using` list span

`r5-spans-await-using-list-span.diff` makes `parse_variable_declaration_list`
consume the `await`, as upstream does, so the list starts there. It is
**not** shipped. Three checker sites recover `await using` from the gap
between the statement's start and the list's start, because this parser never
writes `NodeFlagsAwaitUsing`:

- `is_await_using_list` (`using_declaration.rs`);
- `await_using_keyword_start` (`module_format.rs`);
- the TS1492 keyword in `check.rs`.

Moving the span silently turns all three to "plain `using`". The faithful fix
is to flag the list `Const|Using` (upstream's `AwaitUsing`) together with this
span, and to delete the three gap heuristics. That is a parser-and-checker
change for main, and `contains(CONST)` call sites must then be audited, since
`AwaitUsing` sets the `Const` bit. It unlocks one case.

### 1.5 Held: `checkDeleteExpressionMustBeOptional`'s exact arm

`r5-spans-delete-exact-optional.diff` ports the arm: under
`exactOptionalPropertyTypes`, only the symbol's own `?` makes the operand
optional. The question is asked through `property_is_optional`, since this
binder never writes `SymbolFlagsOptional`. Measured: `f.b` and `f.e` become
right, `g.b` and `g.e` become wrong extras, and no verdict changes.
`delete g.a` on a `Partial<Foo>` receiver resolves to `Foo`'s own `a`, which
is a non-optional declared symbol, so `Partial`'s `?` is never seen. The
non-exact arm has the same problem (`g.a`, `g.c` are extras in both
variants). That is a mapped-member resolution issue, not a delete one. Ship
the diff once `Partial`'s members carry their own symbol.

## 2. TS2589 and currentNode (`tsr-2zk.1103`)

### 2.1 What upstream does

`c.currentNode` (`checker.go:596`) is set, with `c.instantiationCount` reset
to zero, by `checkSourceElement` (`:2243`), `checkDeferredNode` (`:2507`) and
`checkExpressionEx` (`:7561`). Each saves the previous value and restores it.
The current node is where these report TS2589:

- `instantiateTypeWithAlias`'s guard (`:22111`): depth 100 or 5M
  instantiations in one node;
- `getConditionalType`'s tail-recursion guard (`:24311`): 1,000 tail steps.

Eight corpus cases expect TS2589: `awaitedType`, `awaitedTypeStrictNull`,
`circularInlineMappedGenericTupleTypeNoCrash`, `limitDeepInstantiations`,
`recursiveConditionalCrash4`, `recursiveConditionalTypes`,
`circularIndexedAccessErrors` and `recursiveMappedTypes`.

### 2.2 The tracking (shipped as a diff)

`r5-spans-current-node-tracking.diff` touches only main's files plus the new
module, so the whole of it ships as one patch:

- new `current_node.rs`: `enter_current_node` / `leave_current_node`, the
  save, set, reset-count and restore pair;
- `checker.rs`: the `current_node: Option<NodeId>` field;
- `lib.rs`: the `mod` line;
- `check.rs`: `check_node` becomes a thin wrapper that makes the node
  current around the unchanged body, renamed `check_node_worker`;
- `check.rs`: `check_type_alias_circularity` makes the alias's **type node**
  current while it resolves the declared type, because
  `checkTypeAliasDeclaration` resolves the body under
  `checkSourceElement(typeNode)` (`:6897`);
- `expressions.rs`: `check_expression` takes over its existing count reset
  as the `checkExpressionEx` entry.

The walk visits every node, while upstream hands only source elements to
`checkSourceElement`. So `is_current_node_kind` skips names and bare tokens
(`Identifier`, `PrivateIdentifier`, `QualifiedName`,
`ComputedPropertyName`, every token kind). An identifier that is an
expression becomes current through `check_expression`, as upstream's does.
This port has no deferred-node queue, so `checkDeferredNode`'s entry has no
counterpart.

Two first attempts:

- **The hook inside `check_node`'s body** kept the saved value live across
  that function's very large body: Ir +0.217% on domain-model. The wrapper
  is the shipped shape.
- **Without the alias-node hook**, every alias-body report landed on the
  alias's name (`type P2 = …` at col 6), not the type reference (col 11).

Measured with the patch applied, against the frozen base:

- both dumps: no row changed at all (nothing reads the node yet);
- slowcases: only the KNOWN_SLOW cases;
- Ir: domain-model 1,156,271,836 → 1,157,101,812 (+0.072%), generic-imports
  342,948,063 → 342,918,617 (−0.009%);
- median child CPU, 21 samples against the base binary: domain-model 1.010,
  generic-imports 1.003; `diagnostics_match: true`;
- workspace tests: §2.4.

### 2.3 The reports (held)

`r5-spans-ts2589-report-sites.diff` applies on top of §2.2. It adds
`report_excessive_instantiation_depth` and calls it at:

- `instantiate_type`'s guard (`inference.rs`, the faithful `:22111` site);
- the three depth-100 refusals of the alias re-evaluation road in
  `declared.rs`, which is this port's way of instantiating an alias body:
  - the conditional-node evaluation;
  - `evaluate_conditional_alias`;
  - the distributive union arm.

A fourth site, the non-conditional generic alias body, added only false
reports and was dropped.

It is **not shippable**. Unfiltered, against the frozen base:

- right: every TS2589 row of `awaitedType` and `awaitedTypeStrictNull` (both
  still WRONG on two unrelated TS2322 rows), `limitDeepInstantiations` `4:9`,
  `recursiveConditionalCrash4` `16:7`, and `recursiveConditionalTypes` `16:11`
  and `47:12`;
- still missing: `circularInlineMappedGenericTupleTypeNoCrash`,
  `recursiveConditionalCrash4` `10:7`, `recursiveConditionalTypes` `35:11`
  (the tail guard), `circularIndexedAccessErrors`, `recursiveMappedTypes`;
- false rows in cases that were already WRONG: `limitDeepInstantiations`
  `3:33` and `5:9`, and `recursiveConditionalTypes` `46:12` (`B2`, where
  upstream reports only at `B3 = B2[0]`);
- 8 losses, all from reports that upstream does not make:

| Lost case | Site | Current node |
|---|---|---|
| `contextualTypeSelfReferencing` | conditional-node evaluation | array/object literal |
| `declarationEmitRecursiveConditionalAliasPreserved` | conditional node, `evaluate_conditional_alias` | conditional type |
| `genericCallOnMemberReturningClosedOverObject` | `instantiate_type` | call |
| `mappedTypeRecursiveInference2` | `evaluate_conditional_alias` | array literal, arrow |
| `ramdaToolsNoInfinite` | conditional-node evaluation | indexed access, reference |
| `tailRecursiveConditionalTypes` | `instantiate_type`, `evaluate_conditional_alias` | type reference |
| `mappedTypeAsClauseRecursiveNoCrash1` | conditional-node evaluation | mapped type, type operator |
| `recursiveTypesWithTypeof` (RIGHT → WRONG) | `instantiate_type` | call, new |

The cause is not the reporter: **this port's instantiation depth reaches 100
on programs where upstream's does not.**
`genericCallOnMemberReturningClosedOverObject` is the smallest case:
`example<number>()` instantiates `{ foo: <T2>(t2: T2) => typeof x; … }`,
whose members mention the object itself. Upstream's
`instantiateAnonymousType` makes a deferred instance whose members are
resolved only on demand, and it caches the instance per mapper, so the
recursion never happens. This port instantiates the members eagerly and
recurses until the guard silently returns `errorType`. That was invisible
until now, since an `errorType` that nobody prints changes nothing. The
alias road has the same shape: `limitDeepInstantiations`' generic
declaration `{ "true": Foo<T, Foo<T, B>> }[T]` and the out-of-constraint
`Foo<"false", {}>` both recurse here, while upstream defers the generic
indexed access and never resolves a missing `"false"` property.

So TS2589 waits on lazy anonymous-type instantiation with per-mapper
instance caching (`instantiateAnonymousType`, `resolveObjectTypeMembers`).
That is an objects/declared-road change, not a reporting one. The
conditional tail-recursion guard (`:24311`, 1,000 steps) has no counterpart
here either; `recursiveConditionalTypes`' `TupleOf<number, 1000>` needs it.
When those land, apply the held diff and re-measure: the eight rows above
are its falsifier.

### 2.4 Tests

`cargo test --workspace --release` passes with the scanner, parser and
tracking patches applied together. Clippy reports nothing in the touched
code; stable flags pre-existing code in `members.rs`, `signatures.rs`,
`templates.rs` and four other files. No new test file: the tracking has
no observable output until the reporter lands, and the reporter's test
belongs with it.
