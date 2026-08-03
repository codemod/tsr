<!-- refreshed: 2026-08-03 -->
# Codebase Structure

**Analysis Date:** 2026-08-03

## Directory Layout

```
/home/langport/codemod/tsr/
├── crates/                      # Workspace crates — Phase 0 through Phase 4 implementation
│   ├── tsr-core/                # Foundational types: arena, typed indices, side tables
│   ├── tsr-ast/                 # Generated AST nodes, kinds, visitor (351 syntax kinds)
│   ├── tsr-diagnostics/         # Generated diagnostic catalogue (2,000+ error codes)
│   ├── tsr-scanner/             # Lexer: pull-based token cursor, JSDoc, Unicode
│   ├── tsr-parser/              # Recursive-descent parser (95.18% conformance; not in workspace)
│   └── tsr-conformance/         # Test harness: runs corpus, generates snapshots
├── xtask/                       # Build-time codegen: AST, diagnostics, Unicode tables
├── vendor/                      # Submodules
│   └── typescript-go/           # Upstream Go implementation (pinned at 5b1047d10)
│       ├── _scripts/ast.json    # Schema: node definitions, kinds, unions
│       ├── _submodules/TypeScript/tests/  # Test corpus (12,444 files)
│       └── testdata/baselines/reference/  # Expected outputs (49,354 files)
├── docs/                        # Architecture and design decision records
│   ├── adr/                     # Architecture Decision Records (ADR-0001 through ADR-0008)
│   ├── architecture/            # Component docs (scanner, parser, AST, threading, conformance)
│   └── conventions.md           # Cross-cutting conventions and patterns
├── .planning/codebase/          # Generated codebase analysis (this directory)
├── .beads/                      # Issue tracker database (beads/dolt)
├── .claude/                     # Project-local Claude Code configuration
├── .codegraph/                  # CodeGraph knowledge graph index
├── PLAN.md                      # Roadmap: scope, phases, decisions (20 KB)
├── CLAUDE.md                    # Agent instructions for this project
├── Cargo.toml                   # Workspace manifest: 6 crates + xtask
├── Cargo.lock                   # Locked dependency versions
├── README.md                    # Project overview
├── rustfmt.toml                 # Rust code formatting config
└── clippy.toml                  # Clippy linting config
```

## Directory Purposes

**`crates/`**
- Purpose: Rust workspace crates, each a compilation unit with distinct responsibilities
- Contains: 6 crates in Phases 0-1 (scanner, parser, core types, diagnostics, AST, test harness)
- Key files: Each crate has `Cargo.toml`, `src/lib.rs`, `src/main.rs` (if binary), and `tests/` or `examples/`

**`crates/tsr-core/`**
- Purpose: Foundational types every other crate depends on
- Contains: Arena allocator, typed index system, side-table macros, span types
- Key files:
  - `src/arena.rs`: Arena allocator trait and implementation
  - `src/index.rs`: `define_index!` macro for creating newtype indices backed by NonMaxU32
  - `src/side_table.rs`: `side_tables!` macro (dense struct-of-arrays) and `PagedTable` (sparse paged tables)
  - `src/span.rs`: Byte-offset source positions

**`crates/tsr-ast/`**
- Purpose: Node type definitions, syntax kinds, and visitor pattern — all generated
- Contains: 351 syntax kinds, 192 node types, 72 type unions, visitor implementations
- Key files:
  - `src/generated/kind.rs`: `SyntaxKind` enum and `AstKind` (all node kinds)
  - `src/generated/nodes.rs`: Node struct definitions (e.g., `BinaryExpression`, `CallExpression`)
  - `src/generated/alias.rs`: Union type definitions (`Expression`, `Statement`, `TypeNode`, etc.)
  - `src/generated/visit.rs`: Visitor trait and walk functions
  - `src/generated/manifest.json`: Metadata from codegen (kind count, node count)
  - `src/flags.rs`: Node modifier flags, type flags (hand-written)

**`crates/tsr-diagnostics/`**
- Purpose: Diagnostic message catalogue — all generated from TypeScript sources
- Contains: 2,000+ error messages with codes, categories, and placeholder formats
- Key files:
  - `src/generated/messages.rs`: Static diagnostic definitions (auto-generated from TypeScript `diagnosticMessages.json`)
  - Diagnostics are keyed by `u32` error code (e.g., 1002 = "Unterminated string literal")

