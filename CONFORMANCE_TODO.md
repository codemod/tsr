# Remaining Conformance Work (Non-Checker/Non-Diagnostics)

## Session 12 Status (2026-08-08, measured at `8dcdc71`)

`TASK-conformance.md` is the workstream's live handoff; this file is the
case-level roadmap. Numbers below are from the coverage run at `8dcdc71`.

### Current State by Suite

| Suite | Pass Rate | Failures | Notes |
|-------|-----------|----------|-------|
| `corpus_ingest` | 100% | 0 | ✅ Complete |
| `baseline_resolution` | 100% | 0 | ✅ Complete |
| `scanner_termination` | 100% | 0 | ✅ Complete |
| `scanner_clean_files` | 100% | 0 | ✅ Complete |
| `module_resolution` | 100% | 0 | ✅ Complete |
| `file_loader` | 100% | 0 | ✅ Complete |
| `parser_typescript` | 100% | 0 | ✅ Complete |
| `printer_round_trip` | **100%** | **0** | ✅ **Complete this session** (11,776/11,776) |
| `binder_symbols` | 99.15% | 72 | was 163 — alias transparency indexed |
| `dts_emit` | 89.04% | 41 | was 47 |
| `dts_shape` | 85.32% | 148 | dominated by checker-driven import synthesis |
| `isolated_declarations` | 86.67% | 2 | diagnostic-related (TS9025/9026), out of scope |
| `parser_reachable_target` | 47.60% | — | population measurement, not a pass rate |
| `dts_reachable_target` | 42.34% | — | population measurement, not a pass rate |

### What closed printer_round_trip (session 12)

- Parser: `tryParseConstructorDeclaration` ported faithfully — commits on the
  `constructor` keyword alone, parses type parameters and a return type,
  accepts string-literal `"constructor"` + `(` (`parser.go:1917`); an asterisk
  commits to a method before any name (`parser.go:1944`).
- Printer: setter return type annotations print (upstream's `emitSignature`).
- Suite: JSDoc-owned tracked tokens excluded from the histogram positionally —
  trivia the comment-free print can never reproduce, per the gate's own scope.

### Remaining failures, classified

**dts_emit (41)** — most need the checker:
- ~25 checker-owned: inferred arrow/function types (`genericContextualTypes1`,
  `declarationEmitScopeConsistency3`), import synthesis driven by inferred
  types (`typeReferenceDirectives5/13`, `declarationEmitBundlerConditions`),
  TS7056 suppression (`declarationEmitPrivatePromiseLikeInterface`),
  node-builder alias/typeof resolution (`declarationEmitNameConflicts2`,
  `isolatedDeclarationsAddUndefined`).
- ~10 parse skips whose sources deliberately mix invalid syntax — they stay
  skips faithfully (upstream errors too).
- Residue: comment preservation inside types (`unionTypeWithLeadingOperator`,
  `declarationEmitWorkWithInlineComments`), the CommonJS `exports.x = …`
  declaration family (`assignmentToVoidZero1`, `jsDeclarationEmitExportAssigned*`),
  JS static-block `this.x = …` synthesis (`javascriptThisAssignmentInStaticBlock`
  — but its second class needs an inferred type anyway).

**dts_shape (148)** — dominated by "want `import`": upstream synthesizes or
retains imports because the checker-inferred type of an exported declaration
names them. Not reachable per-unit without the checker.

**binder_symbols (72)**:
- Numeric-name canonicalization, ~6 cases — `bd tsr-1` (arena through
  `FileInfo`, or a value field on `NumericLiteral`).
- Computed-name constant folding (`C["some" + "method"]`) — checker late-binding.
- `export =` augmentation targets (`augmentExportEquals3/4/6`) — cross-file
  alias into an export-assigned module.
- JSDoc `@overload` declaration lists (`overloadTag1`, `jsFileMethodOverloads`).
- Escaped/unicode name decodings, JSX-namespace symbols, and singles.

### Investigation Tools

- `cargo run -p tsr-conformance --bin coverage` — full run, writes snapshots
- `cargo run -p tsr-conformance --example dts_failures [-- --cases]` — all-failure classifier
- `cargo run -p tsr-conformance --example dtsdump -- <name>...` — produced vs expected `.d.ts` text per unit (shape failures included)
