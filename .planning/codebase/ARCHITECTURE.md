<!-- refreshed: 2026-08-03 -->
# Architecture

**Analysis Date:** 2026-08-03

## System Overview

This is a Rust port of `microsoft/typescript-go`, an idiomatic rewrite (not a mechanical transliteration) targeting both compatibility and Rust idioms. The architecture centers on one principle:

> **The tree is a tree. Everything cyclic lives in id-keyed side tables.**

```text
┌──────────────────────────────────────────────────────────────────────┐
│                          Input Source Text                            │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         SCANNER (Phase 1)                             │
│  `tsr-scanner`                                                        │
│  Pull-based lexer producing one token at a time; rescans for context │
│  (regex detection, template continuations, JSX modes)                │
│  Status: 100% corpus conformance (12,444/12,444)                     │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         PARSER (Phase 1)                              │
│  `tsr-parser`                                                         │
│  Hand-written recursive descent over Scanner                          │
│  Arena-allocated tree with direct child references                   │
│  Status: 95.18% corpus conformance (5,376/5,648)                     │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
    ┌────────┐          ┌──────────┐      ┌──────────┐
    │ BINDER │          │   SIDE   │      │ SYMBOL / │
    │(Phase 2)│          │ TABLES   │      │  TYPE    │
    │ Symbols,│          │  Parent  │      │   TABLES │
    │ Scopes, │          │  Flags   │      │ (Phase 2)│
    │ FlowNode│          │(NodeTbl) │      └──────────┘
    └────────┘          └──────────┘
        │                   │
        └───────────────────┴─────────────────────────┐
                                                      │
                            ┌─────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│                   CHECKED AST (Phase 4)                               │
│  The AST with type information, semantic analysis complete           │
│  `tsr-checker` (60,000 LOC)                                          │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
        ┌───────────────────┴───────────────────┐
        │                                       │
        ▼                                       ▼
┌──────────────────────────┐        ┌────────────────────────┐
│  TRANSFORMERS & PRINTER  │        │    LANGUAGE SERVICE    │
│  (Phase 5)               │        │    & LSP (Phase 7-8)   │
│  ts→js, jsx, decorators, │        │  Completions, goto-def│
│  cjs/esm, source maps    │        │  References, rename    │
└───────────────────────────┘        └────────────────────────┘
        │
        ▼
   Output (.js, .d.ts)
```

## Component Responsibilities

| Component | Responsibility | Files |
|-----------|----------------|-------|
| **tsr-core** | Foundational types: typed indices (NodeId, SymbolId, TypeId), arena allocator, side tables (dense and sparse), byte-offset spans | `crates/tsr-core/src/{arena,index,side_table,span}.rs` |
| **tsr-ast** | Generated node definitions, syntax kinds (351 variants), type unions, visitor pattern. Schema derived from `vendor/typescript-go/_scripts/ast.json` | `crates/tsr-ast/src/generated/{kind,nodes,alias,visit}.rs` |
| **tsr-diagnostics** | Generated diagnostic message catalogue (2,000+ messages) keyed by TypeScript error codes, paired with categories and placeholders | `crates/tsr-diagnostics/src/generated/messages.rs` |
| **tsr-scanner** | Lexical analysis: tokenization, JSDoc scanning, Unicode handling, keyword detection. Handles context-dependent scanning (regex detection, JSX modes, template continuations) | `crates/tsr-scanner/src/{lib,jsdoc,token}.rs` + `generated/` |
| **tsr-parser** | Syntax analysis: recursive descent parser building arena-allocated AST, error recovery, backtracking, node registration | `crates/tsr-parser/src/{parser,declaration,statement,expression,types,jsx,jsdoc,parsed_file}.rs` |
| **tsr-conformance** | Test harness comparing parser output against 49,354 TypeScript baseline files; measures pass-rate per suite; generates committed snapshots | `crates/tsr-conformance/src/{suite,case,corpus,snapshot}.rs` |
| **xtask** | Build-time code generation: AST nodes from `ast.json`, diagnostic catalogue, Unicode tables, manifest for conformance tests | `xtask/src/{gen_nodes,gen_kind,gen_diagnostics,gen_unicode}.rs` |

## Pattern Overview

**Overall:** Arena-allocated syntax tree with side tables; generated code from upstream schemas; parallel per-file; conformance-gated phases.

