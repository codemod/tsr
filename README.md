# tsr

A Rust port of [microsoft/typescript-go](https://github.com/microsoft/typescript-go).

See [PLAN.md](PLAN.md) for scope, architecture, and phasing.

## Layout

```
crates/tsr-core     spans, typed indices, arenas, side tables
crates/tsr-ast      the TypeScript AST (mostly generated)
xtask               code generation from the vendored upstream definition
vendor/typescript-go  upstream, pinned as a submodule
```

## Getting started

```bash
git submodule update --init --recursive   # required for codegen and conformance
cargo test --workspace
```

## Code generation

`crates/tsr-ast/src/generated` is produced from
`vendor/typescript-go/_scripts/ast.json` — the same schema-validated definition
upstream feeds to its own Go generator.

```bash
cargo xtask codegen && cargo fmt --all
```

Generated output is checked in so a plain `cargo build` works without the
submodule; CI regenerates and diffs to prove it has not gone stale.

## Conformance

Codegen makes the AST match upstream *by construction*; the tests make that
falsifiable:

| Test | Asserts |
|---|---|
| `tsr-ast/tests/kind_conformance.rs` | Every `SyntaxKind` in `internal/ast/kind_generated.go` exists here **with the same discriminant**, and marker constants resolve identically through their alias chains. Checked against the Go source, not against the JSON we generated from — the two do drift. |
| `tsr-ast/tests/node_conformance.rs` | Every node definition has a struct carrying all of its fields, including inherited ones, and appears in the `Node` union. |
| `tsr-ast/tests/visit.rs` | The generated visitor actually descends into children, and pruning works. |

Tests that read the submodule skip when it is absent rather than failing.
