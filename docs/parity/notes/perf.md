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

## §10 Round 3 (`tsr-2zk.17`), source `8b24e49`

Same 4-vCPU container class as §1. Native tsgo rebuilt from the pinned
submodule (`scripts/offline-cargo/build-tsgo.sh`, Go 1.26.8). All numbers in
§10–§13 are callgrind `Ir` (total instructions, deterministic) for the
generated `--modules 100` project, `--singleThreaded --pretty false`, release
build of the `tsr` binary with each patch applied alone to `8b24e49`.
Every variant's complete CLI output is `md5`-identical to the base
(`aac485d0…`).

**Wall ratios against tsgo were not re-measured this round.** The integrator
called the wrap-up while the corpus dumps were occupying the box, and a wall
sample taken under that load would be noise (§1). §3 remains the latest
observed ratio table; the base has meanwhile grown: the same 100-module
project costs **5,578.5 M** instructions at `8b24e49` against 4,758.9 M at
`9c1e3c2` (+17.2%), so the §3 ratios are, if anything, optimistic for today's
tree.

## §11 Patches for the integrator (checker files; not committed by this lane)

| Patch | Change | Ir | Δ vs base |
|---|---|---:|---:|
| — base `8b24e49` | | 5,578,537,115 | — |
| [`perf-mentions-no-names.patch`](perf-mentions-no-names.patch) (C1) | `mentions_type_parameter_inner` returns `false` before printing when `names` is empty | 5,529,983,477 | −0.87% |
| [`perf-env-probes-once.patch`](perf-env-probes-once.patch) (C2) | all 7 per-call `std::env::var` debug probes read once (`debug_env.rs`, one `OnceLock`) | 5,426,715,866 | −2.72% |
| [`perf-reference-member-memo.patch`](perf-reference-member-memo.patch) (C3) | memo for `instantiate_for_reference_with_this` with the port-convention record below | 5,479,746,196 | −1.77% |
| C1+C2+C3 | | 5,279,038,087 | −5.37% |
| [`perf-this-mention-filter.patch`](perf-this-mention-filter.patch) (C5, new) | one exact negative walk before the per-this-type walks in `access_member_lookup` | 4,989,756,755 | **−10.55%** |
| [`perf-effects-signature-memo.patch`](perf-effects-signature-memo.patch) (C6, new) | `signatureLinks.effectsSignature` memo in `get_effects_signature` | 5,250,626,888 | −5.88% |

All five apply to `8b24e49` and stack (`git apply` in order C1, C2, C3, C5,
C6). C1/C5 are behaviour-free by construction; C2 is behaviour-free unless a
developer sets a probe variable after process start; C3/C6 are caches and
carry the convention records in §12.

**Corpus check, C1+C2+C3 together:** the unfiltered `diagverdictdump` is
`cmp`-identical to the `8b24e49` baseline (both §5 loss checks trivially
empty), and so is the unfiltered `verdictdump` (477,917 rows: 467,950
RIGHT, 1,175 GAP, 8,792 WRONG; zero RIGHT losses). **C5 and C6 have not had a corpus run**; they
must pass both §5 loss checks at integration before merging.