**Key Characteristics:**
- **No Back-Edges in AST**: Node structs hold only direct references to children. Parent, symbol, scope, type, flow-node relationships are separate id-keyed side tables (dense struct-of-arrays or sparse paged tables).
- **Typed Indices**: All cross-references (NodeId, SymbolId, TypeId) are newtype indices backed by `NonMaxU32`, making `Option<Id>` 4 bytes via niche optimization, not 8.
- **Generated from Schemas**: AST node definitions, diagnostic codes, and scanner Unicode tables are code-generated from upstream TypeScript Go sources, making conformance a property of the build.
- **Error Recovery**: Parser never fails; missing nodes are synthesized so the tree remains walkable for language service features (completions, goto-def) on incomplete code.
- **Arena Allocation**: Nodes and tokens are bump-allocated; the arena is `Send` (each file gets its own), not `Sync` (allocation mutates the bump pointer).
- **Self-Referential Storage**: Long-lived parsed files (needed for LSP) bundle the arena, source text, and AST using `self_cell` with one audited `unsafe impl Send`.

## Layers

**Core Layer: tsr-core**
- Purpose: Foundational types that every other crate depends on
- Location: `crates/tsr-core/src/`
- Contains: Typed index definitions via `define_index!` macro, arena allocator trait, side-table macros (`side_tables!` and `PagedTable`), span representation
- Depends on: `allocator-api2`, `hashbrown`, `index_vec`, `nonmax`, `smallvec`
- Used by: Every other crate in the compiler

**AST Layer: tsr-ast + tsr-diagnostics**
- Purpose: Schema-defined node types, syntax kinds, diagnostic codes
- Location: `crates/tsr-ast/src/generated/`, `crates/tsr-diagnostics/src/generated/`
- Contains: Node struct definitions (351 syntax kinds, 192 node types), visitor implementations, diagnostic message definitions (2,000+ codes)
- Depends on: tsr-core, `bitflags`, `serde`
- Used by: Scanner, parser, binder, checker, LSP; also consumed by `xtask` for conformance testing

**Lexical Analysis: tsr-scanner**
- Purpose: Convert source text to tokens; handle context-sensitive lexing (regex detection, JSX modes, template continuations)
- Location: `crates/tsr-scanner/src/`
- Contains: Pull-based scanner cursor, token definition, JSDoc pre-scanning, Unicode identifier tables (generated), keyword detection
- Depends on: tsr-core, tsr-ast, `memchr`, `simdutf8`, `unicode-id-start`, `phf`
- Used by: Parser, conformance harness

**Syntax Analysis: tsr-parser**
- Purpose: Build abstract syntax tree using recursive descent over scanner output
- Location: `crates/tsr-parser/src/`
- Contains: Parser main state machine, statement/declaration/expression/type parsing, JSX parsing, error recovery, node finishing (id + span registration), backtracking (`try_parse`), arrow-function lookahead
- Depends on: tsr-core, tsr-ast, tsr-scanner, `smallvec`
- Used by: Conformance harness, binder (Phase 2), checker (Phase 4)
- **Status**: Not yet in workspace due to compilation issues (approximately 40 constructor signature reconciliations needed)

**Conformance Testing: tsr-conformance**
- Purpose: Measure parser pass-rate against TypeScript's 49,354 baseline files
- Location: `crates/tsr-conformance/src/`
- Contains: Test case/corpus loading, baseline index, snapshot generation and comparison, per-suite pass-rate calculation
- Depends on: tsr-core, tsr-ast, tsr-scanner, `insta`, `similar`, `serde_json`
- Used by: CI gates, developer measurement

**Code Generation: xtask**
- Purpose: Generate AST, diagnostics, and Unicode tables from upstream sources
- Location: `xtask/src/`
- Contains: AST JSON schema parser (`AstDefinition`), node generator (`gen_nodes`, handles nullability), kind generator, diagnostic catalogue generator, Unicode identifier generator
- Depends on: `serde_json`, `syn`, `quote`, `proc-macro2`, `convert_case`, `prettyplease`
- Inputs: `vendor/typescript-go/_scripts/ast.json`, `vendor/typescript-go/internal/ast/ast_generated.go` (nullability source), TypeScript `diagnosticMessages.json`, Unicode tables
- Output: `crates/tsr-ast/src/generated/`, `crates/tsr-diagnostics/src/generated/`, `crates/tsr-scanner/src/generated/`

## Data Flow

### Primary Parsing Path

1. **Source Text Input** → `Scanner` reads UTF-8 bytes as a cursor
   - Location: `tsr-scanner/src/lib.rs:Scanner::next`
   - Each call returns one `Token` with kind, span, leading/trailing trivia
   - Token spans are byte-offsets (e.g., `é` is 2 bytes)

2. **Token Pull** → `Parser` calls `Scanner::next` as needed
   - Location: `tsr-parser/src/parser.rs:Parser::next_token`
   - Parser holds one token of lookahead
   - May request rescan (regex detection via `Scanner::rescan_as_regex`, JSX attribute values via `Scanner::rescan_jsx_attribute_value`, template continuations via `Scanner::rescan_template_continuation`)

