# Parity round 7 (tsr-2zk)

The goal is unchanged and not met: 99.9% case parity with pinned tsgo
(`vendor/typescript-go` @ `5b1047d`) on the complete `checker_types`,
`checker_types_configured`, `diagnostics` and `diagnostics_configured` corpus,
with zero previously RIGHT losses and a verified TSR/tsgo median wall ratio
<=0.50 on equivalent complete work.

Round 7 starts from main `f55a2585` (round 6 checkpoint `83c6f58d` plus its
STATUS row). Its numbers are STATUS.md §1's round-6 column:

| Suite | Start (`83c6f58d`) |
|---|---|
| `checker_types` | 8,677/9,538 (90.97%) |
| `checker_types_configured` | 1,752/1,928 (90.87%) |
| `diagnostics` | 4,786/5,502 (86.99%) |
| `diagnostics_configured` | 955/1,091 (87.53%) |

## How round 7 runs

Boxes are **Amp orb threads**, one per lane, each with its own checkout of
`origin/main`. The integration owner is the dispatching thread. The protocol
is [box-protocol.md](box-protocol.md), with three orb-specific changes:

1. **Setup.** The orb runs `.agents/setup`, which installs the pinned 1.96.0
   toolchain, Go 1.26.8, `bd` and the pinned native oracle at
   `target/tsgo-pinned`. Do not run `scripts/offline-cargo/bootstrap.sh` and do
   not set `RUSTUP_TOOLCHAIN=stable`; the pinned toolchain is available.
2. **Branch.** Work on `box/r7-<lane>` cut from `origin/main`, push after every
   verified commit, and `git rebase origin/main` (or merge it) whenever main
   moves. The integration owner merges from these branches only.
3. **Report.** When a cluster is finished, or the lane is blocked, send the
   report of box-protocol §6 to the integration thread with
   `send_thread_message`, then continue with the next cluster. Boxes do not
   write Beads; proposed issues go in the report and the lane note.

Gates are box-protocol §5 unchanged: `scripts/parity_gate.sh freeze` before
editing, `scripts/parity_gate.sh compare` after (both unfiltered; any
TYPE_LOSS/DIAG_LOSS/MISSING line refuses the commit), the coverage bin,
`cargo test --workspace --release`, clippy on touched code, fmt, and the
21-sample child-CPU perf comparison against the frozen binary on
domain-model and generic-imports (re-run at 41 above 1.03).

## Lanes and file ownership

Whole-file ownership unless a function is named. A file not listed belongs to
the integration owner; a change to it, or to another lane's file, is described
in the report and routed, never made.

| Lane | Owns | Starting queue |
|---|---|---|
| r7-shared (`tsr-2zk.1268`) | `symbols.rs`, `signatures.rs` (signature construction, `get_signature_from_declaration`), `members.rs`, `instantiate`/mapper code, `resolution.rs`, `crates/tsr-binder`; the isolated alias/mapper cutover branch `recovery/rejected-alias-shared-20261007` | the cutover's regressions (`tsr-2zk.16.56.1.4`), RESOLVE-ALIAS-INDIRECTION, MERGE-SYMBOL-RESOLVE-ALIAS-TARGET, IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET, DEFAULT-EXPORT-ALIAS-CLASS-MERGE, RESOLVE-ES-MODULE-SYMBOL-CLONE, module augmentation merge (`.38`), lazy signature parameters (`.9.7`) |
| r7-calls (`tsr-2zk.1269`) | `calls.rs`, `call_arity.rs`, `call_reports.rs`, `decorators.rs`, `union_signatures.rs`, `type_argument_arity.rs` | `.1267` (isSignatureApplicable reportErrors hook, +18), CALL-ARGUMENT-APPLICABILITY-REPORT, CANDIDATE-FOR-OVERLOAD-FAILURE, OVERLOAD-FAILURE-REPORT, CALL-SPREAD-ARGUMENT-APPLICABILITY, TAGGED-TEMPLATE-EFFECTIVE-ARGS, RESOLVE-DECORATOR-CALL-ERRORS, CHOOSE-OVERLOAD-GENERIC-WALK, CHECK-NON-NULL-CALLEE, `.9.6`; the calls.rs arms of PARSE-ERROR-FILE-CHECK-DECLINE and JS-FILE-CHECK-DECLINE |
| r7-parser (`tsr-2zk.1270`) | `crates/tsr-parser`, `crates/tsr-scanner` | PARSER-RECOVERY-DIVERGENCE, PARSER-REPARSE-TOP-LEVEL-AWAIT, PARSER-IS-YIELD-EXPRESSION, SCANNER-NUMERIC-AND-ESCAPE-DIAGNOSTICS, REGEXP-SCANNER-VALIDATION, PARSER-DECORATOR-EXPRESSION, PARSER-FUNCTION-BLOCK-OR-SEMICOLON-RECOVERY, PARSER-JSX-IN-JS-UNARY-OPERAND, PARSER-STATIC-BLOCK-AWAIT-CONTEXT, unaligned-count parser divergences (`.1086`) |
| r7-grammar (`tsr-2zk.1271`) | `grammar.rs`, `import_attributes.rs`, `class_fields.rs`, `enum_member_name.rs`, `enum_initializer.rs`, `unused.rs`, `heritage_conformance.rs`, `base_types.rs` | CHECK-GRAMMAR-MODIFIERS, GRAMMAR-MISC-DECLARATIONS, PRIVATE-IDENTIFIER-GRAMMAR, CHECK-GRAMMAR-AWAIT-YIELD-CONTEXT, GRAMMAR-ACCESSOR-DECLARATION, IMPORT-ATTRIBUTES-CHECKS, CLASS-PROPERTY-INITIALIZER-CHECKS, ENUM-MEMBER-COMPUTED-NAME, CHECK-INTERFACE-HERITAGE-AND-BASES, RESOLVE-BASE-TYPES-CIRCULARITY |
| r7-printer (`tsr-2zk.1272`) | `checker.rs` (symbol chain, best name, qualified names), `symbol_access.rs`, `symbol_accessibility.rs`, `module_specifiers.rs`, `printing.rs`, `node_reuse.rs` | every GET-SYMBOL-CHAIN-* and SYMBOL-CHAIN-* cluster, TRY-SYMBOL-TABLE-DEFAULT-IMPORT-ALIAS, TRYSYMBOLTABLE-UMD-ALIAS-EXCLUSION, SPECIFIER-FOR-MODULE-SYMBOL(/NODE-MODULES), TYPE-ALIAS-ACCESSIBILITY-GATE, `.39` |
| r7-declared (`tsr-2zk.1273`) | `declared.rs`, `mapped.rs`, `instantiation_expressions.rs`, `indexed.rs`, `unions.rs` | `.1266` (mapped stack, +75 types, re-cut on print-time plans), `.1265` (deferred conditional fall-through, +70), TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, CONDITIONAL-INLINE-NODE-INSTANTIATION, TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL, IMPORT-TYPE-NODE-*, CONDITIONAL-DEFERRAL-GATE, ORIGIN-SLICE-GATE |
| r7-flow (`tsr-2zk.1274`) | `flow.rs`, `nonexistent_property.rs`, `index_access_reports.rs`, `readonly_target.rs`, `truthiness.rs`, `this_expression.rs`; the parse-error gates outside `calls.rs` | `.1264` (asserts-this narrowing, then the access-site lift), NONEXISTENT-PROPERTY-CERTIFICATION-GATE, PARSE-ERROR-FILE-CHECK-DECLINE (non-calls arms), r6-errorsplit3's read-reference diffs (`.1150`), NARROW-* clusters, FLOW-NARROWING-RESULT-TYPE |
| r7-contextual (`tsr-2zk.1275`) | `contextual.rs`, `inference.rs`, `array_literals.rs`, `destructure.rs`, `binding_patterns.rs`, `objects.rs`, `spreads.rs` | ARG-CONTEXT-RESOLVED-SIG, INFER-NO-CANDIDATE-GUARD, ARRAY-LITERAL-TUPLE-CONTEXT, CONTEXTUAL-BINDING-PATTERN-INITIALIZER, BINDING-PATTERN-IMPLIED-TYPE, OBJLIT-THIS-LITERAL-SELF-FALLBACK, ARRAY-LITERAL-OMITTED-EXPRESSION-ELEMENT, CONTEXTUAL-ARG-SUPER-CALL, REVERSE-MAPPED-TYPE-INFERENCE, SPREAD-PROPERTY-ANNOTATION-REUSE |
| r7-reports (`tsr-2zk.1276`) | `relater.rs`, `assignreport.rs`, `implicit_any.rs`, `jsdoc_*.rs`, `jsx_*.rs`, `js_case_data.rs`, `assignment_declarations.rs` | IMPLICIT-ANY-PARAMETER-REPORT, IMPLICIT-ANY-AUTO-TYPED-VARIABLE, JSDOC-TAG-SEMANTICS, JSX-OVERLOAD-AND-COMPONENT-REPORT, JSX-COMPONENT-ATTRIBUTES-RESOLUTION, JSX-ATTRIBUTES-RELATION-REPORT, relation-report clusters (ASSIGN-REPORT-*, REPORT-UNMATCHED-*, HAS-EXCESS-PROPERTIES-*, ENUM-RELATION-SAME-NAME), `.1124` variance WIP |
| r7-perf (`tsr-2zk.1277`) | `crates/tsr` (driver), `crates/tsr-compiler`, `perf_links.rs`; in `calls.rs` only the resolving-signature sentinel, in `members.rs` only the composite-name decline (`members.rs:3233`), each coordinated with that file's owner | `tsr-2zk.17.3` parallel parse/bind, `.1261`, `.1258`; measured TSR/tsgo median wall ratio on domain-model, domain-model-large, generic-imports and jsTyping |

