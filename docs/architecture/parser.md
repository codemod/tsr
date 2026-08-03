# The parser

**Crate:** `tsr-parser`
**Ported from:** `vendor/typescript-go/internal/parser/parser.go`
**Conformance:** `parser_typescript` 4,999/5,031 (99.36%)

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

A depth guard (`MAX_DEPTH`, 512) turns pathological nesting into a diagnostic
rather than a stack overflow, which a language service cannot recover from.

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
