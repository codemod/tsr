# r7-parser — parser and scanner parity (`tsr-2zk.1270`)

Round-7 lane box for `crates/tsr-parser` and `crates/tsr-scanner`. Native is
`vendor/typescript-go` @ `5b1047d`. Base: `origin/main` @ `9020aa67`, frozen
with `scripts/parity_gate.sh freeze`:

- types: 556,357 aligned lines, 551,176 RIGHT / 4,508 WRONG / 673 GAP;
- diagnostics: 12,238 cases, 5,741 RIGHT / 5,610 EMPTY_RIGHT / 852 WRONG /
  35 EMPTY_WRONG.

Re-measured queue on that base (r6-triage's finished-alone lists): REGEXP-
SCANNER-VALIDATION is already converted on main (0/10 still wrong: the
validator and its checker hook landed in round 6); SCANNER-NUMERIC-AND-ESCAPE
has 3/15 left; every other cluster is unchanged.

## 1. `parseDecoratorExpression` and the unary type assertion

- `parseDecoratorExpression` (`parser.go:3906`): a decorator is a
  left-hand-side expression parsed in the decorator context, which keeps a
  following `[` for the decorated member's computed name
  (`parseMemberExpressionRest`, `:5357`). The context is cleared by
  `parseExpression`, argument lists, function expressions and function bodies,
  as native's. `@await` in an await context is TS1109 plus a missing
  identifier. The previous port parsed only `@(expr)` or a dotted name, so
  `@<…`, `@new …` and `@x?.y()` recovered differently.
- `parseDecoratedExpression` (`:5723`): decorators in expression position
  before anything but `class` are `Expression expected` at the next token's
  full start. Native returns `MissingDeclaration(modifiers)`; this AST's
  `Expression` union has no `MissingDeclaration` (it is generated from
  `ast.json`), so the expression is a missing identifier and the decorators are
  not in the tree. The checker does not visit them natively either
  (`checkExpression` has no arm for it), so diagnostics match; the types
  walker loses the decorator's own lines (`>dec : …` in
  `decoratorOnFunctionExpression.types`). Fixing that needs
  `Expression::MissingDeclaration` in the generated AST: routed.
- A type assertion is a unary expression (`parseSimpleUnaryExpression`'s `<`
  arm, `:5070`), not a primary one: `parsePrimaryExpression` has no `<` arm,
  so `@<T>x`, `new <T>x` and `++<T>x` are TS1109 at the `<`.

## 2. `reparseTopLevelAwait`

### The forcing constraint

Native parses every file once without an await context, then, in a module that
is not a declaration file, re-reads each run of top-level statements that
created an identifier spelled `await` in an await context
(`parseSourceFileWorker`, `parser.go:449`; `reparseTopLevelAwait`, `:514`).
That is what turns `await [x]`, `await (x)` and `` await `` `` at the top of a
module into await expressions, and what makes `class C extends await<string>`
or `@await class C {}` parse errors. This port had no such step: top-level
`await` was an await expression only through `isAwaitExpression`'s
same-line lookahead half, so `await [x]` was an element access on an
identifier (TS1262 + TS2304/TS2552 instead of nothing).

### What was ported

- `statementHasAwaitIdentifier` (`Parser::statement_has_await_identifier`),
  set where native's `newIdentifier` sees the text `await`
  (`create_identifier`; the scanner gives `await`, escaped or not, the keyword
  kind) and saved/restored at native's sites: binding identifiers
  (`parseBindingIdentifier`, new `parse_binding_identifier`), the class name,
  property names, function bodies (`parse_function_block`), JSDoc comments,
  `enum`, `namespace`/`module`, import-equals and import clauses, export
  assignments and declarations, `export as namespace`, and a `declare` class.
  It is part of `ParserState`, as native's is.
- `possibleAwaitSpans` and `parseToplevelStatement`'s span bookkeeping.
- The reparse itself, gated on `!declaration_file && isExternalModule`.
- `NodeFlagsAwaitContext` stamped on every node finished in an await context
  (native's `node.Flags |= p.contextFlags`): `checkGrammarAwaitOrAwaitUsing`
  reads it so a re-parsed top-level `await` reports no TS1378.
- Native contexts the reparse depends on, which this port had not modelled:
  `parseIdentifier` uses `isIdentifier` (so `await` in an await context is a
  missing identifier) while binding names stay context-free; an exported
  class's heritage and members are in an await context
  (`parseClassDeclarationOrExpression`, `parser.go:1751`); a class property
  initializer is in none (`parser.go:1972`); export assignments and export
  declarations are in one; parameter decorators are parsed in the context
  *outside* the signature (`parseParametersWorker`'s `inOuterAwaitContext`,
  `parser.go:3296`), recorded by `with_await_context` as
  `outer_await_context`; object-literal accessor parameters and function /
  constructor type parameters are in no await context; an object literal's
  shorthand test is `isIdentifier` before `parsePropertyName`.

### The judgment call: re-parse the whole file, not the spans

Native splices new statements between the untouched originals and leaves the
replaced nodes allocated. This port cannot: a file's nodes are one contiguous
id range (`ParsedInto::node_range`), and consumers scan that range by id —
`file_include.rs` finds a module-specifier literal *by position* in it, so an
orphaned copy of `await import("./a")` would be found before the live one.

So the reparse truncates the file's node table to its first id and parses the
whole file again, running native's span loop at the same positions:

- statements outside the spans are reproduced by parsing them again with the
  ordinary list loop (`parse_top_level_statements_until`), stopping at the
  recorded full start of the next span's first statement. The parse is
  deterministic and the context is the first parse's, so they are the same
  statements with new ids;
- inside a span, native's loop runs verbatim (`parseStatement` without list
  recovery, the "ate into the next statement" continuation);
- diagnostics follow native exactly: outside the spans they are the first
  parse's, filtered by position the way native's two `FindIndex` slices are;
  inside, the second parse's own, by index window. Scanner diagnostics keep
  their source tag (`carried_scanner_diagnostics`) so `finish`'s same-position
  preference is unchanged.

Accepted cost: a module with a top-level `await` identifier is parsed twice.
Native parses the spans twice; the whole file is at most a constant factor
more, and the trigger is rare (no file in the four perf projects has one;
the perf gate below measures the hot path, not this branch).

How I would know I was wrong: a case whose reparsed statements are not
reproduced by the second list parse (a context leak between statements), or
whose diagnostics around a span boundary differ from native's.

### What this port still cannot decide

