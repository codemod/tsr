# tsr

A Rust port of [microsoft/typescript-go](https://github.com/microsoft/typescript-go).

See [PLAN.md](PLAN.md) for scope, architecture, and phasing.

## Layout

```
crates/tsr-core         spans, typed indices, arenas, side tables
crates/tsr-ast          the TypeScript AST (mostly generated)
crates/tsr-diagnostics  the diagnostic message catalogue (generated)
crates/tsr-scanner      the scanner (lexer)
crates/tsr-parser       the parser
crates/tsr-conformance  the corpus harness and its committed snapshots
xtask                   code generation from the vendored upstream definition
docs/                   decision records and architecture
vendor/typescript-go    upstream, pinned as a submodule
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

### Corpus conformance

```bash
cargo run -p tsr-conformance --bin coverage
```

Runs the 12,444-case TypeScript corpus and writes committed snapshots to
`crates/tsr-conformance/snapshots/`. Stages that do not exist yet report 0%
explicitly rather than being omitted.

| Suite | Now | Measures |
|---|---|---|
| `corpus_ingest` | 100% | the harness reads and splits every case |
| `baseline_resolution` | 100% | every case maps to its baselines |
| `parser_reachable_target` | 5031/10570 | how many cases a parser alone could be judged against |
| `scanner_termination` | 100% | the scanner consumes every file without stalling |
| `scanner_clean_files` | 100% | files TypeScript accepts produce no scan errors |
| `parser_typescript` | 4999/5031 (99.36%) | files TypeScript accepts parse with no diagnostics |
| `binder_symbols` | 4729/7621 (62.05%) | symbols in upstream's `.symbols` baseline exist with the same declaration lines |

Against typescript-go at the pinned commit, single-threaded and at equal work:
parse throughput **1.45×** on the large fixtures (2–3.8× on small ones),
peak RSS **1.74× lower** holding the same trees. CI enforces both
(`.github/workflows/perf.yml`) and publishes the measurements as an artifact. Method, profiles, and the caveats that
matter are in [docs/architecture/performance.md](docs/architecture/performance.md).

See [docs/architecture/conformance.md](docs/architecture/conformance.md).
