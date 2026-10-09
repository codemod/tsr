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

## Round 2: the lists that bypassed `ParsingContext`

Round 2 (lane brief, 2026-10-06) started from the recovery divergence. The
list machinery (`crates/tsr-parser/src/list.rs`) only knew the contexts whose
loops had been ported onto it; type parameters, type arguments, tuple element
types, heritage clauses and their elements were hand loops that stopped at the
first token that was not a `,`. Two things follow from upstream's
`isInSomeParsingContext` that a hand loop cannot reproduce: a stray token inside
`<…>` is reported once with the list's own message and skipped, and an *inner*
list nested in one of these (a type literal inside type arguments, say) aborts
on a token the outer list would take. Both need the context bit to be set.

Ported (`parser.go` @ `5b1047d`): `PCHeritageClauseElement`,
`PCObjectLiteralMembers`, `PCTypeParameters`,
`PCTypeArguments`, `PCTupleElementTypes` and `PCHeritageClauses`, each with its
`isListElement`, `isListTerminator` and `parsingContextErrors` arm, and the
loops of `parseTypeParameters`, `parseTypeArguments` (type references and
heritage types), `parseTupleType`, `parseHeritageClauses` /
`parseHeritageClause` / `parseExpressionWithTypeArguments` moved onto
`parse_list` / `parse_delimited_list`.

Judgment calls:

- **`>` is split before the `PCTypeParameters` terminator test.** Upstream's
  scanner only ever produces a lone `>`; this one packs `>=`, `>>` eagerly. The
  arm calls `rescan_greater_than` first, as the `PCJsxAttributes` arm already
  did. `PCTypeArguments`' terminator is "anything but `,`", so it needs no split.
- **A heritage element is `parseLeftHandSideExpressionOrHigher`.** The
  bespoke `parse_left_hand_side_for_heritage` (identifier, class expression or
  parenthesised expression, then a member/call chain) could not parse
  `"".bogus` or `{ foo: string; }` and reported TS1003 where upstream parses an
  expression and the checker reports TS2507/TS2339. It is replaced by the
  general call/member parser; an instantiation expression it returns (`A<T>,`)
  is the element itself, as in upstream.
