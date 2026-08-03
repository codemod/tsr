# Codebase Concerns

**Analysis Date:** 2026-08-03

## Tech Debt

**Parser conformance gaps:**
- Issue: 32 test cases still fail out of 5,031 clean cases (99.36% pass rate)
- Files: `crates/tsr-parser/` (primary), snapshot in `crates/tsr-conformance/snapshots/parser_typescript.snap`
- Impact: Each failure represents a specific syntax pattern not yet handled; gaps compound during checker phase when semantic context is needed
- Fix approach: Categorize failures by type using `cargo run -p tsr-conformance --example failure_classes`, then address by category. Most are edge cases in private names, import attributes, and ASI within type members.

**JSDoc eager parsing overhead:**
- Issue: JSDoc is parsed during main parse rather than lazily (like upstream does)
- Files: `crates/tsr-parser/src/jsdoc.rs`, `crates/tsr-scanner/src/jsdoc.rs`, decision documented in `docs/adr/0008-jsdoc-parsed-eagerly.md`
- Impact: 7.5% parse-time overhead on realistic TypeScript (~755 JSDoc nodes in 4.5 MB), up to 64% on JSDoc-dense files. Memory holds every parsed comment for file lifetime whether accessed or not.
- Cause: `ParsedFile` uses `self_cell` to bundle arena + source + tree, making post-construction allocation impossible. Lazy parsing would require either a second arena per file or breaking the `Send` guarantee.
- Improvement path: Either (a) add allocate-post-construction capability to `ParsedFile` (requires redesign of `self_cell` usage), or (b) accept the overhead as payment for the threading model. Current cost is measured and acceptable on normal input; worth re-measuring if LSP incremental reparse becomes a bottleneck.

**ASI (Automatic Semicolon Insertion) inside type members:**
- Issue: `a?: number` followed by `extends?: string` on next line reads `extends` as start of conditional type
- Files: `crates/tsr-parser/src/types.rs`
- Impact: Causes parse errors on valid TypeScript; affects ~1-2 of the 32 failing cases
- Fix approach: Implement type-member-context ASI suppression. Upstream's logic is in `parser.go`'s `parseTypeMembers` path.

**Import types with attributes not parsed:**
- Issue: `import("pkg", { with: { type: "json" } })` syntax not supported
- Files: `crates/tsr-parser/src/types.rs` (type argument parsing)
- Impact: Newer TypeScript syntax (5.3+); affects ~1-2 of the 32 failing cases
- Fix approach: Extend type argument parser to handle attribute object syntax. Syntax is `import(specifier, { key: value, … })`.

**JSDoc reparser not implemented:**
- Issue: In `.js` files, `@type` and `@param` should be promoted into real type annotations on the tree
- Files: Not yet written; would be `crates/tsr-parser/src/jsdoc_reparser.rs`
- Impact: Language service has no type information for JavaScript; this blocks Phase 2 JSDoc semantics
- Fix approach: Implement after binder exists (Phase 2 follow-up). Upstream's 748-line `reparser.go` is the reference; the machinery belongs in the checker phase because it depends on symbol resolution.

**JSDoc-only type syntax generated but unused:**
- Issue: Nodes for `*` (JSDocAllType), `?T`, `!T`, `T=`, `...T` are generated but not built by the parser
- Files: `crates/tsr-ast/src/generated/nodes.rs` (generated), `crates/tsr-parser/src/jsdoc.rs` (not used)
- Impact: Dead code in AST; no runtime impact, but clutters introspection
- Fix approach: Either implement parsing for these (`.js` convenience syntax) or filter them from codegen. Low priority; these are `.js`-only conveniences and only matter when JSDoc reparser arrives.

---

## Known Bugs

