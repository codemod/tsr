# The parser

**Crate:** `tsr-parser`
**Ported from:** `vendor/typescript-go/internal/parser/parser.go`
**Conformance:** `parser_typescript` 3,969/5,648 (70.27%)

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

## Not yet built

- **JSX** — needs the scanner's JSX mode (`bd tsr-pum.1`) first.
- **Decorators** (`@foo`) and **private names** (`#x`) in expression position.
- **Mapped types** (`{ [K in T]: U }`) and **template literal types**.
- **JSDoc** parsing.
- **`abstract` members, index signatures, call/construct signatures** in type
  literals.
- Roughly 30% of clean corpus files still report a diagnostic; the snapshot lists
  the first failure per case, which is the fastest way to pick the next gap.
