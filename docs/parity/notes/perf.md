# Parity lane `perf` — whole-project wall time

Lane issue `tsr-2zk.17` (epic `tsr-2zk`). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on
equivalent complete work, with no previously RIGHT losses.

This file records the lane's measurements and judgment calls, numbered so
other documents can cite them (`docs/parity/notes/perf.md` §N). Every number
names the commit and machine it was measured on.

## §1 Machine, binaries and method

- Source `9c1e3c2` (branch `claude/beautiful-shannon-ar5gh0-perf` base).
  Native tsgo built from the pinned submodule by
  `scripts/offline-cargo/build-tsgo.sh` (Go 1.26.8, `Version 7.1.0-dev`).
- 4-vCPU cloud container, Linux 6.18, glibc 2.39, THP `madvise`. No `perf`
  (`perf_event_paranoid` 2, binary absent); profiles below are
  `valgrind --tool=callgrind` (instruction counts, deterministic) and
  `--tool=dhat` (allocation sites) on a `--profile profiling` build, and
  tsgo's own `--pprofDir`.
- Wall time: `scripts/whole_project_perf.py` (fresh processes, alternating
  order, one warmup, warm file cache). Candidate comparisons use a small
  alternating driver over saved binaries with an **A/A control** (the same
  binary under two names) in every run.
- **Noise on this box.** A/A medians differ by up to 2.6% (default mode,
  840 vs 862 ms) and 3.4% (single-threaded, 1952 vs 2019 ms) at 15–21
  rounds. A wall claim below that is noise. Checker-side candidates are
  therefore also given as callgrind instruction deltas, which repeat exactly.

## §2 Verified vs observed

`whole_project_perf.py` hard-codes `complete_input_equivalence_verified` and
`actual_checked_work_verified` to false (`bd tsr-1yb.1.2`,
`bd tsr-1yb.1.2.2`): no cross-tool query-coverage or checked-work telemetry
exists, so `verified_wall_ratio` is `null` for every project and
`--require-comparable` cannot pass. This lane did not change that — making the
flag pass without the telemetry would be a false verification. The numbers
below are **observed** ratios whose loaded scope, effective options and
diagnostic fingerprints all match (`scope_match`, `options_match`,
`diagnostics_match`, `diagnostics_stable` all true), which is the strongest
evidence the harness can give today.

## §3 Baseline at `9c1e3c2` (21 pairs, default scheduling)

| Project | Files (checked) | TSR median | tsgo median | Observed ratio | TSR user CPU | tsgo user CPU | RSS TSR / tsgo |
|---|---:|---:|---:|---:|---:|---:|---:|
| generic-imports | 66 (3) | 77.0 ms | 83.6 ms | **0.921** | 60 ms | 108 ms | 31 / 46 MB |
| domain-model | 105 (42) | 188.6 ms | 242.1 ms | **0.779** | 360 ms | 544 ms | 48 / 85 MB |
| domain-model-large | 265 (202) | 899.1 ms | 863.0 ms | **1.042** | 1813 ms | 2159 ms | 103 / 232 MB |

`domain-model-large` is new in this lane (§4). The integrator's earlier
readings at `f286fc70` (domain-model 0.82, generic-imports 0.89) are within
this box's spread of these.

## §4 Why a larger public project, and which

Both existing projects are dominated by fixed costs: generic-imports checks
three files and spends ~90% of its compilation parsing and binding the 2.8 MB
of bundled libraries; domain-model checks 42 files in ~80 ms. A ratio there
measures startup and `lib.dom.d.ts`, not the checker, and the release target
is about checking real programs.

`benches/projects/domain-model-large` is the same deterministic generator
(`scripts/generate_perf_project.py --modules 200 --out
benches/projects/domain-model-large`; rerun and diff to verify the bytes):
~35,000 lines in 202 files, 1.7 MB on disk. Both compilers report exactly the
generator's one intentional TS2322 (`src/main.ts(1808,14)`), byte-identical
with `--pretty false`, at default and `--singleThreaded` scheduling.

Rejected alternatives: **400 modules** (70k lines, 2.3 MB of source) gives the
same picture with a 2.4 s TSR run (ratio 1.29 observed at 3 runs) but doubles
both the repository weight and every 21-sample run; it is reproducible from
the generator when a larger point is wanted. **A real open-source project**
would be more representative but none is vendored, crates/npm hosts are
unreachable from these containers, and its diagnostics would first need to
be made identical on both compilers — a correctness task, not a measurement.
What would change this: a vendored public project that both compilers check
identically.

## §5 Wall attribution

`--extendedDiagnostics` on both compilers, same box, warm cache. TSR phases
are disjoint host-clock intervals; tsgo's are its own report (its parse and
bind run concurrently across files, so its phase times are not additive
CPU).

