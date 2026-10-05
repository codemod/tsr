# Parity lane: JSX (tsr-2zk.8)

Lane notes for the `jsx` parity box. Judgment calls made while porting
typescript-go's JSX parsing and checking (pinned `vendor/typescript-go` @
`5b1047d`) are recorded here; the case list lives in
`docs/parity/lanes/jsx.txt`.

## 1. JSX parse recovery is a port of `parser.go:4736-5060`, not a reshaping

### Forcing constraint

The previous `crates/tsr-parser/src/jsx.rs` was an idiomatic JSX parser: it
parsed correct JSX correctly and bailed out of malformed JSX at the first
surprise (`parse_jsx_children` broke on any non-child token, attributes skipped
a stray token with "Unexpected token"). Upstream's recovery is not an
afterthought that can be bolted on; it is the *shape* of its functions:

- children are a list under `PCJsxChildren`, whose `isListElement` is always
  true — so while any JSX element is open, every list nested inside a `{…}`
  child aborts at the first token it cannot use (`isInSomeParsingContext`),
  rather than skipping it;
- attributes are `parseList(PCJsxAttributes, …)`, which reports `Identifier
  expected` and either aborts or skips by the same machinery;
- `parseJsxChildren` stops after a child whose closing tag names the *parent*
  (`<div><span></div>`), and the parent then **restructures** that child with a
  synthetic empty closing tag and steals its closing element, so TS17008 lands
  on the unclosed `span` and never twice;
- a missing closing tag at end of file reports TS17008/TS17014 at the opening
  tag, not at EOF.

Fourteen lane cases had parse-code diffs at 06f25e0 (EXTRA 1005/1003/1382,
MISSING 1109/17008/17002/17014/17015/17021). Patching each would have meant
re-deriving these decisions outside the algorithm that makes them, so the file
was rewritten function-for-function: every function in `jsx.rs` names its
`parser.go` counterpart.

### Representational differences kept, and why

- **`>` is split before every test.** Upstream's scanner never produces `>>`
  or `>=` (compounds come from `reScanGreaterToken`); this scanner joins them
  greedily. `at_jsx_greater_than` and `isListTerminator(PCJsxAttributes)` call
  `rescan_greater_than` first. That is upstream's token stream, not a
  heuristic: in every position these run, upstream holds a lone `>`.
- **Spans start at the token, not at leading trivia.** Upstream reports an
  unclosed fragment at `openingTag.Loc`, whose `pos` *includes* leading trivia
  (`tsxFragmentErrors` wants `(9,11)`, the end of the previous line's `>`). The
  fragment's full start is threaded through `JsxOpeningTag::Fragment` for that
  one report rather than changing what spans mean.
- **`lastChild.Children().End()`** has no node to read in a side-table AST; it
  is the end of the child's last child, or of its opening element
  (`jsx_children_end`), which is where the inner `parseJsxChildren` left the
  scanner.
- **An attribute initializer that is a sibling-recovery binary**
  (`<a b=<c/><d/> />`) cannot be stored: the generated `JsxAttributeValue`
  union has no binary arm. The diagnostics are reported; the initializer is
  dropped. Changing the union is an AST codegen change outside this lane.

### Entry points moved to upstream's

JSX now enters at `parseUpdateExpression` (a `<` followed by a name or `>`)
and `parseSimpleUnaryExpression` (any `<` as the operand of a prefix operator,
parsed with `mustBeUnary` so the sibling recovery cannot wrap it), both inside
`parse_unary_expression`. `parse_primary_expression` keeps `<` only for `.ts`
type assertions; in `.tsx` a `<` that is not JSX reaches the
missing-expression default exactly as upstream's `parsePrimaryExpression`
(no `<` arm) does — `var x = <:a …` now parses as upstream's types baseline
shows (`beginOfIdent1 : boolean`).

### Scanner changes this needed (JSX-only arms)

- `languageVariant == JSX` makes `</` (not `</*`) a single token in ordinary
  scanning (`scanner.go:778`). Without it `{ </div>` inside a child read `/`
  as a regular expression (EXTRA TS1161).
- `ScanJsxIdentifier` appends to the *decoded* value and the token keeps its
  escape flag, so `<a-b></a-b>` matches and `<a-c>` reports TS17021
  through `parseIdentifierNameErrorOnUnicodeEscapeSequence`.
- An unterminated JSX attribute string reports zero-width at the end of input
  (`scanString`'s `s.error`), not over the token.

### Known gaps left (not in this lane's files)

- **`finish()` deduplicates parser and scanner diagnostics by sorted
  position**, while upstream's guard is "same start as the *last reported*
  error". When a scanner error is reported, then a parser error elsewhere, then
  a parser error at the scanner error's position, upstream keeps both
  (`tsxAttributeInvalidNames` TS1351+TS1005 at `(10,10)`,
  `jsxAndTypeAssertion` two TS1005 at EOF). Needs the scanner's diagnostics
  routed into the parser's list in report order — parser box.
- **No JavaScript-file flag in the parser**: upstream skips JSX type arguments
  in JS (`contextFlags & NodeFlagsJavaScriptFile`), so `.jsx` parses
  `<Foo<number> />` as TypeScript here (`jsxCheckJsxNoTypeArgumentsAllowed`).
- **`parseTypeArguments` is `parseBracketedList(PCTypeArguments, …)`** upstream,
  which accepts `<>` and leaves TS1099 to the checker; `parse_type_arguments`
  always parses one type and reports TS1110 (`tsxTypeArgumentResolution`).

### How we would know this is wrong

A lane or corpus case whose only diff is a parse code inside a JSX element
after this commit is either one of the three gaps above or a divergence in a
function named here; compare against the named `parser.go` line, not against
the old behaviour.

### A types loss this exposed, not caused

`compiler/jsxNamespacePrefixInName{,React}` lose 16 `a : any` lines each: the
namespace part of `<a:element/>` now prints `undefined`. The parse of that
statement is unchanged; the later `<:a attr=… />` now parses as upstream does,
as `var beginOfIdent1 = <missing> < <missing>, a, attr = …` — upstream's own
`.types` baseline shows that declaration list. With `var a` in scope, the
conformance types producer's `check_expression` on the namespace identifier
resolves it. Upstream never asks: `getTypeOfNode` gates on
`IsExpressionNode`, which is false for an identifier under a
`JsxNamespacedName`, so it prints the error type (`any`). The fix is that gate
in `crates/tsr-conformance/src/types_producer.rs` (not this lane's file).
