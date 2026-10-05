# Parity lane `parser` (tsr-2zk.2) — notes

Judgment calls made while porting parser recovery and grammar diagnostics
(TS1xxx) for epic `tsr-2zk`. The lane's cases are `docs/parity/lanes/parser.txt`
(318 at `06f25e0`). Upstream anchors are typescript-go @ `5b1047d`.

## How the grammar checks are wired

Upstream's `checkGrammar*` functions report through `grammarErrorOnNode` /
`grammarErrorAtPos`, which stay silent in a file with parse diagnostics, and
most of them sit behind `!c.checkGrammarModifiers(node)`. This port's generic
walk already has that gate in one place: `check_grammar_modifier_shapes`
(`crates/tsr-checker/src/check.rs`) is called only when the modifier chain did
not report on the node, and returns at once when the file has parse errors.
The lane's new grammar checks are dispatched from there
(`check_grammar_behind_modifiers`, `crates/tsr-checker/src/grammar.rs`) rather
than from new arms in `check_node`, so the two upstream guards come for free.

Checks upstream reports with `c.error` instead (TS1313 on an empty `if` body)
must stand regardless of parse errors, so they have a second, ungated entry,
`check_parser_lane_statement`, called once from `check_node`. That call is the
lane's only line in the shared walk.

Because upstream's grammar functions `return` on the first report, a port that
splits one upstream function across several Rust functions has to keep the
short-circuit. Where the split already existed the earlier arm is a
predicate the later ones consult:

- `index_signature_parameter_shape_error` is the shape half of
  `checkGrammarIndexSignatureParameters` (TS1096, TS1017–TS1020, TS1022). The
  pre-existing type-reading halves (`check_index_signature_key_type`,
  `check_index_signature_parameter_type`) now ask it first. TS1071 on an index
  signature's modifiers now marks `modifier_chain_reported`, because upstream's
  `checkGrammarIndexSignature` is `checkGrammarModifiers(node) ||
  checkGrammarIndexSignatureParameters(node)`.

- `check_grammar_decorator_target` is `checkGrammarModifiers`' `this`-parameter
  test (TS1433) and decorator arm (`NodeCanBeDecorated` → TS1206 / TS1249).
  It runs at the top of `check_modifier_order` (this port's
  `checkGrammarModifiers`) and, for class expressions, which never reach that
  function, from the lane dispatcher. A report marks both
  `modifier_chain_reported` and `decorator_error_reported`, the two sets the
  rest of the chain already reads. Its legacy private-name arm answers "can be
  decorated" so that `check_decorated_private_name`, which reports it, stays the
  single reporter.
- `check_grammar_property` and `check_grammar_variable_declaration_exclamation`
  are the remaining arms of `checkGrammarProperty` / `checkGrammarVariableDeclaration`.
  The arms before them were already ported as separate rules
  (`check_field_named_constructor`, `check_interface_computed_name`,
  `check_using_is_initialized`, `check_const_is_initialized`), so the new code
  replays their conditions as bounds rather than calling them.
- `invalid_dynamic_name` is `checkGrammarForInvalidDynamicName`'s test. Upstream
  asks `isNonBindableDynamicName` first, which needs the name's type, but a
  bindable name must be an entity name and an entity name never reports, so the
  report condition reduces to *dynamic and not an entity name* — syntax only.

## Trailing commas: a flag, and where the comma is

Upstream answers `NodeList.HasTrailingComma()` from the list's span and reports
TS1009 at `list.End() - 1`. This AST keeps child slices without spans, so the
fact is recorded as `NodeFlags::HAS_TRAILING_COMMA` on the owner (already done
for variable declaration lists and binding patterns; now also for heritage
clauses). The comma's position is recoverable only where the owner node *ends*
at the comma: a variable declaration list and a heritage clause both do, so the
report is the owner's last byte.

Not ported for that reason: TS1009 on `import(a,)` (`dynamicImportTrailingComma`)
and TS1025 on an index signature's parameter list. The call and the signature
end at `)` / after the type, and the checker has no source text to search
backwards. Porting them needs either the comma's offset recorded by the parser
or source text in the checker; neither exists, and adding a side table for two
cases was not worth the new owner.

## `await using` is not `CONST | USING` here — and why

Upstream's `NodeFlagsAwaitUsing` is `NodeFlagsConst | NodeFlagsUsing`
(`nodeflags.go:51`) and `parseVariableDeclarationList` consumes `await using`
itself, so the list starts at `await`. This parser eats the `await` at each
call site and flags the list plain `USING`, so `await using` and `using` are
the same tree. `checkGrammarVariableDeclarationList`'s TS1493/TS1494 and
TS1547/TS1548 need the difference.

