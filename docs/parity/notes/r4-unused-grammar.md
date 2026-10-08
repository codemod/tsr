# r4-unused-grammar — lane notes

Round-4 cloud lane (`tsr-2zk.903`): unused-declaration diagnostics
(`checkUnusedIdentifiers`) and checker-side grammar codes. Pinned native
source: `vendor/typescript-go` @ `5b1047d`. Owned files: `unused.rs`,
`grammar.rs`, `strict_mode.rs`.

## §1 `with` statements: TS1101 and TS2410

**Forcing fact.** Neither code had a producer: the frozen baseline had 15
cases expecting TS2410 and 16 expecting TS1101, every one WRONG, and no
`messages::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE` or
`THE_WITH_STATEMENT_IS_NOT_SUPPORTED_…` use anywhere in the checker.

**Native.** Two producers, one per code:

- `checkStrictModeWithStatement` (`binder.go:1428`) — a bare
  `errorOnFirstToken`. Despite its name it has no `inStrictMode` test at the
  pin (the binder has no such field; see `strict_mode.rs`'s module header for
  the same finding on the eval/arguments rules), no ambient test and no
  parse-error test.
- `checkWithStatement` (`checker.go:4156`) — `grammarErrorAtPos` from
  `SkipTrivia(node.Pos())` to `node.Statement().Pos()`, behind
  `!hasParseDiagnostics`.

**Choice.** Both live in one function, `check_with_statement_grammar`
(`strict_mode.rs`), called from `check_node` beside the other binder-side
strict-mode rules (a one-line dispatch in the hub). Putting TS1101 in the
`tsr_binder` crate instead was rejected for the reason `strict_mode.rs`'s
header gives: it needs nothing the binder has, and the binder rail stays out
of the blast radius. Putting TS2410 in a separate checker function would be
the more literal split; it was folded in because the two share the keyword
span computation and fire on exactly the same node. If `checkWithStatement`
ever grows a TSR counterpart (e.g. when TS1300 becomes portable), TS2410
moves there.

**Span of TS2410.** `node.Statement().Pos()` is the statement's full start,
i.e. the end of the `)` token. This port has trimmed node spans, so the end
is recovered by scanning one token from the expression's end. Under a unit
host without source text it falls back to `expression end + 1`, which is the
`)` when no trivia separates it; line/column (what the suite compares) do not
depend on the end.

**Not ported: TS1300** (`with` inside an async function block) — it reads
`NodeFlagsAwaitContext`, which the parser declares and never sets. Owner:
the parser's await-context tracking. No corpus case expects TS1300 at the
baseline.

**Falsifier.** A case whose baseline has TS1101 or TS2410 at a different
line/column than this port, or a `with` statement under parse errors that
native reports TS2410 for.

Cases converted: `compiler/letDeclarations-scopes`,
`compiler/letDeclarations-validContexts`,
`compiler/sourceMapValidationStatements`, `compiler/superCallsInConstructor`,
`compiler/withStatement`, `compiler/withStatementNestedScope`,
`conformance/typedefOnStatements`. The other eight cases with these codes now
match on them and stay WRONG on unrelated codes.
