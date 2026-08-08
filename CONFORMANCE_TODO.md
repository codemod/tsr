# Remaining Conformance Work (Non-Checker/Non-Diagnostics)

## Session 11 Status (2026-08-08)

This document tracks the remaining work for improving conformance in non-checker/non-diagnostics areas.

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
| `printer_round_trip` | 99.80% | **23** | JSDoc token handling |
| `binder_symbols` | 98.06% | **164** | Symbol merging, visibility |
| `dts_emit` | 87.43% | **47** | Type inference fallbacks |
| `dts_shape` | 85.30% | **148** | Declaration structure |
| `isolated_declarations` | 86.67% | 2* | Diagnostic-related (TS9025, TS9026) |
| `parser_reachable_target` | 47.60% | 5,539 | Large gap, unclear root cause |
| `dts_reachable_target` | 42.34% | 670 | Reachability analysis |

*isolated_declarations failures are diagnostic-related, skip per non-diagnostics directive

### Investigation Results (Session 11)

**printer_round_trip (23 failures)**
- Detailed investigation shows QuestionToken/DotDotDotToken/AsteriskToken disappearing during print/reparse
- Root cause: Specific JSDoc patterns lose tokens during round-trip
- Tests added to verify basic JSDoc handling works (pass), suggesting issue is pattern-specific
- The printer correctly emits tokens via `emit_token_node()`, but tokens aren't present in reparsed AST
- Likely either: (a) parser doesn't create token nodes for certain JSDoc patterns, or (b) printed output loses `?` in specific contexts
- Needs deeper investigation into JSDoc parser/printer interaction

**dts_reachable_target (670 failures)**  
- All failures are "needs inference: TS9xxx" - these are structural limitations
- Cases where @isolatedDeclarations would require type inference
- Not actually failures - these represent the ceiling of checker-free emitters
- Should be considered "expected" rather than bugs to fix

**dts_emit path reference bug (commonSourceDirectory)**
- Input has `/// <reference path="../types/bar.d.ts" />` at `/app/index.ts`
- Output should be `/// <reference path="../../types/bar.d.ts" />` at `/app/bin/index.d.ts`
- We emit `/// <reference path="../types/bar.d.ts" />` (incorrect for new location)
- Root cause: Reference paths not being re-relativized for output location
- Requires understanding declarations module path calculation logic

### Priority Fixes (by impact/effort)

#### High Impact, Medium Effort
1. **printer_round_trip** (23 failures, 99.80% → 100%)
   - **Pattern**: 13/23 failures are QuestionToken losses (1 → 0)
   - **Root cause**: JSDoc-annotated parameters/properties losing optional marker during print/reparse
   - **Affected test cases**: declarationEmitCastReusesTypeNode4, expandoFunctionContextualTypesJs, jsdocInTypeScript, etc.
   - **Investigation needed**: Check JSDoc parser/printer for optional token handling
   - **Code areas**: `crates/tsr-printer/src/lib.rs::emit_parameter`, JSDoc parsing in parser

#### Medium Impact, High Effort  
2. **binder_symbols** (164 failures, 98.06% → 100%)
   - **Pattern**: Symbol merging issues, missing symbols in merged declarations
   - **Root cause**: Symbols not merging properly across files; namespace/module merging incomplete
   - **Typical failures**: 
     - `declared on {7}, expected to include {0, 7}`
     - `no symbol` for nested members
   - **Code areas**: `crates/tsr-binder/src/`, symbol table merging logic

3. **dts_emit** (47 failures, 87.43% → ~95%+)
   - **Pattern 1** (~15): Type fallback to `any` (accessor types, conditional types, destructuring)
   - **Pattern 2** (~14): Quote style/path references (`commonSourceDirectory` path calc bug)
   - **Pattern 3** (~10): Visibility/export filtering
   - **Pattern 4** (~8): Missing/extra exports
   - **Code areas**: `crates/tsr-declarations/`, type builder, visibility filtering

4. **dts_shape** (148 failures, 85.30% → ~95%+)
   - **Pattern**: Declaration structure differences (missing imports, wrong declaration kinds)
   - **Root causes**: 
     - Import statements not emitted
     - Class/interface vs var distinctions
     - Namespace handling
   - **Code areas**: `crates/tsr-declarations/transform.rs`, visibility logic

#### Lower Priority (Large gaps)
5. **parser_reachable_target** (5,539 failures, 47.60%)
6. **dts_reachable_target** (670 failures, 42.34%)

### Recent Progress (Recent commits showing pattern)

The ten commits before this session focused on JSDoc type handling:
- `fix(dts): keep destructuring in the inference bucket`
- `feat(dts): type namespace-import destructuring as typeof`
- `feat(dts): map JSDoc-only type spellings to TypeScript`
- `feat(dts): typedef comment ownership and richer tags`
- `feat(dts): graft real JSDoc types and synthesize typedefs`
- `feat(dts): first slice of JSDoc-typed JS declarations`

These achieved a **+39.94 point improvement** in dts_emit (47.49% → 87.43%), showing focused work on specific areas can yield large gains.

### Recommended Next Steps

1. **Quick Win**: Fix printer_round_trip JSDoc token handling (23 fixes, likely fixes multiple failures with one change)
2. **Medium Term**: Continue dts_emit work on type inference fallbacks
3. **Parallel**: Binder symbol merging investigation
4. **Future**: Large gap areas (parser_reachable_target, dts_reachable_target)

### Known Limitations

From `docs/architecture/declaration-emit.md`:
- Comments are dropped (printer limitation)
- Visibility approximated by reachability only
- No checker available for inference
- Module specifiers re-quoted (style difference)
- No support for CommonJS transforms

These account for some baseline incompatibilities (~2% of failures).

## Session 11 Summary

**Work Completed:**
1. ✅ Ran full conformance suite and captured current state (1,067+ remaining failures)
2. ✅ Updated snapshots with recent improvements (checker_types +10 cases, +1220 lines; diagnostics +6 cases)
3. ✅ Updated STATUS.md with comprehensive progress tracking
4. ✅ Investigated printer_round_trip JSDoc token issue (added diagnostic tests)
5. ✅ Identified dts_reachable_target as structural limitation (not bugs)
6. ✅ Analyzed dts_emit path reference calculation bug
7. ✅ Documented all findings in this roadmap

**Testing/Debugging Done:**
- Added round-trip tests for JSDoc typedefs (basic cases pass)
- Traced printer token handling code (`emit_token_node` works correctly)
- Examined declaration reference path calculation
- Reviewed parser fixes that completed parser_typescript (99.40% → 100%)

**Next Session Priorities:**
1. Implement JSDoc token handling fix (13/23 printer failures)
2. Fix dts_emit path reference re-relativization
3. Continue dts_shape declaration structure improvements
4. Investigate binder_symbols merging issues

## Files Modified This Session
- `STATUS.md` - Updated with current numbers and progress notes
- `CONFORMANCE_TODO.md` - Created with comprehensive roadmap and investigation findings
- `crates/tsr-printer/tests/round_trip.rs` - Added JSDoc debugging tests
- `crates/tsr-conformance/snapshots/checker_types.snap` - Reflected improvements
- `crates/tsr-conformance/snapshots/diagnostics.snap` - Reflected improvements

## Investigation Tools Available

- `cargo run -p tsr-conformance --bin coverage` - Full conformance run
- Individual test case examination in `vendor/typescript-go/testdata/baselines/reference/`
- `crates/tsr-conformance/examples/*.rs` - Diagnostic tools (diaggap.rs, depend.rs, etc.)
