# Coding Conventions

**Analysis Date:** 2026-08-03

## Naming Patterns

**Files:**
- `snake_case` with `.rs` extension
- Example: `scanner.rs`, `parser.rs`, `side_table.rs`, `jsdoc.rs`

**Functions:**
- `snake_case` for all function names
- Example: `is_identifier_start`, `parse_source_file`, `take_diagnostics`

**Structs, Enums, Traits:**
- `PascalCase` for all type definitions
- Example: `Scanner`, `Parser`, `SourceFile`, `NodeTable`, `Diagnostic`

**Constants:**
- `UPPER_CASE` for module-level constants
- Example: `UNDEFINED`, `ZERO`, `HEX_SPECIFIER`

**Generic Parameters:**
- Single uppercase letters: `T`, `I` (common: `T` for type, `I` for index)
- Example: `PagedTable<I: Idx, T>`, `IndexVec<I, T>`

**Private/Local Variables:**
- `snake_case`, same as functions
- Example: `start`, `end`, `rest`

## Code Style

**Formatting:**
- Formatter: `rustfmt` with configuration in `rustfmt.toml`
- Edition: Rust 2024
- Line width: 100 characters maximum
- Heuristics: `use_small_heuristics = "Max"`

**Linting:**
- Linter: `clippy` with configuration in `clippy.toml`
- Warn-level lints enabled: `all`, `pedantic`
- Deny-level: `unsafe_op_in_unsafe_fn`
- Warn-level: `missing_docs` (documentation required for public items)

**Generated Code Exceptions:**
- The following clippy warnings are allowed for generated code under `crates/tsr-ast/src/generated/`:
  - `module_name_repetitions` - Generated AST code repeats names by design
  - `too_many_lines` - Generated code can be large and repetitive
  - `wildcard_imports` - Generated code uses glob imports
  - `enum_glob_use` - Generated enum imports are deliberate
  - `missing_errors_doc` - Auto-generated error types exempt
  - `missing_panics_doc` - Auto-generated panic documentation exempt

## Import Organization

**Order:**
1. Standard library (`std::*`)
2. External crates (dependency imports)
3. Internal crate imports (relative `use` statements)
4. Internal modules (`mod` declarations)

**Path Aliases:**
- Re-export commonly used types in `lib.rs` via `pub use`
- Example: `pub use generated::visit::Visit` at module level

**Wildcard Imports:**
- Allowed only in generated code and test modules
- Discouraged in hand-written source

## Documentation

**Rustdoc Comments:**
- Use `///` for items (not `//`)
- Use `//!` for module-level documentation
- All public items require documentation
- Format: sentence starting with verb or noun, ending with period

**Example:**
```rust
/// Whether a code point may begin an identifier.
///
/// `$` and `_` are permitted by ECMAScript in addition to `ID_Start`.
#[must_use]
pub fn is_identifier_start(cp: char) -> bool {
```

**Upstream Anchoring:**
- Every ported item must include a doc comment naming its typescript-go counterpart
- Format: `/// Ported from typescript-go's \`<module>.<Name>\` (\`<path>\`).`
- Example: `/// Ported from typescript-go's `core.PagedLinkStore` (`internal/core/linkstore.go`).`
- Location: `crates/tsr-core/src/side_table.rs`, `crates/tsr-scanner/src/lib.rs`

**Module-Level Documentation:**
- Every crate and public module must have top-level doc comment with `//!`
- Should explain the module's purpose, shape, and key abstractions
- Should link to related documentation in `docs/`
- Example: `crates/tsr-scanner/src/lib.rs`, `crates/tsr-core/src/lib.rs`

**Code Comments:**
- Use `//` for inline comments explaining non-obvious logic
- Prefer measured facts over assertions: "upstream reads `.Parent` 2,092 times" over "parent access is hot"
- Cite file paths and line numbers when referring to specific code
- Document the *why*, not just the *what*

## Visibility & Exports

**Visibility Modifiers:**
- `pub` for public API surface
- No visibility modifier for private items (default is private)
- `pub(crate)` for internal shared utilities

**Re-exports:**
- Use `pub use` in `lib.rs` to expose the public API
- Re-export commonly used types to reduce import friction
- Example in `crates/tsr-ast/src/lib.rs`:
  ```rust
  pub use generated::{alias::*, kind::SyntaxKind, nodes::*, visit};
  ```

## Attributes & Compiler Hints

**`#[must_use]`:**
- Applied to functions/methods that return values the caller should not ignore
- Especially important for pure functions and fallible operations
- Example: `#[must_use] pub const fn is_defined(self) -> bool`

**`#[inline]`:**
- Applied to performance-critical functions that should be inlined
- Common on small utility functions and const functions
- Example: `#[inline] pub fn push(&mut self, value: T) -> I`

