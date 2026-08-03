# Technology Stack

**Analysis Date:** 2026-08-03

## Languages

**Primary:**
- Rust 1.85+ (Edition 2024) - All application code, from scanner through parser and conformance harness

## Runtime

**Environment:**
- Rust stable toolchain (dtolnay/rust-toolchain via GitHub Actions)
- Unix/Linux (CI runs on `ubuntu-latest`)

**Package Manager:**
- Cargo (Rust's built-in package manager)
- Lockfile: `Cargo.lock` present and committed

## Frameworks

**Core Architecture:**
- Custom arena-based AST with side tables (see `PLAN.md` §3)
- No external framework dependency — this is a from-scratch port of microsoft/typescript-go

**Build/Code Generation:**
- `xtask` crate for codegen (generates AST from upstream schema)
- Macro-based code generation using `proc-macro2`, `quote`, `syn`

**Testing:**
- `insta` 1.x - Snapshot testing for parser, scanner, conformance
- Cargo's built-in test runner (`#[test]`, `#[bench]`)

## Key Dependencies

**Foundations (mirror oxc's choices per PLAN.md §3.1):**
- `allocator-api2` 0.2.21 - Stable allocator trait and custom bump allocators
- `bitflags` 2.9 - NodeFlags / ModifierFlags / TransformFlags in scanner/parser
- `hashbrown` 0.15 - High-performance hash maps with custom allocator support
- `index_vec` 0.1.4 - Typed index vectors (replace raw usize indices)
- `nonmax` 0.5.5 - NonMaxU32 for compact Option<Id> (4 bytes via niche optimization)
- `rayon` 1.12 - Data parallelism for conformance corpus runs
- `rustc-hash` 2 - Fast non-cryptographic hashing (FxHasher)
- `self_cell` 1.2 - Self-referential structures (arena + AST storage cohabitation)
- `smallvec` 1.15 - Inline vector storage (replaces Go's small-array optimizations)

**Codegen (xtask only):**
- `anyhow` 1 - Error handling in build tasks
- `convert_case` 0.11 - Case conversion for generated identifier names
- `proc-macro2` 1 - Tokenization for code generation
- `quote` 1 - Macro for writing generated Rust code
- `syn` 2 - Parsing Rust syntax (used in codegen)
- `serde` 1 - Serialization framework
- `serde_json` 1 - JSON parsing (reads upstream's ast.json schema)
- `prettyplease` 0.2 - Pretty-printing generated code

**Testing & Utility:**
- `insta` 1 - Snapshot testing (committed snapshots in `crates/tsr-conformance/snapshots/`)

**Transitive Dependencies:**
- `unicode-segmentation` 1.13.3 - Used by `convert_case`
- `crossbeam-*` ecosystem - Transitive via `rayon`
- `either` 1.17.0 - Used by `rayon`
- `itoa`, `memchr` - Used by `serde_json`
- `unicode-ident` 1.0.24 - Used by `proc-macro2`, `syn`
- `zmij` 1.0.23 - Used by `serde_json` (JSON number parsing)

## Configuration

**Formatting:**
- `rustfmt.toml` at root
  - Edition: 2024
  - Max line width: 100 characters
  - Heuristics: "Max"
- CI enforces `cargo fmt --all -- --check` (FAIL on format violations)

**Linting:**
- `clippy.toml` at root
  - Doc valid identifiers: JSDoc, JSX, TSX (proper nouns)
  - Workspace lints configured in `Cargo.toml`
    - `unsafe_op_in_unsafe_fn = "deny"`
    - `missing_docs = "warn"`
    - `clippy:all = "warn"`, `clippy:pedantic = "warn"` (excluding generated code rules)
- CI enforces `cargo clippy --workspace --all-targets -- -D warnings` (FAIL on warnings)

**Build Profiles:**
- `profile.release`: opt-level=3, thin LTO, codegen-units=1, panic=abort (optimized builds)
- `profile.dev.package.*`: opt-level=2 (faster dev builds for dependencies; conformance corpus slow otherwise)

## Platform Requirements

**Development:**
- Rust 1.85 or later
- `rustfmt` and `clippy` components (installed via `dtolnay/rust-toolchain`)
- Git with submodules enabled (vendor/typescript-go is a submodule)
- ~2GB disk for target/ and vendor/ directories

**Production:**
- Runs on any platform Rust 1.85 compiles to
- Currently CI-tested on Ubuntu Linux (u

nknown Rust binary size or memory requirements)

**Upstream Pinning:**
- `vendor/typescript-go` @ commit `5b1047d10` (submodule)
- 300,987 LOC of Go code (read-only reference for conformance and codegen)
- Submodule required for: codegen (`cargo xtask codegen`), conformance corpus runs

## Code Generation

**Source:** `vendor/typescript-go/_scripts/ast.json`

**Process:**
```bash
cargo xtask codegen && cargo fmt --all
```

**Output:** 
- `crates/tsr-ast/src/generated/` - Checked-in generated AST code
- CI validates that checked-in output matches what generator produces (FAIL if stale)

## CI/CD & Testing

**CI Pipeline:** `.github/workflows/ci.yml`

**Stages:**
1. Format check: `cargo fmt --all -- --check`
2. Lint check: `cargo clippy --workspace --all-targets -- -D warnings`
3. Unit tests: `cargo test --workspace`
4. Conformance snapshots: `cargo run -p tsr-conformance --bin coverage` (commits must not change pass rates)
5. Generated code validation: `cargo run -p xtask -- codegen` (commits must not have stale AST)

**Matrix:** Single job on `ubuntu-latest` with `actions/checkout@v4` and `Swatinem/rust-cache@v2`

---

*Stack analysis: 2026-08-03*