- **Type parameter modifiers are `parseModifiersEx(false, true, false)`.** The
  old loop took only `in`/`out`/`const`. `parse_modifiers_ex` gained a private
  worker with `allowDecorators`; `<public T>` now parses `public` as a modifier
  (the checker's TS1273) and `<in in>` names its parameter `in` (TS1359).
- **Tuple named-member rest types: tried and reverted.** Upstream parses the
  type after `name:` with `parseTupleElementType`, which accepts `...T`
  (`[rest: ...string[]]`, the checker's TS5087). Porting it removed the extra
  TS1110 in `namedTupleMembersErrors` but turned two RIGHT type rows
  (`Opt : Opt`, `Trailing : Trailing`) into `any`: the checker cannot type a
  `NamedTupleMember` whose type is an `OptionalType` or `RestType`. Reported to
  the integrator; with that checker fix the parser change is the one-line call
  in `parse_tuple_element`.

One type row changed verdict and is recorded rather than reverted:
`varianceAnnotationsWithCircularlyReferencesError:0:0` (`type T1<in in> = T1`).
The delimited list now yields two type parameters (`in`-modified with a
missing name, then a missing name), which is upstream's parse — its baseline
has TS2637 at both columns 9 and 11 and TS2300 for the duplicate empty name.
The old row printed `T1 : any` only because the old loop stopped after one
parameter. Upstream's `any` comes from TS2456 (the alias circularly references
itself through `T1` written without type arguments); this checker does not
detect that circularity and prints `T1<, >`. The fix belongs in the checker's
alias resolution (another lane).

Measured at the commit: diagnostics 3846 → 3851 RIGHT (+1 EMPTY_RIGHT), no
diagnostics losses; checker_types 7602 → 7618; `parser_reachable_target`
unchanged at 5031/10570; median CPU self-ratio 0.972 (domain-model) and 0.994
(generic-imports) at 21 samples.

### Object literals and computed names

`parse_object_literal` is now `parseDelimitedList(PCObjectLiteralMembers, …)`
(`parser.go:5615`). The hand loop already reported the missing `,` and skipped
a `;` separator (§218), but it *left the list* at any token that could not
start a member, where upstream asks `isInSomeParsingContext`: a token no
enclosing list wants is reported ("Property assignment expected") and skipped,
and the literal goes on to its `}`. ``{ `a`: 321 }`` (a template literal as a
name) therefore reports TS1136 at the template and closes cleanly instead of
TS1003 plus a statement-level cascade; the same shape converts the parse side
of `parserSymbolIndexer5`, `privateIndexer2` and
`objectTypesWithOptionalProperties2` (their remaining rows are checker ones).
The old loop's guard (§198 measured −64 files when continuing without it) is
subsumed: continuing is safe once the enclosing contexts are consulted.

`parseComputedPropertyName` parses a full expression with `in` allowed
(`parseExpressionAllowIn`, `parser.go:3476`); this port parsed an assignment
expression, so `[0, 1]` stopped at the comma. With the comma expression kept,
`checkGrammarComputedPropertyName`'s TS1171 is ported into
`check_grammar_object_literal_postfix_tokens` (`grammar.rs`), the port of
`checkGrammarObjectLiteralExpression`'s per-member arms, where upstream calls
it and ignores the result. The class-member call sites (behind
`checkGrammarProperty`, `checkGrammarMethod` and
`checkGrammarFunctionLikeDeclaration`/`checkGrammarAccessor`) are not ported:
`check_grammar_property` does not yet answer whether it reported, which their
short-circuit needs.

Measured at the commit: diagnostics 3852 RIGHT (+1), checker_types 7627,
`parser_reachable_target` 5031; CPU self-ratio 1.004 / 0.983.

### `allowReturnTypeInArrowFunction`

This parser had no counterpart of the flag upstream threads through
`parseAssignmentExpressionOrHigherWorker` (`parser.go:4081`). The true
branch of a conditional parses with it off (`parseConditionalExpressionRest`,
`:4562`), so in `b ? (c) : d => e` the ambiguous `(c) : d => e` — a valid
arrow signature with return type `d` — is refused once its body is parsed
unless another `:` follows it (`:4422`), and the group reparses as the
parenthesised true branch. Ported as `parse_assignment_expression_worker`
with the flag passed to the simple-arrow body, the assignment right operand,
the conditional's false branch and `parse_arrow_body`, as upstream passes it.
A definite arrow (`isParenthesizedArrowFunctionExpression` answering true)
always allows the return type, as `tryParseParenthesizedArrowFunctionExpression`
does.

The refusal comes after the body, so the ambiguous parse with the flag off
runs inside one `try_parse` (upstream's rewind). One deviation: when an
`async` was already consumed before the decision, this port cannot rewind it,
so an `async` arrow keeps its return type there. No corpus case reaches it.

Cases converted: `parserArrowFunctionExpression8`, `9`, `11`, `12`; `10`'s
parse now matches and its remaining row is a checker TS2304 on the arrow's
return type in the `.ts` file.

### `checkGrammarAccessor`'s body arms

`check_grammar_accessor` (`check.rs`) had only the parameter arms. The first
two arms are ported (`grammarchecks.go:1309`, `:1315`): a body-less accessor
outside an ambient context, a type literal or an interface that is not
`abstract` is "'{' expected" on its last character, and an `abstract`
accessor with a body is TS1318. They wait on `modifier_chain_reported`
because upstream reaches `checkGrammarAccessor` only after
`checkGrammarFunctionLikeDeclaration` (whose first test is
`checkGrammarModifiers`) reports nothing, and a report returns before the
parameter arms. Ambient is `file_is_ambient` or
`declaration_is_in_an_ambient_context`, since the parser never sets
`NodeFlagsAmbient` (see above). The TS1183 arm (a body in an interface or type
literal) is not added: `check_grammar_statement_in_ambient_context` already
reports TS1183 on such a body, and a second reporter would double it.

`compiler/giant`'s 36 missing TS1005 are all this; its remaining rows are the
checker's TS2386.

### `checkGrammarTypeOperatorNode`

Not ported before at all. Now in `grammar.rs`, dispatched for every
`TypeOperator` from `check_grammar_behind_modifiers` (a type operator has no
modifiers, so the gate there is only the file's parse diagnostics, which is
`grammarErrorOnNode`'s own). `unique` must apply to `symbol` ("'symbol'
expected" on the operand) and its owner, found through parenthesized types,
must be a `const` identifier-named variable of a variable statement (TS1332 /
TS1333 / TS1334), a `static readonly` class property (TS1331) or a `readonly`
property signature (TS1330); anything else is TS1335. `readonly` on a type
that is not an array or tuple is TS1354 on the keyword. All 60 grammar rows of
`uniqueSymbolsErrors` match; its remaining row is a checker TS2322.

### Tried and reverted: `<T>x` as a unary, not a primary

Upstream parses a type assertion in `parseSimpleUnaryExpression`
(`parser.go:5071`); `parsePrimaryExpression` has no `<` arm, so `new <T> x`
reports TS1109 at the `<` (the callee is a member expression). This port has
the arm in `parse_primary_expression`. Moving it converted
`parserTypeAssertionInObjectCreationExpression1` with no diagnostics loss,
but `tsr-printer`'s `recovered_new_type_assertion_does_not_gain_a_second_call`
pins the old tree (`new <any>Factory()` printed back verbatim); that test is
another lane's. Reported; with the test updated to upstream's tree the parser
change is the two-arm move.

### TS17019 / TS17020: `checkJSDocTypeIsInJsFile`

`T?`, `?T`, `T!`, `!T` outside a JS file are reported by the checker
(`checker.go:2584`), not the parser: the parser builds a `JSDocNullableType` /
`JSDocNonNullableType` and `checkJSDocType` calls `grammarErrorOnNode` with the
type written out. Ported in `grammar.rs` (`check_jsdoc_type_is_in_js_file`),
dispatched behind the modifier chain like the other grammar arms; postfix is
"the node starts where its operand starts", and the suggested type is the
operand's type unioned with `undefined` (postfix `?`) or `undefined | null`
(prefix `?`) unless it is `never` or `void`, i.e. `getNullableType`. The
messages match the baseline text verbatim on `parseInvalidNullableTypes`. The
TS8020 arm for every other JSDoc type in a TS file is not ported (no lane case
waits on it, and those kinds reach the checker on paths not yet upstream's).

## Design note: a JS-file flag in the parser (js lane ask)

Upstream's parser takes the script kind (`ScriptKindJS`, `JSX`, `TS`, `TSX`,
`JSON`) and derives two independent facts from it (`parser.go:300`): the
language variant (JSX for `.jsx`/`.tsx`) and the context flag
`NodeFlagsJavaScriptFile` (for `.js`/`.jsx`, and `.json`), which every node it
finishes inherits. This port's `ScriptKind` has `TypeScript`, `Tsx` and `Json`
only, so a `.js` file parses as `TypeScript` and a `.jsx` as `Tsx`, and the
second fact is lost.

Recommended shape, not built: add `Js` and `Jsx` variants (rather than a bool
beside the kind), keep `allows_jsx()` true for `Tsx | Jsx`, add
`is_javascript()` for `Js | Jsx | Json`, map `.js`/`.cjs`/`.mjs` and `.jsx` in
`from_file_name`, and record the fact once on the `SourceFile` node
(`NodeFlags::JAVASCRIPT_FILE`) rather than on every node: the checker already
walks to the file for `in_js_file`, so per-node flags would buy nothing. The
parser itself reads it in the places upstream reads
`contextFlags&NodeFlagsJavaScriptFile`, which are recovery-relevant:
`parseTypeArgumentsInExpression` returns nil in JS (`f<T>(x)` is a comparison
there), `parseTypeAnnotation`/`checkJSSyntax` record JS-only diagnostics, and
JSDoc reparsing (`reparser.go`) is enabled. Whether the reparser exists is
`tsr-2zk.34`'s decision; the flag is useful without it.

How we would know the shape is wrong: if a consumer needs the fact for a node
whose file cannot be reached cheaply (a synthesized node with no parent), the
per-file flag must become per-node.

## Carried parser recovery ports (box/lane-parser)

- `void` in type position (`parseNonArrayType`): `void` has its own arm with
  no keyword-dot lookahead, so `void.x` is the keyword type followed by a
  TS1005 at `.`; `string.x`/`any.x` stay dotted type references.
- Private names in binding positions (`parseIdentifierOrPatternWithDiagnostic`
  → `createIdentifierWithDiagnostic`): variable declarations pass TS18029,
  parameters TS18009, binding elements keep the default TS18016. The message
  is a parameter of `parse_binding_name_with_diagnostic`, not a per-caller
  pre-check, so the identifier is consumed by the one shared path.
- Catch clause (`parseCatchClause` → `parseVariableDeclaration`): the whole
  declaration is parsed, initializer and private-name policy included; the
  printer emits it through the shared `emitVariableDeclaration` port. The
  grammar arms of `checkCatchClause` (TS1196, TS1197) live in `grammar.rs`
  behind `grammarErrorOnFirstToken`; TS2492 (block-local redeclaration of
  the caught name) needs binder locals and is not ported.
- Enum members (`parseEnumMember`): the member is the JSDoc host
  (`withJSDoc`), so `{@link A}` on a member marks `A` referenced, and the
  initializer parses with `DisallowInContext` cleared.
- Type predicates (`parseTypeOrTypePredicate`, `parseNonArrayType`): only the
  `identifier is` prefix is return-position-specific; `this is T` and
  `asserts x [is T]` are type arms reachable anywhere (asserts guarded by
  `nextTokenIsIdentifierOrKeywordOnSameLine`), and the prefix is tried
  first, so `asserts is T` names a parameter `asserts`. The first arm of
  `checkTypePredicate` (TS1228 via `getTypePredicateParent`) is in
  `grammar.rs`; JSDoc-hosted predicates still lack the reparser parent.
- `intrinsic` (`parseTypeAliasDeclaration`, `parseNonArrayType`): a keyword
  type only as a whole alias body not followed by `.`; elsewhere a type
  reference (TS2304). `checkTypeAliasDeclaration`'s TS2795 for a
  non-compiler alias body is in `grammar.rs` (syntactic, no type queries).
- Function declaration names (`parseFunctionDeclaration`): the name is
  optional only under a `default` modifier (unless a binding identifier
  follows); otherwise `parseBindingIdentifier` reports TS1003.
- Modifier nodes (`parseModifier` → `finishNode`): a modifier ends where its
  keyword ends, so TS1029/TS1042/TS1044-family spans on a modifier no longer
  include the trailing trivia up to the next token.
- Ambient statements (`checkGrammarStatementInAmbientContext`): TS1036 and
  TS1183 go through `grammarErrorOnFirstToken` (the one helper in
  `grammar.rs`, which now returns whether it reported), so the span is the
  statement's first token; the once-per-block flag records the report
  result, as `hasReportedStatementInAmbientContext` does.
- `const` without initializer (`checkGrammarVariableDeclaration`): TS1155 is
  `grammarErrorOnNode(node)`, i.e. the declaration's error span (its name),
  not the whole declaration.
- Entity names (`parseEntityName` → `parseRightSideOfDot`): the right side of
  a dot in a type reference/query takes the same missing-name arm as
  property access — a line break then `identifierOrKeyword
  identifierOrKeyword` is TS1003 right after the dot with a missing
  identifier, so `var x: M.` before `namespace N {` leaves the declaration
  intact.
- Yield context (`NodeFlagsYieldContext`, `isYieldExpression`,
  `parseYieldExpression`, `isIdentifier`): the parser keeps upstream's
  `contextFlags` yield/await bits; the yield bit is set beside
  the await context by `with_function_context` from each signature's `*`
  (function declarations/expressions, class and object-literal methods) and
  cleared by constructors, accessors, arrow signatures/bodies, class static
  blocks, enum members, property initializers and type-member parameters.
  Outside it `yield` opens an expression only before an identifier, keyword
  or literal on the same line; inside it `yield` is no identifier, so
  `<T> yield 0` in a generator is TS1109. `yield` takes an operand only when
  one starts on the same line. The checker's TS1163 declines that stood in
  for the missing context are removed.
- JavaScript files parse in the JSX language variant (`getLanguageVariant`:
  TSX, JSX, JS and JSON), whatever `--jsx` says: `ScriptKind::allows_jsx`
  holds for `.js`/`.cjs`/`.mjs`. The loader's `jsx != None` condition in
  `tsr-compiler` `parse_options` is now dead for those names (out of lane).
  The checker's TS17004 for JSX in a `.js` file without `--jsx`
  (`checkJsxPreconditions`) is still missing (`parseUnaryExpressionNoTypeAssertionInJsx1/3`).
- `ScriptKind::JavaScript` (`.js`/`.cjs`/`.mjs`/`.jsx`) is the parser's
  `NodeFlagsJavaScriptFile` context: JSX tags skip type arguments there
  (`parseJsxOpeningOrSelfClosingElementOrOpeningFragment`).
- Function types (`isStartOfFunctionTypeOrConstructorType` →
  `parseFunctionOrConstructorType`): once `<` or an unambiguous `(` decides a
  function type, the parse commits; a missing `=>` is reported by
  `parseReturnType` and the return type parsed anyway, instead of falling
  back to a parenthesized type (`x: ()` is `'=>' expected`).
- `for await` (`parseForOrForInOrForOfStatement`): after `await` the `of` is
  `parseExpected` (TS1005 `'of' expected`), then `in` may still make a
  for-in, which carries no `await` token. `'{0}' expected` spells a keyword
  as its text (`scanner.TokenToString`), not its kind name.
- `abstract` member outside an abstract class (`checkGrammarModifiers`):
  TS1244/TS1253 are `grammarErrorOnNode(modifier)`, spanning the `abstract`
  keyword rather than the member.
- `'{0}' expected` arguments use `scanner.TokenToString`: the full inverted
  `textToToken` table for punctuation (`...`, `</`, …) and keyword text,
  never a `SyntaxKind` name.
- The other `grammarErrorOnFirstToken` / `GetRangeOfTokenAtPosition(node.Pos())`
  callers now use the same helper: TS1046 (`checkGrammarTopLevelElementForRequiredDeclareModifier`),
  TS1108/TS1104 (`checkReturnStatement`), TS1174 (`typeNodes[1]`), TS1163
  (`checkGrammarYieldExpression`), TS1308 (`checkGrammarAwaitOrAwaitUsing`'s
  non-async arm), TS1206 (`findFirstIllegalDecorator`, the legacy private-name
  arm) and the `checkGrammarModifiers` decorator/`this`-parameter arm. Each
  spans the scanned first token, not the whole node or one character.
- Missing closing brackets (`parseExpectedMatchingBrackets`): block, `if`/`do`/
  `while`/`with` parentheses and array/object literals report `'x' expected`
  and, when the opener was parsed and the report was not dropped by the
  same-position guard, attach TS1007 at the opener. `parseImportAttributes`
  and `parseImportType`'s attribute object attach it to whichever diagnostic
  is last if that one is TS1005, and the attribute members are parsed only
  after a `{`. The related record's file is set when the program file is
  built (`tsr-compiler` `ProgramFile::new`, the related half of
  `attachFileToDiagnostics`), sharing one file image per file.
- Nested namespace segments (`parseModuleOrNamespaceDeclaration(…, nested)`):
  a segment after a dot is `parseIdentifierName`, so `chrome.debugger` is a
  name and only the first segment can report TS1359.
- Numbers (`Scanner.Scan` `case '.'`, `case '0'`): a `.` before a digit is
  `scanNumber` from the dot (so `.1n` reports TS1353), and an empty `0b`/`0o`
  literal reports TS1177/TS1178 rather than the hex message.
- Regular expression bodies (`ReScanSlashToken`'s first pass) walk bytes; an
  unterminated body is cut at the nearest unbalanced `)`/`]`/`}` outside a
  class and decimal quantifier, then trailing whitespace and `;` are dropped:
  TS1161 spans that range and the token ends there, so the `;`/`)` after
  `/ b` and `foo(/notregexp)` are scanned again. The `regExpParser`
  validation pass (TS1125/TS1198/TS1499/TS1527/...) is not ported.
- Union/intersection constituents (`parseFunctionOrConstructorTypeToError`):
  after a `|`/`&` (leading one included), a function or constructor type is
  parsed and reported (TS1385-TS1388) over its range from the token's full
  start, as `parseErrorAtRange(typeNode.Loc)` does.
- Element access without an argument (`parseElementAccessExpressionRest`):
  every member-access loop reports TS1011 at the `]`'s full start and keeps
  a missing identifier; the argument is parsed with `in` allowed.
- `#` that does not start a private name (`Scan` `case '#'`): `#!` past the
  first position is TS18026 over both characters and an `Unknown` token; any
  other `#` is TS1127 and a nameless `PrivateIdentifier` (`HashToken` is only
  ever a rescan, `ReScanHashToken`).