C3 is smaller than the round-1 probe (−3.94% then) because the shippable form
refuses to publish answers the probe cached blindly (errors, and unchanged
`declared` results that depended on `declared`'s current contents); most of
the remaining cost of the function is those unpublished re-computations.

## §12 Port-convention records for the caches

**C3 `reference_member_types`.** Native operation: `getTypeOfInstantiatedSymbol`
(`checker.go:15987`) via `instantiateSymbol` (`:20753`), consumer
`get_type_of_property_with_this_argument` and the array-literal arm of
`members.rs`. Key `(receiver, declared, this_argument)`, all TypeIds of the
owning private `Checker`; value `(polymorphic this of the target at
publication, result)`. Publication: only when the worker returns neither
`errorType` (unresolved parameter list, arity mismatch, pending return, depth
or count limit — all possibly provisional) nor, after a real substitution,
`declared` unchanged (that answer read `declared`'s current contents, and a
reserved object is completed in place by `TypeStore::complete_object`). An
empty map (no type arguments, no this) is content-independent and published.
Context: print mode (`identity_unmapped_type_parameters`), alias evaluation
bindings and mapped-template frames neither read nor publish
(`mapper-mode.md`). The this-type is minted lazily, so a hit is revalidated
against the target's current this-type. Expensive boundary:
`instantiate_type` (42,468 instantiations for 55,205 reads at round 1).
Residual known gap: a union/intersection result built while a constituent was
still a reserved placeholder can be frozen; no corpus case exhibits it
(round-1 probe without any guard was byte-identical). Falsifier: any §5 loss
under the patch.

**C6 `effects_signatures`.** Native operation: `getEffectsSignature`
(`signatureLinks.effectsSignature`), consumers the flow walk's call-node arms
(`flow.rs` three sites). Key: the call-expression `NodeId`, private
`Checker`. Value: `Option<Signature>`; `None` is a completed "no effects"
answer, absence is uncomputed. Published only for decided answers: a callee
symbol proven effect-free, `super`, a signature set with no predicate/never
return, or a non-generic effects signature. An error callee, an unbuilt
signature set, failed overload resolution and generic instantiation are
recomputed as before. Read and publish only when no resolution is on the
stack, no flow loop is active, and no alias/mapper/print frame is open (the
places this port's callee typing can be provisional). Expensive boundary:
`get_type_of_dotted_name`/`check_expression` of the callee — 107,654 requests,
7.8% of instructions. Falsifier: a §5 loss under the patch.

**C5 needs no record** (no cache): it adds one walk with predicate "type
parameter named `this`", a superset of every minted this type (all five
mint sites use `new_named(TYPE_PARAMETER, "this", …)`), so its `false` is an
exact negative for the original per-mint walks, which run unchanged when it
says `true`. Native does the same substitution once, through the receiver
mapper.

## §13 Attribution at `8b24e49` and the next candidates

Inclusive shares, base, 100-module project:

| Native operation (TSR function) | Share | Cause |
|---|---:|---|
| `mentions_type_parameter_inner` (all callers) | 11.1% | 454,195 walks from `access_member_lookup` (one per minted this type per access: C5) and one per `instantiate_type` recursion level (re-walks every subtree: quadratic) |
| `signature_candidates_of_interface_symbol` (`resolveObjectTypeMembers` call/construct signatures) | 14.3% (after C5) | rebuilt per query, 94,728 heritage lookups; native caches in the type's resolved members |
| `get_type_of_symbol` | 11.8% | — |
| `instantiated_heritage_base` | 9.0% | base type re-instantiated per query (native: `resolveBaseTypesOfInterface` once) |
| `resolve_name` | 8.6% incl. / 4.9% self | 151,476 from `heritage_entity_symbol`, 106,951 from `get_type_of_dotted_name` (native: `resolvedSymbol` node links) |
| `get_effects_signature` | 7.8% | C6 |
| `get_signature_from_declaration` | 7.2% | uncached (native `links.resolvedSignature`) |
| malloc/free | ~18% self | allocation count, §6 |

Next three candidates, **not built**: (C7) memoise
`signature_candidates_of_interface_symbol` per `(merged symbol, kind)` for
top-level (`visiting` empty) `Some` answers under the C6 quiescence gate,
plus no pending lazy returns; (C8) a parameter-agnostic
`couldContainTypeVariables` bit cached per TypeId in front of
`mentions_type_parameter` (exact only for types whose edges are final; needs
the publication rule for lazily filled side tables); (C9) per-heritage-entry
cache of `heritage_entity_symbol`/`instantiated_heritage_base`. Each is the
same shape as §8 C4: work native caches in links, redone per query here.
