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

## Files Modified This Session
- `STATUS.md` - Updated with current numbers
- `crates/tsr-conformance/snapshots/checker_types.snap` - Reflected improvements
- `crates/tsr-conformance/snapshots/diagnostics.snap` - Reflected improvements

## Investigation Tools Available

- `cargo run -p tsr-conformance --bin coverage` - Full conformance run
- Individual test case examination in `vendor/typescript-go/testdata/baselines/reference/`
- `crates/tsr-conformance/examples/*.rs` - Diagnostic tools (diaggap.rs, depend.rs, etc.)