**`crates/tsr-scanner/`**
- Purpose: Lexical analysis: source text → tokens
- Contains: Pull-based scanner cursor, token types, JSDoc pre-scanning, Unicode identifier tables
- Key files:
  - `src/lib.rs`: Main `Scanner` struct and token generation
  - `src/token.rs`: Token kind enum, token struct definition
  - `src/jsdoc.rs`: JSDoc comment parsing and annotation extraction
  - `src/generated/unicode.rs`: Unicode identifier start/part tables (generated from Go)

**`crates/tsr-parser/`**
- Purpose: Syntax analysis: tokens → abstract syntax tree
- **Status**: Not yet in workspace (`Cargo.toml` excludes it; ~40 constructor signature reconciliations needed)
- Contains: Recursive-descent parser, error recovery, backtracking, node registration
- Key files:
  - `src/lib.rs`: Crate overview and public API (when enabled)
  - `src/parser.rs`: Main parser state machine, token flow, node finishing, backtracking
  - `src/statement.rs`: Statement parsing (if/for/while/try/class/interface/etc.)
  - `src/declaration.rs`: Declaration parsing (var/const/function/enum/namespace/etc.)
  - `src/expression.rs`: Expression parsing (binary precedence climbing, ternary, assignment, arrow functions)
  - `src/types.rs`: Type syntax parsing (unions, intersections, generics, conditionals, etc.)
  - `src/jsx.rs`: JSX element and attribute parsing
  - `src/jsdoc.rs`: JSDoc reparser (promoted annotations in .js files)
  - `src/parsed_file.rs`: `ParsedFile` container (self-referential arena + source + AST)

**`crates/tsr-conformance/`**
- Purpose: Test harness comparing parser output against 49,354 TypeScript baseline files
- Contains: Corpus loading, baseline index, case execution, snapshot generation and comparison
- Key files:
  - `src/lib.rs`: Harness public API
  - `src/main.rs`: `coverage` binary entry point (prints pass-rate summary, writes snapshots)
  - `src/corpus.rs`: Loads test corpus from `vendor/typescript-go/_submodules/TypeScript/tests/`
  - `src/case.rs`: Single test case execution and result collection
  - `src/snapshot.rs`: Reads baseline expectations, writes committed snapshots for ratcheting
  - `src/suite.rs`: Test suite grouping (e.g., `scanner_clean_files`, `parser_typescript`)
  - `src/suites.rs`: All suites and per-suite pass-rate calculation
  - `examples/failure_classes.rs`: Analyzes failures by category (useful for prioritizing next work)

**`xtask/`**
- Purpose: Build-time code generation — must run before any crate that depends on generated code
- Contains: AST schema parser, node generator, diagnostics generator, Unicode table generator
- Key files:
  - `src/main.rs`: Entry point (`cargo xtask codegen`), orchestrates all generators
  - `src/ast_json.rs`: Parses `vendor/typescript-go/_scripts/ast.json` into `AstDefinition` struct
  - `src/gen_nodes.rs`: Generates node struct definitions and visitor code; parses upstream Go for nullability info
  - `src/gen_kind.rs`: Generates `SyntaxKind` enum and `AstKind`
  - `src/gen_diagnostics.rs`: Generates diagnostic message catalogue from TypeScript sources
  - `src/gen_unicode.rs`: Extracts Unicode identifier tables from upstream Go

**`vendor/typescript-go/`**
- Purpose: Pinned upstream Go implementation (submodule at commit `5b1047d10`)
- Contains: 300,987 LOC of Go source; test cases; reference baselines
- **Do not edit**: This is read-only source for conformance and schema
- Key subdirectories:
  - `_scripts/ast.json`: Schema for 351 syntax kinds and 192 node types
  - `_submodules/TypeScript/tests/cases/`: 12,444 test input files
  - `testdata/baselines/reference/`: 49,354 expected output files (errors, AST dumps, etc.)

**`docs/`**
- Purpose: Architecture documentation and design decisions
- Contains: ADRs (immutable), architecture guides (living docs), conventions
- Key files:
  - `adr/0001-idiomatic-rewrite.md`: Decision to rewrite idiomatically, not mechanically
  - `adr/0003-tree-plus-side-tables.md`: The central AST architectural decision
  - `adr/0005-codegen-from-ast-json.md`: AST codegen philosophy
  - `adr/0006-conformance-oracle.md`: Using Go-generated baselines as the truth
  - `adr/0007-generated-code-policy.md`: Which code is generated and why
  - `architecture/ast.md`: Node shapes, side tables, unions, traversal
  - `architecture/scanner.md`: Pull-based lexing, context-dependent scanning, JSX modes
  - `architecture/parser.md`: Recursive descent, error recovery, backtracking pitfalls
  - `architecture/threading.md`: Per-file parallelism, arena safety, determinism
  - `architecture/conformance.md`: Test harness design and current pass rates
  - `conventions.md`: Cross-cutting rules (upstream anchoring, code gen policy, etc.)