The cluster names and case counts are r6-triage's ranked table
([notes/r6-triage.md](notes/r6-triage.md) §2), cut at `e6eadf4`; a lane
re-measures its own clusters on its frozen base before porting.

## Integration

The integration owner merges green box heads in batches onto main, runs the
same gates on the merged tree against the previous main freeze, refreshes the
conformance snapshots, records the batch below, and pushes. A batch that
loses a previously RIGHT key, or regresses perf, is cut back to the commits
that pass; the backed-out commit returns to its lane with the loss named.

The alias/mapper cutover never enters a batch until its own branch passes
every gate against the then-current main.

## Batches

### Batch 1 — r7-flow `bc17c1f8`, r7-shared `9dea4118`

Merged onto main `9020aa67` as `ad0c9c22`. Gate against the `9020aa67` freeze:
types +21 RIGHT / 0 lost / 0 missing, diagnostics +5 cases / 0 lost / 0
missing; workspace tests pass; fmt clean; perf child-CPU new/old at 21
samples domain-model 0.960, generic-imports 0.983, diagnostics identical.
Strict clippy reports the seven errors main already carries (enum_initializer,
index_signatures, printing, symbols:4513, templates, unique_symbols,
tsr-dts accessibility test); none is in a line the batch touched.

| Suite | `9020aa67` | Batch 1 |
|---|---|---|
| `checker_types` | 8,677 | 8,680 |
| `checker_types_configured` | 1,752 | 1,753 |
| `diagnostics` | 4,786 | 4,791 |
| `diagnostics_configured` | 955 | 955 |

Ownership change: r7-grammar also owns `class_function_merge.rs`,
`merge_conflicts.rs` and `check.rs::check_merged_namespace_prototype` for the
two multi-checker drops r7-perf found under `tsr-2zk.1258`.

### Batch 2 — r7-calls `5ea8f7e4`, r7-flow `216195f6`, r7-perf `a51bc525`, r7-reports `46318b3a`, r7-shared `582a2b0b`

Merged onto main `87146adf` as `ae835bfe`. Gate against the batch-1 freeze:
types +4 RIGHT / 0 lost / 0 missing, diagnostics +30 cases / 0 lost / 0
missing; workspace tests pass; fmt clean; strict clippy unchanged (the same
pre-existing errors, none in a touched line). Perf child-CPU new/old at 21
samples: domain-model 0.940, generic-imports 0.736 (r7-perf's lazy JSDoc,
ADR-0053), diagnostics identical.

| Suite | Batch 1 | Batch 2 |
|---|---|---|
| `checker_types` | 8,680 | 8,682 |
| `checker_types_configured` | 1,753 | 1,753 |
| `diagnostics` | 4,791 | 4,816 |
| `diagnostics_configured` | 955 | 958 |

Ownership changes: r7-contextual also owns `widening.rs` (const-context
literal candidates, `tsr-2zk.1275.1`); r7-calls may make
`flow.rs::get_effects_signature`/`get_type_of_dotted_name` `pub(crate)` for
TS2775/TS2776.

### Batch 3 — r7-calls `2745f162`, r7-contextual `dcd22416`, r7-declared `2f46ba40`, r7-flow `0633293e`, r7-grammar `f298083c`, r7-parser `dd6f9d88`, r7-reports `3d2cfdfa`

Merged onto main `1b466dc8`. Gate against the batch-2 freeze: types +453
RIGHT / 0 lost / 0 missing, diagnostics +33 cases / 0 lost / 0 missing;
workspace tests pass; fmt clean; strict clippy unchanged (pre-existing errors
only). Perf child-CPU new/old at 21 samples: domain-model 1.006,
generic-imports 0.996, diagnostics identical.

| Suite | Batch 2 | Batch 3 |
|---|---|---|
| `checker_types` | 8,682 | 8,708 |
| `checker_types_configured` | 1,753 | 1,757 |
| `diagnostics` | 4,816 | 4,839 |
| `diagnostics_configured` | 958 | 968 |

Ownership changes: r7-grammar also owns `module_format.rs`, `strict_mode.rs`
and `meaning_mismatch.rs`; r7-parser is granted the three
`NodeIsMissing(body)` readers (return type and TS7010 in `signatures.rs`,
`checkFunctionOrConstructorSymbol` in `check.rs`) for the missing-block
recovery, in one commit with its parser half.

### Batch 4 — integ lint `a29e996c`, r7-calls `332d624d`, r7-contextual `3c301c08`, r7-declared `12b2c034`, r7-printer `843832ef`, r7-reports `35e1af5f`

Merged onto main `7e9f37eb`. r7-parser and r7-perf conflicted (their rebased
copies of already-merged commits) and were returned for a rebase. Gate
against the batch-3 freeze: types +320 RIGHT / 0 lost / 0 missing,
diagnostics +10 cases / 0 lost / 0 missing; workspace tests pass; fmt clean.
**Strict clippy is green for the first time this round**: the lint commit
clears main's pre-existing errors, and one new raw-pointer borrow from
r7-reports' TS2820 head is fixed in the batch. Perf child-CPU new/old:
domain-model 0.984 (21) / 1.009 (41), generic-imports 1.029 (21) / 0.960
(41), diagnostics identical.

| Suite | Batch 3 | Batch 4 |
|---|---|---|
| `checker_types` | 8,708 | 8,736 |
| `checker_types_configured` | 1,757 | 1,771 |
| `diagnostics` | 4,839 | 4,849 |
| `diagnostics_configured` | 968 | 968 |

Ownership changes: r7-contextual owns `const_inference.rs`; r7-perf owns the
program-diagnostics wiring in `tsr-conformance/src/diagnostics_suite.rs`.

### Batch 5 — r7-calls `e6d8f4e8`, r7-flow `e7ca1519`, r7-perf `1c204cd1`, r7-reports `20469d65`