Native's gate is `ExternalModuleIndicator != nil`, computed from the compiler
options (`moduleDetection: force`, a format-forced module, `jsx:
react-jsx`'s tag rule) and `IsDeclarationFile`. The first commit carried them
on a `Parser` setter, because a new `ParseOptions` field breaks the full
struct literals in other crates' benches; on the integrator's call (after
r7-perf's `defer_ts_jsdoc` set the precedent of updating those literals) they
are `ParseOptions::module_indicator` (`ModuleIndicatorOptions`). The default is
the syntactic decision (`isFileProbablyExternalModule`) and "not a
declaration file". Until the compiler's loader fills it in (r7-perf's
`loader.rs`), a declaration file that is a module and has a top-level `await`
identifier is reparsed where native does not, and a format-forced module with
no import/export is not.

## 3. The yield context and `isYieldExpression`

Native tracks `NodeFlagsYieldContext` beside the await bit: a generator's
parameters and body are in it (`parseParametersWorker` and
`parseFunctionBlock`, `parser.go:3298`, `:3498`); every other signature, an
arrow body (`:4485`), a class static block (`:1910`), a property initializer
(`:1972`) and an enum (`:2139`) are not. `isYieldExpression` (`:4150`)
opens a yield expression inside the context, and outside it only when the
next token is an identifier, keyword or literal on the same line;
`isIdentifier` refuses `yield` inside it. This port built a yield expression
for every `yield`, so `yield(foo)` at the top of a script, `yield * []` and
`{ [yield]: foo }` outside a generator were yield expressions (no TS1212 /
TS2304 / TS2363). Ported as `in_yield_context`, set with the await bit by
`with_function_context(is_await, is_yield, …)` at native's signature sites;
the places native sets only the await bit keep `with_await_context`.
`parseYieldExpression` (`:4172`) is ported with it: an operand follows only
on the same line and only after `*` or at the start of an expression, which
replaces the old token blacklist. `isStartOfLeftHandSideExpression` now ends
in `isIdentifier` as native's does; its context-free stand-in existed only
because there was no yield context.

## 4. JavaScript is a JSX-variant script kind

`getLanguageVariant` (`parser/utilities.go:11`) gives `ScriptKindJS` the
JSX variant beside `.jsx` and `.tsx`, and `ScriptKindJS`/`ScriptKindJSX` set
`NodeFlagsJavaScriptFile` (`parser.go:304`). This port mapped `.js`, `.cjs`
and `.mjs` to the TypeScript dialect (the loader flipped them to `.tsx` only
under a `jsx` option), so `+ <number> x` in a `.js` file was a cast, and
`.jsx` read type arguments on a JSX tag and in calls. `ScriptKind::JavaScript`
is the new member for all four extensions: JSX variant (`allows_jsx`), and
`is_javascript` gates the two parse decisions the JavaScript context makes —
no `tryParseTypeArgumentsInExpression` (`:5247`, in
`parse_type_arguments_for_call`) and no type arguments on a JSX tag
(`:4938`). `docs/parity/notes/jsx.md`'s "No JavaScript-file flag in the
parser" is superseded by this. The parser still does not report
`checkJSSyntax`'s TypeScript-only-syntax diagnostics (`:6696`); routed to
this lane by r7-grammar, queued.

## 5. `parseFunctionBlockOrSemicolon`'s missing body

Native's `parseFunctionBlockOrSemicolon` (`parser.go:3481`) returns no body
only at a `;` or an ASI point. Otherwise it calls `parseFunctionBlock` →
`parseBlock`, and when the `{` is missing, that returns an empty `Block` that
covers no text (`createMissingList`, pos == end at the next token's full
start). The checker then asks two different questions of that body
(r5-smallcodes3 §4.1 measured both):

- `Body() == nil` — `checkFunctionOrConstructorSymbol`'s last test
  (`checker.go:3681`). The body exists, so there is no TS2391.
- `ast.NodeIsMissing(body)` / `NodeIsPresent` — `getReturnTypeOfSignature`
  (return type `any`), `checkFunctionOrMethodDeclaration`'s TS7010, and
  `checkFunctionOrConstructorSymbolWorker`'s `bodyIsPresent` (`:3627`) and
  `reportImplementationExpectedError`'s subsequent-body test. The body is
  missing, so the declaration counts as an overload.

This port returned `None`, which answers both questions "no body". The
parser half builds the empty block, zero-width at `node_end()` (it also
corrects `parse_block_ex`'s missing-brace span, which ran from the next
token's start back to the previous token's end). The checker half, granted by
the integrator for this commit only, makes the four `NodeIsMissing` readers
test the block's zero width: `signatures.rs`' return-type arm,
`implicit_any.rs::check_implicit_any_return`, and in `check.rs` the
`bodyIsPresent` test and `report_implementation_expected`'s subsequent-body
test (both native `checkFunctionOrConstructorSymbolWorker`).

`check_implicit_any_return` also stops answering for object-literal methods.
TSR's walk calls it for every `MethodDeclaration`. Native reaches
`checkFunctionOrMethodDeclaration` only for class members through
`checkSourceElement`; an object-literal method goes through
`checkObjectLiteralMethod` (`checker.go:13865`), which never asks TS7010.
Before this commit the two agreed only by accident, because the object-literal
body was a recovery block with an inverted span that never read as missing.

Measured: diagnostics +8 (dottedModuleName, reservedWords3,
destructuringParameterDeclaration6, objectTypesWithOptionalProperties2,
parser.asyncGenerators.classMethods.es2018, parserErrantEqualsGreaterThan
AfterFunction1/2, parserSkippedTokens16), types +18, 0 lost. The three
losses of the first draft (overloadConsecutiveness,
objectLiteralMemberWithoutBlock1, FunctionPropertyAssignments4_es6) were
the subsequent-body reader and the object-literal dispatch above.

Still open in the cluster: parser.asyncGenerators.objectLiteralMethods
(`async * x: 1;` — native reads `: 1` as a return type), and
parserErrorRecovery_ParameterList6 (`x: break` — native's
`parseTypeReference` admits a reserved word as the type name; this port's
`parse_non_array_type` reports TS1110 because of the type-parameter-list
recovery noted there).

## 5a. Types parse outside the yield and await contexts

`parseType` (`parser.go:2606`) clears `NodeFlagsTypeExcludesFlags`
(`YieldContext | AwaitContext`) for the whole type. Without it, `var v:
await` in an async function stamped `NodeFlagsAwaitContext` on the type
reference, which the binder's TS1359 arm and `checkGrammarYieldExpression`
read (routed by r7-grammar). No dump moves on its own: it unblocks those
checker ports.

## 6. Recovery ports, one parse function each

PARSER-RECOVERY-DIVERGENCE was a family; it splits by the native parse
function whose recovery differed. Each landed as its own commit:

| native function | divergence | cases |
|---|---|---|
| `parsePropertyAccessExpressionRest` (`:5402`) | no TS18030 for a private name in an optional chain | privateIdentifierChain.1, privateNameUncheckedJsOptionalChain |
| `parseMemberExpressionRest` / `parseTaggedTemplateRest` (`:5512`) | a template after `?.` read as a member name (TS1003) | taggedTemplateChain still needs the checker's TS1358 (`checkGrammarTaggedTemplateChain`, unported) |
| `tryParseImportClause` (`:2329`) | a clause and `from` expected with no binding, `*` or `{` | importCallExpressionIncorrect1 |
| `parseTypeReference` (`:2858`) | a reserved word as a type name reported TS1110 | parserErrorRecovery_ParameterList6, derivedClassSuperCallsInNonConstructorMembers |
| `parseMappedType` (`:3158`) | trailing type members not parsed | mappedTypeProperties (types; diagnostics wait on TS7061) |
| `parseForOrForInOrForOfStatement` (`:1307`) | `for await` did not expect `of` | parser.forAwait.es2018 |
| `typeHasArrowFunctionBlockingParseError` (`:4450`) | `(a): => {}` taken as an arrow | parserX_ArrowFunction3, ArrowFunction3 |
| `parseThisTag` (`jsdoc.go:985`) | `@this` required braces | thisPrototypeMethodCompoundAssignmentJs |
| `ReScanSlashToken` (`scanner.go:1067`) | an unterminated regex ran to the line end | parserRegularExpressionDivideAmbiguity4 |
| `parseVariableDeclarationList` (`:1563`) | `await using` flagged `Using` alone | awaitUsingDeclarationsInForIn, awaitUsingDeclarationsWithImportHelpers |
| `checkJSDecoratorSyntax` (`:6657`) | no TS1206/TS8038 decorator placement in `.js` | esDecorators-classDeclaration-exportModifier |
| `checkJSDecoratorSyntax`'s related TS1486 (`:6657`) with `attachFileToDiagnostics`' file | the related `Decorator used before 'export' here.` had no file, so it printed with no position (`check.rs::check_js_syntax`, the earlier grant) | esDecorators-classDeclaration-exportModifier |
| `parseFunctionOrConstructorType` (`:2731`) | speculative function type fell back to a parenthesized one | functionTypesLackingReturnTypes, parseErrorIncorrectReturnToken |
| `parseElementAccessExpressionRest` (`:5436`) | `x[]` reported TS1109, not TS1011 | badArrayIndex, objectLitArrayDeclNoNew |
| `nextTokenIsBindingIdentifierOrStartOfDestructuringOnSameLine` (`:6324`) | `for (using of …)` read as a declaration | usingDeclarationsInForOf.2 |
| `tryParseTypeArgumentsInExpression` (`:5247`) | a type-argument list with any diagnostic was refused (a "complaint gate"), so `foo<>()`, `foo<?>` and `foo<string?>` were comparisons | expressionWithJSDocTypeArguments; emptyTypeArgumentList(WithNew) now wait only on TS1099 |
| `parseLeftHandSideExpressionOrHigher` (`:5180`) | `import` without `(`/`<` taken as a keyword expression | importCallExpressionIncorrect2 |
| `parseModuleSpecifier` (`:2320`) | the parser reported TS1141 itself (the checker's) | — (es6ImportNamedImportParsingError with the next row and a checker skip) |
| `parseNamedImports` (`:2390`) | `import d, from` reported nothing | es6ImportNamedImportParsingError's parse errors |
| `parseNameOfParameter` (`:3363`) | a missing name at a modifier keyword did not advance | reservedWords2 |
| `parsePostfixTypeOrHigher` (`:2714`) | `C[#bar]` parsed an index type | privateNamesAndIndexedAccess's parse errors |
| `canFollowGetOrSetKeyword` (`:4036`) | `get #foo()` in an object literal was a property | privateNameInObjectLiteral-3 |
| `parseParameterWorker`'s `this` arm (`:3325`) | `this?:`/`this: C = …` parsed `?`/initializer | thisTypeInFunctionsNegative's parse errors |
| `createIdentifierWithDiagnostic` at EOF (`:5841`) | a missing type at EOF reported over the EOF token | commonJsExportTypeDeclarationError |
| `parseJsxChild`'s EOF arm (`:4847`) | TS17008 for a missing tag name sat on the next token and swallowed its report | parseJsxElementInUnaryExpressionNoCrash3 |
| `parseConditionalExpressionRest` (`:4552`) | `whenTrue` kept a `for` head's no-`in` context, and a missing `:` still parsed `whenFalse` (a second report, TS1109) | jsxAndTypeAssertion (with §7's argument-aware fold) |
| `ScanJsxTokenEx`'s text loop (`scanner.go:1281`) | a `<<<<<<< ` marker inside JSX text scanned as `<` tokens | conflictMarkerTrivia3 (types; diagnostics wait on the checker's TS17004, unported) |
| `parseJSDocComment`'s `PCJSDocComment` (`jsdoc.go:168`) | a list inside a comment's type expression skipped a token it could not use instead of aborting, as every token is that context's element | jsdocFunctionTypeFalsePositive's parse errors (diagnostics wait on TS1098 from the list facts, §10) |
| `parsePrimaryExpression` / `parseTemplateHead` re-scan gate (`:5551`, `:3691`) | every untagged template was re-scanned, so an unterminated one reported TS1160 a second time after a parser error at it, and the guard dropped the next error at end of file | labeledStatementDeclarationListInLoopNoCrash4 |
| `parseExpectedMatchingBrackets` (`:971`), `parseImportAttributes` (`:3085`) and the import type's attributes (`:3047`) | a missing `}`, `)` or `]` reported no TS1007 pointing back at its opening bracket; an attribute list without `{` still parsed entries | parser half of ~20 exact-oracle cases (missingCloseBrace*, parserErrorRecovery_ObjectLiteral2–5, …); the file half is the next row |
| `attachFileToDiagnostics` (`:6392`), called by `parseSourceFileWorker` | parse diagnostics' related information had no file, so TS1007 and TS8034 printed with no position (the oracle's `R` rows read `-`). The parser never sees the file name (ADR-0016), so `attach_file_to_diagnostics` is the parser's port and `ProgramFile::new` (tsr-compiler, integrator's grant), where name, text and parse diagnostics first meet, calls it. Head diagnostics keep no file: the program reports them per file already. The file image is made only when a diagnostic has related information | the TS1007 bucket (missingCloseBrace\*, exportInFunction, parserErrorRecoveryIfStatement2/3, importAttributes10, …), typedefDuplicateTypeDeclaration |
| `attachFileToDiagnostics` on `jsdocDiagnostics` (`parser.go:468`) | the JSDoc comments' own diagnostics (a JavaScript file's `JSDocDiagnostics()`) still had no file on their related information; `JSDocTable::attach_file_to_diagnostics`, called beside the other in `ProgramFile::new` | typedefDuplicateTypeDeclaration |
| `parseTypedefTag` (`jsdoc.go:1017`) and the reparser's `checkNonIdentifierName` (`reparser.go:41`) | a nameless `@typedef` reported nothing: native reports `Identifier expected` in the comment, and — in a JavaScript file, when the tag has a type — again on the character before the missing name in the file's own diagnostics, as `reparseTags` runs at attach time. This port has no reparser pass; the typedef parse records the position and `attach_jsdoc` reports it. The tag keeps no name rather than native's empty identifier, which the binder would declare | jsdocTypedefNoCrash, jsdocTypedefNoCrash2, misspelledJsDocTypedefTags, checkJsdocTypedefOnlySourceFile (exact oracle) |
| `parseClassDeclarationOrExpression` (`:1751`), `parseDeclarationWorker`'s `export` arm, `parseImportDeclarationOrImportEqualsDeclaration`'s phase, `parseImportType` (`:3026`) | a node made and then dropped stayed in the node table with no parent: a class expression parsed a declaration and copied it (and a decorated one copied the expression again), `export` became a token node before the parser knew it was not a modifier (`export =`, `export default`, `export {…}`, `export *`, `export as namespace`), `import type A = require(…)` made a `type` token it then recorded as a flag, and `typeof import(…)` wrapped a finished import type in a second one. Across the TypeScript test cases that was 2,155 `ExportKeyword`, 623 `ClassDeclaration`, 68 `ClassExpression` and 9 `TypeKeyword` nodes. Each is now made once. Native leaves an `ExpressionWithTypeArguments` behind when a call or tagged template absorbs it (`parser.go:5462`) and so does this port; the JSDoc tags a rewound speculative parse leaves are not looked at yet | no case moves; `crates/tsr-parser/tests/no_orphans.rs` |
| `parseSatisfiesTag` (`jsdoc.go:928`) | `@satisfies T` without braces parsed nothing; native expects the braces (`parseJSDocTypeExpression(false)`), reports `'{' expected.` and still takes `T` | checkJsdocSatisfiesTag14 |
| `parseImportType`'s attribute arm (`parser.go:3033`) | after `import(x,` with no `with`/`assert` the port stopped; native reports `'with' expected` (dropped beside an earlier report at the same position), keeps the token as the attributes' kind, parses on and relates TS1007 to the `{` when the `}` is missing | parseUnmatchedTypeAssertion's diagnostics (its type row differs in the checker) |
| `finishNode` for a node that covers no token | it ended at the previous token's end, before its own start when trivia came between: 200 empty `JsxAttributes` (`<div >`), 35 omitted `BindingElement`s, 32 `SourceFile`s, 8 `ExpressionStatement`s, a missing type after `:` and others across the TypeScript test cases. Such a node is now zero-width at its start, this port's place for missing nodes (native's is the full start, which this port does not track). The alternative, ending every such node at the previous token's end and starting it there too, is native's position but moves every missing node the checker already places (§6's refused full-start row) | constructorWithIncompleteTypeAnnotation (the exact oracle's writer rejected its inverted diagnostic span: TSR_FAILED) |
| `checkJSSyntax`'s type-parameter arm (`:6777`), in `check.rs::check_js_syntax` (integrator's grant) | TS8004 spanned the first type parameter, not the list (`list.Loc`): `class B<T: BaseA>` recovers `BaseA` as a second parameter and native's report covers `T: BaseA`. It now spans the first parameter's start to the last's end through `report_js_only_over`; native's list starts at its full start, after `<`, which differs only when trivia follows the `<` | fillInMissingTypeArgsOnJSConstructCalls (exact oracle) |
| `parseFunctionOrConstructorTypeToError` (`:3747`) | a function or constructor type right after `|` or `&` parsed silently; native reports TS1385–TS1388 over it from its full start. This port parses those types in `parse_primary_type`, so only the report and its start test (`isStartOfFunctionTypeOrConstructorType`) were missing | unparenthesizedFunctionTypeInUnionOrIntersection, unparenthesizedConstructorTypeInUnionOrIntersection |
| `checkJSSyntax`'s call sites, in `check.rs::check_js_syntax` (integrator's grant) | native calls it from particular parse functions, and this port's walk visits every node, so it also reported TS8010/TS8017 on index-signature and signature parameters, interface method signatures and type-member index signatures. `reaches_check_js_syntax` applies the call sites' tests: parameters of a function-like with a body only (`parseParametersWorker` without `ParseFlagsType`), index signatures only as class elements, method signatures never, and an expression with type arguments only in a class's `extends` clause (`:1777`; an interface's `extends M<U>` is never looked at) | jsDeclarationsClassesErr, jsDeclarationsInterfaces (extra TS8010/TS8017/TS8011) |
| `parseTypedefTag`'s second `@type` (`jsdoc.go:1048`) | TS8033 carried no related report; native relates TS8034 "The tag was first specified here." at `(0, 0)`, the range it builds | typedefDuplicateTypeDeclaration (with the compiler's related-info file) |
| `scanner.TokenToString` (`scanner.go:124`) | `'{0}' expected` knew twenty token texts and printed any other kind's name (`'LessThanSlashToken' expected.`) | message text only (the exact oracle; the diagnostics suite compares codes) |
| `scanString` (`scanner.go:1586`) | an unterminated string literal's value dropped its last character: the slow path ended the value one before the cursor, as if a closing quote had been consumed | scannerStringLiterals (types) |
| `parseMethodDeclaration`'s `parseFunctionBlockOrSemicolon` (`:1950`, `:3481`) for an object-literal method, with `checkGrammarMethod`'s missing-body arm (`grammarchecks.go:1451`, in `grammar.rs`) and the walk's dispatch (`check.rs`), both integrator's grants | an object-literal method always parsed a block, so `async * x: 1;` reported `'{' expected` at `;` and checked an empty body against the return type `1` (TS2322). The parser's `'{' expected` stood in for the grammar arm, which reports only when the file has no parse errors. With no body, the walk also had to stop running `checkFunctionOrConstructorSymbol` (TS2391/TS2393) on object-literal methods: native reaches them through `checkObjectLiteralMethod` (`checker.go:13865`), never `checkFunctionOrMethodDeclaration` | parser.asyncGenerators.objectLiteralMethods.es2018; objectLiteralMemberWithoutBlock1 now parses cleanly (printer_round_trip +1) |
| `parseEntityName` (`:2901`) under `parseTypeReference` (`:2858`), with `resolveEntityName`'s missing-name skip (`checker.go:15773`, in `check.rs::check_qualified_type_name_at`, integrator's grant) | native's missing type name is still the head of a dotted name, so as an import type's argument `import.defer("./a")` is the reference `<missing>.defer`, with `')' expected` at `(`; this port ended the type at the missing name and took `.defer` as the import type's qualifier. The parser half alone measured diagnostics +0 / −1 (held earlier): the checker reported TS2503 `Cannot find namespace ''` on the missing head, which native's `resolveEntityName` returns nil for. Without a `.` after it a missing type stays this port's `any` keyword stand-in, where native has a reference to the missing name; both check to `any` with no report | typeofImportDefer (exact oracle: TS1141 span) |
| `parseArrayLiteralExpression` / `parseObjectLiteralExpression` (`NodeList.HasTrailingComma` of their elements), with the TS1013 readers (`check.rs::check_rest_element_trailing_comma`, integrator's grant): `checkGrammarBindingElement` (`grammarchecks.go:1542`) and the destructuring-assignment arms (`checker.go:12636`, `:12690`) | a rest element followed by a trailing comma, `const [...a,] = []` or `[...c,] = []`, reported nothing. Binding patterns already carried `HAS_TRAILING_COMMA`; array and object literals now do as well, as the flag is this AST's place for the list's comma. The comma's position is the token after the last element, scanned from the source, because the flag carries no position (a list fact would, but these owners never need the empty-list half) | trailingCommasInBindingPatterns |

The type-argument list is now native's `parseDelimitedList(PCTypeArguments,
parseType)`; its diagnostics stand when the list is accepted and the
caller's `try_parse` drops them when it is not. One adaptation: native's
scanner returns a lone `>` and `reScanGreaterThanToken` combines it, while
this port's scanner returns `>=`, `>>`, … already combined, so a combined
token after the list is native's refusal and is not split (the first draft
split it and read `ver < (a >= b ? …)` as an instantiation,
instantiationExpressionErrors).

`await using` needed its reader in the same commit: before it,
`using_declaration.rs::is_await_using_list` recovered the `await` from the
source text in front of the list's span, which now starts at the `await`.
The parser half alone added TS2741 to every `await using` case. The reader
reads the flags now (integrator's grant). `checkJSDecoratorSyntax` lands in
`check.rs::check_js_syntax`, because this port runs `checkJSSyntax` in the
checker (grant as well).

Held, with the reason:

- parserSuperExpression2: the tree matches native's; the extra TS2304 on
  `super<T>` is the checker resolving the type arguments of a super call
  outside a constructor, which native does not.

Refused, with the number:

- Every missing identifier at its full start, as native's `createIdentifier`
  (`nodePos()`) places it, instead of at the next token's start. Measured
  against batch 22: diagnostics +1 / −2. The two losses,
  taggedTemplatesWithIncompleteTemplateExpressions4 and 5, are checker
  reports whose spans assume a trivia-free start on the missing node. The
  JSX end-of-file arm above applies the full start locally instead, where
  only the parser reads the span. Revisit when the checker's error spans
  skip trivia (`getErrorSpanForNode`'s `SkipTrivia`) for missing nodes.
  Re-measured 2026-10-12 on batch 56 plus this lane's stack, with
  `error_span` already stepping a zero-width node back to its full start:
  exact oracle +0 / −2, the same two cases (their TS2554 spans), so it stays
  refused.

## 7. Scanner reports go through the parser's sink

Native's scanner reports through `scanError` → `parseErrorAtRange`
(`parser.go:318`, `:327`): one list, whose guard drops an error only when the
**previous** report is at the same position. This port's scanner keeps its own
list, and `finish` used to merge the two by position and keep one diagnostic
per start (§195). That also dropped a parser error the scanner had reported
at the same start earlier, with another report in between:
`<test1 32data={32} />` reports TS1351 at `data` while `32` is scanned, then
TS1003 at `32`, then TS1005 at `data`. Native keeps all three.

Now each scan moves the scanner's reports into the parser's list through the
same guard, in report order (`sync_scanner_diagnostics`: one emptiness test
per scanned token, and the move itself is out of line). `finish` sorts by
position and folds only exact repeats (same span, code and arguments), as
`SortAndDeduplicateDiagnostics` does: it compares the message text, so
`':' expected` and `'</' expected` at one end of file both stand
(jsxAndTypeAssertion). Folding on span and code alone merged them. A speculative parse rewinds both
together, because the reports are in the list `save_state` measures.
§195's case (TS1127 alone at an invalid character) is the consecutive guard's,
and still holds.

## 8. Measurements

See the commit messages and the box-protocol §6 reports for per-commit
numbers; §9 below keeps the running table.

## 9. Running table

| commit | port | diag cases | types lines | perf (CPU ratio, dm / gi) |
|---|---|---|---|---|
| `dd6f9d88` | §1 decorator expression, unary type assertion | +9 / −0 | +242 / −0 | 0.996 / 0.996 |
| `1321f0aa` | §2 top-level await reparse | +4 / −0 | +460 / −0 | 1.009 / 1.008 |
| `f44a4956` | §3 yield context | +10 / −0 | +95 / −0 | 0.992 / 1.007 (vs `1b466dc8`) |
| `41c141b2` | `ParseOptions::module_indicator` | 0 | 0 | — |
| `2d64b1ee` | §4 JavaScript JSX variant | +4 / −0 | +29 / −0 | 1.000 / 0.967 |
| `696d4c4f` | §5 missing function body | +8 / −0 | +18 / −0 | 1.009 / 1.002 |
| (this) | §5a type contexts | 0 | 0 | — |

## 10. List facts: what native's `NodeList` knows and this AST does not

### The forcing constraint

Native keeps every written list as a `NodeList` with its own position. The
checker reads two things from it that this port's `&[T]` slices cannot hold:

- an empty list that was written: `f<>()`, `new C<>()`, `class C<> {}`,
  `constructor<>()`. `checkGrammarForAtLeastOneTypeArgument` (TS1099) and
  `checkGrammarTypeParameterList` (TS1098) report from `list.Pos() - 1` to
  `SkipTrivia(list.End()) + 1`. An empty slice also means "no list".
- `HasTrailingComma`, read at `list.End() - 1`:
  `checkGrammarForDisallowedTrailingComma` (TS1009, TS1013 after a rest
  parameter) and the index signature's TS1025.

### The decision

The parser records a sparse side table, owner `NodeId` → `ListFact { kind,
open, close, empty, trailing_comma }`. Facts are recorded only for those two
shapes, which ADR-0003 calls the tree-plus-side-tables shape. A list parser
queues a fact with the node count at that moment. When the parse ends,
`take_list_facts` resolves each fact's owner: the first node finished after the
list that owns lists of that kind and starts at or before the list's `<`/`(`.
Nodes inside the list, such as a nested function type, finish before the list
is recorded, so a nested function type's comma stays its own. The first draft
claimed owners inside `finish_node_with_flags`. That cost +1.6% Ir on
generic-imports, because the function stopped being inlined. Resolving at the
end costs +0.22% Ir on gi and +0.06% on dm, measured; most of it is the extra
`ParserState` field in `restore_state`. The queue
is part of `save_state`, so a refused speculative list (`a < b > c`) leaves
nothing behind. The table is published with the nodes:
`ParsedInto::list_facts` and `ParsedSourceFile::list_facts`, and
`ParsedFile::publish` relocates the owner ids along with the nodes.

The alternative was a `NodeFlags` bit on the owner, as
`HAS_TRAILING_COMMA` already is on binding patterns and heritage clauses.
I rejected it for three reasons:
1. An owner can have several lists. A function has type parameters and
   parameters, so one bit cannot say which list.
2. The checker needs a position, not just a boolean. For an empty `<>` that
   means `<` and `>`, which a flag would make the checker recover by
   re-scanning the source text.
3. The few free `NodeFlags` bits are shared with every other crate.

The bit would win if the checker only needed yes/no for a single-list owner.
For binding patterns and heritage clauses that holds, and the bit stays.

### The readers

Wired in one commit, under the integrator's grant for the compiler and
grammar files:

- `ProgramFile` keeps the parse's facts, sorted by owner and converted to
  the checker's mirror type `WrittenList`/`WrittenListKind` (`resolution.rs`;
  `tsr-checker` does not depend on the parser), and the program answers the
  checker's new `ModuleHost::written_lists(file)`. `check_source_file` fetches
  the slice once per file (`Checker::file_written_lists`, `checker.rs`), and a
  reader binary-searches it; a file without facts, nearly every file, costs
  one emptiness test per candidate node. The first draft asked the host per
  node, through `source_file_of_for_diagnostics`, and cost +1.44% Ir on
  domain-model; this shape measured +0.25% for the whole stack.
- `grammar.rs`: `checkGrammarTypeArguments` (TS1009, then TS1099) for a
  call, `new`, tagged template (behind TS1358), expression with type
  arguments (not `import<T>`, whose TS1326 is unported), type query, type
  reference and JSX tag; `checkGrammarTypeParameterList` (TS1098) for every
  owner but an interface and a type alias, which native never asks; the index
  signature's TS1025.
- `check.rs`: TS1098 first in `check_grammar_parameter_list`, as
  `checkGrammarFunctionLikeDeclaration`'s `||` chain orders it; a last rest
  parameter's TS1013 outside an ambient context; and
  `checkGrammarConstructorTypeParameters`' empty arm, TS1092 zero-width
  after the `<`, because an empty list's `Pos() == End()`.

Every report goes through the file's parse-error gate, as native's
`grammarErrorAtPos` does. Spans are native's: an empty list from `<` to one
past `SkipTrivia(list.End())`, a trailing comma over the comma.

How it would be wrong: a fact whose owner resolved to the wrong node (a
nested signature taking its parent's list) would put the report on the
right span but the wrong condition, and show up as an extra TS1098/TS1099 in
a case native leaves clean.

## 11. JSDoc trivia cost

Callgrind on generic-imports (`--singleThreaded`) put 11.3% of all
instructions in `jsdoc_ranges_in` and another 2.3% in `mentions_tag`, nearly
all of it inside the lib files' long comments:

- `jsdoc_ranges_in` walked each comment's body a byte at a time to find its
  `*/`. It now skips to each `*` with `memchr` (`str::find(char)`) and checks
  the next byte. The old loop also let a line break inside a comment set
  that comment's own `preceded_by_line_break`; the field means a break before
  the comment, and nothing reads it.
- In a `.ts` file (`JSDocMode::SeeOrLink`, ADR-0053) the parser keeps a
  construct's comments only when one carries `@see`/`@link`, but
  `leading_jsdoc_marker` asked only for *a* comment, so every commented
  construct had its ranges found and then dropped. The scanner already sets
  `PRECEDING_JSDOC_WITH_SEE_OR_LINK` with the same tag test as
  `jsdoc_has_see_or_link`, so the marker asks for that flag in this mode.
  The alternative — keep finding the ranges and test each — is what it did.
  It would win back only if the scanner flag and the parser's test could
  disagree. They apply one test to the same trivia, and the test in
  `parse_jsdoc_at` stays as a second check.
- `classify_block_comment` scanned each comment's `@`s twice, once per tag
  set. It now does one pass and stops once both flags are set.

| step | generic-imports Ir | domain-model Ir |
|---|---|---|
| `memchr` body skip | 0.924 | 0.981 |
| marker asks for the see/link flag; one tag pass | 0.920 | 0.982 |
| together | 0.850 | 0.963 |

How it would be wrong: a `.ts` construct whose `@see`/`@link` comment is no
longer attached, which shows up as lost `@link` references in the checker's
JSDoc output (types suite) or in the exact oracle.

## 12. Re-triage on `02301e3` with the exact oracle

Measured 2026-10-12 by the replacement thread, after the queue's first four
items turned out to be on main already (a206b053, bf980613, the scanner
cluster, jsxAndTypeAssertion, importCallExpressionIncorrect2, the static-block
await context). Native run frozen once (`oracle-native`, one worker, under
`ulimit -v 4500000`: 12,796 cases, 1 `NATIVE_FAILED`, largeControlFlowGraph,
which is the memory cap, not a parity fact). Base report: 10,295 / 12,797
EXACT, the integrator's batch-48 number.

`full_oracle_run rank` over the 2,502 non-exact cases, filtered to parse
diagnostics (TS1xxx, TS17xxx), `parser.*`/`scanner.*` emitters and the
`node-selection` class:

- parse diagnostics are exact everywhere except the related-information file
  (§6's `attachFileToDiagnostics` row, ≈21 cases);
- `node-selection` is 3 cases: decoratorOnArrowFunction and
  decoratorOnFunctionExpression wait on `Expression::MissingDeclaration` in
  the generated AST (§1), manyCompilerErrorsInTheTwoFiles on §6's refused
  full-start placement of missing identifiers (its other row was the
  `scanString` fix);
- every other parser-adjacent bucket is the checker's: TS1046 and TS1108
  spans (`grammarErrorOnFirstToken`, `checkReturnStatement`), TS17004
  (`checkJsxPreconditions`, unported), the list-fact readers of §10
  (TS1098/TS1099/TS1092/TS1025/TS1013, 12 cases), TS1326, TS1200, TS1341,
  TS1368, TS1141 on a JSDoc `@import` specifier, TS8023.

Two parser ports need a checker partner. An object-literal method's
`parseFunctionBlockOrSemicolon` landed with it, under the integrator's grant
(§6's table; the parser half alone reports TS2391 on `var v = { foo(); }`).
`parseEntityName` under `parseTypeReference` landed with `resolveEntityName`'s
skip of a missing name, under a grant as well.

Parse time is not a wall bottleneck: on domain-model TSR's parse phase is
17–36 ms against tsgo's 53–66 ms (`--extendedDiagnostics`, noisy 4-core box),
and the scanner is 8% of single-threaded Ir.

## 13. Checker items from the §12 work list

The integrator asked this lane to work through §12's list in order of
cases, with a grant per file. One row per commit, each naming the native
function it ports:

| native function | divergence | cases |
|---|---|---|
| `checkReturnStatement` (`checker.go:4086`) and `checkBreakOrContinueStatement` (`:4081`) behind `checkGrammarStatementInAmbientContext` (`grammarchecks.go:2045`) | the walk refused TS1108/TS1104/TS1105 anywhere in an ambient context, a stand-in from before TS1036 was ported. Native's ambient check reports once per block and answers `false` for every later statement, which then reaches the container test. TS1108 also spanned the whole statement, not its first token (`grammarErrorOnFirstToken`) | ambientWithStatements, parserReturnStatement2, parserNotRegex1, fileWithNextLine3, multiLinePropertyAccessAndArrowFunctionIndent1, parserErrorRecovery_VariableList1 (exact oracle) |
| `checkJsxPreconditions` (`jsx.go:159`), from `checkJsxOpeningLikeElementOrOpeningFragment` (`jsx_factory.rs`, `check.rs` walk hook, grant) | without a `jsx` option an element or fragment reported nothing; native's TS17004 is a `c.error` over the opening tag, so it stands beside parse errors. The `noImplicitAny` arm (TS7026, no global `JSX.Element`) is not ported | checkJsxNotSetError, conflictMarkerTrivia3, parseUnaryExpressionNoTypeAssertionInJsx1–3 |
| `addTypeOnlyDeclarationRelatedInfo` (`checker.go:2168`) at its four `c.error` sites: `resolveNameHelper`'s type-only alias used as a value (`:1866`, TS1361/TS1362), `checkExportAssignment`'s re-export (`:5649`), `checkAliasSymbol`'s verbatim import and re-export (`:6818`, `:6835`) | the diagnostics carried no `'{0}' was imported here.` / `'{0}' was exported here.` related information. One helper (`isolated_alias.rs::add_type_only_declaration_related_info`) over the existing `getTypeOnlyAliasDeclarationEx` port, with the declaration's file; `check.rs`'s TS1361 site and `isolated_alias.rs`' three, under grants. The decorator-metadata site (`TS1272`) is a different path and stays open | 21 exact-oracle cases (typeOnly/*: enums, exportDeclaration×2, exportNamespace1/2/4/6/7/8/10, exportNamespace_js, exportSpecifiers, extendsClause, importClause_namedImports, importSpecifiers1, …; isolatedModulesReExportType, verbatimModuleSyntaxNoElisionESM) |
| `createModeMismatchDetails` / `CreateModeMismatchDetails` (`checker.go:15524`, `checker/utilities.go:1821`), chained under `resolveExternalModule`'s CommonJS-imports-ESM report (`:15339`) | TS1479 (and the two type-only-import messages beside it) carried no `To convert this file to an ECMAScript module, …` chain. The chain needs the importer's `package.json` scope, so the host grows `ModuleHost::package_json_scope` (`resolution.rs`, answered by the program's `meta_datas`, `lib.rs`); the checker half is `check.rs::mode_mismatch_details`, under grants | esmModuleExports3, nodeModulesCJSResolvingToESM1/2/4 (nodeModulesAllowJsSynchronousCallErrors' diagnostics too; its types differ elsewhere) |
| `checkGrammarArrowFunction`'s tail (`grammarchecks.go:790`), the link after `checkGrammarParameterList` in `checkGrammarFunctionLikeDeclaration` (`check.rs`, grant) | an arrow whose `=>` follows a line break reported nothing. Native tests the token's full text, so a line break inside a block comment before `=>` counts; this AST keeps the token's trivia-free span, so the checker walks back over whitespace and block comments from it | disallowLineTerminatorBeforeArrow, arrowFunctionErrorSpan |
| `checkExternalImportOrExportDeclaration`'s TS1141 (`checker.go:5333`) for the `JSImportDeclaration` an `@import` tag reparses into (`parser/reparser.go:119`) (`jsdoc_checks.rs`, `check.rs` walk hook, grant) | a non-string `@import` specifier reported only the parse error. The reparsed declaration exists only for a tag with an import clause, and reaches the TS1141 test only when `parseListIndex` placed it in a source file's or module block's statements (`checkGrammarModuleElementContext`) | importDeferJsdoc, importTag13 |
| `checkGrammarClassDeclarationHeritageClauses`' `@augments` arm (`grammarchecks.go:915`) (`check.rs::check_jsdoc_augments_matches_extends`, grant) | a JSDoc `@augments`/`@extends` naming another class than the `extends` clause reported nothing. The JSDoc read is the class's eagerly parsed comments, which this port's `jsdoc_entries` holds in the same set (all of a JavaScript file's, a `.ts` file's `@see`/`@link` ones) | extendsTagEmit, jsdocAugments_nameMismatch |
| `parsePropertyAccessEntityNameExpression` (`jsdoc.go:971`) | a bare `@augments`/`@implements` reported `Identifier expected` and dropped the class name; native keeps a missing identifier (`parseJSDocIdentifierName`), whose empty text the TS8023 arm above compares | jsdocAugmentsMissingType |
| `grammarErrorOnFirstToken` at two more sites: `checkGrammarForInOrForOfStatement`'s second declaration (`grammarchecks.go:1279`, `check.rs`) and `checkGrammarMethod`'s object-literal modifier arm (`:1441`, `grammar.rs`), grants | TS1188 spanned the whole second declaration and TS1184 one character, not the first token | for-of-excess-declarations, objectLiteralMemberWithModifiers1 |
| `GetErrorRangeForNode`'s case/default-clause arm (`scanner.go:2614`) in `check.rs::error_span`, grant | a diagnostic on a clause with statements spanned the whole clause; native ends it at the first statement's `Pos()`, after the `:`. No case converts alone (plainJSGrammarErrors' TS1113 span; its other differences are the plain-JS filter's) | — |
| `GetErrorRangeForNode`'s `KindConstructor` arm (`scanner.go:2645`) in `check.rs::error_span`, grant | a diagnostic on a constructor (TS2390, TS2377, TS2392, TS2394, TS2385, …) spanned the whole declaration; native scans from its first token to the end of the `constructor` keyword (or its string-literal spelling) | ≈25 exact-oracle cases (ClassDeclaration8/10/11/14, parserClassDeclaration8/10–12/14, constructorOverloads1/3/8, classUpdateTests, constructorsWithSpecializedSignatures, …) |
| `GetErrorRangeForNode`'s arrow arm (`getErrorRangeForArrowFunction`, `scanner.go:2542`) and its first-token fallback for a declaration with no name (`:2652`), in `check.rs::error_span`, grant | a diagnostic on an anonymous function or class expression (or `export default function () {}`) spanned the whole node, not its first token; one on an arrow with a block body spanning lines spanned the whole arrow, not up to the first line break after `=>` | assignLambdaToNominalSubtypeOfFunction, arrowFunctionErrorSpan (once the body's `Pos()` is the end of `=>`, not the `{` stepped back over whitespace, which stopped at a comment), commaOperatorLeftSideUnused, contextualTyping33, noImplicitReturnsExclusions, overloadresolutionWithConstraintCheckingDeferred, undeclaredModuleError, generatedContextualTyping, parserUnaryExpression2, … |
| `checkSourceElementUnreachable`'s run (`checker.go:2431`) in `check.rs::check_unreachable`, grant | TS7027 spanned the run's first statement only; native extends the range to the last of the following statements that are potentially executable and unreachable, in a parent that can have statements | reachabilityChecks1/9/10, unreachableDeclarations ×2, unreachableJavascriptChecked, tryCatchFinallyControlFlow |
| `GetErrorRangeForNode`'s name-narrowing kinds (`scanner.go:2598`) in `check.rs::error_span`, with `getAdjustedNodeForError` kept for duplicate declarations (`merge_conflicts.rs`), grants | `error_span` narrowed a method signature to its name; native's list does not include `MethodSignature` (nor a reparsed function or method declaration), so TS2411, TS2322 elaborations and others on a method signature span the whole member. Duplicate-declaration reports name the declaration through `getAdjustedNodeForError` (`GetNameOfDeclaration`) and keep the name: the first draft lost duplicateIdentifierRelatedSpans3–7 by sending them through the new rule | interfaceMemberValidation, interfacedeclWithIndexerErrors, propertiesAndIndexers(2), derivedInterfaceIncompatibleWithBaseIndexer, objectTypeHidingMembersOfExtendedObject, arrayAssignmentTest1, assignmentCompatability_checking-*-member-off-of-function-interface, … (≈19) |
| `checkGrammarImportCallExpression`'s type-argument arm (`grammarchecks.go:2176`) and `checkGrammarExpressionWithTypeArguments`' `import` arm (`:885`) (`import_call_grammar.rs`, `grammar.rs`, `check.rs` hook, grants) | `import<T>("…")` and `import<T>` reported nothing (TS1326); an empty `<>` counts as a list (`TypeArguments != nil`, the parser's list facts). An `import(…)` call is `checkImportCallExpression`'s, so `checkGrammarTypeArguments`' TS1099/TS1009 skip it | importCallExpressionWithTypeArgument, importWithTypeArguments |
| `DeclarationNameToString`'s `(Missing)` for a name with no width (`check.rs::declaration_name_to_string`, `implicit_any.rs` TS7010, consumer hunks) | a recovered declaration's missing name printed as `''` in messages; native prints `(Missing)` | parserEqualsGreaterThanAfterFunction1/2 |
| `checkGrammarConstructorTypeParameters`/`…TypeAnnotation` over the JavaScript reparse of a constructor's comment (`gatherTypeParameters`, `parser/reparser.go:293`; the `@returns` arm), consumer hunks in `check.rs` | a constructor's `@template` and `@returns` in a `.js` file reported nothing; native reparses them into the constructor's type parameters (a list from the first tag's start to the last tag's end) and its `Type` (the tag's type, inside `{…}`) | jsdocIllegalTags |
| `checkAsyncFunctionReturnType` over the JavaScript `@returns` reparse (consumer hunk in `expressions.rs::check_async_function_return_type`) | an async JS function's `@returns {string}` reported nothing; native's reparsed `Type` gets TS1064 | asyncFunctionDeclaration16_es5 |
| `checkAccessorDeclaration`'s TS1341 (`checker.go:2936`) and `checkMethodDeclaration`'s TS1368 (`:2799`) (`check.rs::check_constructor_named_member`, grant) | an accessor or generator method named `constructor` in a class reported nothing | constructorNameInAccessor, constructorNameInGenerator |
| `checkGrammarForUseStrictSimpleParameterList` (`grammarchecks.go:722`), the last link of `checkGrammarFunctionLikeDeclaration`'s chain (`check.rs`, grant) | a `"use strict"` prologue beside a non-simple parameter list (ES2016+) reported nothing: TS1346 per parameter and TS1347 on the directive, related to each other. `check_grammar_parameter_list` now answers whether an earlier link reported, as the `||` chain does | functionWithUseStrictAndSimpleParameterList_es2016 |
| `checkInferType`'s grammar arm (`checker.go:3289`) (`check.rs::check_infer_type_position`, grant) | an `infer` outside a conditional type's `extends` type reported nothing (TS1338). No case converts alone (inferTypes1's other differences are type relations) | — |
| `checkEnumMember`'s private-name arm (`checker.go:5122`) (`check.rs` walk, grant) | an enum member named `#x` reported nothing (TS18024). privateNameEnum's diagnostics now match; its type line differs in the binder's name for the member (`�missing`) | — |