| Phase | generic-imports TSR / tsgo | domain-model TSR / tsgo | domain-model-large TSR / tsgo |
|---|---:|---:|---:|
| Process start (`-v`, median of 31) | 2.1 / 8.0 ms | same | same |
| Config | 0 / 0 ms | 0 / 0 ms | 1 / 1 ms |
| Parse (TSR serial, JSDoc eager) | 44 / 42 ms | 49–61 / 74 ms | 85 / 131 ms |
| Bind (TSR serial) | 17 / 19 ms | 20–30 / 23 ms | 42 / 72 ms |
| Loader other (reads, discovery) | ~6 ms | ~8 ms | ~17 ms |
| Check (4 checkers) | 1 / 1 ms | 81–116 / 129 ms | 714 / 654 ms |
| Reporting | 0 ms | <1 ms | 1 ms |
| Compilation | 67 / 68 ms | 163–219 / 232 ms | 866 / 860 ms |

**The checker is the whole story on real work.** The pooled numbers hide it:

| domain-model-large, wall / user CPU | TSR | tsgo |
|---|---:|---:|
| default (4 checkers) | 0.83 s / 1.70 s | 0.83 s / 2.26 s |
| `--singleThreaded` | 1.81 s / 1.76 s | 0.87 s / 0.93 s |
| `--checkers 1` | 1.92 s / 1.81 s | 0.91 s / 1.23 s |

(dm400, 400 modules: TSR 2.41 / 4.97 s, tsgo 1.86 / 1.92 s, default /
single.) Native gains nothing from its pool on this shape, because each
checker re-resolves the shared module graph and the checker owning `main.ts`
is the critical path; TSR gains 2.2× from the same pool only because its
single checker is slow. **Per unit of checking work TSR is ~2.6× slower than
native** (dm400 single-threaded check: 4.97 s vs 1.92 s wall).

It is not extra types: a temporary probe of `Checker::type_count` (never
committed) read 47,413 types for TSR's single checker against native's
`Types: 40,932` (+16%), and 93,813 vs 80,732 on dm400. TSR does about the same
work per type graph and spends ~2.7× as long on it.

Per worker at the default pool on domain-model-large (probe, three runs):
checker 0 ends at 699–737 ms, checkers 1–3 at 307–403 ms. Checker 0 owns
`src/main.ts` (program index ≡ 0 mod 4). The assignment `i % count` is native
`checkerpool.go` and **must stay**: a checker's history decides type-id
creation order, which decides union ordering in printed diagnostics, so
rebalancing work across checkers is not output-identical in general.

The program phase is smaller than native's on every project (TSR 145 ms vs
~205 ms on domain-model-large), and process start is 4× cheaper.

## §6 Where the instructions go (callgrind, 100-module project, single checker)

`--modules 100` project, 4,758.9 M instructions in total, 88.1% inside
`check_program_files`, 11.8% in `Program::from_root_files`.

- **Allocation: ~20% of all instructions** (`_int_free` 7.1%, `malloc` 5.4%,
  `free` 3.4%, `_int_malloc` 1.7%, plus `realloc`/`consolidate`/
  `__rdl_alloc`). DHAT: 5.96 M blocks, 279 MB allocated in one 0.8 s check.
  Every top allocation site is in `tsr-checker` (§8): signature vector clones,
  heritage/type-parameter lists, `collect_structured_property_names`,
  `TypeData::clone`, and `type_to_string` called from non-printing paths.
- `getenv` 2.3%: 103,815 calls, 95,917 of them from one debug probe (§8 C2).
- `BindResult::resolve_name` 2.6% self, `lookup_scoped` 1.4%.
- Scanner/parser ~6% (dominated by `lib.dom.d.ts` and eager JSDoc).

## §7 Refused in this lane (with the numbers that refused them)

| Candidate | Measured | Verdict |
|---|---|---|
| glibc `malloc.hugetlb=1` (THP for the heap, the mechanism behind the earlier mimalloc gain) | 400-module project, 7 pairs: 2691 vs 2556 ms base | Refused: no gain (median +5%, minima equal at 2490 ms), +7 MB RSS |
| glibc `top_pad` 64 MB + `trim_threshold` 256 MB + `mmap_threshold` 32 MB | 400-module project, 7 pairs: 2581 vs 2556 ms | Refused: inside noise, +15 MB RSS |
| glibc `tcache_count` 256 / 4096, and 4096 with `tcache_max` 4096 | domain-model-large, 15 pairs, default: 882 / 870 / 882 vs 860 base, A/A 878; single: 1951 / 1971 / 1985 vs 1952, A/A 2019 | Refused: no movement beyond A/A; +2–20 MB RSS. The 20% allocator share is *count* of allocations, not glibc's per-call speed |
| A different global allocator crate | not built | Not possible in a box: adds a dependency, and `Cargo.lock` is integrator-only (protocol §4). The earlier mimalloc measurement (`whole-project-performance.md`, perf-2) already showed its gain was transparent huge pages, and the hugetlb row above shows THP does not help here |
| Rebalancing checker work (dynamic file stealing, more checkers) | checker 0 is 2× the others (§5) | Refused on correctness: changes which checker produces a file's diagnostics, and type-id order with it |
| Process-exit teardown (leak program/checkers) | no `drop_glue` of `Checker`/`Program` reaches 1.1% of instructions; workers drop their checkers in parallel | Not pursued; earlier refusal (perf-2, 159.8 → 158.5 ms) stands |