Merged onto main `660718af` (r7-flow's lane-note conflict resolved to the
lane's copy; r7-parser still conflicted and waited for its rebase). Gate
against the batch-4 freeze: types +12 RIGHT / 0 lost / 0 missing,
diagnostics +5 cases / 0 lost / 0 missing; tests, fmt and strict clippy
green. Perf child-CPU new/old at 21 samples: domain-model 0.963,
generic-imports 0.980, diagnostics identical. r7-perf's fat-LTO `dist`
profile (ADR-0054) lands here; gates stay on `release`.

| Suite | Batch 4 | Batch 5 |
|---|---|---|
| `checker_types` | 8,736 | 8,737 |
| `checker_types_configured` | 1,771 | 1,771 |
| `diagnostics` | 4,849 | 4,852 |
| `diagnostics_configured` | 968 | 970 |

### Batch 6 — r7-calls `23550f9d`, r7-contextual `90967e42`, r7-declared `e291ff89`, r7-flow `533e29b6`, r7-parser `41c141b2`, r7-printer `eacaa674`, r7-shared `f11e37b8`

Merged onto main `20501307`. Gate against the batch-5 freeze: types +609
RIGHT / 0 lost / 0 missing, diagnostics +15 cases / 0 lost / 0 missing;
tests, fmt and strict clippy green. Perf child-CPU new/old: generic-imports
0.940 (21); domain-model read 1.057 at 21 samples and was re-run at 41
twice, 1.009 and 1.000, inside the threshold. Diagnostics identical.

| Suite | Batch 5 | Batch 6 |
|---|---|---|
| `checker_types` | 8,737 | 8,755 |
| `checker_types_configured` | 1,771 | 1,783 |
| `diagnostics` | 4,852 | 4,863 |
| `diagnostics_configured` | 970 | 972 |

Function grants: r7-calls adds the `Node::Decorator` dispatch arm in
`check.rs`; r7-parser takes `implicit_any.rs::check_implicit_any_return`,
the signatures.rs missing-body return reader and
`check.rs::check_function_or_constructor_symbol` for the missing-block
recovery; r7-declared takes `members.rs::access_member_lookup`'s
this-substitution for the mapped stack (`.1266`); r7-grammar owns
`name_slots.rs::names_in_unchecked_region`.

### Batch 7 — r7-declared `1416b385`, r7-flow `4a4b5b5b`, r7-parser `2d64b1ee`, r7-perf `c36a7706`, r7-printer `668b4795`, r7-shared `14ccd250`

Merged onto main `5f2713fe`. Gate against the batch-6 freeze: types +32
RIGHT / 0 lost / 0 missing, diagnostics +9 cases / 0 lost / 0 missing;
tests, fmt and strict clippy green. Perf child-CPU new/old at 21 samples:
domain-model 0.972, generic-imports 1.019, diagnostics identical.
r7-perf's GetProgramDiagnostics wiring reaches the CLI and the diagnostics
suite here (TS5090/TS5011/TS5069).

| Suite | Batch 6 | Batch 7 |
|---|---|---|
| `checker_types` | 8,755 | 8,765 |
| `checker_types_configured` | 1,783 | 1,783 |
| `diagnostics` | 4,863 | 4,872 |
| `diagnostics_configured` | 972 | 972 |

Function grant: r7-reports owns `signatures.rs`'s `has_no_contextual_type`
arms for TS7057.

### Batch 8 — r7-declared `4cf4796e`, r7-flow `1161ce84`, r7-reports `15995777`

Merged onto main `2c34a50e`. Gate against the batch-7 freeze: types +5
RIGHT / 0 lost / 0 missing, diagnostics +5 cases / 0 lost / 0 missing;
tests, fmt and strict clippy green. Perf child-CPU new/old at 21 samples:
domain-model 1.024, generic-imports 0.954, diagnostics identical.

| Suite | Batch 7 | Batch 8 |
|---|---|---|
| `checker_types` | 8,765 | 8,768 |
| `checker_types_configured` | 1,783 | 1,783 |
| `diagnostics` | 4,872 | 4,877 |
| `diagnostics_configured` | 972 | 972 |

Grant: r7-flow adds `Intrinsics::auto` (native `autoType`, distinct from
`anyType`) in `intrinsics.rs`, which must not escape the flow walk.

### Batch 9 — r7-calls `0b86f9d6`, r7-contextual `249f9754`, r7-declared `273e9989`, r7-flow `71f8076c`, r7-perf `4c765057`, r7-reports `a8fe82ca`

Merged onto main `0aa00136`. Gate against the batch-8 freeze: types +99
RIGHT / 0 lost / 0 missing, diagnostics +11 cases / 0 lost / 0 missing;
tests, fmt and strict clippy green. Perf child-CPU new/old at 21 samples:
domain-model 0.882, generic-imports 0.940 (mimalloc, ADR-0055), diagnostics
identical. r7-declared's union-origin fix (getUnionTypeWorker admitting a
non-union object entry) cuts jsTyping's false TS2345 74→32 and TS2339 32→1.

| Suite | Batch 8 | Batch 9 |
|---|---|---|
| `checker_types` | 8,768 | 8,782 |
| `checker_types_configured` | 1,783 | 1,784 |
| `diagnostics` | 4,877 | 4,888 |
| `diagnostics_configured` | 972 | 972 |

TSR/tsgo-pinned wall · CPU after batch 9 (r7-perf, 21 samples, `dist`):
domain-model 0.540 · 0.416, domain-model-large 0.716 · 0.479,
generic-imports 0.598 · 0.303. jsTyping is not equivalent work yet (259 vs
86 diagnostics).

### Batch 10 — r7-declared `70961797`, r7-grammar `f3e6979b`, r7-perf `42f3f672`, r7-printer `2adaf373`

Merged onto main `92fe8f05`, plus a fmt fix to r7-perf's
`front_end_ceiling` example. Gate against the batch-9 freeze: types +59
RIGHT / 0 lost / 0 missing, diagnostics +16 cases / 0 lost / 0 missing;
tests and strict clippy green. Perf child-CPU new/old: domain-model 0.993
(21); generic-imports 1.099 at 21 samples, re-run at 41 twice, 1.021 and
1.012, inside the threshold. Diagnostics identical. `tsr-2zk.1258`'s two
multi-checker drops are fixed (r7-grammar 375afb66).

| Suite | Batch 9 | Batch 10 |
|---|---|---|
| `checker_types` | 8,782 | 8,797 |
| `checker_types_configured` | 1,784 | 1,787 |
| `diagnostics` | 4,888 | 4,895 |
| `diagnostics_configured` | 972 | 981 |

Grants: r7-grammar owns `delete_operand.rs` and `member_completeness.rs`,
and `check.rs`'s interface-extends type-name arm and
`check_value_identifier`'s TS2301/TS2844 choice.

### Batch 11 — r7-calls `f2f6696f`, r7-contextual `e5e0494b`, r7-declared `f853238f`

Merged onto main `0f7a1165`. Gate against the batch-10 freeze: types +10
RIGHT / 0 lost / 0 missing, diagnostics +1 case / 0 lost / 0 missing; tests,
fmt and strict clippy green. Perf child-CPU new/old read 1.037/1.063 at 21
samples and 0.984/0.992 at 41 (domain-model/generic-imports), diagnostics
identical. r7-calls drops its const-type-parameter decline on top of
r7-contextual's const-literal markers.

| Suite | Batch 10 | Batch 11 |
|---|---|---|
| `checker_types` | 8,797 | 8,799 |
| `checker_types_configured` | 1,787 | 1,787 |
| `diagnostics` | 4,895 | 4,896 |
| `diagnostics_configured` | 981 | 981 |

### Batch 12 — r7-calls `5937dd79`, r7-contextual `63a147ac`, r7-declared `db7f69d6`, r7-parser `c83bb41b`, r7-printer `deda2594` (cherry-picked)

The first candidate carried r7-calls' spread-argument report (`ea1a1286`).
The corpus compare passed, but a new jsTyping check found three false TS2345
that tsgo does not report: checker.ts 9968/9981/51482. These were type-road
defects that the report exposed. The batch was cut back to `5937dd79` and
re-gated. From this batch on, the gate also diffs jsTyping's unique error
lines against `target/tsgo-pinned`, and refuses on `new_false` or
`lost_true` > 0.

Re-gated against the batch-11 freeze: types +70 RIGHT / 0 lost / 0 missing,
diagnostics +23 cases / 0 lost / 0 missing; jsTyping 141 → 127 unique error
lines (tsgo 86), new_false 0, lost_true 0; tests, fmt and strict clippy
green. Perf child-CPU new/old: domain-model 1.000. generic-imports read
1.080 at 21 samples and 0.963/0.949 at 41 against batch 11. Against batch 9
it reads 1.035/1.075, so a cumulative generic-imports drift since batch 9
has gone to r7-perf to attribute.

| Suite | Batch 11 | Batch 12 |
|---|---|---|
| `checker_types` | 8,799 | 8,807 |
| `checker_types_configured` | 1,787 | 1,788 |
| `diagnostics` | 4,896 | 4,908 |
| `diagnostics_configured` | 981 | 991 |

### Batch 13 — r7-contextual `5c037c96`, r7-declared `8d4c10cc`, r7-flow `ec55197d`, r7-grammar `4f331d0c` minus `1055c38c`, r7-reports `a38c2ffc`, r7-shared `2fd5c9f4`

The first candidate's compare refused two diagnostics losses,
errorLocationForInterfaceExtension and interfacedeclWithIndexerErrors
(RIGHT→WRONG). r7-grammar's `1055c38c` checks an interface's extends element
as a type reference and adds a TS2552 beside native's lone TS2840. That
commit was reverted on the integration pin and returned to the lane. The
batch was then re-gated against the batch-12 freeze: types +67 RIGHT / 0 lost /
0 missing, diagnostics +34 cases / 0 lost / 0 missing; jsTyping new_false 0,
lost_true 0; tests, fmt and strict clippy green. Perf child-CPU new/old at
21 samples: domain-model 1.022, generic-imports 0.973. Callgrind Ir
single-threaded: generic-imports 1.0001, domain-model 1.0011.

