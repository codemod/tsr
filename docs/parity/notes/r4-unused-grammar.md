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

## §3 TS1355: `as const` on an operand it cannot apply to

**Forcing fact.** No producer: the baseline had two cases expecting TS1355
(`compiler/constantEnumAssert`, `conformance/constAssertions`), both WRONG
on that code alone.

**Native.** `checkAssertion` (`checker.go:12303`): when the type node
`isConstTypeReference`, report `c.error(node.Expression(), …)` unless
`isValidConstAssertionArgument` (`checker.go:13623`) holds — a literal,
array/object literal or template, a parenthesised valid argument, `-`/`+` on
a numeric (or `-` on a bigint) literal, or a property/element access whose
receiver, parentheses skipped, is an entity name expression resolving
(`resolveEntityName`, `Value`, `ignoreErrors`) to an `Enum` symbol.

**Choice.** `check_const_assertion_argument` (`grammar.rs`), dispatched from
`check_node`'s `AsExpression | TypeAssertion` arm beside
`check_assertion_overlap` — the port's existing home for the non-typing half
of `checkAssertion` (TS2352). The alternative, reporting from
`check_const_assertion` in `assertions.rs`, is the faithful call position but
not this lane's file; and it would run once per *type* request of the
expression, where the walk visits the node once — upstream dedups the
repeated `c.error` in `SortAndDeduplicateDiagnostics`, this port does not.
`isConstTypeReference` reuses `assertions::is_const_type_reference`, which
already knows the parser's nameless encoding of `as const`. A first draft
tested for an identifier `const` and reported nothing: the parser writes
`as const` as a type reference with *no* name.

The entity-name resolution is a local copy of
`meaning_mismatch.rs`'s private `resolve_entity_name_expression` (same
`resolveQualifiedName` shape) plus `resolveEntityName`'s final alias walk;
making that one `pub(crate)` is the smaller change but is outside this lane.

**Falsifier.** A TS1355 at a node this predicate accepts, or an `as const`
in an ambient or never-checked position that native does not report.

## §4 TS1211: a class declaration without a name

**Native.** `checkClassDeclaration` (`checker.go:4285`):
`node.Name() == nil && !HasSyntacticModifier(node, Default)` →
`grammarErrorOnFirstToken(node, …)`. No producer existed.

**Choice.** Reported from `check_parser_lane_statement` (`grammar.rs`, the
lane's per-node statement dispatcher), so no hub edit. The first token is
scanned from the node's start, which includes its modifiers and decorators
as upstream's `node.Pos()` does (`export class {}` reports on `export`).

Case converted: `compiler/exportClassWithoutName`.

## §2 A private member read through `this` from its own body

**Forcing fact.** `noUnusedLocals_selfReference` (`class P { private m() {
this.m; } }`) and `noUnusedLocals_destructuringAssignment` (`private f() {
({ f } = this); }`) each missed one TS6133: the by-name stand-in for
`markPropertyAsReferenced` (`unused.rs`, `note_member_name_at`) marked the
name on every read.

**Native.** `markPropertyAsReferenced` (`checker.go:27718`): when
`isSelfTypeAccess`, the nearest `IsFunctionLikeDeclaration` ancestor of the
access whose symbol *is* the property leaves it unmarked. `isSelfTypeAccess`
is true for a `this` receiver (`checker.go:27275`), and the destructuring
path passes `rightIsThis` only for a top-level `{…} = this`
(`checkBinaryLikeExpression`, `checker.go:12339`).

**Choice.** The by-name form: the receiver is `this` (or the top-level
destructuring source is `this`), and the nearest function-like ancestor is a
class method or accessor whose name text is the member's. Inside such a body
`this` is that class's instance, so the property `this.<name>` resolves to
that declaration — the symbol identity native compares. An arrow or function
expression in between is the nearest ancestor instead and the read counts,
as upstream. The static `Class.member` receiver keeps its own key and
`reference_is_inside_named_member` rule (§704 of the diag2 notes).

**What would make it wrong.** A `this` whose type is not the enclosing class
(a `this:` parameter on a method — methods cannot rebind `this` that way
without a function-like in between) or a member inherited under the same name
from a base class: `this.m` inside an override `m` reads the *derived* `m`,
which is the same name and the same answer.

Cases converted: `compiler/noUnusedLocals_destructuringAssignment`,
`compiler/noUnusedLocals_selfReference`.

## §5 `using _` is exempt like a `for…of` variable

`isUnreferencedVariableDeclaration` (`checker.go:7221`) exempts an
underscore-prefixed name on a parameter, on a `for…in`/`for…of` variable,
**and on any declaration whose combined node flags carry `NodeFlagsUsing`**
(`using` and `await using`, the latter being `Const | Using`). The port had
the first two only, so `using _ = …` reported TS6133. The flag lives on the
`VariableDeclarationList` in this parser (`tsr-parser/src/statement.rs`),
the level `getCombinedNodeFlags` reaches for a declaration.

Cases converted: `conformance/usingDeclarations.15`,
`conformance/awaitUsingDeclarations.15` (both EMPTY_WRONG → EMPTY_RIGHT).
