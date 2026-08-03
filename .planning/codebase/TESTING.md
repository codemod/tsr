# Testing Patterns

**Analysis Date:** 2026-08-03

## Test Framework

**Runner:**
- Rust built-in test framework with `cargo test`
- No external test harness

**Assertion Library:**
- `assert!`, `assert_eq!`, `assert_ne!` macros (standard library)
- Optional: `insta` for snapshot testing (in workspace dependencies via `Cargo.toml`)

**Run Commands:**
```bash
cargo test --workspace                    # Run all tests across all crates
cargo test --lib                          # Unit tests only (inline + lib tests dir)
cargo test --test "*"                     # Integration tests only (tests/ dir)
cargo test --package <crate>              # Test specific crate
cargo clippy --workspace --all-targets -- -D warnings  # Lint check (part of quality gates)
cargo fmt --all --check                   # Format verification
```

## Test File Organization

**Location Patterns:**

**Inline Tests (Unit):**
- Located in the module being tested
- Wrapped in `#[cfg(test)]` block at module end
- Path: any `.rs` file in `crates/*/src/`

**Integration Tests:**
- Located in `tests/` directory at crate root
- Each test file is compiled as a separate binary
- Path: `crates/*/tests/*.rs`

**Conformance Tests:**
- Special integration tests that verify against vendored upstream
- Gracefully skip when upstream (`vendor/typescript-go`) is not checked out
- Path: `crates/tsr-ast/tests/`, `crates/tsr-diagnostics/tests/`, `crates/tsr-parser/tests/`

**Naming:**
- Test files: `<component>.rs` (e.g., `scan.rs`, `parse.rs`, `visit.rs`)
- Test functions: `#[test] fn <behavior_description>()`
- Helper functions: descriptive snake_case, no `test_` prefix

## Test Structure

**Unit Test Module Layout:**
```rust
// At end of src/lib.rs or any src/*.rs file
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_describes_behavior() {
        // Arrange
        let input = ...;

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

**Integration Test Layout:**
```rust
// In tests/*.rs
use tsr_scanner::{Scanner, tokenize};
use tsr_ast::SyntaxKind;

#[test]
fn behavior_description() {
    // Arrange / Act / Assert
}
```

**Example from `crates/tsr-scanner/tests/scan.rs`:**
```rust
#[test]
fn empty_source_yields_only_end_of_file() {
    let (tokens, diagnostics) = tokenize("");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, EndOfFile);
    assert!(diagnostics.is_empty());
}