**Gate change:** r7-perf showed that the median child CPU swings about 13%
between binaries with identical instruction counts on this orb (the "batch
9→12 generic-imports creep" was host noise: Ir −0.08% end to end). From batch
14 on, callgrind Ir on generic-imports and domain-model (`--singleThreaded`)
is the deterministic hot-path guard; the timing check stays as a secondary
signal. `.agents/setup` installs valgrind.

| Suite | Batch 12 | Batch 13 |
|---|---|---|
| `checker_types` | 8,807 | 8,811 |
| `checker_types_configured` | 1,788 | 1,796 |
| `diagnostics` | 4,908 | 4,927 |
| `diagnostics_configured` | 991 | 1,003 |

### Batch 14 — r7-declared `a1e71942`, r7-perf `2c69a228` (docs)

The first candidate also carried r7-calls `3ccbce20`. The corpus compare passed,
but the jsTyping leg found one new line that tsgo does not report
(parser.ts 2634 TS2352). The cause is the createToken fix exposing a relater
gap (`T["kind"]` against its constraint), routed to r7-reports. Re-gated without
calls against the batch-13 freeze: types +5 RIGHT / 0 lost / 0 missing,
diagnostics 0 / 0 lost / 0 missing; jsTyping new_false 0, lost_true 0;
tests, fmt, strict clippy green; callgrind Ir generic-imports 1.0001,
domain-model 0.9999 (median timing read 0.987/1.227 and is noise on this
host, as batch 13 recorded). All suites: no failure count rose.

| Suite | Batch 13 | Batch 14 |
|---|---|---|
| `checker_types` | 8,811 | 8,812 |
| `checker_types_configured` | 1,796 | 1,796 |
| `diagnostics` | 4,927 | 4,927 |
| `diagnostics_configured` | 1,003 | 1,003 |

Cutover decision (`tsr-2zk.16.56.1.4`): r7-shared measured the rejected
cutover in its own era. Between `db726c9c` and the recovery head it gains
+587 and loses −94 type lines; its diagnostics dump is OOM-killed at 7 GB.
Against current main, 410 of the 587 gains are already RIGHT, 177 remain
(about 40 cases), and all 94 of its losses would be main losses. Squashing it
onto main conflicts in 27 files (about 90 hunks), and its first commit also
conflicts semantically with main's `(SymbolRef, kind)` interface-signature
cache. **Rebase-and-repair is retired in favour of a harvest.** The branch
stays as the record and is never merged. r7-shared keeps the harvest list (its
lane note §C) and the five cutover unit tests as behaviour specs. Each
still-missing behaviour is re-ported from native in the lane that owns its
file, through the normal gate.

### Batch 15 — r7-calls `9c79de88`, r7-contextual `fb76bab8`, r7-flow `71db0715`, r7-printer `1e0f8ac7`, r7-shared `57c4920a`

Merged onto main `9db14f99`. Gate against the batch-14 freeze: types +94
RIGHT / 0 lost / 0 missing, diagnostics +7 cases / 0 lost / 0 missing;
jsTyping 127 → 126 unique error lines, new_false 0, lost_true 0; tests,
fmt, strict clippy green; callgrind Ir generic-imports 1.0000,
domain-model 1.0001. No suite's failure count rose. r7-flow's autoType
identity (`Intrinsics::auto`) and `identifier_flow_type_is_auto` land here.

| Suite | Batch 14 | Batch 15 |
|---|---|---|
| `checker_types` | 8,812 | 8,817 |
| `checker_types_configured` | 1,796 | 1,818 |
| `diagnostics` | 4,927 | 4,934 |
| `diagnostics_configured` | 1,003 | 1,003 |

Grants: r7-declared adds the variadic/rest tuple arm to `members.rs`
`get_type_of_property_with_this_argument`'s Array-base fallback
(getTupleBaseType). r7-printer applies r7-shared's `resolve_symbol` diff,
including its `symbols.rs` definition.

### Batch 16 — r7-calls `0ff376e0`, r7-declared `77418645`, r7-parser `1482cfad`, r7-printer `ef891f87`, r7-reports `ac5dba13`

Merged onto main `4bdd575d`. r7-parser's nine-commit stack lands with
r7-printer's three round-trip fixes (mapped-type members, the `?.` of a
tagged template, object-literal member modifiers). Without the fixes, the
native trees cost printer_round_trip two passing cases. Gate against the
batch-15 freeze: types +106 RIGHT / 0 lost / 0 missing, diagnostics +16
cases / 0 lost / 0 missing; jsTyping new_false 0, lost_true 0; tests, fmt,
strict clippy green. Callgrind Ir: generic-imports 1.0019, domain-model
1.0008 (parser recovery work). printer_round_trip 11,804/11,815 →
11,807/11,816 (failures 11 → 9), and no suite's failure count rose.

| Suite | Batch 15 | Batch 16 |
|---|---|---|
| `checker_types` | 8,817 | 8,825 |
| `checker_types_configured` | 1,818 | 1,818 |
| `diagnostics` | 4,934 | 4,949 |
| `diagnostics_configured` | 1,003 | 1,003 |

### Batch 17 — r7-calls `b8168c30`, r7-contextual `2abb9ccd`, r7-flow `3d2d17f8`, r7-grammar `f562b3f8`, r7-perf `804c649f`

Every lane went idle for about 18 hours after a session interruption. The
first gate run was cut off part-way and re-run on resumption. Merged onto
main `4a5c81bf`, plus a fmt fix in `strict_mode.rs`. Gate against the
batch-16 freeze: types +39 RIGHT / 0 lost / 0 missing, diagnostics +11
cases / 0 lost / 0 missing; jsTyping 126 → 125 unique lines, new_false 0,
lost_true 0; tests and strict clippy green. Callgrind Ir: generic-imports
0.9995, **domain-model 0.9600**. That is r7-perf's lazy object-literal member
text: native prints only on error paths, and the CLI-message audit is in
r7-perf.md. No suite's failure count rose. The gate scripts are now in
`scripts/integration/`.

| Suite | Batch 16 | Batch 17 |
|---|---|---|
| `checker_types` | 8,825 | 8,831 |
| `checker_types_configured` | 1,818 | 1,818 |
| `diagnostics` | 4,949 | 4,958 |
| `diagnostics_configured` | 1,003 | 1,005 |

### Batch 18 — r7-declared `5e8714ed`, r7-parser `529fa4ba`, r7-printer `c228f3a7`, r7-shared `4caebf2b`

Merged onto main `9acf69d7`. Gate against the batch-17 freeze: types +43
RIGHT / 0 lost / 0 missing, diagnostics +10 cases / 0 lost / 0 missing;
jsTyping 125/125, new_false 0, lost_true 0; tests, fmt, strict clippy green;
callgrind Ir generic-imports 0.9999, domain-model 1.0002. No suite's failure
count rose.

| Suite | Batch 17 | Batch 18 |
|---|---|---|
| `checker_types` | 8,831 | 8,839 |
| `checker_types_configured` | 1,818 | 1,818 |
| `diagnostics` | 4,958 | 4,968 |
| `diagnostics_configured` | 1,005 | 1,005 |

**Exact native oracle baseline.** The full-output oracle (`parity_gate.sh
oracle-native` / `oracle-tsr`, docs/parity/notes/oracle.md) compares
messages, chains, related information, spans, order and type text. It reads
**9,554 / 12,797 EXACT (74.66%)** on batch-17 main `9acf69d7`, up from 8,303
at the round-6 owner continuation. It becomes a gate leg (`exact-candidate`,
no EXACT loss) once its run fits inside a session. Session interruptions
killed long background jobs three times today, so gates now run as
supervised orb services (`scripts/integration/svcgate.sh`).

### Batch 19 — r7-contextual `2bd6ce94`, r7-declared `9b77d498`, r7-grammar `71d23989`, r7-perf `246ff8b6`, r7-printer `7d71f6ba`, r7-reports `6e683aec`

Merged onto main `ee67a1d5` (r7-calls conflicted and was returned for a rebase).
This is the first batch gated by the exact native oracle as well: **9,554 → 9,582 EXACT
(74.88%), 28 gained, 0 lost, 0 missing** against the batch-17 report. The
legacy compare against the batch-18 freeze: types +30 RIGHT / 0 lost / 0
missing, diagnostics +8 cases / 0 lost / 0 missing; jsTyping 125/125,
new_false 0, lost_true 0; tests, fmt, strict clippy green; callgrind Ir
generic-imports 1.0001, domain-model 1.0029 (r7-reports' TS7034/TS7005
auto reporter walks flow once per auto-typed read, as native's
checkIdentifier does). No suite's failure count rose.