**Arrow function lookahead exponential backtracking (FIXED but fragile):**
- Symptoms: Parser enters infinite loop and consumes 16 GB RAM
- Files: `crates/tsr-parser/src/expression.rs`, function `is_arrow_function_ahead`
- Trigger: Deep nesting of parenthesized assignments: `(E = (E = (E = …)))` repeated 69+ levels
- Example file: `compiler/parsingDeepParenthensizedExpression.ts` (9 KB, 69 levels deep)
- Root cause: Naive `try_parse` approach re-entered expression parser for each parameter, leading to 2ⁿ re-parsing. Upstream handles identically.
- Fix implemented: Token-only scan (no node allocation, no expression parser re-entry) via `is_arrow_function_ahead`. File now parses in 2.3 ms instead of hanging.
- Why mention: The lookahead is now correct but **deeply subtle**. It tracks bracket depth *and* angle brackets (`<K extends Key<U>>`), and must stop at statement boundaries (`(a); x => y`). Any change to bracket matching logic must verify against this specific file.

**Bracket matching in arrow function lookahead:**
- Symptoms: Would incorrectly match closing paren for `(): (() => T) => x` or mishandle `Iterable<number, any> => x`
- Files: `crates/tsr-parser/src/expression.rs`, function `is_arrow_function_ahead`
- Root cause: `>>` and `>>>` are single tokens, so naive bracket counter never balances `<K extends Key<U>>`. Both directions need splitting via `greater_than_count` and `rescan_less_than`.
- Fix implemented: Logic tracks both `<` and `<<` from shift tokens by splitting them internally.
- Fragility: Lookahead stops at statements, not at arbitrary positions. The test is critical; any refactoring must verify it.

**JSDoc type parser handoff off-by-one (FIXED):**
- Symptoms: JSDoc types are parsed with source spans one token off from the written type
- Files: `crates/tsr-parser/src/jsdoc.rs`, function `parse_jsdoc_type_expression`
- Trigger: `@typedef {number} Point` would have wrong span
- Root cause: `eat_jsdoc(OpenBraceToken)` consumed the brace and scanned the following token under JSDoc rules, then `next_token()` scanned a second one, entering type parser one token late
- Fix implemented: Note whether `{` is present, rewind scanner to just past it, then `next_token()` to rescan under ordinary rules
- Why mention: This bug was silent in parsing (a JSDoc type one token late is still valid), and only caught by tests asserting **source spans**. Lessons: (a) JSDoc diagnostics are discarded, so malformed comments don't fail tests; (b) span-based testing caught what tree-shape testing missed.

---

## Security Considerations

**Parser error recovery must never escape context:**
- Risk: Pathological nesting (`@@@`, deeply nested expressions) could exhaust stack if guards fail
- Files: `crates/tsr-parser/src/parser.rs`, constant `MAX_DEPTH` (512), and `parse_statement_list` loop termination
- Current mitigation: Depth guard converts 512+ levels to a diagnostic instead of stack overflow. Statement parser discards zero-width statements to prevent infinite loops. Both verified with tests.
- Recommendation: Maintain the two guards and test pathological inputs regularly. Consider adding fuzzing if not already in CI.

**Unwrap/panic in test code is acceptable; production code must not:**
- Risk: `unwrap()` calls in parsing or core library code would panic on malformed input
- Files: Grep found 40 instances; all are in test code or conformance harness where panic is acceptable
- Current mitigation: Core parsing code (`crates/tsr-parser/src/`, `crates/tsr-scanner/src/`) uses `Option`/`Result` and recovers or synthesizes nodes rather than panicking
- Recommendation: Maintain the invariant with a linter rule (consider `#![deny(unsafe_code)]` at crate level for production crates).

**The scanner limit field is not bounds-checked:**
- Risk: Setting `limit` incorrectly could cause out-of-bounds reads from `source`
- Files: `crates/tsr-scanner/src/lib.rs`, method `set_range`
- Current mitigation: Setter is only called from JSDoc parser with computed bounds; the value is private and not exposed to external callers. `rest()` and all character tests use the limit transparently (no panics even if limit > source.len()).
- Recommendation: Keep the setter private and audit its callers. If exposed, add debug-checked bounds assertions.

