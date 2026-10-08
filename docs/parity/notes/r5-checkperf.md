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
