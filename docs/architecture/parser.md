# The parser

**Crate:** `tsr-parser`
**Ported from:** `vendor/typescript-go/internal/parser/parser.go`
**Conformance:** `parser_typescript` 5,000/5,031 (99.38%)

## Shape

Hand-written recursive descent over [`tsr_scanner::Scanner`], allocating into a
[`tsr_core::Arena`]. Recursive descent rather than a generated parser because
TypeScript's grammar is not context-free where it matters — arrow functions versus
parenthesised expressions, `<` as type arguments versus comparison — and every real
implementation resolves those by backtracking, which a table-driven parser cannot
express.

Binary operators use **precedence climbing** rather than one function per level.
TypeScript has 15 binary precedence levels; a function each would be 15
near-identical bodies that drift apart. `binary_precedence` is the single place the
grammar's shape is stated.

## Error recovery

The parser never fails. Every entry point returns a tree *plus* diagnostics,
because the language service must work on incomplete source — a user halfway
through typing a declaration still expects completions. Missing nodes are
synthesised so the tree stays walkable.

Termination is **enforced, not assumed**. `parse_statement_list` compares the
position before and after each statement, and discards a statement that consumed
nothing. This is not defensive padding: a statement parser can legitimately produce
a node while consuming nothing — an expression statement whose expression was
synthesised from a token it could not use — and keeping it would add a zero-width
statement *and* spin the loop forever. A `debug_assert` caught exactly that on
input `@@@`.

A depth guard turns pathological nesting into a diagnostic rather than a stack
overflow, which a language service cannot recover from. **`MAX_DEPTH` is 192**, not
the 512 this document claimed until 2026-08-05: 512 was the original value and was
lowered when the expression parser grew and a 2,000-paren input overflowed in debug
anyway. For scale, the corpus's deepest *real* nesting is 69.

Upstream has no such guard — Go grows a goroutine's stack on demand — so this is a
deliberate divergence, and it is now the *only* depth limit in the codebase.

It does not cover the gap that matters most: `a + a + a …` is parsed **iteratively**
by precedence climbing, so `descend()` never fires, yet the tree is as deep as the
chain is long and consumers walk it recursively. Parsing
`compiler/binderBinaryExpressionStress` costs 256 KiB of stack; parsing *and binding*
it once cost 4 MiB. So the parser protects itself and leaves the binder, printer and
checker exposed to the trees it legitimately produces.

That exposure is closed elsewhere, and **not** by more depth guards: consumers wrap
their recursive walks in `tsr_core::stack::ensure_sufficient`, which grows the stack
on demand on native targets. Guards were tried first and abandoned — see
[ADR-0030](../adr/0030-grow-the-stack-natively-wasm-traps.md), which supersedes
ADR-0029. `MAX_DEPTH` here survives that reversal only because the parser already had
it and it is pinned by a test; a parser rewrite would not add it today.

## Backtracking, and the trap in it

`try_parse` saves the scanner position, runs a closure, and rewinds if it returns
`None` — discarding diagnostics and node registrations from the abandoned path, so
speculation cannot leave phantom errors behind.

**The obvious way to use it for arrow functions is exponential.** `(a)` and
`(a) => a` are identical up to the `=>`, so the parser must look ahead. Speculating
by parsing a real parameter list re-enters `parse_assignment_expression` for each
parameter's initializer — which speculates again. On nested assignments like
`E = (E = (E = …))` every level re-parses the whole tail: 2ⁿ.

The corpus contains exactly that shape
(`compiler/parsingDeepParenthensizedExpression.ts`, 9 KB, 69 levels deep). It took
the parser to **16 GB of RSS and no termination**.

`is_arrow_function_ahead` replaces it with a **token-only scan**: walk to the
matching `)`, then check whether `=>` follows (or `:` and then `=>` before the
body). No nodes are built and no expression parser is re-entered, so it is linear
in the group's length. TypeScript resolves the same ambiguity the same way. The
file now parses in 2.3 ms.

The lookahead must stop at a statement boundary, or `(a); x => y` would read `(a)`
as an arrow's parameter list — covered by a test.

