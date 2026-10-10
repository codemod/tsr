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

## 5. Measurements

See the commit messages and the box-protocol §6 reports for per-commit
numbers; §6 below keeps the running table.

## 6. Running table

| commit | port | diag cases | types lines | perf (CPU ratio, dm / gi) |
|---|---|---|---|---|
| `dd6f9d88` | §1 decorator expression, unary type assertion | +9 / −0 | +242 / −0 | 0.996 / 0.996 |
| `1321f0aa` | §2 top-level await reparse | +4 / −0 | +460 / −0 | 1.009 / 1.008 |
| `f44a4956` | §3 yield context | +10 / −0 | +95 / −0 | 0.992 / 1.007 (vs `1b466dc8`) |
| `41c141b2` | `ParseOptions::module_indicator` | 0 | 0 | — |
| (this) | §4 JavaScript JSX variant | +4 / −0 | +29 / −0 | see report |
