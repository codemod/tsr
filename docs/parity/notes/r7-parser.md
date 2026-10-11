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
| `parseFunctionOrConstructorType` (`:2731`) | speculative function type fell back to a parenthesized one | functionTypesLackingReturnTypes, parseErrorIncorrectReturnToken |
| `parseElementAccessExpressionRest` (`:5436`) | `x[]` reported TS1109, not TS1011 | badArrayIndex, objectLitArrayDeclNoNew |
| `nextTokenIsBindingIdentifierOrStartOfDestructuringOnSameLine` (`:6324`) | `for (using of …)` read as a declaration | usingDeclarationsInForOf.2 |
| `tryParseTypeArgumentsInExpression` (`:5247`) | a type-argument list with any diagnostic was refused (a "complaint gate"), so `foo<>()`, `foo<?>` and `foo<string?>` were comparisons | expressionWithJSDocTypeArguments; emptyTypeArgumentList(WithNew) now wait only on TS1099 |

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
position and folds only exact repeats (same span and code), as
`SortAndDeduplicateDiagnostics` does. A speculative parse rewinds both
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

Consumers do not exist yet. The compiler has to keep the table per file next
to its JSDoc table (`tsr-compiler` `file.rs`, r7-perf), and the checker's
grammar checks (TS1098, TS1099, TS1009/TS1013, TS1025, r7-grammar) have to
read it through the host. Routed. The cases waiting on them are
emptyTypeArgumentList(WithNew), parserConstructorDeclaration11/12,
indexSignatureWithTrailingComma, trailingCommasInFunctionParametersAndArguments,
and the importCallExpressionWithTypeArgument `<>` arm.