## Node identity

`finish_node` registers every node's kind and span in a `NodeTable` and stamps the
returned `NodeId` into the node's `Cell`. Spans exclude leading trivia and end
where the previous token ended.

## What is implemented

Statements (all control flow, labels, `try`/`catch`/`finally`, `switch`), variable
declarations with destructuring, functions, classes (properties, methods,
constructors, accessors, heritage clauses), interfaces, type aliases, enums,
imports and exports in all their forms, namespaces, and the expression grammar
including template literals, regular expressions, `as`/`satisfies`, optional
chaining, and non-null assertions.

Types: unions, intersections, arrays, tuples, type literals, type references with
arguments, `typeof`, `keyof`, indexed access, literal types, function and
constructor types, conditional types, and `infer`.

## Threading

`ParsedFile` bundles the arena, source, and tree into one owned `Send` value so a
worker can parse a file and hand the result back. See
[threading.md](threading.md).

## Tuple labels and rest ownership

At native revision `5b1047d`, `parseTupleElementNameOrTupleElementType` scans an
optional ellipsis followed by an IdentifierName and `:` or `?:`. Reserved words
are legal labels: ioredis uses `[function: string | Buffer, ...args: RedisValue[]]`.
This does not make `function` a legal ordinary parameter binding. Tuple-list
admission is also separate from label lookahead: native accepts a plain `function`
label, but admits `class` and `return` labels only after an ellipsis. Keep the
existing `isStartOfType` boundary rather than admitting every keyword to the list.

The named member owns its ellipsis and question token. Its operand uses
`parseTupleElementType`, including optional/rest recovery; an unnamed rest's
operand uses `parseType`. Changing the old `RestType(NamedTupleMember)` shape
therefore requires the checker consumers to read the named member's flags for
normalization, alias rendering and contextual rest signatures.

The first parser/consumer patch passed all workspace tests but lost two previously
RIGHT corpus results (`Opt` and `Trailing` in `namedTupleMembersErrors`). The
missing `OptionalType`, `NamedTupleMember` and `RestType` semantic dispatch workers
must preserve native recovered types even when grammar reports an error. The
array-element helper borrows the parsed operand and traverses arrays,
parenthesized types and single-rest tuples; it performs no semantic resolution
and publishes no cache entry. The corrected patch retains every previously RIGHT
type/diagnostic result and gains one type result across the complete legacy dumps.

This is a correctness prerequisite under `tsr-1yb.35`, not a performance result.
Removing the four ioredis parser errors lets the CLI reach semantic checking;
the application still exposes checker gaps while native exits successfully.

## Dialect

`<` means two different things and the readings are mutually exclusive: in `.ts`
it opens a type assertion (`<Foo>x`), in `.tsx` it opens a JSX element. No cheap
lookahead separates `<Foo>x` from `<Foo>x</Foo>`, which is why TypeScript ties the
choice to the file extension rather than to context — and why `ScriptKind` is
threaded through the parser rather than inferred.

JSX parsing is the one place the parser drives the scanner's mode explicitly:
children, attribute values, and names each need a different scan, and only the
parser knows which position it is in. See [scanner.md](scanner.md).

## Two lookaheads that keep getting subtler

Both of the parser's hard decisions are token-level scans, and both were wrong in
ways that only the corpus revealed.

**"Is this an arrow function?"** must track bracket depth *and* angle brackets. In
`(): (() => T) => x` the inner `)` must not end the scan and the inner `=>` must
not satisfy it; in `(): Iterable<number, any> => x` the comma inside the type
arguments must not end it either. Everything after the `:` is a type, so `<` is
unambiguously a bracket there.

**Shift tokens are several brackets.** The scanner emits `>>` and `>>>` as single
tokens, so a naive bracket counter never balances `<K extends Key<U>>`, and
`ReturnType<<T>() => number>` opens with one `<<`. Both directions need splitting;
`greater_than_count` and `rescan_less_than` do it.

## Three things that were parsed lossily

Each was found from the binder side, and none produced a diagnostic — a lossy
parse of a valid program is silent by construction, which is what makes this
class of bug worth naming.