**`#[repr(transparent)]`:**
- Used on newtype indices to ensure no memory overhead
- Example: `#[repr(transparent)] pub struct NodeId(...)`

**`#[repr(u8)]`:**
- Used on enums that need a fixed discriminant size
- Example: `#[repr(u8)] pub enum Category { Warning = 0, Error = 1, ... }`

**`#[derive(...)]`:**
- Standard derives: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`
- Use `Copy` only when all fields are `Copy` (avoid copies of expensive allocations)
- Uncommon derives (e.g., `Serialize`) only when explicitly needed

**`#[allow(dead_code)]`:**
- Macros emit complete accessor sets; consumers use subsets
- Suppress warning rather than remove unused accessors
- Example: `side_tables!`, `define_index!`

## Error Handling

**Diagnostics Pattern:**
- Parse/scan never fails; they always return a tree and a diagnostic list
- Errors are collected as `Diagnostic` values during scanning/parsing
- Synthesized nodes are inserted to keep the tree walkable even with errors
- Error recovery is mandatory: bad input should not prevent tree construction

**Failing Loudly:**
- Codegen fails on unrecognized upstream constructs rather than silently degrading
- Use `bail!` (from `anyhow`) on invariant violations in codegen
- Do not silently default to permissive behavior when specificity is required

**Result Types:**
- Functions that can fail use `Result<T, E>` 
- Codegen uses `Result<T, anyhow::Error>`
- Parsing/scanning use inline error accumulation (no `Result` type)

**Panic Safety:**
- `#[must_use]` on const functions that panic on overflow (e.g., `NodeId::new(u32::MAX)`)
- Out-of-bounds accesses in side tables are checked; return `None` rather than panicking
- Example: `Span::text()` returns `Option<&str>` for invalid spans

## Macros

**Declarative Macros for Structural Repetition:**
- `define_index!` - Define newtype indices with all standard methods
- `side_tables!` - Define dense struct-of-arrays side tables
- Located in `crates/tsr-core/` for cross-crate use

**Codegen via `xtask`:**
- `cargo xtask codegen` regenerates AST from typescript-go's `ast.json`
- Generated code is checked in to `.gitignore`-free repository (no exclusions)
- CI verifies generated code has not drifted: regenerates and diffs

**Macro Limitations:**
- Avoid identifier concatenation; spell accessor names fully
- Example: `define_index!` emits `as_u32`, not `as_{name}`; this is documented
- Use `#[allow(dead_code)]` on emitted complete accessor sets

## Generated Code

**Location:**
- `crates/tsr-ast/src/generated/` - AST nodes, syntax kinds, visitor trait
- `crates/tsr-scanner/src/generated/` - Unicode property tables
- `crates/tsr-diagnostics/src/generated/` - Diagnostic message catalogue

**Policy:**
- Generated code is **never hand-edited**
- Changes to generation logic go into `xtask/` Rust code
- Regeneration: `cargo xtask codegen && cargo fmt --all`
- CI diffs regenerated output against checked-in version

**Schema Source:**
- AST: `vendor/typescript-go/_scripts/ast.json` (same schema as Go compiler)
- Unicode: Computed from Unicode property database
- Diagnostics: `vendor/typescript-go/internal/diagnostics/diagnosticMessages.json`

## Data Structure Patterns

**The Tree + Side Tables Pattern:**
- Node structs hold only syntax children (arena references, not pointers)
- Everything cyclic (parent, symbol, scope, type, flow node) lives in id-keyed side tables
- See `docs/adr/0003-tree-plus-side-tables.md` and `PLAN.md §3.2`
- Locations: `crates/tsr-ast/src/lib.rs`, `crates/tsr-core/src/side_table.rs`

**Newtype Indices:**
- All cross-references use typed newtype indices, not pointers
- Backed by `nonmax::NonMaxU32` so `Option<Id>` is 4 bytes (niche optimization)
- Enables cyclic structures without `Rc<RefCell<_>>`; makes arenas `Send`
- Defined via `define_index!` macro in `crates/tsr-core/src/index.rs`

**Arenas:**
- Single-threaded lifetime-coupled allocation for AST nodes
- `tsr_core::Arena` in `crates/tsr-core/src/arena.rs`
- Parser allocates all nodes into one arena per file

## Type Annotations

**Function Signatures:**
- Explicit parameter and return types always (no type inference in signatures)
- Lifetimes explicit when references cross function boundaries
- Example: `pub fn parse<'a>(arena: &'a Arena, source: &'a str) -> ParsedSourceFile<'a>`

**Const Context:**
- Const functions marked `#[must_use]` when they could silently fail
- Const constructors and pure utility functions are common
- Example: `pub const fn new(start: u32, end: u32) -> Self`

---

*Convention analysis: 2026-08-03*