## Key File Locations

**Entry Points:**
- `tsr-conformance` binary: `crates/tsr-conformance/src/main.rs` (prints test results)
- Parser API: `crates/tsr-parser/src/lib.rs:Parser::parse_source_file` (not yet compilable)
- Codegen: `xtask/src/main.rs:codegen()` (called via `cargo xtask codegen`)

**Configuration:**
- Workspace manifest: `Cargo.toml` (6 crates, workspace dependencies, lints)
- Codegen config: `xtask/Cargo.toml` (pulls dependencies for build-time work)
- Format config: `rustfmt.toml` (runs via `cargo fmt`)
- Lint config: `clippy.toml` (runs via `cargo clippy`)
- Edition: `Cargo.toml` `edition = "2024"` (latest Rust)
- Rust version: `Cargo.toml` `rust-version = "1.85"`

**Core Logic:**
- Scanner state machine: `crates/tsr-scanner/src/lib.rs`
- Parser state machine: `crates/tsr-parser/src/parser.rs`
- Expression precedence: `crates/tsr-parser/src/parser.rs:binary_precedence()` (15 levels)
- Arrow function lookahead: `crates/tsr-parser/src/parser.rs:is_arrow_function_ahead()` (token-only scan)
- Node type definitions: `crates/tsr-ast/src/generated/nodes.rs` (392 KB, all generated)
- Visitor pattern: `crates/tsr-ast/src/generated/visit.rs` (generated)

**Testing:**
- Conformance tests: `crates/tsr-conformance/tests/` (regression checks on snapshot format)
- Corpus loading tests: `crates/tsr-conformance/src/corpus.rs` (has inline test modules)
- Scanner tests: `crates/tsr-scanner/tests/` (keyword conformance, etc.)
- Parser tests: `crates/tsr-parser/tests/` (would be here when parser is re-enabled)
- Snapshots: `crates/tsr-conformance/snapshots/*.snap` (committed, ratcheted pass-rate)

**Documentation:**
- Architecture overview: `PLAN.md` (20 KB, 9 phases, decisions, consequences)
- Architecture decisions: `docs/adr/` (immutable, numbered ADRs 0001–0008)
- Component guides: `docs/architecture/` (living docs, edit in place)
- Coding conventions: `docs/conventions.md` (cross-cutting rules)

## Naming Conventions

**Files:**
- Crate names: `tsr-<component>` (e.g., `tsr-parser`, `tsr-scanner`)
- Generated code: `crates/*/src/generated/` (everything in this directory is `@generated`, never hand-edit)
- Test files: `crates/*/tests/*.rs` (one test per file, or `tests/` directory with submodules)
- Examples: `crates/*/examples/*.rs` (runnable examples, e.g., `failure_classes.rs`)

**Directories:**
- Module directories: `src/` (one `lib.rs` per crate)
- Tests: `tests/` (integration tests; unit tests are inline in `src/`)
- Examples: `examples/` (runnable binaries)
- Generated output: `generated/` (inside `src/`, read-only)

**Rust Types:**
- Syntax kinds: `SyntaxKind` enum (generated, all-caps variants: `VariableStatement`, `IfStatement`, etc.)
- Node types: PascalCase matching TypeScript names (e.g., `BinaryExpression`, `CallExpression`, `IfStatement`)
- Newtype indices: PascalCase ending in `Id` (e.g., `NodeId`, `SymbolId`, `TypeId`)
- Enums: PascalCase (e.g., `TokenKind`, `StatementKind`)
- Structs: PascalCase (e.g., `Scanner`, `Parser`)
- Modules: snake_case (e.g., `parsed_file`, `binary_expression`)

**Spans & Positions:**
- Byte-offset ranges: `Span` struct in `tsr-core` (start: u32, length: u32 — both byte offsets, not character offsets)
- Error line/column: Computed from span when generating diagnostic output (LSP protocol uses line/column)

## Where to Add New Code

**New Language Feature (e.g., TypeScript 5.1 feature):**
1. If it changes AST shape:
   - Add to `vendor/typescript-go/_scripts/ast.json` (if not already there)
   - Run `cargo xtask codegen` to regenerate `crates/tsr-ast/src/generated/`
   - Update `crates/tsr-parser/src/expression.rs` or `src/statement.rs` or `src/types.rs` to parse it
