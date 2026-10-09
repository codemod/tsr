# Parity lane `r5-checkperf` — the check phase's Ir, attributed and cut (round 5)

Lane: the checker's performance lane for round 5 of epic `tsr-2zk` (parent
`tsr-2zk.17`; items `tsr-2zk.998`, `.967`, `.997`, `.996`, `.935`). Box
protocol: `docs/parity/box-protocol.md`. Pinned upstream:
`vendor/typescript-go` @ `5b1047d`. Predecessor: [`r5-perf4.md`](r5-perf4.md).
Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent
complete work. No change here alters an answer: every commit and diff keeps
both conformance dumps and the CLI output byte-identical.

Sections are numbered so code can cite them (`r5-checkperf.md` §N). Every
number names the source state and the measurement it came from.

## §1 Method

- Source base `ccb48e7` (`integrate: merge …-r5-vardecl`). 4-vCPU cloud
  container, Linux 6.18, glibc `malloc`, `RUSTUP_TOOLCHAIN=stable` (1.97).
- **Ir**: `valgrind --tool=callgrind` on the `profiling` profile build
  (release + debuginfo), `tsr -p benches/projects/<p>/tsconfig.json
  --singleThreaded --pretty false`, on `domain-model` ("dm") and
  `domain-model-large` ("dml"). The check phase alone is
  `--toggle-collect='*check_program_files*'`. Every variant's CLI output is
  `cmp`-identical to the base release binary's on both projects.
- **Corpus**: both unfiltered dumps (`diagverdictdump`, 12,238 rows;
  `verdictdump`, 552,533 rows) frozen from the base build before any edit;
  "byte-identical" below means `cmp` of the whole TSV.
- **Wall/CPU**: fresh processes, base / new / native tsgo interleaved with the
  order rotated each round, one warmup round dropped, medians of wall and
  `wait4` user+sys, `whole_project_perf.py`'s flags, default (multi-threaded)
  mode; base and new stdout compared on every run. A control (a byte copy of
  the base binary) in the same rotation measured the noise floor: on
  domain-model, 41 rounds, control/base was 1.016 wall and 1.012 CPU, so
  ratios within about ±2% are noise.
- **Environment.** The offline bootstrap's `assemble.py` imports `tomlkit`,
  which PyPI (blocked) cannot supply. A ~30-line stdlib stand-in (`tomllib`
  to parse, a writer for the tables, inline tables and arrays of tables
  `assemble.py` emits) on `PYTHONPATH`, outside the repository, was enough;
  nothing tracked changed (as `r5-operators3.md` §4 describes).

Base Ir: dm **1,202,276,461** (check phase 794,462,176, 66.1%); dml
**5,229,776,873** (check phase 4,554,353,865, 87.1%).

## §2 Where the check phase's Ir goes

Check-phase-only profiles (`--toggle-collect='*check_program_files*'`) of
the base build. "Self" merges a function's own and inlined code; malloc's
family is listed as itself. Recursive frames (`'2`) are folded into self and
left out of the inclusive column.

**domain-model-large, check phase 4,553,647,438 Ir.**

| # | self Ir | % | function | | inclusive Ir | function |
|---:|---:|---:|---|---|---:|---|
| 1 | 340,578,654 | 7.48 | `get_type_at_flow_node` (§3) | | 4,465,881,176 | `check_node` |
| 2 | 274,685,474 | 6.03 | `_int_free` | | 2,298,287,833 | `check_expression` |
| 3 | 254,277,199 | 5.58 | `BindResult::resolve_name` (§4) | | 1,443,904,999 | `check_call_expression_diagnostics` |
| 4 | 202,963,381 | 4.46 | `malloc` | | 1,062,139,674 | `check_return_statement` |
| 5 | 148,409,634 | 3.26 | `check_node` | | 1,049,396,581 | `check_call_expression` |
| 6 | 139,513,428 | 3.06 | `free` | | 913,443,538 | `check_expression_at_node` |
| 7 | 116,554,783 | 2.56 | `BindResult::lookup_scoped` (§4) | | 892,625,262 | `get_flow_type_of_reference_ex` |
| 8 | 96,747,493 | 2.12 | `_int_malloc` | | 867,141,232 | `get_type_at_flow_node` |
| 9 | 79,502,631 | 1.75 | `resolve_name_with_alias_meaning` (export-alias) | | 862,446,924 | `check_generic_call_with_mode` |
| 10 | 78,057,147 | 1.71 | `get_property_of_type_ex` | | 824,525,086 | `get_type_of_property_with_this_argument` |
| 11 | 69,822,815 | 1.53 | `memcpy` | | 775,429,766 | `relate_with_signature_diagnostic` |
| 12 | 62,156,831 | 1.36 | `memo_frames` | | 768,408,705 | `check_property_access_expression` |
| 13 | 60,967,453 | 1.34 | `SymbolTableField::get` | | 756,606,842 | `Relater::is_related_to_with_flags` |
| 14 | 48,867,772 | 1.07 | `BindResult::merged_symbol` | | 703,913,537 | `get_type_from_type_node_worker` |
| 15 | 48,395,868 | 1.06 | `alias_in_scope_for` | | 660,595,107 | `check_single_candidate_arguments` |
| 16 | 43,840,888 | 0.96 | `get_property_of_declared_symbol` | | 625,968,102 | `get_type_of_symbol` |
| 17 | 42,589,270 | 0.94 | `get_type_of_property_with_this_argument` | | 556,251,632 | `get_instantiated_type_reference` |
| 18 | 40,467,870 | 0.89 | `__rdl_alloc` | | 543,663,375 | `get_type_of_function_expression` |
| 19 | 37,645,023 | 0.83 | `String::clone` | | 539,358,008 | `Relater::recursive_type_related_to` |
| 20 | 36,846,644 | 0.81 | `RawVecInner::finish_grow` | | 530,576,571 | `Relater::structured_type_related_to_worker` |
| 21 | 35,618,638 | 0.78 | `base_symbols_of_ex` | | 518,643,110 | `get_type_of_func_class_enum_module_worker` |
| 22 | 30,089,192 | 0.66 | `best_name` | | 500,954,577 | `get_signatures_of_symbol_for_type` |
| 23 | 29,826,038 | 0.65 | `parameter_initializer_scope` | | 499,695,494 | `get_signature_from_declaration` |
| 24 | 28,896,676 | 0.63 | `global_type_symbol_with_arity` | | 458,430,672 | `evaluate_conditional_node` |
| 25 | 26,896,972 | 0.59 | `memcmp` | | 447,627,375 | `infer_from_types_within` |