| Suite | Batch 18 | Batch 19 |
|---|---|---|
| `checker_types` | 8,839 | 8,845 |
| `checker_types_configured` | 1,818 | 1,818 |
| `diagnostics` | 4,968 | 4,976 |
| `diagnostics_configured` | 1,005 | 1,005 |

### Batch 20 — r7-calls `f58c43de`, r7-contextual `cb00835f`, r7-declared `b66cdba0`, r7-flow `52536a40`, r7-perf `2dff7977`, r7-printer `ec650c97`

Merged onto main `41f3c70b`. Exact oracle 9,582 → **9,591 EXACT (74.95%)**,
0 lost, 0 missing; legacy compare against the batch-19 freeze: types +20
RIGHT / 0 lost / 0 missing, diagnostics +10 cases / 0 lost / 0 missing;
jsTyping 125/125, new_false 0, lost_true 0; tests, fmt, strict clippy green;
callgrind Ir generic-imports 0.9999, domain-model 1.0006. No suite's failure
count rose.

| Suite | Batch 19 | Batch 20 |
|---|---|---|
| `checker_types` | 8,845 | 8,851 |
| `checker_types_configured` | 1,818 | 1,820 |
| `diagnostics` | 4,976 | 4,984 |
| `diagnostics_configured` | 1,005 | 1,005 |

**Integration loop.** From batch 21 on, `scripts/integration/autoloop.sh`
runs the gate continuously as a supervised orb service, so integration keeps
going through session interruptions. It lands a batch only when every leg
is green: legacy compare, exact oracle, jsTyping, clippy, tests, callgrind Ir
≤ 1.005, and the all-suite failure count. Anything else blocks the loop for
the integration owner to cut back or route. Records it writes are marked as
loop-landed.

### Batch 21 — r7-grammar `6f2dad2d`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9597 / 12797 EXACT (74.99%)**, 6 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +0 RIGHT / 0 lost / 0 missing, diagnostics +6 cases / 0 lost / 0 missing. jsTyping 125 → 125 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 0.9999. No suite's failure count rose.

| Suite | Before | Batch 21 |
|---|---|---|
| `checker_types` | 8,851 | 8,851 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 4,984 | 4,988 |
| `diagnostics_configured` | 1,005 | 1,007 |

### Batch 22 — r7-flow `bc591f33`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9597 / 12797 EXACT (74.99%)**, 0 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +0 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 125 → 125 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 0.9999. No suite's failure count rose.

| Suite | Before | Batch 22 |
|---|---|---|
| `checker_types` | 8,851 | 8,851 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 4,988 | 4,988 |
| `diagnostics_configured` | 1,007 | 1,007 |

### Batch 23 — r7-parser `a9390a61`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9600 / 12797 EXACT (75.02%)**, 3 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +126 RIGHT / 0 lost / 0 missing, diagnostics +5 cases / 0 lost / 0 missing. jsTyping 125 → 125 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0021, domain-model 1.0008. No suite's failure count rose.

| Suite | Before | Batch 23 |
|---|---|---|
| `checker_types` | 8,851 | 8,857 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 4,988 | 4,991 |
| `diagnostics_configured` | 1,007 | 1,009 |

The loop blocked this batch on the all-suite check: printer_round_trip failures
went 9 → 10. The new failure is conformance/instantiationExpressions, which the
parser now parses cleanly as native does, so the round trip judges it for the
first time; it was skipped before, not passing. The tree printer drops an
instantiation expression's type arguments. That is routed to r7-printer.
No previously passing case was lost, and the integration owner landed the
batch by hand. The loop's suite check now counts passes lost beyond the
judged-population change, not raw failure counts.

### Batch 24 — r7-contextual `31bc0da8`, r7-flow `69ce9bb0`, r7-grammar `b3883acb`, r7-reports `e53a77eb`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9605 / 12797 EXACT (75.06%)**, 5 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +8 RIGHT / 0 lost / 0 missing, diagnostics +11 cases / 0 lost / 0 missing. jsTyping 125 → 123 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0002, domain-model 1.0010. No suite's failure count rose.

| Suite | Before | Batch 24 |
|---|---|---|
| `checker_types` | 8,857 | 8,858 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 4,991 | 4,997 |
| `diagnostics_configured` | 1,009 | 1,009 |

The loop's first run of this batch was OOM-killed in the exact-oracle leg (4 workers on the 7 GB orb); the leg now runs on 2 workers and clears a stale report directory before it starts.

### Batch 25 — r7-declared `b414e1af`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9607 / 12797 EXACT (75.07%)**, 2 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +13 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 123 → 123 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0004. No suite's failure count rose.

| Suite | Before | Batch 25 |
|---|---|---|
| `checker_types` | 8,858 | 8,861 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 4,997 | 4,997 |
| `diagnostics_configured` | 1,009 | 1,009 |

### Batch 26 — r7-parser `604ce809`, r7-printer `8807cfe1`, r7-shared `4a75de04`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9625 / 12797 EXACT (75.21%)**, 18 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +71 RIGHT / 0 lost / 0 missing, diagnostics +6 cases / 0 lost / 0 missing. jsTyping 123 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0011, domain-model 1.0004. No suite's failure count rose.

| Suite | Before | Batch 26 |
|---|---|---|
| `checker_types` | 8,861 | 8,879 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 4,997 | 5,002 |
| `diagnostics_configured` | 1,009 | 1,010 |

### Batch 27 — r7-declared `cf8bae56`, r7-perf `8af171fc`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9625 / 12797 EXACT (75.21%)**, 0 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +2 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 27 |
|---|---|---|
| `checker_types` | 8,879 | 8,880 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 5,002 | 5,002 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 28 — r7-perf `7631c8d8`, r7-printer `e17ee095`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9627 / 12797 EXACT (75.23%)**, 2 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +7 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 0.9998. No suite's failure count rose.

| Suite | Before | Batch 28 |
|---|---|---|
| `checker_types` | 8,880 | 8,882 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 5,002 | 5,002 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 29 — r7-calls `34a165f0`, r7-printer `f76c6eff`, r7-shared `5db1a558`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9639 / 12797 EXACT (75.32%)**, 12 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +58 RIGHT / 0 lost / 0 missing, diagnostics +12 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0002. No suite's failure count rose.

| Suite | Before | Batch 29 |
|---|---|---|
| `checker_types` | 8,882 | 8,894 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 5,002 | 5,014 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 30 — r7-contextual `ddac14d0`, r7-printer `536991f4`, r7-reports `123ddc39`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9643 / 12797 EXACT (75.35%)**, 4 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +7 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0002. No suite's failure count rose.

| Suite | Before | Batch 30 |
|---|---|---|
| `checker_types` | 8,894 | 8,897 |
| `checker_types_configured` | 1,820 | 1,820 |
| `diagnostics` | 5,014 | 5,017 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 31 — r7-parser `be9683bd`, r7-printer `f2cf696f`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9651 / 12797 EXACT (75.42%)**, 8 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +9 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9998, domain-model 0.9999. No suite's failure count rose.

| Suite | Before | Batch 31 |
|---|---|---|
| `checker_types` | 8,897 | 8,899 |
| `checker_types_configured` | 1,820 | 1,821 |
| `diagnostics` | 5,017 | 5,017 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 32 — r7-calls `68ef70c0`, r7-parser `a83e1dfc`, r7-shared `051eea36`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9654 / 12797 EXACT (75.44%)**, 3 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +36 RIGHT / 0 lost / 0 missing, diagnostics +6 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.8523, domain-model 0.9640. No suite's failure count rose.

| Suite | Before | Batch 32 |
|---|---|---|
| `checker_types` | 8,899 | 8,901 |
| `checker_types_configured` | 1,821 | 1,822 |
| `diagnostics` | 5,017 | 5,023 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 33 — r7-grammar `0714e1a1`, r7-parser `ab5d2c18`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9657 / 12797 EXACT (75.46%)**, 3 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +0 RIGHT / 0 lost / 0 missing, diagnostics +3 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0012, domain-model 1.0015. No suite's failure count rose.

| Suite | Before | Batch 33 |
|---|---|---|
| `checker_types` | 8,901 | 8,901 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,023 | 5,026 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 34 — r7-flow `b49d7113`, r7-reports `60f87520`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9657 / 12797 EXACT (75.46%)**, 0 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +2 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0003. No suite's failure count rose.

| Suite | Before | Batch 34 |
|---|---|---|
| `checker_types` | 8,901 | 8,902 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,026 | 5,028 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 35 — r7-contextual `a470d228`, r7-shared `00ea0e17`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9659 / 12797 EXACT (75.48%)**, 2 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +17 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 0.9669. No suite's failure count rose.

