# ADR-0005: Generate the AST from upstream's `_scripts/ast.json`

**Status:** Accepted
**Date:** 2026-08-02

## Context

The AST has **351 syntax kinds** and **192 node types**. Hand-writing that, and
then hand-maintaining it against a moving upstream, is not viable — it is exactly
the surface where a missed upstream addition becomes a silent conformance gap.

Three possible codegen inputs:

1. `_submodules/TypeScript/src/compiler/types.ts` — the original TypeScript
   source. Requires parsing TypeScript to build a TypeScript parser.
2. `vendor/typescript-go/internal/ast/ast_generated.go` — the generated Go.
   Requires parsing Go, and is one derivation step removed from the truth.
3. `vendor/typescript-go/_scripts/ast.json` — a **schema-validated, machine-readable
   definition of the entire TypeScript AST**, validated against
   `_scripts/ast.schema.json`, and the input upstream's own
   `_scripts/generate-go-ast.ts` consumes to emit both `ast_generated.go` and
   `kind_generated.go`.

## Decision

Generate from `ast.json`, via `cargo xtask codegen`.

## Reasoning

`ast.json` is upstream's own source of truth, one derivation step *closer* to the
truth than the generated Go, and it needs no parser to read. Deriving our AST from
the same input upstream uses makes conformance a property of the build rather than
a claim in a document: when upstream adds a node kind, regeneration picks it up.

What is generated:

- `SyntaxKind` — 351 variants with **explicit discriminants**, marker constants,
  and range predicates. Discriminants are written out rather than left implicit
  because TypeScript treats kind ordering as semantic (`isAssignmentOperator` is a
  range check), so a reordering upstream must appear as a visible diff.
- 192 node structs, with fields resolved through the base hierarchy.
- 72 alias unions as Rust enums, so `match` is exhaustive.
- A `Visit` trait and `walk_*` functions over all of it.

oxc reached the same conclusion independently: `tasks/ast_tools` is ~20k LOC
generating visitors, `AstKind`, traversal, serialisation, and struct-size
assertions from annotated definitions.

## Consequences

- Generated output is **checked in**, so a plain `cargo build` works without the
  submodule. CI regenerates and diffs to prove it has not gone stale.
- The generator must fail loudly on anything it does not recognise
  ([conventions](../conventions.md)) — a permissive fallback is how a conformance
  gap hides.
- Codegen output is not rustfmt-clean; `cargo fmt` is part of the pipeline, and CI
  formats before diffing.

## The catch that motivated ADR-0006

`ast.json` **can lag the generated Go**. At the pinned commit it lists 349 kinds
while `kind_generated.go` has 351 — `DeferKeyword` and `JSDocAllType` are missing
from its element list. Both are encoded as `{"name": …, "comment": …}`, a third
element shape distinct from a bare string and from `{"comment": …}`.

This is not merely a data quirk; it produced a real bug. A serde `untagged` enum
listing the bare-comment variant before the named variant silently matched
`{"name", "comment"}` as a comment and **dropped both kinds**. Untagged variants
must be ordered most-specific first.

That near-miss is why the conformance oracle is the generated Go, not the JSON we
generate from. See [ADR-0006](0006-conformance-oracle.md).