**Tried and refused.** Porting the encoding (list takes the `await`, flags
`CONST | USING`) converted `awaitUsingDeclarationsInForIn` and kept the
diagnostics and types gates loss-free once `check_const_is_initialized`
compared the whole block-scope kind, but the coverage run's
`printer_round_trip` row fell from 11816/11822 to 11800/11816: the printer
(`list_keyword` in `crates/tsr-printer/src/statements.rs`, which tests `USING`
first) writes `using` for the list, and the reparse no longer matches. That
file belongs to another lane, so the encoding is reverted and the printer change
(write `await using` for `CONST | USING`) is reported to the integrator; with it
the parser change is the `KindAwaitKeyword` arm of `parseVariableDeclarationList`
plus dropping the three call sites' `await` skip.

What stands without it:

- TS1547/TS1548 (case clause): a `VariableStatement` with no modifiers starts
  where its list does unless the parser consumed an `await` in front of it, so
  the gap identifies `await using`, and the report spans from the statement
  start, where upstream's list starts.
- TS1493/TS1494 (`for…in`): no such gap is visible in a `for` head, so the arm
  declines (`usingDeclarationsInForIn`, `awaitUsingDeclarationsInForIn` stay
  missing). It reports TS1494 if a list ever carries `CONST | USING`.
- `check_const_is_initialized` now compares `blockScopeKind == Const` as
  upstream does instead of testing one bit; it doubled TS1155 on `await using a;`
  (`awaitUsingDeclarations.8`) under the refused encoding, and is correct either
  way.

## Reserved words are not primary expressions

`parse_primary_expression` turned every reserved word into a
`KeywordExpression`, so `1 +⏎return;` parsed `return` as the right operand and
reported nothing. Upstream's `parsePrimaryExpression` takes only `this`,
`super`, `null`, `true` and `false` as token nodes (and `import` reaches the
same arm for `import(…)` / `import.meta`); any other reserved word goes to
`parseIdentifierWithDiagnostic(Expression_expected)`, which reports TS1109 at
the token and does not consume it, so the statement that follows parses
normally. The contextual-keyword arm after it is now bounded to non-reserved
keywords, or a reserved word would have reached `parse_identifier` and taken
TS1359's message instead. Five lane cases converted on this alone
(`parserErrorRecovery_Block1`, `…_ObjectLiteral3`, `…_SwitchStatement1`,
`parserMissingToken1`, `parserUnaryExpression5`).

## Positions that differ from the node a report hangs on

- **TS1003 after a dangling dot** (`parseRightSideOfDot`): reported at the next
  token's *full* start, i.e. right after the dot, not on the next line's token.
  `report_missing_right_side_of_dot`.
- **TS1109 at end of file** (`parseIdentifierWithDiagnostic(Expression_expected)`
  from `parsePrimaryExpression`'s default): zero-width at the EOF token's full
  start, matching what `report_missing_identifier` already did for TS1003.
- **TS1477** (`parsePropertyAccessExpressionRest`) spans `<`…`>` of the
  instantiation expression. Neither bracket is kept in the AST, so the member
  loop records the range when it builds the `ExpressionWithTypeArguments`.
- **TS1142 on `throw`⏎** (`checkThrowStatement`) is at the missing
  expression's `Pos()`, the end of the `throw` keyword. This parser places a
  missing identifier at the next token's trimmed start, so the position is the
  statement start plus `len("throw")`. Not ported: the
  `checkGrammarStatementInAmbientContext` guard in front of it.

- **TS1126 for a backslash at end of text** (`scanEscapeSequence`,
  `scanner.go:1694`): reported at the end, unconditionally. The string loop's
  TS1002 at the same position that follows is then dropped by the parser's
  same-position dedup, as upstream's is (`unterminatedStringLiteralWithBackslash1`).

## Contexts the ancestor walk had wrong

- A **method's computed name** is evaluated outside the method, so `yield` in
  `{ [yield 0]() {} }` inside a generator is legal (`generatorTypeCheck42`). The
  TS1163 walk already passed through accessors and properties reached via a
  computed name; methods were missing.
- A **function declaration's own name** is parsed before its `async` opens the
  await context (`parser.go:1715`), so `async function await() {}` has no
  TS1359; an `async function` *expression*'s name is inside it.
- A **catch clause's declaration** never reaches `checkGrammarVariableDeclaration`
  (`checkCatchClause` calls `checkVariableLikeDeclaration` alone), so
  `catch ({ a })` has no TS1182.

## JSON values are validated

`parseJSONText` ends with `validateJsonValue` (`parser.go:232`), which this
port's `parse_json_text` did not have: a single-quoted string or a non-string
property name is TS1327, a property that is not an assignment TS1136, any other
non-JSON value TS1328. They are appended to the parser's diagnostics directly,
as upstream does, so `parseErrorAtRange`'s same-position dedup does not apply.
The quote style comes from the string literal's `token_flags`.

## Grammar reports that were not gated on parse errors

`check_illegal_decorator` (`reportObviousDecoratorErrors`) reported in files
with parse diagnostics; upstream's `grammarErrorOnFirstToken` is silent there
(`decoratorOnUsing`, where TS1134 is the file's only error). It is now gated
like every other grammar report.

## Smaller arms ported

- TS1184 — `checkGrammarMethod`: an object-literal method with modifiers other
  than a lone `async`, on the method's first token (beside the TS1042 per
  modifier `checkGrammarObjectLiteralExpression` already reports).
- TS1267 — `checkPropertyDeclaration`: an `abstract` property with an
  initializer. A `c.error`, so it lives in the ungated entry.
- TS1106 — `checkGrammarForInOrForOfStatement`: `for (async of …)` outside an
  await context, approximated by the enclosing function's `async`.

## TS1141 is a `c.error`

Both of upstream's TS1141 sites (`checkExternalImportOrExportDeclaration`,
`getTypeFromImportTypeNode`) report with `c.error`, so they stand in files with
parse errors — which is where the corpus has them (`import * from Zero from
"./0"` reads `Zero` as the specifier). The import-type check had a parse-error
gate; it is removed. The declaration check is new, in the ungated entry, and
bounded to a declaration at file or module-block level unless parse errors
silence `checkGrammarModuleElementContext`, whose report would return first.