| Suite | Before | Batch 35 |
|---|---|---|
| `checker_types` | 8,902 | 8,906 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,028 | 5,030 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 36 — r7-contextual `c16fbfd1`, r7-reports `6ad2679f`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9665 / 12797 EXACT (75.53%)**, 6 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +53 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 36 |
|---|---|---|
| `checker_types` | 8,906 | 8,916 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,030 | 5,032 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 37 — r7-calls `74503679`, r7-parser `09538483`, r7-shared `fd187adc`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9667 / 12797 EXACT (75.54%)**, 2 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +3 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 37 |
|---|---|---|
| `checker_types` | 8,916 | 8,917 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,032 | 5,032 |
| `diagnostics_configured` | 1,010 | 1,010 |

### Batch 38 — r7-contextual `ef468c68`, r7-grammar `0ee7a7a9`, r7-printer `46ba242c`, r7-reports `1a83b728`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9671 / 12797 EXACT (75.57%)**, 4 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +2 RIGHT / 0 lost / 0 missing, diagnostics +9 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0002, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 38 |
|---|---|---|
| `checker_types` | 8,917 | 8,918 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,032 | 5,037 |
| `diagnostics_configured` | 1,010 | 1,014 |

### Batch 39 — r7-contextual `5f6b27a4`, r7-flow `ad1d0093`, r7-grammar `a62f6779`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9727 / 12797 EXACT (76.01%)**, 56 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +2 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0004. No suite's failure count rose.

| Suite | Before | Batch 39 |
|---|---|---|
| `checker_types` | 8,918 | 8,919 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,037 | 5,037 |
| `diagnostics_configured` | 1,014 | 1,014 |

### Batch 40 — r7-calls `3cfd38a7`, r7-declared `13710bd9`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9732 / 12797 EXACT (76.05%)**, 5 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +96 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0006. No suite's failure count rose.

| Suite | Before | Batch 40 |
|---|---|---|
| `checker_types` | 8,919 | 8,928 |
| `checker_types_configured` | 1,822 | 1,822 |
| `diagnostics` | 5,037 | 5,039 |
| `diagnostics_configured` | 1,014 | 1,014 |

### Batch 41 — r7-declared `070fe065`, r7-flow `379e4ed5`, r7-grammar `4cd1fbf1`, r7-perf `85a5cd36`, r7-printer `bc416782`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9747 / 12797 EXACT (76.17%)**, 15 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +58 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 41 |
|---|---|---|
| `checker_types` | 8,928 | 8,940 |
| `checker_types_configured` | 1,822 | 1,827 |
| `diagnostics` | 5,039 | 5,039 |
| `diagnostics_configured` | 1,014 | 1,014 |

### Batch 42 — r7-contextual `e61f57a7`, r7-declared `6dc72ec9`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9747 / 12797 EXACT (76.17%)**, 0 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +3 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 42 |
|---|---|---|
| `checker_types` | 8,940 | 8,941 |
| `checker_types_configured` | 1,827 | 1,827 |
| `diagnostics` | 5,039 | 5,039 |
| `diagnostics_configured` | 1,014 | 1,014 |

### Batch 43 — r7-contextual `5a3b12de`, r7-declared `24d86553`, r7-reports `433b0fad`, r7-shared `f693bb13`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9752 / 12797 EXACT (76.21%)**, 5 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +20 RIGHT / 0 lost / 0 missing, diagnostics +5 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0010. No suite's failure count rose.

| Suite | Before | Batch 43 |
|---|---|---|
| `checker_types` | 8,941 | 8,947 |
| `checker_types_configured` | 1,827 | 1,829 |
| `diagnostics` | 5,039 | 5,044 |
| `diagnostics_configured` | 1,014 | 1,014 |

### Batch 44 — r7-flow `0a4150b0`, r7-parser `31d21945`, r7-printer `1f183c83`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9761 / 12797 EXACT (76.28%)**, 9 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +9 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0003, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 44 |
|---|---|---|
| `checker_types` | 8,947 | 8,952 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,044 | 5,046 |
| `diagnostics_configured` | 1,014 | 1,016 |

### Batch 45 — r7-flow `ef61f7d9`, r7-grammar `c91f027e`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9813 / 12797 EXACT (76.68%)**, 52 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +2 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 45 |
|---|---|---|
| `checker_types` | 8,952 | 8,953 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,046 | 5,046 |
| `diagnostics_configured` | 1,016 | 1,016 |

### Batch 46 — r7-calls `432b94e3`, r7-contextual `76096764`, r7-flow `e5de971c`, r7-grammar `f32a7263`, r7-shared `a653479c`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **9884 / 12797 EXACT (77.24%)**, 71 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +27 RIGHT / 0 lost / 0 missing, diagnostics +6 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 46 |
|---|---|---|
| `checker_types` | 8,953 | 8,956 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,046 | 5,052 |
| `diagnostics_configured` | 1,016 | 1,016 |

### Batch 47 — r7-calls `9092f916`, r7-contextual `9ea35bb4`, r7-declared `007e770e`, r7-flow `57d327e6`, r7-grammar `f576b775`, r7-printer `d827ab62`, r7-reports `22861482`, r7-shared `ff310002`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10185 / 12797 EXACT (79.59%)**, 301 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +39 RIGHT / 0 lost / 0 missing, diagnostics +9 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0011. No suite's failure count rose.

| Suite | Before | Batch 47 |
|---|---|---|
| `checker_types` | 8,956 | 8,962 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,052 | 5,059 |
| `diagnostics_configured` | 1,016 | 1,018 |

**Batch 47 note (integration owner).** The loop's gate was OOM-killed four
times in the exact-oracle leg. The memory logger the gate now carries
(`mem.log`) showed TSR's serve worker growing past 4.5 GB on one case,
compiler/recursiveConditionalCrash3. It was a plain TSR_TIMEOUT up to batch 46
and allocates much faster with batch 47's merge set. The oracle leg runs on one
worker under `ulimit -v 4500000`, so such a case reads TSR_FAILED instead of
killing the orb. It was never EXACT, so this is not a parity loss, but it is a
resource regression, routed to r7-perf to attribute and to port native's
instantiation-depth/count limits.

### Batch 48 — r7-calls `2bc62c1c`, r7-contextual `ec9ebcaf`, r7-declared `58a511f1`, r7-shared `ee179acf`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10295 / 12797 EXACT (80.45%)**, 110 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +8 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 48 |
|---|---|---|
| `checker_types` | 8,962 | 8,963 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,059 | 5,062 |
| `diagnostics_configured` | 1,018 | 1,019 |

### Batch 49 — r7-contextual `2e750c0f`, r7-declared `4cedacf1`, r7-flow `08c242f7`, r7-reports `040ceb8b`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10367 / 12797 EXACT (81.01%)**, 72 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +4 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0002, domain-model 1.0004. No suite's failure count rose.

| Suite | Before | Batch 49 |
|---|---|---|
| `checker_types` | 8,963 | 8,964 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,062 | 5,065 |
| `diagnostics_configured` | 1,019 | 1,020 |

### Batch 50 — r7-flow `97cc68fc`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10367 / 12797 EXACT (81.01%)**, 0 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +0 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 0.9999. No suite's failure count rose.

| Suite | Before | Batch 50 |
|---|---|---|
| `checker_types` | 8,964 | 8,964 |
| `checker_types_configured` | 1,829 | 1,829 |
| `diagnostics` | 5,065 | 5,065 |
| `diagnostics_configured` | 1,020 | 1,020 |

### Batch 51 — r7-contextual `b3f3b8ec`, r7-flow `6efc0ae6`, r7-parser `6008b74d`, r7-reports `4926dd11`, r7-shared `5139f5a4`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10458 / 12797 EXACT (81.72%)**, 91 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +80 RIGHT / 0 lost / 0 missing, diagnostics +1 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 0.9999. No suite's failure count rose.

| Suite | Before | Batch 51 |
|---|---|---|
| `checker_types` | 8,964 | 8,975 |
| `checker_types_configured` | 1,829 | 1,830 |
| `diagnostics` | 5,065 | 5,066 |
| `diagnostics_configured` | 1,020 | 1,020 |

### Batch 52 — r7-flow `a59e39d6`, r7-grammar `4068b308`, r7-printer `ef9f5a01`, r7-reports `ec17ba88`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10540 / 12797 EXACT (82.36%)**, 82 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +10 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 122 → 122 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 0.9999. No suite's failure count rose.