---

## Performance Bottlenecks

**JSDoc parsing overhead (measured):**
- Problem: Eager parsing adds 7.5% to normal TypeScript, 64% on JSDoc-dense files
- Files: `crates/tsr-parser/src/jsdoc.rs`
- Measured: `cargo run --release -p tsr-parser --example jsdoc_cost <dir>`
- Cause: Every token's flag test runs (almost always false), and the cost compounds per-file
- Improvement path: (a) Lazy parsing (blocked on `ParsedFile` lifetime issue), (b) Arena pooling (won't help JSDoc, but reduces per-file allocation cost), (c) measure on real editor workloads to validate the 7.5% is acceptable. Currently acceptable; re-measure if language service reparse becomes a bottleneck.

**No arena reuse for long-running processes:**
- Problem: Every file allocates a fresh arena and drops it; LSP holding 100+ files wastes allocator churn
- Files: `crates/tsr-core/src/arena.rs`
- Cause: Arena pool not yet implemented
- Improvement path: Implement `AllocatorPool` (oxc design), similar to `oxc_allocator::AllocatorPool`. Measure on a real LSP with incremental edits. Not urgent for single-file CLI, critical for long-running editor.

**Conformance harness runs 12,444 cases serially in snapshots:**
- Problem: Development cycle feedback is slow (25 s CPU, 4.6 s wall with rayon)
- Files: `crates/tsr-conformance/src/`, snapshot collection in `bin/coverage.rs`
- Cause: Determinism requirement forces case-order collection and tally-after-collect
- Improvement path: Parallelism is already in place (rayon over cases). Single-threaded fallback exists for verification. The 4.6 s wall time is acceptable for CI; if CI latency becomes a problem, profile to confirm rayon contention.

**Parser has no per-node limit on children:**
- Problem: Deeply nested structures could produce huge trees and slow traversal
- Files: `crates/tsr-parser/src/parser.rs`, depth guard only applies to parsing depth
- Cause: Only statement nesting is guarded (MAX_DEPTH = 512); expression nesting could be very deep
- Improvement path: The depth guard is per-statement, not per-expression, which is correct (statements nest in control flow; expressions nest in operator precedence). Expression depth is inherently limited by precedence levels (15 in TypeScript). No known pathological cases beyond the one fixed (`parsingDeepParenthensizedExpression.ts`). Monitor with fuzzing if added to CI.

---

## Fragile Areas

**Arrow function lookahead token-only scan:**
- Files: `crates/tsr-parser/src/expression.rs`, function `is_arrow_function_ahead`
- Why fragile: Logic must track bracket depth AND angle brackets, handle `>>` split, and stop at statement boundary. Any of these can regress silently.
- Safe modification: (a) Always run `parser_typescript` suite after changes (32 failures must not increase), (b) Manually verify against `compiler/parsingDeepParenthensizedExpression.ts`, (c) Add test for each edge case (nested `<>`, `>>` in type args, etc).
- Test coverage: Several tests; the regression suite is the strongest check. No fuzzing yet.

**JSDoc scanner state restoration:**
- Files: `crates/tsr-scanner/src/jsdoc.rs`, methods `set_range`, `save`, `restore`
- Why fragile: The `limit` field affects all character-lookahead logic implicitly. An incorrect restoration could cause scanner to read past comment boundary.
- Safe modification: (a) Verify that `ScannerState` captures `limit` correctly, (b) Test that `save`/`restore` pairs always reset correctly, (c) Add assertions that position <= limit after any `restore`.
- Test coverage: Integration tests with real JSDoc; no unit tests on state restoration itself.

**Token type re-reading when returning from type parser:**
- Files: `crates/tsr-parser/src/jsdoc.rs`, after type expression parsing
- Why fragile: The type parser stops on a token scanned with ordinary rules; that token must be re-scanned as JSDoc. If off-by-one in position, or if re-scanning logic differs, the two scans could diverge.
- Safe modification: (a) Verify the position wound back to is exactly the token start, (b) Compare JSDoc scan result with ordinary scan to catch divergence, (c) Test with complex types spanning multiple JSDoc lines.
- Test coverage: JSDoc integration tests; span assertions catch position errors.

---

## Scaling Limits

**Memory per file:**
- Current capacity: Files up to ~10 MB parse without issue; 69 levels of nesting required 16 GB before the lookahead fix
- Limit: Depth guard (512 statement levels) prevents stack overflow. Expression depth is inherently limited by precedence. No known hard limit.
- Scaling path: Monitor with fuzzing and real-world projects. Arena pooling will reduce per-file allocation cost in long-running processes.

**Conformance harness corpus size:**
- Current capacity: 12,444 cases, 4.5 MB corpus, ~25 s CPU with rayon
- Limit: No hard limit; corpus-driven testing can scale to larger suites. Snapshot format handles arbitrary case counts.
- Scaling path: Parallelism is already in place. If corpus grows to 50k+ cases, profile to confirm rayon scales. Determinism is enforced by case-order collection, so scaling is straightforward.

**Symbol table size (Phases 2+):**
- Current capacity: No binder yet; unknown
- Limit: Upstream keeps symbols in `LinkStore` (side table) keyed by dense ids. Should scale linearly with source size. Watch for quadratic behavior in declaration-merging loops.
- Scaling path: Measure on large real projects (e.g., TypeScript/TypeScript codebase). Upstream's techniques (paged stores for sparse data) are documented and ready to adopt.

---

## Dependencies at Risk

**No direct `oxc_*` dependencies — lower risk:**
- Status: By design, oxc's *crates* are not linked (unstable, weekly breaking changes)
- Impact: Buys independence at the cost of rewriting infrastructure. Mitigated by using oxc's *dependencies* (allocator, indices, etc.).
- Recommendation: Continue this approach. Monitor oxc's releases for design patterns, not for breaking changes to adopt.

**Submodule dependency on `microsoft/typescript-go`:**
- Risk: Upstream is active; commits after our pin (5b1047d10) are not automatically tracked
- Files: `vendor/typescript-go` (submodule), PLAN.md §1, issue `tsr-l68` (upstream drift tracker)
- Impact: Checker fixes in upstream could be missed if drift tracking is not automated
- Mitigation: Upstream-anchored doc comments (enforced by lint) + manual drift tracking (issue `tsr-l68` not yet automated)
- Improvement path: Automate drift tracker (scheduled job walks commits since pin, files bd issues). This is a P0 blocking item, not yet done.

**`simdutf8`, `unicode-id-start`, `phf` for scanner:**
- Risk: These are stable, mature dependencies with low change frequency
- Impact: Scanner depends on them; failure to update could leave security issues
- Mitigation: Use latest stable versions; monitor for security advisories
- Recommendation: Include in dependency audit CI.

---

## Missing Critical Features

**No Binder (Phase 2):**
- Problem: Symbols, scopes, declaration merging, control-flow graph not implemented
- Blocks: Checker (Phase 4), which depends on symbol resolution
- Files: `crates/tsr-binder/` exists but is empty scaffolding
- Acceptance criteria: Symbol-table dumps match Go across corpus (PHase 2 gate in PLAN.md)
- Timeline: Follows parser completion (Phase 1 gates at 99.36%, now complete-ish)

**No Checker (Phase 4):**
- Problem: Type checking, type inference, error reporting not implemented; this is 60k LOC upstream
- Blocks: Correctness of the entire compiler. Currently only syntactic diagnostics work.
- Files: `crates/tsr-checker/` does not exist
- Acceptance criteria: Type-baseline and error-baseline pass rates ratchet toward 100%
- Timeline: Second half of project; preceded by memoization spike (bd `tsr-6n3`, unresolved)

**No Module resolution (Phase 3):**
- Problem: TypeScript module resolution (node16, nodenext, bundler, path mapping) not implemented
- Blocks: Binder needs module boundaries, checker needs to resolve imports
- Files: `crates/tsr-module/` exists but is empty
- Acceptance criteria: Upstream module-resolution baselines pass
- Timeline: Follows binder (Phase 2 → Phase 3)

**No Language service (Phase 7):**
- Problem: Completions, goto-definition, find-references, rename, etc. not implemented
- Blocks: Editor integration; requires fourslash DSL port first (9,860 LOC)
- Files: `crates/tsr-ls/` exists but is empty
- Acceptance criteria: fourslash tests pass
- Timeline: Year 2 of project; after checker mostly complete (Phase 4–6)

**No LSP server (Phase 8):**
- Problem: Language Server Protocol server not implemented
- Blocks: IDE/editor integration
- Files: `crates/tsr-lsp/` exists but is empty
- Acceptance criteria: Editor smoke tests against VS Code
- Timeline: After language service (Phase 7)

---

## Test Coverage Gaps

**Parser positive cases (99.36% of 5,031 clean cases pass):**
- What's not tested: 32 specific failures listed in `crates/tsr-conformance/snapshots/parser_typescript.snap`
- Files: Parser snapshot, individual case files in `_submodules/TypeScript/tests/cases/`
- Risk: Each untested pattern could regress without notice. Most are edge cases; some (import attributes, ASI in types) block real code.
- Priority: High for completeness, but lower priority than binder/checker since syntax is understood
- Action: `cargo run -p tsr-conformance --example failure_classes` categorizes them; address by category

**Configuration-varied baselines (793 cases):**
- What's not tested: Cases with `@target: es5, es2015, …` are skipped because per-configuration runs are not yet implemented
- Files: 793 case files with varied configurations (found via regex in conformance corpus)
- Risk: A parser bug that only manifests under specific configurations (e.g., only in es5 mode) would not be caught
- Priority: Medium; per-configuration runs are a Phase 1 follow-up task
- Action: Implement per-configuration snapshot collection (see `crates/tsr-conformance/snapshots/conformance.md`)

**Cases with no recorded baseline (617 cases):**
- What's not tested: Cases with no `.errors.txt`, `.types`, `.symbols`, `.js`, or `.diff` file
- Files: 617 cases in corpus; upstream never ran them
- Risk: These cases are skipped (not counted as passes), but a future parser could unknowingly pass them
- Priority: Low; these are likely test infrastructure issues, not real failures
- Action: Audit which cases are skipped and why; consider adding them if they have real code

**`.types`, `.symbols`, `.js` conformance suites (Phases 2, 4, 5):**
- What's not tested: Type baselines, symbol-table dumps, emitted JavaScript — not yet measured
- Files: Not yet implemented; baselines exist upstream in `testdata/baselines/reference/`
- Risk: Binder and checker will ship without numerical conformance gates, making regressions invisible
- Priority: Critical for Phases 2, 4, 5; each must have a committed snapshot before work begins
- Action: Implement suites early in each phase, before significant work. See `docs/architecture/conformance.md`.

**Checker memoization design not tested (P0 spike):**
- What's not tested: Lazily-memoized type computation under Rust's borrow checker
- Files: Spike work in bd `tsr-6n3`; no code yet
- Risk: Checker memoization is the longest pole in Phase 4; wrong architecture could force a rewrite
- Priority: Critical, blocks Phase 4 (blocking issue, P0)
- Action: Implement prototype in each viable style (append-only + interior mutability vs. id-returning &mut self vs. RefCell-guarded memo tables), then decide. Document the winning approach before Phase 4 begins.

---

## Architectural Concerns

**Checker memoization vs. borrow checker (BLOCKING P0 SPIKE):**
- Issue: Checker lazily computes and caches types while holding references into its own arenas
- Files: Not yet written; spike work in bd `tsr-6n3`
- Impact: This decision constrains 60k LOC of checker code and has no prior art in oxc (`oxc_type_checker` is a 144-line no-op)
- Current status: Unresolved; blocking Phase 4
- Approaches under consideration:
  1. Append-only arenas with interior mutability (elsa-style)
  2. Id-returning &mut self methods that never hand out long-lived &Type
  3. RefCell-guarded memo tables separate from the arena
- Recommendation: Prototype all three approaches with a narrow vertical slice (one type inference decision), benchmark, and pick based on code clarity + performance. Write the decision in an ADR before Phase 4 starts.

**Upstream drift not yet automated (P0 ISSUE):**
- Issue: Scheduled job to track upstream commits since pin is not yet built
- Files: Work in bd `tsr-l68`; `xtask/` is the right location
- Impact: Critical for idiomatic-rewrite strategy (upstream fixes cannot be diff-and-replayed, so drift tracking must be mechanical)
- Current status: Unimplemented
- Recommendation: Build before Phase 2 starts. Job should walk commits since 5b1047d10, classify by `internal/` package, and file a bd issue per touched package. Must be idempotent across runs.

**Threading only for parsing; binder/checker thread-safety undefined:**
- Issue: Parallel parsing works (rayon over files, per-file arenas). Binder is planned per-file (inherits model). Checker is program-wide and must handle shared memoization.
- Files: `docs/architecture/threading.md`, bd `tsr-6n3`
- Impact: Binder should be straightforward; checker's concurrency model depends on memoization architecture (see spike above)
- Current status: Parsing is parallel; binder/checker threading is unsolved
- Recommendation: Resolve checker memoization first (the spike). Then decide whether to run multiple checker instances (upstream approach) or share one checker with concurrent access to memo tables (`papaya` is in the bill of materials for exactly this).

---

## Upstream Divergences & Accepted Limitations

**AST shape differs from oxc for idiomatic fidelity:**
- Divergence: Our AST matches TypeScript's node kinds/shapes exactly; oxc's deliberately removes "ambiguous nodes" (splits `Identifier` into `BindingIdentifier` / `IdentifierReference`)
- Files: PLAN.md §8, `docs/adr/0002-own-ast.md`, `crates/tsr-ast/src/`
- Why accepted: Checker fidelity requires TypeScript's node shapes; paying a per-function translation tax across 60k LOC is not worth a 9k-LOC parser savings
- Consequence: Checker code cannot be copied from oxc wholesale; must be ported with shape adaptation

**Eager JSDoc parsing instead of lazy:**
- Divergence: Upstream parses JSDoc on-demand (`ast.SetParseJSDocForNode`); we parse eagerly during main parse
- Files: `docs/adr/0008-jsdoc-parsed-eagerly.md`
- Why accepted: Lazy parsing requires post-construction arena allocation, which conflicts with `self_cell` lifetime model needed for `ParsedFile` to be `Send`
- Consequence: 7.5% parse overhead on normal TS; up to 64% on JSDoc-dense files. Acceptable cost, re-measure if incremental reparse becomes bottleneck.

**1,208 known upstream divergences (`.diff` baselines):**
- Divergence: TypeScript-go intentionally differs from TypeScript in specific cases (recorded as `.diff` baseline files)
- Files: Cases in conformance corpus with `.diff` baseline file; `conformance.md` documents this
- Why accepted: Those cases are skipped (not counted as failures) because the divergence is intentional
- Consequence: Conformance will never be 100% against TypeScript; snapshot records the final baseline and must match upstream's `.diff` baseline where it exists

---

## Next Steps (P0 Blocking Items)

1. **Resolve checker memoization architecture** (bd `tsr-6n3`): Prototype, benchmark, decide, ADR it before Phase 4
2. **Automate upstream drift tracking** (bd `tsr-l68`): Build scheduled job + bd filing before Phase 2
3. **Close 32 parser failures** (lowest priority but nice to have): Categorize and address by category; `failure_classes` example shows categories

---

*Concerns audit: 2026-08-03*