**A contextual keyword in expression position became a `KeywordExpression`**,
which carries a kind and no text. `module`, `type`, `of`, `as`, `declare`,
`async`, `get` — every non-reserved keyword used as a value — referred to
nothing, so `module.exports` and `const x = type` bound no symbol and narrowed
nothing. Only a *reserved* word (`this`, `super`, `true`, `false`, `null`) is the
keyword when it appears as a value; upstream falls through to `parseIdentifier()`
for the rest, and so does this parser now. Worth 14 conformance cases in
`binder_symbols`, and 1,977 more flow nodes across the benchmark fixtures — an
identifier is a narrowable reference and a keyword expression is not.

The two export forms:

- **`export default class C {}` dropped the `default` keyword.** Only `export`
  survived as a modifier, so nothing downstream could tell it from
  `export class C {}` — and `default` is the *only* thing that distinguishes
  them. It is now a modifier alongside `export`, as upstream has it, which is
  what lets the binder file the export under the name `default`.
- **`export as namespace N` was recorded as an `ExportAssignment`**, the same
  node as `export default N`. Those mean different things: one claims a global
  name for a UMD module, the other exports a value. It is now a
  `NamespaceExportDeclaration` with `N` as its name.

## Whether a modifier keyword is really a name

`class C { static static }` declares a static member called `static`, and no
rule about *what follows* reaches that: the second `static` is followed by `}`
in one test and by `[x: string]: string` in another. The parser used to decide
with a blacklist of tokens a name could be followed by, which got this and
several neighbours wrong.

Upstream tests the opposite way (`tryParseModifier`, `nextTokenCanFollowModifier`),
and it is now ported arm for arm:

- a **whitelist** of what may follow a modifier — `[`, `{`, `*`, `...`, or a
  literal property name, keywords included;
- `hasSeenStaticModifier`: a second `static` in one list is never a modifier;
- everything except `static` must be followed **on the same line**;
- `export` may be followed by a decorator and not by `*`, `as` or `{`;
- `default` is followed by the declaration it exports.

The blacklist had been silently losing `@dec export @dec class C {}` — the whole
class. Against that, the parser now *reports* errors on six error-recovery cases
it used to mis-parse quietly, which costs the binder suite six judged cases; see
[binder.md](binder.md#the-denominator-moved-by-six-and-not-on-purpose).

### `class implements` is the same question, one token later

`implements` is a *future reserved* word, so it is a legal binding identifier
outside strict mode, and `class implements … ` is ambiguous:

```ts
const C = class implements number {};   // no name; `implements` opens the clause
const D = class implements {};          // a class named `implements`
```

Upstream disambiguates by looking one token past `implements` — an identifier or
keyword means a heritage clause (`isImplementsClause`, `parser.go:1806`, used by
`parseNameOfClassDeclarationOrExpression`, `:1791`). Both directions matter, and the
test asserts both: a fix that always answered "heritage clause" would lose the
second line.

Missing this made every `class implements T` a class *named* `implements`, so two of
them in one file were a duplicate identifier — found as a binder TS2300, six of them,
and fixed here because a wrong name is what the binder is handed. See
[ADR-0026](../adr/0026-class-expressions-and-multiple-default-exports.md), which also
records that the fix moved the `binder_symbols` and `printer_round_trip`
**denominators** by two: units that had not parsed now do.

## Not yet built

- **The JSDoc reparser** — `@type` and `@param` promoted to real annotations in
  `.js` files. JSDoc itself parses; see [jsdoc.md](jsdoc.md).
- **ASI inside type members** — `a?: number` followed by `extends?: string` on the
  next line reads the `extends` as a conditional type.
- **Import types with attributes** (`import("pkg", { with: … })`).
- 32 corpus cases still fail; `cargo run -p tsr-conformance --example
  failure_classes` buckets them.
- Roughly 8% of clean corpus files still report a diagnostic; the snapshot lists
  the first failure per case, and
  `cargo run -p tsr-conformance --example failure_classes` buckets all of them,
  which is the fastest way to pick the next gap.