The remaining program-phase levers on the small projects — lazy JSDoc for
TypeScript files and deterministic parallel parse/bind — were already
measured in `whole-project-performance.md` ("Remaining levers, measured
(perf-2)") and belong to the parser/AST and binder lanes.

## §8 Checker-side candidates for the integrator

Not editable in this lane (checker hot paths belong to the parity boxes this
round). Each was built as a throwaway patch, measured, and reverted; the
canonical binary was restored byte-identical (`cmp`). Ir is callgrind total
instructions for the 100-module project, single checker (base 4,758,869,477).

**C1 — `mentions_type_parameter_inner` prints types it cannot use**
(`crates/tsr-checker/src/inference.rs`, the final fallback). When `names` is
empty — always, on the `mentions_registered_type_parameter` path — the
function still renders `type_to_string(ty)` and then tests no names. DHAT:
205,478 allocations (3.4% of all blocks) come from this printing, reached
from `access_member_lookup`. Change: `if names.is_empty() { return false; }`
before printing. Behaviour-free by construction (`any` over an empty slice is
false). **Ir −0.91%** (4,715.5 M). Wall: below this box's resolution alone.

**C2 — `TSR_PROJ_TRACE` read per property access**
(`crates/tsr-checker/src/members.rs:1278`, inside
`get_type_of_property_with_this_argument`). `std::env::var` per call takes
the process environment lock and scans the environment: 95,917 calls, 2.5% of
instructions. Change: read once into a `OnceLock` (as `assignreport.rs:56`
and `calls.rs:375` already do), or delete the probe. **Ir −2.50%**
(4,639.8 M). The other per-call `env::var` probes (`declared.rs:7214`,
`symbols.rs:3900`/`4158`, `optionality.rs:102`, `flow.rs:3093`/`3159`) are
cheaper here (≈8 k calls) but have the same shape.

C1 + C2 together: **Ir −3.46%** (4,594.0 M); domain-model-large
single-threaded wall 1898 ms vs 1982 base / 1962 A/A at 21 rounds (−3.3%,
at the edge of noise); default mode 845 vs 840 / 862 (noise).

**C3 — memoise `instantiate_for_reference_with_this`**
(`crates/tsr-checker/src/members.rs:1893`). Native instantiates a member
once per instantiated symbol (`instantiateSymbol`, `checker.go:20753`, then
the symbol's links cache its type); this port "has no instantiated symbols"
(the function's own comment), so every property read re-runs
`instantiate_type`: 42,468 instantiations, 5.3% of instructions, for 55,205
reads. Probe: a per-checker `FxHashMap<(receiver, declared, this_argument),
TypeId>` in front of the function. Outputs identical on all four generated
projects; **Ir −3.94%** (4,571.5 M). Corpus check: see §9. Before shipping
it needs the checker port convention's record — the key must also carry
whatever context `instantiate_type` reads besides its arguments (mapper mode;
see `docs/architecture/mapper-mode.md`, where a print-mode answer leaking into
later semantic requests is already a recorded hazard), and the publication
state of `declared` (a member type still being resolved must not be cached).

**C4 — the structural gap, for scheduling rather than a patch.** The ~2.6×
per-work deficit (§5) is spread across type resolution rather than one
hotspot. The inclusive leaders on the 100-module profile are
`get_type_of_function_expression` → `get_type_of_symbol` (7.4%),
`get_signature_from_declaration` (7.3%, ≈268 k instructions per call over
1,282 calls), `instantiate_for_reference_with_this` (6.9%, C3) and
`get_type_facts` (2.5%). The allocation sites in §6 (signature `Vec` clones,
`local_type_parameter_types_of` and `instantiated_heritage_base` rebuilding
lists per call, `collect_structured_property_names`) are the same pattern as
C3: work native does once per symbol or type and caches in links, redone per
query here. Reaching 0.50 on domain-model-large needs TSR's pooled check to
fall from ~714 ms to ~270 ms; no CLI-side change can supply that.

## §9 C3 corpus check and the combined probe

With the C3 probe compiled in, both unfiltered dumps are **byte-identical** to
the `9c1e3c2` baseline: 10,570 diagnostic rows and 476,844 type rows,
`cmp`-equal, so both zero-loss checks are trivially empty. That proves the
memo is invisible on today's corpus, not that its key is complete (§8 C3's
caveats stand).

The combined throwaway patch C1+C2+C3 is kept for the integrator as
[`perf-checker-candidates.patch`](perf-checker-candidates.patch) (applies to
`9c1e3c2`; it is a measurement probe, not the shippable form — the memo field
is named `probe_ref_memo` and carries no port-convention record). Outputs are
identical on all four generated projects.

| Combined C1+C2+C3 | base | A/A | C3 alone | C1+C2+C3 |
|---|---:|---:|---:|---:|
| Ir, 100-module, single checker | 4,758.9 M | — | 4,571.5 M (−3.9%) | **4,405.3 M (−7.4%)** |
| domain-model-large default, 21 rounds | 843.9 ms | 852.3 ms | 830.5 ms | **809.9 ms (−4.0%)** |
| domain-model-large `--singleThreaded`, 21 rounds | 2001.5 ms | 2002.3 ms | 1938.3 ms | **1847.5 ms (−7.7%)** |

Against tsgo's 863 ms median (§3) the combined probe would move
domain-model-large from 1.04 to roughly 0.94–0.97 observed. Real progress, and
an order of magnitude short of 0.50: see §8 C4.

## §10 Verified ratio (round 5, base `6539256c`)

`whole_project_perf.py` now publishes `verified_wall_ratio` (supersedes §2):
diagnostics, `--showConfig`, loaded lists and loaded-file bytes must agree,
and an untimed traced run per tool must report the same `(file, checker)`
check list — tsgo's `--generateTrace` `checkSourceFile` spans against TSR's
`TSR_WORK_TRACE` `source_file_check` spans. The measured TSR must therefore be
a `--features work-trace` build. A/A against the default build on
domain-model-large, 11 pairs: wall ratio 1.005, median CPU ratio 1.000, so the
feature costs nothing measurable when the variable is unset. The previous
branch's receipt/qualification layers (`box/recover-performance-r4`, 42
commits) were not carried forward: none is needed for this verdict.

14-CPU Linux box, pinned tsgo `target/tsgo-pinned`, 21 pairs, all verified:

| Project | TSR wall | tsgo wall | Verified ratio | TSR CPU | tsgo CPU |
|---|---:|---:|---:|---:|---:|
| generic-imports | 106 ms | 113 ms | 0.94 | 97 ms | 222 ms |
| domain-model | 212 ms | 210 ms | 1.01 | 414 ms | 742 ms |
| domain-model-large | 821 ms | 572 ms | 1.44 | 1808 ms | 2563 ms |

## §11 Resolved-module lookup by file index (`tsr-2zk.17`)

Profile (perf, frame pointers, domain-model-large, default pool): on the
critical checker (checker 0, owner of `src/main.ts`)
`Program::resolved_module_in_mode` was 8.3% inclusive — two path-keyed
`FxHashMap` probes per query (`resolved_modules[file.path()]`, then
`files_by_path[target]`), each hashing and `memcmp`-ing a full canonical path.
The checker asks it per alias resolution (`resolveExternalModule`).

- **Native operation:** `Program.GetResolvedModule` (`program.go:521`, map
  `p.resolvedModules` keyed by `file.Path()` then `{Name, Mode}`) and
  `GetSourceFileForResolvedModule` (`filesByPath` lookup), consumed by
  `checker.go` `resolveExternalModule`.
- **Identity and owner:** `Program`-owned, immutable after
  `from_root_files`. Outer key is now the file index whose `path()` the old
  map was keyed by (a second spelling of one path shares the first's entry);
  `ModeResolution::resolved_file` is `files_by_path[resolved]`, computed after
  package redirects are inserted, so it equals the per-query hop.
- **Publication:** built once before binding; no partial state is visible.
- **Work boundary:** the checker's repeated queries remain (checker-owned);
  each is now an index, one specifier hash and a mode scan.

Native control (`/tmp/ctl1`, `module nodenext`, `importHelpers`): resolved
import (TS2322 through its type), missing file (TS2307), resolved-but-not-held
untyped JS package (TS7016, must stay distinct from TS2307) and missing
`tslib` (TS2354): tsgo, base and new TSR print the same four diagnostics.
Dumps byte-identical to base. Perf vs base, 21 pairs, median CPU ratio:
domain-model 1.008, generic-imports 0.991, domain-model-large 0.991 (wall
798 vs 824 ms).

## §12 Unbounded bind lookahead (`tsr-2zk.17.3`)

`Program::bind_source_files` binds files on `front_end::ordered` workers and
publishes them in file order. Each worker could hold one result ahead, so
while one worker bound `lib.dom.d.ts` (9.6 ms, program index 9) every other
worker stalled after two results and the rest of the program bound only
afterwards. Native `BindSourceFiles` (`program.go`) queues every file on the
work group and keeps every binding; `ordered` now takes a `Lookahead`, and
binding uses `All` (each worker's channel holds all its results). Publication
order, relocation and therefore every identity are unchanged. Root parsing
keeps `One` (private ASTs coexist with their published copies; `All` measured
no loader gain).

domain-model-large `Bind time` 40 → 25 ms. Native control `/tmp/ctl3`
(script-global `Window` merges across files, `declare global` in a module,
UMD `export as namespace` on the ordered path, `dom` lib): tsgo, base and new
TSR print the same four diagnostics. Dumps byte-identical. Perf vs base, 21
pairs, median CPU ratio / wall: domain-model 0.954 / 200 vs 209 ms,
generic-imports 0.944 / 101 vs 104 ms, domain-model-large 0.976 / 780 vs
803 ms; peak RSS within 2%.

## §13 Where the remaining domain-model-large gap is (HEAD `786787c3`)

Verified ratio 1.44 → **1.33** (759 vs 572 ms, 21 pairs, `work-trace` build;
generic-imports 0.91, domain-model 0.97). Phases, this box (virtiofs: stat
≈136 µs, which inflates both tools' file I/O):

| Phase | TSR | tsgo (`--generateTrace`) |
|---|---:|---:|
| Program (load, parse, resolve) | ~158 ms | 137 ms |
| Bind | 25 ms | 37 ms |
| Check (4 checkers, native `i % 4` affinity) | ~575 ms | 475 ms |

tsgo's four checkers are busy 473/388/391/395 ms; TSR's checker 0 (owner of
`src/main.ts`, which imports all 200 modules) runs ~590 ms while checkers 1–3
finish near 300 ms. The gap is checker 0's extra work, all in
`tsr-checker` (not this lane). Shares are of checker 0's samples (perf, frame
pointers, `profiling` build):

- **C5 — eager printing in `check_object_literal_members`**
  (`objects.rs`, `member_text_at` per property, ~line 1520): 21.1% inclusive
  via `type_to_string_at_worker`, including most `best_name`/`symbol_chain`
  work. Native `checkObjectLiteral` builds a symbol table; printing happens
  only in `typeToString` at report time. Proposed: keep the member type and
  print on display.
- **C6 — `resolve_alias` is not memoised** (`symbols.rs:1040`): 20.3%
  inclusive; `Program::resolved_module_in_mode` is called 709,026 times
  (366,800 for `main.ts`) for ~1,400 import specifiers. Native `resolveAlias`
  caches the target in `aliasLinks.aliasTarget` (with `resolvingSymbol` for
  cycles). Most calls come from printing (`best_name` 7.5%,
  `module_alias_at` 4.7%, `alias_in_scope_for` 4.4%).
- §8 C1/C2 still stand (`mentions_type_parameter_inner` 1.7% self; per-call
  `TSR_PROJ_TRACE` `getenv`).

Refused in this lane: **parallel private parse of the library closure**
(`filesParser.start` queues `lib.dom.d.ts` beside the roots). The closure from
`lib_file_names` + `parse_file_references` parsed on two pool workers and
published at the original visit was output-identical and saved wall
(domain-model 202 → 194 ms, 41 pairs; domain-model-large −1.8%), but the
publication copy of `lib.dom.d.ts` (12 ms) plus a slower worker parse
(35 vs 30 ms) raised median CPU 4.0% on domain-model (gate ≤1.03), and 15%
on generic-imports without the 128 KiB root bound. It becomes free with a
zero-copy publication (parse into a reserved node-id range), which is
parser/AST work outside this lane.

## §14 `resolve_alias` publishes `aliasTarget` (`tsr-1yb.7.7.3`)

Port of `resolveAlias` (`checker.go:16266`) memoisation: a private
`Checker::alias_targets` keyed by alias `SymbolId`, states uncomputed /
`Resolving` / `Resolved(Option)` (completed `None` = `unknownSymbol`); a
re-entrant query answers `None` and the outer resolution completes `None`, as
`pushTypeResolution` failing does. TS2303 stays in `circular_alias.rs`. The
record is on `resolve_alias`'s doc comment.

Work boundary, temporary counters (not committed), default pool:

| Project | `resolve_alias` queries | worker runs after | `resolved_module_in_mode` before → after |
|---|---:|---:|---:|
| domain-model-large | 693,083 | 7,755 | 709,026 → 23,698 |
| domain-model | 81,727 | 1,515 | — → 4,658 |
| generic-imports | 409 | 20 | — → 61 |

Dumps byte-identical to the rebased base `ba0370ec`. Control `/tmp/ctl4`
(re-export rename chain, `import = require` of an `export =` object, a
two-file `export { p } from` cycle): base and new print the same six lines;
tsgo prints five — the extra `TS2708` on the cycle use is pre-existing.
Perf vs base, 21 pairs, CPU ratio / wall: domain-model 0.956 / 200 vs 205 ms,
generic-imports 0.996 / 103 vs 104 ms, domain-model-large 0.902 / 687 vs
803 ms; RSS +0.7%. Verified ratio (work-trace builds) before → after:
generic-imports 0.908 → 0.911, domain-model 0.951 → 0.962,
domain-model-large 1.351 → **1.204**.

Re-profiled after this commit (checker 0, domain-model-large):
`type_to_string_at_worker` fell from 21.3% to 5.5% inclusive and
`check_object_literal_members` is 8.3% inclusive including its non-printing
work — most of §13 C5's cost was `resolve_alias` under the printer.

**§13 C5 not ported (`tsr-1yb.16.3.10`).** The printed member text is the
literal type's identity, not a cache: `check_object_literal_members` mints
`TypeStore::new_named(OBJECT, render_object_type(&members), symbol)` and
stores `object_literal_members` as printed `Member`s; 131 `TypeData::Named`
reads in 27 files consume that text. Printing later is not equivalent:
`member_text_at` reads state that changes after the mint (pending signature
returns, alias accessibility at the reference site). Native's
`checkObjectLiteral` → symbol table → print at report time needs a semantic
object-literal type behind `Named` for those readers — a representation change
outside a function-level grant, now worth ≤5.5% of the critical checker.

**Global allocator refused (`mimalloc` 0.1.52 in `crates/tsr`, measured in a
scratch tree).** Verified ratio vs pinned tsgo (21 pairs) improved —
generic-imports 0.94 → 0.85, domain-model 0.955 → 0.86, domain-model-large
1.199 → 1.063 — but peak RSS rose 40 → 120, 58 → 147 and 137 → 249 MB
(+81–196%). The gain is mimalloc's eager arena commit plus transparent huge
pages: domain-model-large, 9 runs, glibc 686 ms / 137 MB; mimalloc default
639 ms / 237 MB; `ARENA_EAGER_COMMIT=0` or `ALLOW_THP=0` 688 ms / 156 MB;
with `PURGE_DELAY=0` 759 ms / 122 MB. No configuration is faster than glibc
within +10% RSS on this Linux box. macOS (no THP, slower system malloc) is
unmeasured; setting options in code needs unsafe FFI, which the workspace
denies.

## §15 Checker 0 hot-path round (base `5d8d97f3`, this box)

14-vCPU Linux box, `perf` with frame pointers on a `profiling` build of
`domain-model-large`; shares are of checker 0's samples (it owns `main.ts` and
finishes last). Each step is output-preserving: same answers, less work.

### §15.1 C2: the `TSR_PROJ_TRACE` probe is gone

`get_type_of_property_with_this_argument` (`members.rs`) read the
environment on every property access for a behaviour-free debugging probe
(§829.2): `getenv` was 0.8% of checker 0. Removed with its `eprintln!`.

### §15.2 C1: `mentions_type_parameter_inner` stops printing for nothing

The walk's last resort renders the type and scans it for the caller's
parameter `names`. Callers that pass no names (`mentions_registered_type_parameter`,
the polymorphic-`this` checks) paid the render and could never match; the
fallback now returns `false` before printing when `names` is empty.

The `this` checks themselves (`get_type_of_property_with_this_argument`,
`get_property_names_of_type`) copied every minted `this` type into a `Vec`
per member read and, for `fn(): this` substitution, ran one full walk per
minted `this` type. They now run one walk with `mentions_this_type`
(membership: `TYPE_PARAMETER` flag, then the `this_types`/`this_type_nodes`
values) and only search for the first mentioned type (same table order as
before) when that walk says one is present. No cache or table is added; the
answer per query is unchanged. `mentions_type_parameter_inner` fell from
2.0% self (plus 2.7% under `access_member_lookup`) to 0.6%.

### §15.3 `compareSymbols` sorts compute each key once

`compare_symbols` (`compareSymbolsWorker`, `utilities.go:366`) walks both
first declarations up to their `SourceFile` on every comparison. The three
whole-table sorts (`collect_structured_property_names`,
`collect_static_property_names`, the anonymous-property printer) now use
`sort_by_cached_key` over `compare_symbols_key`, the same ordering as a
tuple (has declarations, (file, position), name, id), so each symbol's walk
runs once per sort instead of O(log n) times. The comparator itself uses the
key; minimum searches keep calling it. Sorting fell from 1.8% to ~0.3% of
checker 0.

### §15.4 `best_name` copies a scope table only when it reaches it

`best_name` (the `getAccessibleSymbolChain` scope walk) copied every scope's
locals, every enclosing module's exports and the whole globals table into
vectors before looking at the first one, although the innermost hit returns.
It now records which tables exist (locals, exports, class-expression name,
globals) in the same order and copies each when the loop reaches it, so a
name found in its file's locals never copies globals. Iteration order within
a table is the table's own, as before.

### §15.5 Interface call/construct signatures publish once per checker

Native op: `resolveClassOrInterfaceMembers` → `resolveObjectTypeMembers`
(pinned 5b1047d) stores an interface's declared `CallSignatures` /
`ConstructSignatures` on its structured type; every later
`getSignaturesOfType` reads them. TSR's
`signature_candidates_of_interface_symbol` (`signatures.rs`) re-walked the
heritage clauses on every query — resolving each base entity,
instantiating it (`instantiated_heritage_base`) and instantiating the base's
signatures — and `get_property_of_type_ex` asks for both kinds on every
property miss of an object type (the `Function`/`Object` augment test).

- **Key and owner:** (merged interface or type-literal symbol, kind) →
  `Vec<Signature>` in `Checker::interface_signatures`, private to the
  checker for its lifetime, like native per-checker type links.
- **Publication:** only a completed `Some` is stored. `None` (a cycle on the
  `visiting` stack, an unresolvable or invalid base, an unbuilt
  declaration) is not stored and recomputes, exactly as before.
- **Receiver context:** the stored set is the declared type's, before the
  receiver mapper; `signature_candidates_of_named_type` still instantiates
  it for each receiver per query.
- **Work boundary:** domain-model-large, all four checkers: 372k queries →
  7.3k worker runs; 99.9% of hits are empty sets.

Dumps byte-identical to the base. Checker-0 CPU −6% against the previous
step (15 pairs).

### §15.6 Declared property names publish once per owner

Native op: `getPropertiesOfType` over `resolveClassOrInterfaceMembers`
(pinned 5b1047d): a declared type's own members then `addInheritedMembers`,
resolved once on the structured type. TSR's `get_property_names_of_type`
(`members.rs`, `Named` arm) re-ran `collect_structured_property_names` — the
own table sorted by `compareSymbols`, late-bound names, then every base —
per query; relation (`is_pure_signature_type`, excess-property checks) asks
it repeatedly for the same targets.

- **Key and owner:** the `Named` type's member owner symbol → names, in
  `Checker::structured_property_names`, private to the checker. Type
  arguments do not rename members, so `Foo<A>` and `Foo<B>` share the entry.
- **Publication:** only a completed walk (`true`) is stored, and only when
  no `late_bound_members_of` worker is active (`late_bound_active`, new): an
  active worker leaves an empty placeholder in `late_bound_member_names`
  that a walk could have read. A failed walk (unfollowable base) recomputes.
- **Work boundary:** domain-model-large: 42k queries over 2.4k owners
  (all checkers); `get_property_names_of_type` 3.9% → 1.3% of checker 0.
  Total CPU −3.6% (21 pairs); checker 0's own time is flat (most repeat
  queries were on checkers 1–3).

Dumps byte-identical to the base.

### §15.7 `extends` base symbols publish once per owner

Native op: `getBaseTypes` (`resolveBaseTypesOfClass` /
`resolveBaseTypesOfInterface`, pinned 5b1047d) stores `resolvedBaseTypes`
on the declared type. TSR's `base_symbols_of_ex` (`members.rs`) re-resolved
every heritage entity (`heritage_entity_symbol` → `resolve_name`, alias
chains) on each call, and member lookup calls it for every base on every
property read through inheritance.

- **Key and owner:** (owner symbol, `refuse_type_arguments`) → base
  symbols in `Checker::base_symbols`, private to the checker.
- **Publication:** completed `Some` only, and only while no `resolve_alias`
  worker is active (`alias_resolving`, a new counter in `resolve_alias`):
  a `Resolving` alias answers `None` provisionally, and the answer is
  otherwise a function of the immutable binder and completed alias targets.
  `None` recomputes.
- **Work boundary:** total CPU −2.9%, wall −2.9% (21 pairs against §15.6).

Dumps byte-identical to the base.

### §15.8 The object-literal printer finds its fresh source by index

`certified_object_literal_text_at` (`printing.rs`) walks a regular or
widened object image back to its fresh literal by scanning all of
`regular_object_literal_types` and `widened_object_types` for an entry whose
target is the current type — O(table) per step, per member print. Writers
now go through `record_object_type_transfer` (`widening.rs`), which keeps
`object_type_transfer_origins` (target → (original, which map), only entries
with original < target, the scan's own condition) in step, including the
widening cache's overwrite of its `id → id` recursion marker. A sole
original is the scan's answer; several distinct originals still run the scan,
so its table-order choice is unchanged. Single-threaded check time −3%
(1.105 → 1.074 s); four-checker CPU −0.4%. No native counterpart: native
prints from the symbol table and never walks images back.

### §15.9 A declared method access skips the flow walk, as natively

Native op: `getFlowTypeOfAccessExpression` (pinned 5b1047d,
`checker.go:11400`) returns the property type without
`getFlowTypeOfReference` unless the property symbol is a variable, property
or accessor, or a method whose type is a union (an optional method).
TSR's `access_member_lookup` (`members.rs`) walked the flow graph for every
property access; in domain-model-large's `run()` (200 sequential blocks)
each `out.push`, `serviceN.create` and `serviceN.move` walked back to the
function start. `declared_method_access_skips_flow` applies native's gate
where this port's property symbol is the native one: a member of a declared
class/interface receiver (instantiation keeps flags). Union/intersection,
mapped, reverse-mapped and object-literal receivers keep the walk.

Wall −6.5%, CPU −2.5% (21 pairs against §15.8). Dumps byte-identical.
Native control (optional method narrowed on interface, class and union
receivers; plain methods; function-typed property narrowed; `Array.push`;
`String.toUpperCase`): base, new and tsgo print the same eight lines.

### §15.10 Round result and what remains outside this lane's files

Verified ratio against pinned tsgo (`work-trace` builds, 21 pairs, all
`actual_checked_work_verified`): domain-model-large **1.184 → 0.972**,
domain-model 0.971 → 0.895, generic-imports 0.905 → 0.910 (noise; median
CPU vs base 1.011 at 41 pairs). Median CPU vs base: domain-model-large
0.820, domain-model 0.882. All dumps byte-identical to base `5d8d97f3`.

Checker 0 (domain-model-large) after §15.9, inclusive: flow walk 18.5%
(`get_type_at_flow_node` 9% self), `get_effects_signature` 4.2%,
`get_type_of_dotted_name` 3.4%, eager object-literal printing 6.2%. The
remaining native-faithful cuts are in files this lane does not own:

- `flow.rs` `get_effects_signature`: native memoises
  `signatureLinks.effectsSignature` per call node (including the unknown
  answer); TSR stores only completed negatives.
- `flow.rs` `get_type_of_dotted_name`, `references_match`,
  `contains_matching_reference`: call `binder.resolve_name` per visit
  where native reads `getResolvedSymbol` (`links.resolvedSymbol`, one
  resolution per identifier node); `resolve_name` is 9.7% of checker 0.
- `flow.rs` `intersection_has_never_discriminant`: clones `TypeData`
  before matching the intersection arm (allocates for every non-intersection
  constituent).
- §13 C5 (objects/printing representation) unchanged.

## §16 Allocation count on hot paths (base `470ec060`, pinned 1.96.0)

macOS's allocator makes TSR's allocation churn far more expensive than on
Linux glibc, so this round ranks allocation sites. Method: `valgrind
--tool=dhat --num-callers=40` on a `profiling` build of domain-model-large
(blocks per call site, attributed to the first frame outside `std`), and an
`LD_PRELOAD` counter of `malloc`/`posix_memalign` calls on release builds
(deterministic to ±2 across runs). Base: **8,112,413** mallocs + 241,892
reallocs per run. Verified ratio at base: domain-model-large 0.971,
domain-model 0.898, generic-imports 0.911.

### §16.1 Path guards allocate only when the walk recurses

Four cycle guards pushed onto a fresh `Vec` before knowing they would
recurse: `is_generic_homomorphic_mapped_type_inner` (`mapped.rs`, every
query, 580k allocations), `get_property_of_declared_symbol` (`members.rs`,
every declared member lookup, 555k), `generic_heritage_member` (its owner
plus a copy of the declaration list, 206k) and `binding_type_alias_body`
(`destructure.rs`, 85k). Each now records its entry only where it can
recurse: a non-mapped type, an owner answered by its own tables or with no
bases, an owner without `extends`, the starting type (kept apart from the
list). An unrecorded entry can only be re-searched through a diamond and
answers the same. Answers unchanged; dumps byte-identical.

mallocs 8,112,413 → **6,748,163** (−16.8%). Verified ratio:
domain-model-large 0.971 → 0.942, domain-model 0.898 → 0.883,
generic-imports 0.911 → 0.922 (noise band). CPU vs base: domain-model 1.015,
generic-imports 0.999.

### §16.2 Read type data in place instead of cloning it

Hot readers cloned a whole `TypeData` (or a list) to pattern-match it:
`get_index_infos_of_type` (`index_signatures.rs`), the two union/intersection
tests in `constraints.rs`, `is_type_parameter_at_top_level_with_depth`
(`inference.rs`) and `indexed_access_object_is_generic` (`indexed.rs`) now
match by reference and copy only a composite's constituent list when they
recurse. `instantiate_for_reference_with_this` reads the reference's
arguments in place instead of cloning them, and `get_property_of_type_ex`'s
miss path keeps its at-most-three global fallbacks in a fixed array.

mallocs 6,748,163 → **6,339,063** (−6.1%; −21.9% vs base). Verified ratio:
domain-model-large 0.942 → 0.966, domain-model 0.883 → 0.884,
generic-imports 0.922 → 0.941 — all inside this box's ±3% wall noise for
these pairs. Median-CPU ratios on domain-model swung 0.93–1.05 between runs
of the same binaries; interleaved `perf stat -e task-clock -r 40` rounds put
§16.2 at or below base (388.5 / 376.9 / 373.8 ms and 337.4 / 342.3 /
330.1 ms for base / §16.1 / §16.2). Dumps byte-identical.

### §16.3 A fresh literal keeps its regular type

Native op: `getRegularTypeOfLiteralType` reads the fresh literal's
`regularType` link (pinned 5b1047d). `get_regular_type_of_literal_type`
(`literals.rs`) cloned the literal's data (a `String` for string literals)
and re-interned it on every query. The first interned answer is now kept in
`Checker::regular_literal_types` (fresh `TypeId` → regular `TypeId`, private
to the checker; interning is deterministic, so the stored answer is the one
every later query would compute).

mallocs 6,339,063 → **6,246,910** (−1.5%; −23.0% vs base). Verified ratio:
domain-model-large 0.922, domain-model 0.884, generic-imports 0.917.
Dumps byte-identical.

### §16.4 Pending-return completion reads before it snapshots

`complete_pending_signature_returns_of_type` (`signatures.rs`, called from
`instantiate_type` for every signature-bearing type) cloned the type's whole
signature list (parameters' names included) before a walk that, in the
common case, only reads: it mutates only when a slot's return is `Pending`.
A borrowed read-only pass now answers whenever no slot is `Pending`; when
one is, the original snapshot walk runs from the start (the read-only prefix
changed nothing, so the replay is identical).

mallocs 6,246,910 → **6,005,882** (−3.9%; −26.0% vs base). Verified ratio:
domain-model-large 0.916, domain-model 0.879, generic-imports 0.936 (noise
band). Dumps byte-identical.

Remaining top allocation sites (dhat, share of all blocks after §16.4), all
outside this lane's grant:

| Site | Blocks | Owner |
|---|---:|---|
| `get_property_names_of_type` result clones for `is_pure_signature_type` / relater (only emptiness or membership is read) | 9.0% | `relater.rs` callers; needs a non-cloning query |
| `intersection_has_never_discriminant` clones `TypeData` before matching | 6.8% | `flow.rs` |
| `local_type_parameter_types_of` rebuilds `Vec<(TypeId, String)>` per call | 4.2% | `declared.rs` |
| `Signature` clones in `signatures_of_type_kind` | ~2% | `flow.rs` |
| `get_type_at_flow_branch_label` antecedent vectors | 3.1% | `flow.rs` |
| `create_type_reference_with_display`, `instantiated_heritage_base`, `type_literal_key` | ~6% | `declared.rs` |