**domain-model, check phase 794,462,176 Ir.**

| # | self Ir | % | function | | inclusive Ir | function |
|---:|---:|---:|---|---|---:|---|
| 1 | 52,533,527 | 6.61 | `_int_free` | | 775,465,996 | `check_node` |
| 2 | 38,570,138 | 4.85 | `malloc` | | 336,558,271 | `check_expression` |
| 3 | 38,238,273 | 4.81 | `BindResult::resolve_name` | | 216,006,186 | `check_return_statement` |
| 4 | 30,065,682 | 3.78 | `check_node` | | 185,633,110 | `check_expression_at_node` |
| 5 | 26,411,576 | 3.32 | `free` | | 180,504,264 | `check_call_expression_diagnostics` |
| 6 | 19,749,326 | 2.49 | `_int_malloc` | | 175,441,768 | `check_generic_call_with_mode` |
| 7 | 19,517,182 | 2.46 | `BindResult::lookup_scoped` | | 173,815,100 | `check_generic_call_worker` |
| 8 | 16,393,506 | 2.06 | `resolve_name_with_alias_meaning` (export-alias) | | 172,685,375 | `get_type_of_property_with_this_argument` |
| 9 | 15,838,166 | 1.99 | `get_property_of_type_ex` | | 170,390,608 | `check_call_expression` |
| 10 | 14,146,423 | 1.78 | `memcpy` | | 162,705,904 | `relate_with_signature_diagnostic` |
| 11 | 12,455,551 | 1.57 | `memo_frames` | | 158,907,328 | `Relater::is_related_to_with_flags` |
| 12 | 12,344,714 | 1.55 | `SymbolTableField::get` | | 145,397,730 | `get_type_from_type_node_worker` |
| 13 | 8,835,608 | 1.11 | `get_property_of_declared_symbol` | | 132,568,035 | `check_call_expression` (the `inference.rs` copy) |
| 14 | 8,721,811 | 1.10 | `get_type_of_property_with_this_argument` | | 130,183,643 | `get_type_of_symbol` |
| 15 | 8,452,036 | 1.06 | `alias_in_scope_for` | | 114,617,544 | `Relater::recursive_type_related_to` |
| 16 | 7,708,479 | 0.97 | `String::clone` | | 114,140,563 | `get_instantiated_type_reference` |
| 17 | 7,660,647 | 0.96 | `__rdl_alloc` | | 112,659,932 | `Relater::structured_type_related_to_worker` |
| 18 | 7,170,871 | 0.90 | `base_symbols_of_ex` | | 108,451,587 | `get_type_of_function_expression` |
| 19 | 6,988,562 | 0.88 | `BindResult::merged_symbol` | | 106,397,220 | `check_property_access_expression` |
| 20 | 6,408,804 | 0.81 | `RawVecInner::finish_grow` | | 105,448,586 | `get_type_of_func_class_enum_module_worker` |
| 21 | 6,262,385 | 0.79 | `get_type_at_flow_node` | | 101,180,629 | `get_signatures_of_symbol_for_type` |
| 22 | 6,022,198 | 0.76 | `parameter_initializer_scope` | | 101,060,122 | `get_signature_from_declaration` |
| 23 | 5,883,556 | 0.74 | `global_type_symbol_with_arity` | | 93,939,701 | `evaluate_conditional_node` |
| 24 | 5,091,723 | 0.64 | `memcmp` | | 93,505,080 | `infer_from_types_within` |
| 25 | 5,014,848 | 0.63 | `Signature::clone` | | 86,863,237 | `instantiate_type` |

Three clusters stand out:

1. **The allocator** — `malloc`/`free`/`_int_*`/`__rdl_alloc`/`finish_grow`/
   `memcpy`: about 17% of dml's check phase and 21% of dm's. Its callers
   are spread (clones of `TypeData`, `Signature`, `String`, parameter and
   type-parameter lists; per-call `Vec`s), so it is attacked through those
   (§5, §6), not as one item.