| Suite | Before | Batch 52 |
|---|---|---|
| `checker_types` | 8,975 | 8,978 |
| `checker_types_configured` | 1,830 | 1,830 |
| `diagnostics` | 5,066 | 5,066 |
| `diagnostics_configured` | 1,020 | 1,020 |

### Batch 53 — r7-calls `7e68143c`, r7-contextual `8eac2773`, r7-flow `5aed9899`, r7-grammar `dd7de8bb`, r7-shared `6fefc4d1`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10561 / 12797 EXACT (82.53%)**, 21 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +23 RIGHT / 0 lost / 0 missing, diagnostics +6 cases / 0 lost / 0 missing. jsTyping 122 → 117 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 53 |
|---|---|---|
| `checker_types` | 8,978 | 8,981 |
| `checker_types_configured` | 1,830 | 1,830 |
| `diagnostics` | 5,066 | 5,070 |
| `diagnostics_configured` | 1,020 | 1,021 |

### Batch 54 — r7-contextual `963a0c47`, r7-flow `4c94490e`, r7-grammar `5c3f8e90`, r7-printer `fffe03f4`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10573 / 12797 EXACT (82.62%)**, 12 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +18 RIGHT / 0 lost / 0 missing, diagnostics +1 cases / 0 lost / 0 missing. jsTyping 117 → 114 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 54 |
|---|---|---|
| `checker_types` | 8,981 | 8,986 |
| `checker_types_configured` | 1,830 | 1,830 |
| `diagnostics` | 5,070 | 5,071 |
| `diagnostics_configured` | 1,021 | 1,021 |

### Batch 55 — r7-flow `ead5eb83`, r7-grammar `76403572`, r7-parser `a59c11b0`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10595 / 12797 EXACT (82.79%)**, 22 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +17 RIGHT / 0 lost / 0 missing, diagnostics +1 cases / 0 lost / 0 missing. jsTyping 114 → 111 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 55 |
|---|---|---|
| `checker_types` | 8,986 | 8,988 |
| `checker_types_configured` | 1,830 | 1,830 |
| `diagnostics` | 5,071 | 5,071 |
| `diagnostics_configured` | 1,021 | 1,022 |

### Batch 56 — r7-calls `01e05828`, r7-contextual `3c812a3e`, r7-flow `7005ed3f`, r7-perf `77f37379`, r7-printer `94456ba1`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10609 / 12797 EXACT (82.90%)**, 14 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +82 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 111 → 111 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0005. No suite's failure count rose.

| Suite | Before | Batch 56 |
|---|---|---|
| `checker_types` | 8,988 | 8,993 |
| `checker_types_configured` | 1,830 | 1,838 |
| `diagnostics` | 5,071 | 5,073 |
| `diagnostics_configured` | 1,022 | 1,022 |

### Batch 57 — r7-contextual `78049d0f`, r7-declared `d87e190a`, r7-flow `c1a10e20`, r7-grammar `cee33e03`, r7-parser `c2b02ae8`, r7-shared `acdb1069`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10635 / 12797 EXACT (83.11%)**, 26 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +84 RIGHT / 0 lost / 0 missing, diagnostics +7 cases / 0 lost / 0 missing. jsTyping 111 → 110 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9995, domain-model 1.0014. No suite's failure count rose.

| Suite | Before | Batch 57 |
|---|---|---|
| `checker_types` | 8,993 | 9,002 |
| `checker_types_configured` | 1,838 | 1,842 |
| `diagnostics` | 5,073 | 5,077 |
| `diagnostics_configured` | 1,022 | 1,024 |

### Batch 58 — r7-flow `954817a5`, r7-parser `74cd57bb`, r7-perf `018bbdb0`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10636 / 12797 EXACT (83.11%)**, 1 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +1 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 110 → 110 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9994, domain-model 0.9996. No suite's failure count rose.

| Suite | Before | Batch 58 |
|---|---|---|
| `checker_types` | 9,002 | 9,003 |
| `checker_types_configured` | 1,842 | 1,842 |
| `diagnostics` | 5,077 | 5,077 |
| `diagnostics_configured` | 1,024 | 1,024 |

### Batch 59 — r7-contextual `ef4c6ea0`, r7-declared `8d89f0ed`, r7-flow `34772dd8`, r7-grammar `efe4a325`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10643 / 12797 EXACT (83.17%)**, 7 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +1 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 110 → 110 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 59 |
|---|---|---|
| `checker_types` | 9,003 | 9,003 |
| `checker_types_configured` | 1,842 | 1,842 |
| `diagnostics` | 5,077 | 5,077 |
| `diagnostics_configured` | 1,024 | 1,024 |

### Batch 60 — r7-calls `738dbaf8`, r7-grammar `7412fbb1`, r7-perf `ec1b4be5`, r7-shared `292b5f18`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10653 / 12797 EXACT (83.25%)**, 10 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +8 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 110 → 110 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0002. No suite's failure count rose.

| Suite | Before | Batch 60 |
|---|---|---|
| `checker_types` | 9,003 | 9,006 |
| `checker_types_configured` | 1,842 | 1,843 |
| `diagnostics` | 5,077 | 5,081 |
| `diagnostics_configured` | 1,024 | 1,024 |

### Batch 61 — r7-reports `d4f95651`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10775 / 12797 EXACT (84.20%)**, 122 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +0 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 110 → 110 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 61 |
|---|---|---|
| `checker_types` | 9,006 | 9,006 |
| `checker_types_configured` | 1,843 | 1,843 |
| `diagnostics` | 5,081 | 5,081 |
| `diagnostics_configured` | 1,024 | 1,024 |

### Batch 62 — r7-grammar `e50e75c2`, r7-perf `9dfda8e2`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10811 / 12797 EXACT (84.48%)**, 36 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +0 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 110 → 110 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 0.9990. No suite's failure count rose.

| Suite | Before | Batch 62 |
|---|---|---|
| `checker_types` | 9,006 | 9,006 |
| `checker_types_configured` | 1,843 | 1,843 |
| `diagnostics` | 5,081 | 5,083 |
| `diagnostics_configured` | 1,024 | 1,026 |

### Batch 63 — r7-contextual `935e0b47`, r7-declared `0b6e1218`, r7-grammar `ac98c6a7`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10815 / 12797 EXACT (84.51%)**, 4 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +38 RIGHT / 0 lost / 0 missing, diagnostics +3 cases / 0 lost / 0 missing. jsTyping 110 → 107 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0007. No suite's failure count rose.

| Suite | Before | Batch 63 |
|---|---|---|
| `checker_types` | 9,006 | 9,011 |
| `checker_types_configured` | 1,843 | 1,843 |
| `diagnostics` | 5,083 | 5,085 |
| `diagnostics_configured` | 1,026 | 1,026 |

### Batch 64 — r7-calls `492ef168`, r7-contextual `16199eb1`, r7-flow `fbb108f7`, r7-grammar `0eb482de`, r7-parser `73b17c5f`, r7-perf `359a20af`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10841 / 12797 EXACT (84.72%)**, 26 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +17 RIGHT / 0 lost / 0 missing, diagnostics +12 cases / 0 lost / 0 missing. jsTyping 107 → 107 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9982, domain-model 0.9245. No suite's failure count rose.

| Suite | Before | Batch 64 |
|---|---|---|
| `checker_types` | 9,011 | 9,011 |
| `checker_types_configured` | 1,843 | 1,843 |
| `diagnostics` | 5,085 | 5,096 |
| `diagnostics_configured` | 1,026 | 1,027 |

### Batch 65 — r7-calls `f3c2444e`, r7-contextual `9120a2d2`, r7-perf `954f0690`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10855 / 12797 EXACT (84.82%)**, 14 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +24 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 107 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0002, domain-model 1.0002. No suite's failure count rose.

| Suite | Before | Batch 65 |
|---|---|---|
| `checker_types` | 9,011 | 9,014 |
| `checker_types_configured` | 1,843 | 1,843 |
| `diagnostics` | 5,096 | 5,096 |
| `diagnostics_configured` | 1,027 | 1,027 |

### Batch 66 — r7-contextual `288a4eea`, r7-flow `45fdd504`, r7-grammar `9968aae6`, r7-printer `ad44d4a6`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10925 / 12797 EXACT (85.37%)**, 70 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +44 RIGHT / 0 lost / 0 missing, diagnostics +11 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0020. No suite's failure count rose.

| Suite | Before | Batch 66 |
|---|---|---|
| `checker_types` | 9,014 | 9,027 |
| `checker_types_configured` | 1,843 | 1,847 |
| `diagnostics` | 5,096 | 5,101 |
| `diagnostics_configured` | 1,027 | 1,033 |