#[test]
fn identifiers_and_keywords_are_distinguished() {
    assert_eq!(kinds("foo if bar"), vec![Identifier, IfKeyword, Identifier]);
    assert_eq!(kinds("$_ _x $1"), vec![Identifier, Identifier, Identifier]);
}
```

## Patterns

**Setup Pattern:**
- Minimal inline construction (no factories or fixtures directories)
- Build small trees/structures by hand in the test
- Example from `crates/tsr-ast/tests/visit.rs`:
  ```rust
  let a = Identifier::new("a");
  let b = Identifier::new("b");
  let plus = Token::new(SyntaxKind::PlusToken);
  let inner = BinaryExpression::new(&[], Some(Expression::Identifier(&a)), None, Some(&plus), Some(Expression::Identifier(&b)));
  ```

**Teardown Pattern:**
- Not used; tests are isolated and self-contained
- No cleanup needed (arena is stack-allocated, dropped at test end)

**Assertion Pattern:**
- Use `assert_eq!(actual, expected)` for equality
- Use `assert!(condition, "message")` for boolean conditions
- Provide context in assertion messages when non-obvious
- Example: `assert!(!codes.is_empty(), "0x should report a missing digit")`

**Helper Functions:**
- Keep helpers in the same test module to reduce boilerplate
- Example from `crates/tsr-scanner/tests/scan.rs`:
  ```rust
  fn kinds(source: &str) -> Vec<SyntaxKind> {
      let (tokens, _) = tokenize(source);
      tokens[..tokens.len() - 1].iter().map(|t| t.kind).collect()
  }

  fn scan_with_errors(source: &str) -> (Vec<SyntaxKind>, Vec<u32>) {
      let (tokens, diagnostics) = tokenize(source);
      (
          tokens[..tokens.len() - 1].iter().map(|t| t.kind).collect(),
          diagnostics.iter().map(|d| d.message.code()).collect(),
      )
  }
  ```

## Mocking

**Framework:**
- No external mocking library; implement mock types by hand
- Use trait implementations to create mock objects

**Pattern:**
- Define a minimal struct that implements the trait you need to test
- Example from `crates/tsr-ast/tests/visit.rs`:
  ```rust
  #[derive(Default)]
  struct Recorder {
      identifiers: Vec<String>,
      binary_expressions: usize,
  }

  impl<'a> Visit<'a> for Recorder {
      fn visit_identifier(&mut self, node: &'a Identifier<'a>) {
          self.identifiers.push(node.text.to_string());
      }

      fn visit_binary_expression(&mut self, node: &'a BinaryExpression<'a>) {
          self.binary_expressions += 1;
          tsr_ast::visit::walk_binary_expression(self, node);
      }
  }
  ```

**What to Mock:**
- Visitor implementations (to verify tree traversal)
- Custom data collectors for checking complex behavior

**What NOT to Mock:**
- Parser, scanner, diagnostics — test against real implementations
- Internal details; test through public API

## Fixtures and Factories

**Test Data Construction:**
- Build fixtures inline; no separate factory functions or `TestFixtures` modules
- Use constructor methods on types
- Example: `Identifier::new("x")`, `Token::new(SyntaxKind::PlusToken)`

**Scope:**
- Fixtures are local to test functions (stack-allocated)
- No reusable fixture sets or shared test data

**Arena Usage:**
- Each test that needs an arena creates one: `let arena = Arena::new();`
- Stack-allocated; cleaned up when test ends
- Lifetime coupled to arena (references live as long as arena)
- Example from `crates/tsr-parser/tests/parse.rs`:
  ```rust
  #[test]
  fn empty_source_parses_to_an_empty_file() {
      let arena = Arena::new();
      assert!(statements(&arena, "").is_empty());
  }
  ```

## Coverage

**Requirements:**
- No enforced coverage target mentioned
- No code coverage measurement in CI configuration observed

**Guidance:**
- Unit tests for core algorithms (scanner, diagnostics, visitor)
- Integration tests for end-to-end flows (parse → diagnostics)
- Conformance tests for upstream alignment

## Test Types

**Unit Tests:**
- Scope: Individual functions and small modules
- Approach: Inline in modules using `#[cfg(test)]` blocks
- Frequency: Most tests are unit tests
- Location: `crates/tsr-core/src/`, `crates/tsr-ast/src/`, `crates/tsr-diagnostics/src/`
- Example: `span.rs` tests span creation, containment, and coverage

**Integration Tests:**
- Scope: Cross-module behavior, scanner → parser → AST
- Approach: Separate test files in `tests/` directories
- Frequency: Comprehensive integration suites for major components
- Location: `crates/tsr-scanner/tests/`, `crates/tsr-parser/tests/`
- Example: `crates/tsr-parser/tests/parse.rs` tests precedence, associativity, error recovery

**Conformance Tests:**
- Scope: Verify AST, diagnostics, and kinds match upstream typescript-go
- Approach: Parse upstream Go source and compare against generated output
- Special: Skip gracefully if submodule not checked out; never fail silently
- Location: `crates/tsr-ast/tests/kind_conformance.rs`, `crates/tsr-diagnostics/tests/message_conformance.rs`
- Example: `crates/tsr-ast/tests/kind_conformance.rs` compares `SyntaxKind` enum against Go's `kind_generated.go`

## Conformance Testing Pattern

