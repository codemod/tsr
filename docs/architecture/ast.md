# The AST

**Crates:** `tsr-core` (ids, spans, side tables), `tsr-ast` (nodes, kinds, visitor)
**Generated from:** `vendor/typescript-go/_scripts/ast.json` @ `5b1047d10`
**Decisions:** [ADR-0002](../adr/0002-own-ast.md), [ADR-0003](../adr/0003-tree-plus-side-tables.md), [ADR-0005](../adr/0005-codegen-from-ast-json.md)

## Shape

The organising principle is [ADR-0003](../adr/0003-tree-plus-side-tables.md): **the
tree is a tree.** Node structs hold syntax children as direct references and
nothing else. Parent, symbol, scope, type, and flow-node relationships — every
edge that would make the graph cyclic — live in side tables keyed by `NodeId`.

```rust
pub struct BinaryExpression<'a> {
    pub modifiers: &'a [ModifierLike<'a>],
    pub left: Expression<'a>,
    pub r#type: Option<TypeNode<'a>>,
    pub operator_token: &'a Token<'a>,
    pub right: Expression<'a>,
}
```

No `parent`. No `symbol`. Those are `NodeTable` columns.

## Ids

`define_index!` generates newtype indices backed by `nonmax::NonMaxU32`, so
`Option<NodeId>` occupies **4 bytes** rather than 8 via niche optimisation —
asserted in `tsr-core`'s tests, because it is the kind of property that silently
regresses.

A `NodeId` is scoped to one source file, not to the program.

## Side tables

Two shapes, mirroring the two upstream uses in `internal/core/linkstore.go`:

- **`side_tables!`** — dense struct-of-arrays, one `Vec` per column with a shared
  length. For data every node has. Iterating one column touches no cache lines
  belonging to the others, which matters because upstream reads `.Parent` **2,092
  times** across `checker`/`ls`/`binder` without touching flags. This is the
  analogue of oxc's `multi_index_vec!`, itself modeled on Zig's `MultiArrayList`.

  Accessor names are spelled out per column (`get, get_mut, column`) because
  `macro_rules!` cannot concatenate identifiers. oxc makes the same trade for the
  same reason. An earlier version tried to generate all three from one name and
  produced three same-named methods — Rust has no arity overloading.

- **`PagedTable`** — sparse, materialised lazily in 256-entry pages. A port of
  upstream's `PagedLinkStore`, which is what the checker's 23 link stores are built
  on. Allocating a dense row per node in each of 23 stores would be wasteful.

## Unions

The 72 alias unions (`Expression`, `Statement`, `TypeNode`, …) are Rust enums.
This is the central idiomatic-Rust payoff: upstream models these as an untyped
`*ast.Node` plus a runtime kind check, so a new node kind is a silent
fallthrough. Here it is a non-exhaustive-match compile error.

Each union gets a `From<Alias> for Node` impl, since widening to the universal
`Node` union is needed constantly by traversal and the language service.

## Tokens

Upstream models tokens as a generic `Token[TKind]` with 33 named instantiations
(`AsteriskToken`, `QuestionToken`, `BinaryOperatorToken`, …). Since they differ
only in which kinds they admit, they collapse to a single `Token` carrying its
kind; the permitted set is recorded in the doc comment on each field that uses it.

This loses a type-level constraint upstream also does not enforce at the Go type
level, so nothing is given up in practice.

## Traversal

`Visit` has one `visit_*` method per node type, each defaulting to the matching
`walk_*` free function. Overriding and calling `walk_*` continues into children;
omitting the call prunes the subtree. Both behaviours are covered by tests, because
a visitor that compiles but silently skips children looks fine until the binder
starts missing declarations.

## Kind count — a corrected number

**351** syntax kinds, per `internal/ast/kind_generated.go`.

Earlier planning documents said 386. That was wrong: it counted marker aliases
(`KindFirstAssignment`, …) and `KindCount` alongside real kinds. The figure is
corrected here and in `PLAN.md` rather than silently replaced, per
[docs/README.md](../README.md).

Separately, `ast.json` lists only **349** — it lags the generated Go by
`DeferKeyword` and `JSDocAllType`. See [ADR-0006](../adr/0006-conformance-oracle.md).

## Not yet built

- **Arena allocation.** Node structs are lifetime-parameterised (`<'a>`) in
  anticipation, but nothing allocates them yet — there is no parser. The
  `allocator-api2`-based arena and `AllocatorPool` are Phase 1 work.
- **`self_cell` storage.** Long-lived ASTs across LSP edits need the
  arena-plus-AST bundled in a self-referential cell with one audited
  `unsafe impl Send`, as oxc does in `oxc_type_checker::compiler::source_file`.
  Not needed until something holds an AST.
- **Struct-size assertions.** oxc fails CI when a node grows. We should do the
  same; not yet wired.