### Batch 67 — r7-contextual `5ac7c73c`, r7-grammar `34b98bac`, r7-printer `2a03f0b7`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10928 / 12797 EXACT (85.40%)**, 3 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +5 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0023. No suite's failure count rose.

| Suite | Before | Batch 67 |
|---|---|---|
| `checker_types` | 9,027 | 9,029 |
| `checker_types_configured` | 1,847 | 1,847 |
| `diagnostics` | 5,101 | 5,101 |
| `diagnostics_configured` | 1,033 | 1,033 |

### Batch 68 — r7-contextual `ae5eb54a`, r7-flow `44169174`, r7-grammar `7dcbd7b8`, r7-reports `2a52ec83`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10982 / 12797 EXACT (85.82%)**, 54 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +9 RIGHT / 0 lost / 0 missing, diagnostics +6 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0012. No suite's failure count rose.

| Suite | Before | Batch 68 |
|---|---|---|
| `checker_types` | 9,029 | 9,032 |
| `checker_types_configured` | 1,847 | 1,847 |
| `diagnostics` | 5,101 | 5,107 |
| `diagnostics_configured` | 1,033 | 1,033 |

### Batch 69 — r7-contextual `c40f9130`, r7-declared `8e600899`, r7-printer `993d1013`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10984 / 12797 EXACT (85.83%)**, 2 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +17 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 0.9997. No suite's failure count rose.

| Suite | Before | Batch 69 |
|---|---|---|
| `checker_types` | 9,032 | 9,034 |
| `checker_types_configured` | 1,847 | 1,847 |
| `diagnostics` | 5,107 | 5,107 |
| `diagnostics_configured` | 1,033 | 1,033 |

### Batch 70 — r7-flow `370e40c4`, r7-perf `3c228f8b`, r7-printer `a0768c5a`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **10998 / 12797 EXACT (85.94%)**, 14 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +1 RIGHT / 0 lost / 0 missing, diagnostics +1 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0003. No suite's failure count rose.

| Suite | Before | Batch 70 |
|---|---|---|
| `checker_types` | 9,034 | 9,035 |
| `checker_types_configured` | 1,847 | 1,847 |
| `diagnostics` | 5,107 | 5,108 |
| `diagnostics_configured` | 1,033 | 1,033 |

### Batch 71 — r7-shared `cf84d96d`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11002 / 12797 EXACT (85.97%)**, 4 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +2 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 71 |
|---|---|---|
| `checker_types` | 9,035 | 9,036 |
| `checker_types_configured` | 1,847 | 1,847 |
| `diagnostics` | 5,108 | 5,108 |
| `diagnostics_configured` | 1,033 | 1,033 |

### Batch 72 — r7-contextual `24e34b12`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11004 / 12797 EXACT (85.99%)**, 2 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +13 RIGHT / 0 lost / 0 missing, diagnostics +0 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0001. No suite's failure count rose.

| Suite | Before | Batch 72 |
|---|---|---|
| `checker_types` | 9,036 | 9,037 |
| `checker_types_configured` | 1,847 | 1,848 |
| `diagnostics` | 5,108 | 5,108 |
| `diagnostics_configured` | 1,033 | 1,033 |

### Batch 73 — r7-contextual `1457fff5`, r7-flow `ee9b2307`, r7-reports `f214bf07`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11036 / 12797 EXACT (86.24%)**, 32 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +4 RIGHT / 0 lost / 0 missing, diagnostics +3 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0000, domain-model 1.0003. No suite's failure count rose.

| Suite | Before | Batch 73 |
|---|---|---|
| `checker_types` | 9,037 | 9,038 |
| `checker_types_configured` | 1,848 | 1,848 |
| `diagnostics` | 5,108 | 5,110 |
| `diagnostics_configured` | 1,033 | 1,034 |

### Batch 74 — r7-calls `c31cb547`, r7-parser `312c66e3`, r7-printer `3c1f3b24`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11118 / 12797 EXACT (86.88%)**, 82 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +13 RIGHT / 0 lost / 0 missing, diagnostics +12 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0002, domain-model 1.0017. No suite's failure count rose.

| Suite | Before | Batch 74 |
|---|---|---|
| `checker_types` | 9,038 | 9,040 |
| `checker_types_configured` | 1,848 | 1,848 |
| `diagnostics` | 5,110 | 5,121 |
| `diagnostics_configured` | 1,034 | 1,035 |

The loop blocked this batch on its jsTyping leg. moduleNameResolver.ts(1087,9) is a TS2322 that tsgo does not report and the base already printed; only its message changed, because optional parameters now print as `T | undefined`, as native's typeToString does. The integration owner landed the batch by hand. From this batch on, the leg keys false lines by location and code, so a reworded message on a line that was already false is not counted as new.

**Ownership change (after batch 74).** r7-calls exhausted its call-rooted rank
buckets, so the JS/JSDoc checking files move to it from r7-reports:
`jsdoc_*.rs`, `js_case_data.rs`, `assignment_declarations.rs`. r7-reports
keeps relater, assignreport, implicit_any and JSX. Grants are default-on for
small atomic consumer hunks in another lane's file, when they share the commit's
native root cause and pass every gate (named in the lane note). ADR-0056
(rebasable node ids, per-file bind) is funded as a no-flag-day migration in
r7-perf.

### Batch 75 — r7-contextual `5a5b2283`, r7-flow `98781152`, r7-parser `3fe71bfd`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11135 / 12797 EXACT (87.01%)**, 17 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +23 RIGHT / 0 lost / 0 missing, diagnostics +4 cases / 0 lost / 0 missing. jsTyping 104 → 104 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0006. No suite's failure count rose.

| Suite | Before | Batch 75 |
|---|---|---|
| `checker_types` | 9,040 | 9,044 |
| `checker_types_configured` | 1,848 | 1,848 |
| `diagnostics` | 5,121 | 5,125 |
| `diagnostics_configured` | 1,035 | 1,035 |

### Batch 76 — r7-contextual `644412cf`, r7-declared `2cb99999`, r7-reports `71426c39`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11150 / 12797 EXACT (87.13%)**, 15 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +57 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 104 → 100 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 1.0001, domain-model 1.0004. No suite's failure count rose.

| Suite | Before | Batch 76 |
|---|---|---|
| `checker_types` | 9,044 | 9,045 |
| `checker_types_configured` | 1,848 | 1,852 |
| `diagnostics` | 5,125 | 5,127 |
| `diagnostics_configured` | 1,035 | 1,035 |

### Batch 77 — r7-flow `8fddcb64`, r7-printer `b5bebcc3`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11187 / 12797 EXACT (87.42%)**, 37 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +18 RIGHT / 0 lost / 0 missing, diagnostics +1 cases / 0 lost / 0 missing. jsTyping 100 → 100 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0004. No suite's failure count rose.

| Suite | Before | Batch 77 |
|---|---|---|
| `checker_types` | 9,045 | 9,053 |
| `checker_types_configured` | 1,852 | 1,852 |
| `diagnostics` | 5,127 | 5,127 |
| `diagnostics_configured` | 1,035 | 1,035 |

### Batch 78 — r7-contextual `6a3ed5e0`, r7-flow `dba618bd`, r7-perf `45e05179`, r7-printer `c4e63488`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11190 / 12797 EXACT (87.44%)**, 3 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +23 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 100 → 100 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9252, domain-model 0.9775. No suite's failure count rose.

| Suite | Before | Batch 78 |
|---|---|---|
| `checker_types` | 9,053 | 9,056 |
| `checker_types_configured` | 1,852 | 1,853 |
| `diagnostics` | 5,127 | 5,127 |
| `diagnostics_configured` | 1,035 | 1,036 |

### Batch 79 — r7-contextual `d889e122`, r7-flow `4d769348`, r7-printer `96769498`

Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg
passed. Exact oracle **11207 / 12797 EXACT (87.58%)**, 17 gained, 0 lost, 0 missing. Legacy compare against the
previous freeze: types +42 RIGHT / 0 lost / 0 missing, diagnostics +2 cases / 0 lost / 0 missing. jsTyping 100 → 100 unique lines (tsgo 86), new_false 0, lost_true 0. Tests, fmt and strict clippy green. Callgrind Ir: generic-imports 0.9999, domain-model 1.0000. No suite's failure count rose.

| Suite | Before | Batch 79 |
|---|---|---|
| `checker_types` | 9,056 | 9,061 |
| `checker_types_configured` | 1,853 | 1,853 |
| `diagnostics` | 5,127 | 5,129 |
| `diagnostics_configured` | 1,036 | 1,036 |