3. **Parse Tree Construction** → Recursive descent over scanner
   - Location: `tsr-parser/src/parser.rs` (top-level dispatcher), then:
     - `tsr-parser/src/statement.rs` for statements
     - `tsr-parser/src/expression.rs` for expressions (binary operator precedence climbing)
     - `tsr-parser/src/types.rs` for type syntax
     - `tsr-parser/src/declaration.rs` for declarations
     - `tsr-parser/src/jsx.rs` for JSX elements
   - Each parser function allocates nodes into the arena (borrowed from `tsr-core::Arena`)
   - Backtracking via `Parser::try_parse` saves/restores scanner position and discards diagnostics from abandoned paths

4. **Node Registration** → `Parser::finish_node` assigns NodeId and span
   - Location: `tsr-parser/src/parser.rs:Parser::finish_node`
   - NodeId is registered in `NodeTable` (side table tracking kind and span)
   - NodeId is stamped into the node's `Cell<NodeId>` field (allows binder to set ids without `&mut` on the tree)

5. **Return ParsedFile** → Self-referential container bundling arena + source + AST
   - Location: `tsr-parser/src/parsed_file.rs:ParsedFile`
   - Uses `self_cell` to ensure arena and AST stay together
   - Sendable across thread boundaries (audited `unsafe impl Send`)

### Conformance Measurement

1. Load test corpus (`.ts`/`.tsx` files from `vendor/typescript-go/_submodules/TypeScript/tests/`)
   - Location: `tsr-conformance/src/corpus.rs:Corpus::load`

2. Load baseline index (mapping case names to expected outputs)
   - Location: `tsr-conformance/src/snapshot.rs:BaselineIndex`
   - Handles configuration variants (e.g., `case(target=es5).errors.txt`)

3. For each test case:
   - Parse file via `tsr-parser`
   - Collect diagnostics (scanner + parser diagnostics only; type diagnostics are Phase 4+)
   - Compare against baseline `.errors.txt` (or expect clean output if no baseline exists)

4. Accumulate pass/fail per suite, write snapshot
   - Location: `tsr-conformance/src/snapshot.rs:Snapshot`
   - Committed snapshots enable baseline ratchet (pass-rate can only go up)

**State Management:**
- **Parser state**: Held in `Parser` struct; scanner position, token, node-id counter
- **Node identity**: Registered in `NodeTable` side table; keyed by `NodeId`
- **Symbol/Type state**: Empty until Phase 2 (binder) and Phase 4 (checker)
- **Diagnostic state**: Accumulated in `Vec<Diagnostic>` throughout parsing; discarded on backtrack

## Key Abstractions

**Typed Index**
- Purpose: Replace Go pointers with type-safe integer ids that cannot be confused with other ids (NodeId ≠ SymbolId at compile time)
- Examples: `NodeId`, `SymbolId`, `TypeId` defined via `define_index!` macro in each crate
- Pattern: `NonMaxU32` via `nonmax` crate; `Option<Id>` is 4 bytes (niche optimization)

**Side Table**
- Purpose: Store properties keyed by node/symbol/type id without adding fields to the node structs
- Examples: `NodeTable` (kind, span), parent table (parent id), flags table, checker link stores
- Pattern: Two shapes—dense struct-of-arrays (via `side_tables!` macro) for data every node has, sparse paged tables (via `PagedTable`) for sparse data

**Arena**
- Purpose: Bulk-allocate all nodes for one source file
- Examples: `tsr_core::Arena` (stores individual allocations), reset and reuse across files
- Pattern: Bump allocator; each file gets its own to avoid contention and enable parallel parsing

**Self-Referential Cell**
- Purpose: Bundle arena, source text, and AST lifetime without Rust rejecting the self-reference
- Examples: `ParsedFile` holds `(Arena, String, Program<'a>)` as a unit
- Pattern: `self_cell` crate; one audited `unsafe impl Send` per cell type

**Visitor**
- Purpose: Traverse the AST tree without explicit recursion management
- Examples: `Visit` trait generated from AST schema; one `visit_*` method per node type
- Pattern: Each method defaults to calling matching `walk_*` free function; override to customize; call or omit the walk to traverse children

## Entry Points

**Parser Entry Point**
- Location: `tsr-parser/src/parser.rs:Parser::parse_source_file`
- Triggers: Called by conformance harness (`tsr-conformance::run_test`) or future compiler driver
- Responsibilities: Initialize scanner and parser, invoke top-level statement list parser, return `(ParsedFile, Vec<Diagnostic>)`

**Conformance Entry Point**
- Location: `tsr-conformance/src/main.rs` (coverage binary)
- Triggers: `cargo run -p tsr-conformance --bin coverage`
- Responsibilities: Load corpus and baselines, run parser suite, generate snapshots, print pass-rate summary

**Codegen Entry Point**
- Location: `xtask/src/main.rs`
- Triggers: `cargo xtask codegen`
- Responsibilities: Parse `ast.json` and upstream Go source, generate node definitions, diagnostics, Unicode tables, write `crates/tsr-ast/src/generated/`