2. **Name resolution** — `resolve_name`, `lookup_scoped`, the export-alias
   walk, symbol-table probes and `merged_symbol`: 12.3% of dml's check phase,
   11.8% of dm's (§4).
3. **The shared-flow scan** inside `get_type_at_flow_node`: 237 M on dml
   alone, nothing on dm (§3).

## §3 The shared-flow stack (diff for the integrator)

**Forcing measurement** (dml, base): `get_type_at_flow_node` had 340.6 M Ir
of *self* cost, the largest checker self-cost function in the profile;
237 M of it was inlined `slice::iter` / `cmp` / `NonMax::get` code. A
temporary counter showed 435,254 lookups of the shared-flow stack scanning
about 38 M entries (~88 per lookup, up to 587). domain-model's walks are
short (≤107 entries), so there the scan is negligible.

**Native operation.** `c.sharedFlows` (`checker.go:799`), a stack of
`(flow node, flow type)` pairs that `getFlowTypeOfReference` truncates back
to its own `sharedFlowStart` (`flow.go:96-99`); `getTypeAtFlowNode` looks a
`FlowFlagsShared` node up with a linear scan from `sharedFlowStart`
(`flow.go:136`) and appends its answer at the end of the step
(`flow.go:201`). TSR mirrors it exactly (`flow.rs`, the `SHARED` arm), with a
`Vec<(FlowId, FlowType)>`.

**Change** (`r5-checkperf-shared-flows.diff`): the stack becomes
`perf_links::SharedFlows`, with the `Vec` methods the call sites use (`len`,
`push`, `truncate`) and a `find(start, id)` that answers what the scan
answered: the first entry for `id` at or after `start`. Each entry records
the position of the previous live entry for the same node, and a per-node
array holds the newest position, so `find` walks only that node's own
entries — almost always none or one — keeping the smallest position at or
after `start`. `truncate` pops entries newest first and restores the per-node
head from each popped entry, so the structure after a truncation is exactly
the structure before the corresponding pushes. `checker.rs` changes the
field's type, `flow.rs` the one lookup; the other six call sites compile
unchanged.

Not a cache: no answer is kept beyond what the stack keeps, and every
entry's lifetime is the stack's. A unit test
(`shared_flows_find_the_first_entry_at_or_after_start`) checks the first-match
rule against a duplicate id, every start, and truncation.

**Measured** (vs base; CLI output `cmp`-identical on both):

| variant | dm Ir | dml Ir |
|---|---:|---:|
| base | 1,202,276,461 | 5,229,776,873 |
| ids in their own `u32` array, 16-lane scan | 1,202,059,473 | 5,085,488,541 (−2.76%) |
| plus a live-count per id (a miss with count 0 skips the scan) | 1,201,827,301 | 5,060,976,076 (−3.23%) |
| **per-node chain (shipped)** | **1,201,026,325 (−0.10%)** | **5,013,317,267 (−4.14%)** |

The scan variants left 51–92 M in `find` because about a quarter of lookups
hit and many misses still saw a non-zero count from an outer query's entries.