2. If it's a new diagnostic:
   - Add to `vendor/typescript-go/_submodules/TypeScript/src/compiler/diagnosticMessages.json`
   - Run `cargo xtask codegen` to regenerate `crates/tsr-diagnostics/src/generated/messages.rs`
3. Add conformance tests:
   - Ensure upstream test corpus covers the feature (already in `vendor/typescript-go/_submodules/TypeScript/tests/`)
   - Run `cargo run -p tsr-conformance --bin coverage` to measure pass-rate

**New Parser Construct:**
- Location: `crates/tsr-parser/src/` — add to `expression.rs`, `statement.rs`, `types.rs`, or `declaration.rs`
- Naming: Match upstream's function names (e.g., `parse_assignment_expression`, `parse_binary_expression`)
- Pattern: Each function returns `Option<Node>` or synthesises a node on error; register with `finish_node`
- Testing: Conformance harness automatically runs against corpus

**New Scanner Token Mode:**
- Location: `crates/tsr-scanner/src/lib.rs` (add `scan_*` method or rescan variant)
- Example: `scan_jsx_token`, `rescan_as_regular_expression`, `rescan_template_continuation`
- Testing: `scanner_clean_files` suite checks zero diagnostics on valid source

**New Diagnostic Code:**
- Source: Upstream TypeScript `diagnosticMessages.json`
- Registration: Run `cargo xtask codegen`
- Usage: Reference via `tsr_diagnostics::messages::<CONSTANT>` in parser

**New Conformance Suite:**
- Location: `crates/tsr-conformance/src/suites.rs` (define new suite struct)
- Pattern: Inherit from existing suite (e.g., `ScannerSuite`, `ParserSuite`) or create new one
- Registration: Add to `all_suites()` function so it runs in `cargo run -p tsr-conformance --bin coverage`

**Utility/Helper Function:**
- If shared across crates: add to `tsr-core` or create new crate
- If specific to one stage: add to that crate's `lib.rs` or a new module (e.g., `src/helpers.rs`)
- Pattern: Document upstream counterpart in rustdoc (e.g., `//! Corresponds to typescript-go's internal/parser/parser.go:Parser.parseAssignmentExpression`)

## Special Directories

**`generated/` (inside each crate's `src/`)**
- Purpose: Output from `cargo xtask codegen`
- Generated: Yes (by xtask)
- Committed: Yes (to git, so diff against `ast.json` changes is reviewable)
- **Never hand-edit**: Regenerate via `cargo xtask codegen`
- Tracked: `tests/kind_conformance.rs` asserts that regenerated files match upstream

**`vendor/`**
- Purpose: Pinned upstream Go implementation
- Generated: No (submodule)
- Committed: No (via `.gitmodules`)
- **Never modify**: Upstream source; conformance depends on this exact commit
- Initialize: `git submodule update --init --recursive`

**`.planning/codebase/`**
- Purpose: Generated codebase analysis (ARCHITECTURE.md, STRUCTURE.md, etc.)
- Generated: Yes (by `/gsd-map-codebase` or similar)
- Committed: Yes (to git)
- Never hand-edit: Regenerate via agent tools

**`.beads/`**
- Purpose: Issue tracker database (embedded Dolt)
- Generated: Yes (by beads/bd)
- Committed: No (git-ignored)
- Auto-sync: `refs/dolt/data` on remote tracks changes
- Commands: `bd prime`, `bd ready`, `bd show <id>`, `bd update <id>`, `bd close <id>`

**`.codegraph/`**
- Purpose: Knowledge graph index for symbol navigation
- Generated: Yes (auto-indexed via file watcher)
- Committed: No (git-ignored)
- Lag: ~1 second behind file changes
- Query: `codegraph explore "<symbol>"` or CodeGraph MCP tool

## Build & Test Commands

```bash
# Generate AST, diagnostics, Unicode tables from upstream sources
cargo xtask codegen && cargo fmt --all

# Run full test suite (all conformance suites)
cargo run -p tsr-conformance --bin coverage

# Run specific conformance example (analyze failures by category)
cargo run -p tsr-conformance --example failure_classes

# Check code formatting and lints
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings

# Rebuild for release (optimized, faster conformance runs)
cargo build --release -p tsr-conformance

# Watch for changes and rebuild (handy during development)
cargo watch -x "build"  # requires `cargo install cargo-watch`
```

---

*Structure analysis: 2026-08-03*
