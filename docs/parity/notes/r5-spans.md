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
| `deleteOperatorInvalidOperations` (`delete ;`), `jsFileCompilationBindMultipleDefaultExports` (`export default var`) | TS1102/TS2703 and TS2528, one column late | the same missing-node arm: the operand or expression is missing | r5-smallcodes2 §3.1 (sent to that box) |
| `awaitUsingDeclarationsWithImportHelpers` | TS2354 at `using`, not `await` | the report is on the declaration list, and `parseVariableDeclarationList` (`parser.go:1563`) consumes the `await` itself, so the list starts at `await` | parser, §1.4 (held) |
| `deleteExpressionMustBeOptional_exactOptionalPropertyTypes` | TS2790 on `f.b`, `f.e` missing; extra on `g.a`, `g.c` | `checkDeleteExpressionMustBeOptional`'s `exactOptionalPropertyTypes` arm (`checker.go:10829`) is not ported; `Partial<Foo>`'s members resolve to `Foo`'s own symbols | §1.5 (held) |
| `bigintArbirtraryIdentifier` | not a span. `export { foo as 0n }` misses TS2304 on `foo`; `import { 0n as foo }` reports an extra TS2304 on `foo` | recovery in the specifier checks | not taken; module/specifier owners |
| `complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound` | TS2322 at a different line (33 vs 40) | relation of `{ type: T }` to `Pick<ChannelOfType<…>, "type">` | relater (r5-relater7) |
| `inferTypePredicates` | TS2322 at a different line | inferred type predicates | `inference.rs` / flow (main's) |
| `keyRemappingKeyofResult` | TS2322 at a different line | `keyof` of a key-remapped mapped type | `mapped.rs` (r5-mapped5) |
| `generatorReturnTypeInference` | TS7057 at a different line | the contextual type of a `yield` in an unannotated generator | main's |

Seven of the 20 are fixed by the two diffs in §1.3. Six are r5-smallcodes2's
missing-node arm. Two are held (§1.4, §1.5). Five are semantic, not span
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

*In progress.*