Removing the gate exposed a recovery divergence: `import abstract class D {}`
was parsed as an import clause whose specifier was the class expression.
Upstream decides between an import clause and `import x = …` on the token after
the identifier (anything but `,`/`from` is import-equals,
`parser.go:2263`), and parses the reference with
`parseEntityName(allowReservedWords: false)` and `require` only before `(`.
Those three are now mirrored in `parse_import_declaration` /
`parse_module_reference`.

## Ambient context without `NodeFlagsAmbient`

The parser never sets `NodeFlags::AMBIENT`. Where an arm reads it
(`checkGrammarProperty`'s and `checkGrammarVariableDeclaration`'s `!` arms) the
port asks `file_is_ambient`, the member's own `declare`, and
`declaration_is_in_an_ambient_context` (the ancestor walk the rest of the
checker uses). `checkGrammarVariableDeclarationList`'s ambient `using` arm and
`checkThrowStatement`'s ambient guard are not ported.

## Performance

Measured at the end of the session against the frozen baseline binary.
`whole_project_perf.py` read 1.037 (domain-model) and 1.097 (generic-imports)
at 21 samples, but on this 4-core box its A/A run (baseline against a copy of
itself) read 1.043 and swapping the two slots flipped the sign (0.97–0.99). An
interleaved run, 61 alternating invocations of each binary, read wall 1.003 /
CPU 0.989 on domain-model and wall 0.957 / CPU 0.961 on generic-imports. The
new work per node is a `match` on the node kind in two dispatchers; the parser
changes add no lookahead the old code did not already take (the import-equals
decision replaces an equivalent `peek_kind`).

## Blocked outside this lane

- TS1101 (`with` in strict mode) and TS1212 (`yield` as an identifier in strict
  mode) are binder diagnostics (`binder.go:1430`, `checkStrictModeIdentifier`).
  Their cases also miss a checker diagnostic (TS2410 / TS2304), so neither half
  alone converts a case.
- `ClassStaticBlockDeclaration` has no `modifiers` field in the generated AST,
  so `@dec static {}` cannot carry TS1206 (`classStaticBlock19`); the decorator
  is parsed elsewhere and its expression is checked (an extra TS2304).
- `checkDecorators` skips the decorator expressions of a node
  `NodeCanBeDecorated` rejects; `decorators.rs` still checks them, which is the
  extra TS2304 left in `classExpressionWithDecorator1`.
- TS1098 (`Type parameter list cannot be empty`) needs `class C<>` to be told
  from `class C`; both parse to an empty slice and no flag records the brackets.
- TS1200 (`Line terminator not permitted before arrow`, `checkGrammarArrowFunction`)
  reads the source text of the `=>` token's full span; the checker has no
  source text and the token records no preceding-line-break flag
  (`arrowFunctionErrorSpan`).