**Oracle:**
- Test against **generated Go source**, not against `ast.json` or JSON exports
- Reason: JSON and Go source can drift; we assert against what the Go compiler actually uses
- See `docs/adr/0006-conformance-oracle.md`

**Skip Strategy:**
- When `vendor/typescript-go` is not checked out, tests skip gracefully
- Do NOT fail; do NOT skip silently (make skip message visible)
- Example from `crates/tsr-diagnostics/tests/message_conformance.rs`:
  ```rust
  static GO_SOURCE: LazyLock<Option<String>> = LazyLock::new(|| {
      let path = repo_root().join("vendor/typescript-go/internal/diagnostics/diagnostics_generated.go");
      std::fs::read_to_string(path).ok()
  });

  #[test]
  fn every_upstream_message_exists_here_with_matching_fields() {
      let Some(source) = GO_SOURCE.as_ref() else {
          eprintln!("skipping: vendor/typescript-go not checked out");
          return;
      };
      // Test continues with parsed Go source
  }
  ```

**Detail Reporting:**
- Collect all mismatches before asserting, report all together
- Example from `crates/tsr-diagnostics/tests/message_conformance.rs`:
  ```rust
  let mut problems = Vec::new();
  for expected in &go {
      // Check all fields, push problems
  }
  assert!(problems.is_empty(), "{} message(s) disagree:\n  {}", 
          problems.len(), problems.join("\n  "));
  ```

## Common Patterns

**Async Testing:**
- Not used; codebase is synchronous

**Error Testing:**
- Parse/scan always succeed; test that bad input produces diagnostics with expected codes
- Example from `crates/tsr-scanner/tests/scan.rs`:
  ```rust
  #[test]
  fn malformed_numbers_report_diagnostics_but_still_produce_a_token() {
      let (kinds, codes) = scan_with_errors("0x");
      assert_eq!(kinds, vec![NumericLiteral]);
      assert!(!codes.is_empty(), "0x should report a missing digit");
  }
  ```

**Tree Traversal Testing:**
- Build small tree by hand, walk it, verify visitor reaches every node
- Example from `crates/tsr-ast/tests/visit.rs`:
  ```rust
  #[test]
  fn walk_reaches_nested_children() {
      let a = Identifier::new("a");
      let b = Identifier::new("b");
      let c = Identifier::new("c");
      let plus = Token::new(SyntaxKind::PlusToken);

      let inner = BinaryExpression::new(&[], Some(Expression::Identifier(&a)), None, Some(&plus), Some(Expression::Identifier(&b)));
      let outer = BinaryExpression::new(&[], Some(Expression::BinaryExpression(&inner)), None, Some(&plus), Some(Expression::Identifier(&c)));

      let mut recorder = Recorder::default();
      walk_node(&mut recorder, Node::BinaryExpression(&outer));

      assert_eq!(recorder.binary_expressions, 2, "should reach both binary expressions");
      assert_eq!(recorder.identifiers, vec!["a", "b", "c"], "should reach every identifier, in source order");
  }
  ```

**Parsing Error Recovery:**
- Verify that bad input produces a tree with diagnostics
- Do not test only the diagnostics; verify the tree is still walkable
- Example philosophy: a parser that reports an error AND leaves the tree corrupted has not recovered

**Snapshot Testing:**
- Optional: `insta` crate available in workspace dependencies
- Not observed in current test suite (all assertions are inline)
- Would be appropriate for baseline comparisons (e.g., diagnostic output)

## Build & Quality Gates

**Required Tests:**
```bash
cargo test --workspace              # Must pass
cargo clippy --workspace --all-targets -- -D warnings  # Must pass
cargo fmt --all --check             # Must pass
```

**Codegen Verification:**
```bash
cargo xtask codegen && cargo fmt --all
# Then verify no uncommitted changes to generated/ directories
```

**Conformance Runs:**
```bash
cargo run -p tsr-conformance --bin coverage   # Snapshot generator, not part of test suite
```

---

*Testing analysis: 2026-08-03*