Gate (the diff on top of §5's commit): both dumps `cmp`-identical;
`tsr-checker` tests pass. Interleaved wall (31 rounds) against the §5
commit: domain-model-large **0.938 wall** / 0.991 CPU — the deep-flow file
is on the critical path, so the wall moves more than the CPU — and
domain-model 0.981 / 0.997.

**How we would know it is wrong.** A flow answer that differs between the
stack and a linear scan: the unit test pins the rule, and a §1 dump or CLI
difference on a walk with a re-entered query (nested
`getFlowTypeOfReference` from inside a walk) would show it.

## §4 Identifier name resolution (`tsr-2zk.996`, diff for the integrator)

**Re-measured forcing number** (dml base): name resolution is 11–12% of the
check phase — `BindResult::resolve_name` 254.3 M self, `lookup_scoped`
116.6 M, the export-alias variant 79.5 M, symbol-table probes 62.7 M,
`merged_symbol` 48.9 M. By caller (`resolve_name` inclusive):
`get_type_of_dotted_name` 120.4 M (101,506 calls),
`check_used_before_its_declaration` 94.6 M (76,079),
`uninitialized_variable_reads_declared` 48.2 M (41,108),
`check_umd_global_reference` 25.3 M (21,460); the rest is under 15 M per
caller. On dm the same four lead.

The last three are `checkIdentifier` rules in `check.rs`'s identifier arm,
and all three resolve the *same identifier* with `getResolvedSymbol`'s own
meaning, `VALUE | EXPORT_VALUE`. Native resolves it once
(`links.resolvedSymbol`) and every rule reads that symbol. That is also why
r5-perf4 §6's memo, which left `check_used_before_its_declaration` out, saw
that site as "first queries": with the other two rules routed through the
same key it is the one that publishes.

**Minimal landing shape** (`r5-checkperf-resolve-identifier.diff`): one
memo method in `perf_links.rs`, `Checker::resolve_identifier_memo(node,
name, meaning)`, keyed `(identifier NodeId, meaning bits)` with the name
implied by the key — a `start` that is not an `Identifier` whose text is
`name` bypasses the memo — and **four call sites in two files**:
`check_umd_global_reference`, `check_used_before_its_declaration`,
`uninitialized_variable_reads_declared` (`check.rs`) and
`get_type_of_dotted_name` (`flow.rs`). The convention record is on the
method: native `getResolvedSymbol`; private `Checker`, Program lifetime;
every answer, `None` included, is complete (the walk reads only immutable
`&'a` binder, node-table and node-map state and takes no callback); no
receiver or mapper context.

**Measured** (vs base; CLI output `cmp`-identical):

| routed sites | dm Ir | dml Ir |
|---|---:|---:|
| the four above (**shipped shape**) | **1,188,030,695 (−1.19%)** | **5,073,391,558 (−2.99%)** |
| plus `check_type_argument_arity`, `receiver_type_is_the_declared_one` | 1,188,133,585 | 5,074,403,745 (+1.0 M: first queries) |
| four plus `is_matching_reference`, `contains_matching_reference`, `references_match`'s element arm | 1,187,944,586 | 5,069,812,190 (−0.07% more) |

On dml the memo saw 246,951 requests and ran 82,885 walks. Gate (the diff
on top of §5's commit): both dumps `cmp`-identical; `tsr-checker` tests
pass. Interleaved wall (31 rounds) against the §5 commit: domain-model
1.010 wall / 1.009 CPU, domain-model-large 0.983 / 1.004 — inside the ±2%
noise floor (§1); Ir is the reliable measure here. The four sites
capture about as much as r5-perf4's 132-site rewrite measured on its base
(dm −1.27%), so the remaining 128 sites are not worth their conflict
surface; they can be routed one at a time by whoever owns the file, when a
profile names one.

**How we would know it is wrong.** `resolve_name` starting to read mutable
state (a checker-side table or a callback): the key would then need that
state.

## §5 Clone-to-match `TypeData` (`tsr-2zk.998`, committed)

**Forcing measurement** (dml base): `TypeData::clone` 368,242 calls, 51.6 M
inclusive, plus 29.5 M of `TypeData` drops; the clone is a whole type payload
(a `Named` type's text `String` included) copied only to `match` on it. The
largest callers in files no active box owns: `get_index_infos_of_type`
(97,619 clones), `generic_type_with_union_constraint` (45,520),
`narrowable_type_for_reference` (29,910) and `get_regular_type_of_literal_type`
(31,035, plus 6,876 more inside `intern_literal`).

**Change.**
- `get_index_infos_of_type` (`index_signatures.rs`),
  `narrowable_type_for_reference`, `generic_type_with_union_constraint`,
  `generic_type_without_nullable_constraint` and the two union/intersection
  arms of `constraints.rs` borrow the payload to pick the arm and copy only a
  union's or intersection's member list. `narrowable_type_for_reference`'s
  non-union case no longer allocates `vec![ty]`.
- `TypeStore::literal_twin(id, fresh)` (`types.rs`) remembers the fresh or
  regular twin per `(literal, fresh)`, which `get_regular_type_of_literal_type`
  and `get_fresh_type_of_literal_type` (`literals.rs`) now ask. **Native
  operation:** each literal type's `freshType`/`regularType` links
  (`getFreshTypeOfLiteralType`/`getRegularTypeOfLiteralType`, `checker.go`).
  **Identity and owner:** key `(TypeId, bool)`, value the interned twin;
  `TypeStore`, so per checker. **Publication:** the twin is
  `intern_literal(flags, data, fresh)` of the literal's own payload, and the
  interned table never removes or rewrites an entry, so every answer is
  complete; `complete_object`, the one writer that rewrites an identity's
  payload, drops that identity's twins. **Context:** none.
  **Work boundary:** one payload clone and hash per literal and direction
  instead of one per request.

**Measured** (vs base): dm 1,202,276,461 → **1,190,949,247 (−0.94%)**; dml
5,229,776,873 → **5,176,857,053 (−1.01%)**. Both dumps `cmp`-identical; CLI
output identical; `cargo test --workspace --release` passes. Wall,
interleaved (§1), base / new / control / tsgo, 41 rounds, domain-model:
new/base **0.989 wall, 0.990 CPU** (control/base 1.016, 1.012); new/tsgo
0.694 wall. generic-imports (31 rounds, its check phase is ~1 ms): new/base
1.027 wall, 1.001 CPU — noise, its Ir path is untouched.

**How we would know it is wrong.** A literal whose payload changes after it
is interned (a new writer like `complete_object` on an interned id): its
twin would then be stale. Today only `complete_object` rewrites a payload,
and it drops the twins.

## §6 Signature copies (`tsr-2zk.997`): one local diff, the representation written up

**Forcing measurement** (dml check phase, base): `Signature::clone` 201,564
calls, 112.5 M inclusive; `Signature` drops 199,770, 74.7 M; inside the
clones, `Vec<Parameter>::clone` 46.5 M (115,020) and
`Vec<TypeParameter>::clone` 24.6 M (226,416). Together about 4.1% of the
check phase. The largest single clone caller is `signatures_of_type_kind`
(45,566 clones, 21.0 M), then `instantiate_signature` (12.1 M),
`instantiate_type` (10.8 M) and `resolve_call_signature_at` (8.2 M).

**Local change** (`r5-checkperf-kind-filter.diff`, `flow.rs`, main's file):
`signatures_of_type_kind`'s `signature_types` arm cloned the whole stored
list and then filtered it by kind; it now filters the borrowed list and
clones only the requested kind. Same signatures, same order, same
`complete_signature_return` calls. Measured on top of §5's commit: dm
1,190,949,247 → **1,189,081,899 (−0.16%)**; dml 5,176,857,053 →
**5,167,433,189 (−0.18%)**; CLI output identical; both dumps
`cmp`-identical; `tsr-checker` tests pass. Interleaved wall against the §5
commit: domain-model 1.012 / 1.015 CPU (31 rounds); domain-model-large read
1.046 wall at 31 rounds and **0.986 wall / 1.011 CPU** at 41 with a control
in the rotation — noise, as expected for a 0.2% Ir change.

**The representation change, not made.** Sharing parameter lists
(`Rc<[Parameter]>`, or `Rc<Vec<_>>` with `make_mut` at writers) would remove
most of the 46.5 M parameter copies and part of the drops, since 97,736 of
the 115,020 parameter-list clones come from whole-`Signature` clones that
never change the list. It is not a local change: `Signature` is built by
struct literal at 29 sites, its `parameters` are read through `.parameters`
in 37 checker files and mutated in place at about 28 sites in 10 files
(`union_signatures.rs` 6, `decorators.rs` 4, `contextual.rs` 4,
`jsdoc_params.rs` 3, and others), most of them owned by active lanes. The
brief allows it only as a mechanical, local change; it is neither, so it is
written up here for a single owner between rounds. Expected size: up to
~2% of dml's check phase (the parameter copies plus their drops); what
would show it was wrong to defer: a later profile where signature copies
rank above name resolution (§4) and the allocator's callers (§2).

## §7 The global table in type printing's scope walks (diff for the integrator)

**Forcing measurement** (dml base): `best_name` (1,000 calls) 76.2 M
inclusive, `symbol_chain` (800 calls) 111.6 M with `alias_in_scope_for`
61.9 M and `module_alias_at` 34.4 M — about 4% of the check phase for
fewer than 2,000 printed names, ~77,000 Ir per `alias_in_scope_for` call.
Each call copies the *whole* global symbol table (lib.dom and friends,
thousands of entries) into a `Vec`, scans it for the symbol's own name with
string compares, then visits every entry only to skip the ones that are not
`ALIAS`.

**Native operation.** `getAccessibleSymbolChain`'s `trySymbolTable`
(`symbolaccessibility.go:535-575`): a direct own-name hit by key, then the
table's aliases. The global table is the binder's, immutable for the
checker's lifetime.

**Change** (`r5-checkperf-global-aliases.diff`, `checker.rs`):
`Checker::global_alias_entries` lists the global table's `ALIAS`-flagged
entries once, in the table's iteration order. `best_name` takes the global
table's direct hit with `globals().get(own)` (keys are unique, so it is the
entry the scan found) and loops over that list; `alias_in_scope_for` and
`module_alias_at` take their global candidates from it. Every loop body
begins with the `ALIAS` test, so a non-alias entry never had an effect; the
order of the entries that do is unchanged. Convention record: native
`trySymbolTable` over `globals`; key none (one list per private `Checker`,
Program lifetime); complete on first computation (immutable binder state);
no receiver or mapper context; work boundary one pass over the globals per
checker instead of one per printed name. `export_equals_alias_name_at`
and `symbol_chain`'s `try_table` over globals have the same shape but did
not show in either profile; they are left alone.

**Measured** (on top of §5's commit; CLI output identical):

| | dm Ir | dml Ir |
|---|---:|---:|
| §5 commit | 1,190,949,247 | 5,176,857,053 |
| `best_name` + `alias_in_scope_for` | 1,178,436,255 (−1.05%) | 5,114,552,149 (−1.20%) |
| **plus `module_alias_at` (shipped)** | **1,175,689,308 (−1.28%)** | **5,099,867,106 (−1.49%)** |

Both dumps `cmp`-identical; `tsr-checker` tests pass. Interleaved wall
(31 rounds) against the §5 commit: domain-model 0.999 wall / 0.985 CPU,
domain-model-large **0.952 wall** / 0.990 CPU.

**How we would know it is wrong.** A global entry whose behaviour in these
loops does not start with the `ALIAS` test (a new arm before it), or a
binder that adds globals after checking starts.

## §8 Multi-thread scaling on jsTyping (`tsr-2zk.935`): imbalance, not duplication

**Question** (brief): `r4-realworld.md` measured TSR's user time at twice
its wall on `src/jsTyping` (check 21.9 s, real 20.0 s, user 40.0 s). Do the
pooled checkers duplicate work?

**Setup.** `r4-realworld.md`'s scratch config in
`vendor/typescript-go/_submodules/TypeScript/src/jsTyping`
(`{"extends": "./tsconfig.json", "compilerOptions": {"types": [], "pretty":
false, "noEmit": true}}`, removed afterwards), release `tsr` at this
lane's §5 commit; 128 files, 83 checked. Check time is now **10.6 s** with
the default pool (21.9 s in round 4). A temporary per-file timer around
`check_source_file` in `checker_pool.rs` (not committed) attributed the
time per checker and per file.

| mode | wall | user | sys |
|---|---:|---:|---:|
| `--singleThreaded` | 19.82 s | 19.49 s | 0.20 s |
| `--checkers 1` | 20.08 s | 19.78 s | 0.24 s |
| `--checkers 2` | 14.66 s | 20.91 s | 0.31 s |
| default (4 checkers) | 11.28 s | 22.19 s | 0.36 s |

Per-file check time summed per checker (default pool): checker 0
**11.31 s** (20 files), checker 1 3.89 s, checker 2 4.37 s, checker 3 3.33 s
— 22.90 s in all against 19.91 s single-threaded.

**Findings.**

1. **Duplicated work is +15%** (22.9 s against 19.9 s), the lazily resolved
   declarations every checker resolves for itself — native does the same
   (`checker_pool.rs`'s header; native's own default pool spends 1.23 s of
   check against 1.86 s single-threaded, but its user time was not
   measured). It is not what keeps the wall high.
2. **The wall is checker 0's share.** Files go to checkers round-robin by
   program index, as native's `createCheckers` does (`checkerpool.go:98`),
   and checker 0 drew `compiler/checker.ts`, **6.6 s on its own** (a third of
   all checking), plus `binder.ts` (2.0 s). With 4 checkers the wall
   (11.3 s) is that one checker; the other three finish in under 4.4 s.
   The 1.76× speed-up from 4 checkers (native: 1.5×) is the most this file
   order allows.
3. So the lever is per-file speed on large real-world files — TSR's
   single-threaded check is about 10× native's (19.5 s against 1.86 s) —
   not the pool. A finer-grained pool (work stealing per file) would bring
   the wall to about max(6.6 s, 22.9/4 s) but would change which checker
   produces which file's diagnostics, which native's association fixes; it
   is not proposed.

With this lane's diffs stacked (§13), jsTyping's default-pool wall is
11.07 s → **8.27 s** and its CPU 21.56 s → 16.77 s (5 interleaved rounds);
native tsgo, run with `--incremental false --composite false` so that it
does not replay a `.tsbuildinfo`, takes 2.27 s wall, 6.01 s CPU (check
1.71 s). The ratio moves from 4.88 to 3.64; the rest is §9's remaining
per-file cost, chiefly the property walk's misses and the relater.

## §9 jsTyping's check phase, attributed

`domain-model` is small, flat application code; `src/jsTyping` checks the
TypeScript compiler's own sources (`checker.ts` alone is ~50,000 lines,
with enums of hundreds of members and AST interfaces several `extends`
deep), and it profiles differently. Check-phase-only callgrind
(`--singleThreaded`, profiling build of the §5 commit plus §7's diff):
**133,472,242,255 Ir**. Its top self-cost entries: `_int_free` 7.67 G,
`malloc` 5.53 G, `SymbolTableField::get` 4.61 G, `base_symbols_of_ex`
4.43 G, `get_property_of_declared_symbol` 3.89 G + 2.23 G,
`push_children` 3.42 G, `get_property_of_type_ex` 3.28 G, `resolve_name`
3.01 G, `get_type_at_flow_node::{closure#2}` 2.86 G, `late_bound_members_of`
2.81 G, `get_regular_type_of_literal_type` 2.78 G, `format_inner` 2.41 G,
`enum_member_value` 2.07 G, `core::fmt::write` 1.85 G. Three causes
account for over a third of the phase:

| cause | inclusive Ir | share | where |
|---|---:|---:|---|
| the declared-symbol property walk | 24.30 G | 18.2% | §11 |
| `enum_member_value` formatting its key | 15.13 G | 11.3% | §9.1 |
| `symbol_has_any_assignment`'s per-symbol container walk | 6.21 G | 4.6% | §10 |

Fresh-process CPU below is `--singleThreaded`, base and new interleaved,
same stdout on every run; Ir on the bench projects is §1's.

### §9.1 `enum_member_value` (diff for the integrator)

`Relater::is_simple_type_related_to` (`relater.rs`) asks
`Checker::enum_member_value` (`flow.rs`) for both sides of every simple
relation — 29,696,948 calls on jsTyping, where `SyntaxKind`-member pairs
are related constantly — and each call that finds an enum member built its
`n:<value>`/`s:<value>` key with `format!`: 24,269,304 `format_inner`
calls, 13.06 G Ir, about 540 Ir per key. The payload is already a
`String`, so the key is now concatenated (`String::with_capacity` plus two
`push_str`) in an out-of-line helper, which keeps the non-member early
return as light as before (`r5-checkperf-enum-key.diff`). Same key, same
answer. A further step — comparing the borrowed payloads in the relater
instead of building keys at all — needs `relater.rs` (r5-relater6's) and is
left for its owner.

**Measured.** jsTyping `--singleThreaded` CPU 18.97 s → 17.97 s (−5.3%, 5
interleaved rounds; an earlier in-line build) and 18.96 s → 17.38 s (3
rounds, the shipped build). The bench projects barely relate enum members:
dm 1,190,949,247 → 1,191,577,516 (+0.05%), dml 5,176,857,053 →
5,176,250,599 (−0.01%), the difference being glibc's allocator internals
(`_int_free_merge_chunk`, `unlink_chunk`) after exact-size allocations
replaced `format!`'s growth, not the changed function, whose own Ir fell.
The first in-line version measured +0.02%/+0.03% for the same reason plus
a heavier prologue on the non-member path, which moving the key out of
line removed. Gate: both dumps `cmp`-identical; `tsr-checker` tests pass;
CLI output identical.

## §10 `symbol_has_any_assignment`: one walk per container (diff for the integrator)

**Forcing measurement** (jsTyping, §9): `get_type_at_flow_node::{closure#2}`
— the START arm's `symbol_has_any_assignment` — 6.21 G inclusive over
15,299 calls, ~405,000 Ir each, with 74.7 M `push_children` calls.
`symbol_has_any_assignment` (`flow.rs`, `checker-notes-narrow.md` §9.7) is
memoised per symbol, but each new symbol walks its declaration's whole
control-flow container looking for an assignment, `++` or `--` whose target
identifier is spelled like the symbol and resolves to it. In `checker.ts`
that container is `createTypeChecker`'s body, ~50,000 lines, walked once per
outer `let`/parameter referenced from a closure.

**Change** (`r5-checkperf-assignment-targets.diff`, `flow.rs` plus a
`PerfLinks` field): `Checker::assignment_targets_under(root)` walks a
container once and indexes its assignment-target identifiers by text;
`symbol_has_any_assignment` answers whether any target spelled like the
symbol resolves to it. The per-symbol walk's answer was exactly that `any`
(its early return only stopped the search; the walk has no side effects),
and `resolve_name` is pure (§4), so every answer is the same. Convention
record: native has no counterpart (this is a port-side syntactic
approximation of `isSymbolAssigned`'s `markNodeAssignments`, which native
also does once per container); key the scan root's `NodeId`, value the
target index; private `Checker`, Program lifetime; complete on first walk
(the bound tree is immutable); no context.

**Measured.** jsTyping `--singleThreaded` CPU 19.04 s → 18.16 s (−4.6%, 3
rounds) and 18.96 s → 16.82 s (3 rounds, a later noisier run). Bench
projects: dm 1,190,949,247 → 1,189,668,484 (−0.11%), dml 5,176,857,053 →
5,176,932,026 (+0.001%, noise: its containers are small). Gate: both dumps
`cmp`-identical; `tsr-checker` tests pass; CLI output identical.

## §11 The declared-symbol property walk (`tsr-2zk.967`; diff for the integrator)

**Forcing measurement** (jsTyping, §9): `get_property_of_declared_symbol`
18.2% of the check phase, 23.8 M lookups from `get_property_of_type_ex`.
Native answers a property of a class or interface from its resolved member
table (`resolveStructuredTypeMembers` layers the bases in once); this port
walks the symbol's own members, then its late-bound members, then each base
in turn, per lookup. Each step copied two cached lists:
`late_bound_members_of` returned a clone of its published
`Vec<(String, NodeId)>` (49.2 M calls, 1.63 G in the clones alone, plus
their frees), and `base_symbols_of` a clone of the published base list
(52.3 M calls, 7.0 G inclusive).

**Change** (`r5-checkperf-property-walk.diff`, `members.rs`, main's file):
the walk searches a published late-bound list in place, and reads a
published base list by index. Both tables only gain entries — the
late-bound sentinel is replaced, never mutated under a reader, and a base
list is stored once and never recomputed while present — so the in-place
reads see exactly what the copies held. An unpublished list goes through
the old calls. Same walk, same order, same answers.

**Not done here: a `(owner, name)` property memo.** Native's resolved
table would make each lookup one probe. In this port the walk's answer can
be provisional (a base list is `None` while an alias resolves, a late-bound
list is a placeholder while it computes), so such a memo needs the
publication rules `receiver_signature_kinds` uses (`r5-perf4.md` §2) and
belongs to whoever owns `members.rs`; §12 is that memo, as a separate diff.

**Measured.** jsTyping `--singleThreaded` CPU 18.59 s → 17.54 s (−5.6%, 3
rounds); 18.96 s → 17.85 s in the later run. dm 1,190,949,247 →
1,183,630,460 (−0.61%); dml 5,176,857,053 → 5,140,838,820 (−0.70%). Gate:
both dumps `cmp`-identical; CLI output identical (its tests ran with §12,
which contains it).

**jsTyping Ir after §9.1, §10 and §11** (stacked, profiling build): 133.47 G
→ **108.08 G (−19.0%)**. The "before" binary also carried §7's diff and the
"after" did not, so the three diffs' own reduction is at least that.

## §12 A settled-walk property memo (`tsr-2zk.967`; diff for the integrator)

**Forcing measurement** (jsTyping with §9.1, §10 and §11 stacked, 108.08 G
Ir): the walk is still first — `get_property_of_declared_symbol` 6.07 G +
3.72 G self, `SymbolTableField::get` 4.61 G (the per-step member-table
probes), `get_property_of_type_ex` 3.28 G — each of the 23.8 M lookups
re-walking the same few interface chains.

**Native operation.** `getPropertyOfType` over a class or interface reads
`resolveStructuredTypeMembers`' table, which layers the bases in once per
type; a lookup is one probe.

**Change** (`r5-checkperf-property-memo.diff`, `members.rs` and two
`PerfLinks` fields; it contains §11's change):
`get_property_of_declared_symbol_fresh(owner, name)` keeps its answer per
`(owner, name)` in `PerfLinks::declared_properties`, `None` included.

- **Identity and owner.** Key: the owner `SymbolId` as passed and the name.
  The walk from an empty path is a function of the owner's member table
  (binder, immutable), its published late-bound list, its published base
  list, and the same three for each base. Private `Checker`, Program
  lifetime. Only the fresh (empty-path) entry is memoised; the two callers
  that pass their own path (`members.rs` near lines 1442 and 2987) walk as
  before.
- **Publication.** A walk publishes only if no step read unsettled state;
  `PerfLinks::property_walk_provisional` counts such steps, and the fresh
  entry publishes when the count did not move. Unsettled: a late-bound
  list that is still its computing placeholder (`late_bound_active`); a
  base list that was not published (a gap, or computed while
  `alias_resolving > 0`, both of which `base_symbols_of_ex` already refuses
  to store); an own member without `VALUE` flags, whose value-ness
  `symbol_is_value` takes from an alias chain that may be mid-resolution.
  Everything else a walk reads is either immutable or a published entry
  that is never rewritten (§11), so a later walk would read the same tables
  and give the same answer; that is the whole claim.
- **Context.** None: no receiver `this`, mapper or alias frame enters the
  walk; the caller applies the receiver to the symbol it gets back, as
  before.
- **Work boundary.** One walk per `(owner, name)`; a hit is two hash probes.
- **Side effects skipped on a hit.** Publication requires the late-bound
  and base lists the walk touched to be published already, so a repeated
  walk would only have read caches.

**Measured** (contains §11): jsTyping `--singleThreaded` CPU 17.87 s →
16.76 s against §11 alone (−6.2%, 2 rounds) and 18.96 s → 17.05 s against
the base (3 rounds). dm 1,190,949,247 → 1,182,167,836 (−0.74%); dml
5,176,857,053 → 5,132,137,959 (−0.86%). Gate: both dumps `cmp`-identical;
`tsr-checker` tests pass; CLI output identical.

**How we would know it is wrong.** A table the walk reads that changes
after it is published (a new writer that rewrites `base_symbols` or a
published late-bound list, or a member table that is not the binder's), or
an alias member whose answer changes without its flags saying `ALIAS`.


## §13 The whole lane, stacked

`r5-checkperf-stack.diff` is §3, §4, §6, §7, §9.1, §10 and §12 applied
together on top of `ab77d39` (§5), with their insertion points merged; the
individual diffs each apply alone to `ab77d39`, and where two of them add
lines at the same place (the end of `PerfLinks`, the block before
`perf_links.rs`'s tests, the `shared_flows` field in `checker.rs`) both
sides are kept. Gate on the stack: both dumps `cmp`-identical,
`tsr-checker` tests pass (205), `cargo fmt --check` clean, clippy reports
nothing in touched code (stable flags pre-existing findings in
`signatures.rs`, `enum_initializer.rs`, `index_signatures.rs:246` and
`templates.rs`).

| | base `ccb48e7` | §5 commit | stack |
|---|---:|---:|---:|
| dm Ir | 1,202,276,461 | 1,190,949,247 | **1,150,017,864 (−4.35%)** |
| dml Ir | 5,229,776,873 | 5,176,857,053 | **4,671,587,004 (−10.67%)** |

Interleaved fresh processes against the base binary and native tsgo
(default mode; 31 rounds, domain-model 41 with a control):

| project | base/tsgo wall | stack/tsgo wall | stack/base wall | stack/base CPU |
|---|---:|---:|---:|---:|
| domain-model | 0.678 | 0.673 | 0.992 | 0.988 (control 0.998 / 0.988) |
| domain-model-large | 0.738 | **0.659** | **0.893** | 0.947 |
| generic-imports | 0.790 | 0.790 | 1.000 | 0.999 |
| jsTyping (§8, 5 rounds) | 4.88 | **3.64** | **0.747** | 0.778 |

domain-model's wall is bounded by its program construction and its
largest file, so a 4% Ir cut stays inside the noise; domain-model-large's
critical path is the deep-flow file §3 fixes; generic-imports checks three
files (r5-perf4 §7).

**What is left, by measured size.**

1. The allocator (§2): 16–21% of the check phase, from `TypeData`,
   `Signature` and `String` copies. Signature parameter sharing (§6) is the
   largest representation change on that list.
2. Name resolution beyond the four routed sites (§4): the export-alias walk
   (79.5 M self on dml) takes a checker callback and is not exact to memoise
   as is.
3. jsTyping: the relater's enum keys (comparing borrowed payloads,
   `relater.rs`), `get_regular_type_of_literal_type` (61.9 M calls, 2.8 G,
   one hash probe each for `enum_member_regular`), and
   `is_pure_signature_type`'s index-info reads (6.2 G).