## Architectural Constraints

- **Threading**: Files are independent through parsing; each gets its own arena. `par_iter` over files is the unit of parallelism. Arena is `Send`, deliberately not `Sync` (allocation mutates bump pointer). Long-lived ASTs are stored via `self_cell` with audited `unsafe impl Send`.
- **Global state**: Keyword lookup is a perfect hash function (phf) computed from `SyntaxKind` enum (no hand-maintained table). Unicode identifier tables are generated. No module-level mutable state in parser.
- **Circular imports**: Acyclic crate dependency graph: tsr-core → tsr-ast/tsr-diagnostics → tsr-scanner → tsr-parser → tsr-conformance. No cycles.
- **Backtracking scope**: `Scanner::save()` / `Scanner::restore()` work together; parser discards diagnostics and node registrations from abandoned paths to avoid phantom errors.
- **Node nullability**: AST node fields that upstream marks as nullable (`Option<T>`) are derived by parsing Go source (`internal/ast/ast_generated.go`), not from `ast.json`. This corrects upstream's `ast.json` incompleteness.

## Anti-Patterns

### Pointer-Based Cycles in the AST

**What happens**: Creating fields like `parent: &'a Node` or `symbol: Option<SymbolId>` directly on nodes creates cyclic references (parent → child → parent).
**Why it's wrong**: Makes the tree both complex to traverse and impossible to parallelize without mutex. Breaks the ability to move the AST between threads.
**Do this instead**: Store all cyclic relationships in id-keyed side tables. See `tsr-core/src/side_table.rs` for the `side_tables!` and `PagedTable` macros. Parent is a column in `NodeTable`, not a field.

### Materialised Token Vector

**What happens**: Lexing entire source into `Vec<Token>` before parsing, then parsing from the vector.
**Why it's wrong**: TypeScript requires context-dependent re-scanning (regex detection, JSX modes, template continuations). A materialised vector cannot re-tokenize the same position.
**Do this instead**: Use a pull-based scanner that produces one token at a time. See `tsr-scanner/src/lib.rs:Scanner`. The parser calls `next_token()` and requests rescans via `Scanner::rescan_as_regex()` etc.

### Exponential Backtracking in Arrow Function Detection

**What happens**: Trying to parse a full parameter list with expressions (`(a) => a` vs. `(a)`) re-enters the expression parser for each parameter, which speculates again. On `((...))` nested assignments, this becomes 2ⁿ.
**Why it's wrong**: Real files (e.g., `compiler/parsingDeepParenthensizedExpression.ts`, 9 KB) trigger this and consume 16 GB of memory.
**Do this instead**: Walk only tokens to find the matching `)`, then check if `=>` follows. See `tsr-parser/src/parser.rs:Parser::is_arrow_function_ahead`. TypeScript does exactly this.

### Unified Shift Token Handling

**What happens**: Treating `>>` and `>>>` as single tokens when closing type arguments (`List<List<T>>`), causing bracket counting to fail.
**Why it's wrong**: `<` and `>` are both brackets and comparison operators depending on context. Counting misses `<K extends Key<U>>` (one `<<` vs. four `<`).
**Do this instead**: Split shift tokens at use site. See `tsr-parser/src/parser.rs:Parser::greater_than_count` and `Parser::rescan_less_than`.

## Error Handling

**Strategy**: Parser never fails. Every entry point returns tree + diagnostics. Missing nodes are synthesised so the tree stays walkable. This is load-bearing for language service features (completions, goto-def) on incomplete code.

**Patterns**:
- Diagnostic emission: `Parser::report_error`, `Parser::report_unsupported` methods append to `parser.diagnostics` vec
- Error recovery: Missing required nodes are synthesised (e.g., synthesised identifiers, empty statements); termination is enforced by comparing scanner position before/after each statement (a statement that consumes nothing is discarded)
- Depth guard: `MAX_DEPTH` (512) prevents stack overflow on pathological nesting; exceeded depth becomes a diagnostic
- Backtracking cleanup: `Scanner::restore` discards diagnostics from abandoned speculative parse paths

## Cross-Cutting Concerns

**Logging**: No structured logging framework yet (Phase 0 baseline). Diagnostics are the primary error signal. Real-time instrumentation is via `eprintln!` at development time; conformance harness uses snapshot comparison.

**Validation**: Conformance validation is external (via `tsr-conformance`); no in-tree semantic validation until checker is built (Phase 4).

**Upstream Anchoring**: Every ported item names its TypeScript Go counterpart in a doc comment. Example: `//! Corresponds to typescript-go's internal/parser/parser.go`. This is enforced by linting (planned). See `docs/adr/0001-idiomatic-rewrite.md`.

---

*Architecture analysis: 2026-08-03*
