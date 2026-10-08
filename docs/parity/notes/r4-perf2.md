# Parity lane `r4-perf2` — member memo tables (round 4)

Lane issue `tsr-2zk.937` (parent `tsr-2zk.17`). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Predecessor: [`r4-perf.md`](r4-perf.md) (C7, C9; its gates and
`memo_frames` admission rule are reused here unchanged). Release target
(CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent complete work.

Sections are numbered so code can cite them (`r4-perf2.md` §N). Every number
names the source state and measurement it came from.

## §1 Method

- Source base `eec4b6b` (r4-perf's C7+C9 `cdc49ee` merged with the
  integration branch at `bc1e54e`). 4-vCPU cloud container, Linux 6.18,
  `RUSTUP_TOOLCHAIN=stable`.
- **Ir**: `valgrind --tool=callgrind` total instructions of the release `tsr`
  on `python3 scripts/generate_perf_project.py --modules 100`,
  `--singleThreaded --pretty false`. Deterministic; every variant's complete
  CLI output is `cmp`-identical to the base's.
- **Corpus**: both unfiltered dumps (`diagverdictdump`, 10,570 rows;
  `verdictdump`, 477,985 rows) against the base, frozen from a separate
  worktree build of `eec4b6b`.
- **Wall/CPU**: `scripts/whole_project_perf.py`, 21 samples, default
  scheduling, medians; against the previous binary (self-comparison) and
  against native tsgo built from the pinned submodule
  (`scripts/offline-cargo/build-tsgo.sh`).

Base Ir at `eec4b6b`: **3,404,376,675** (r4-perf measured 3,530,032,122 at
`cdc49ee`; the integration branch's merges account for the difference).

## §2 C3 — instantiated member types (`PerfLinks::reference_member_types`)

**Forcing measurement.** At `eec4b6b`, `instantiate_for_reference_with_this`
was 7.73% of inclusive Ir (263.2 M), about 52,000 requests on the
100-module project, each re-running `instantiate_type` for the receiver's
mapper.

**Native operation.** `getTypeOfInstantiatedSymbol` (`checker.go:16528`):
`instantiateSymbol` (`:20753`) gives each member of an instantiated
reference an instantiated symbol whose `links.resolvedType` is
`instantiateType(getTypeOfSymbol(target), mapper)`, computed once. Consumers
here: `get_type_of_property_with_this_argument` (the symbol road and the
tuple-through-`Array` arm) and `instantiate_for_reference`.

**Identity and owner.** Key `(receiver, declared, this_argument)`, all
`TypeId`s of the owning checker. `declared` stands for the target symbol
(`getTypeOfSymbol(links.target)`), `receiver` for the mapper (its
`type_reference_targets` arguments, or none for a non-generic class or
interface kept as `Named`), and `this_argument` for the mapper's polymorphic
this image. Value: `(target's polymorphic this at publication, result)`.
Private `Checker`, Program lifetime.

**Mapper mode is not in the key.** Print mode
(`identity_unmapped_type_parameters`) substitutes identity for foreign
parameters, so a print-mode answer is not the real instantiation; the
admission (`memo_frames`) never admits print mode or a mapped-template frame,
so neither reads nor publishes, and the key needs no mode bit. Adding the mode
to the key would let print mode publish answers no real consumer may read.

**Publication.** Absent = uncomputed. Published only when all hold:
1. admitted by `memo_frames` over the target symbol's declarations (r4-perf
   §2's rule: an alias-evaluation frame whose bound parameter's scope encloses
   a declaration keeps the old frame-sensitive computation); the computation
   runs with the admitted frames taken;
2. no resolution on the stack and no flow loop active
   (`signature_links_publishable`) — the brief's "never cache a member still
   being resolved": under an active resolution a circular read answers
   `errorType` somewhere inside the instantiation;
3. the answer is not `errorType` (an unresolved parameter list, arity
   mismatch, pending return, depth or count limit — all possibly
   provisional);
4. if a non-empty mapper was applied, the answer is not `declared` unchanged,
   unless `declared` is a leaf type (primitive, literal or enum, or a union of
   them). An unchanged object answer read `declared`'s current contents:
   `TypeStore::complete_object` fills a reserved object in place and
   `peek_property_type` skips edges not yet published, so "mentions no type
   parameter" can become false later. A leaf has no such content. An empty
   mapper answers `declared` whatever it holds and is always decided.
5. the target's polymorphic this did not change during the computation.

The this-type is minted lazily, so a hit is accepted only when the target's
current polymorphic this equals the published one.

**Work boundary.** The worker is `local_type_parameter_types_of` plus
`instantiate_type`; a hit is the admission test, one hash lookup and the
polymorphic-this read.

**Probe** (temporary counters, not committed; 100-module project): of about
52,000 requests, about 3% refused admission, 66% hits, 8% computed and published,
10% not decided (rule 4: an object unchanged after a real mapper; error
answers: 0), 12.5% computed under an active resolution. About half of the
last group repeat a key, so allowing publication under resolution would save
roughly another 1% Ir; the brief forbids it and this port has no cheap way to
see whether a circular read happened inside the computation (the cycle marks
live in `resolution.rs`'s private frames).

**Measured** (§1):

| | Ir | Δ |
|---|---:|---:|
| base `eec4b6b` | 3,404,376,675 | — |
| C3, unchanged answers never published (round 3's rule) | 3,331,940,339 | −2.13% |
| C3 as shipped (leaf exception, rule 4) | 3,330,344,380 | **−2.18%** |

`instantiate_for_reference_with_this` inclusive (at 3,329,809,578, a build of
the same source before formatting): 263.2 M → 189.2 M, of which
the worker is 164.7 M and `memo_frames` admission 40.6 M (§5).

**Corpus.** Both dumps `cmp`-identical to the base.

**Wall and CPU** (§1; C3 against the `eec4b6b` binary, 21 samples, medians):

| Project | wall | CPU |
|---|---:|---:|
| generic-imports | 96.8 / 98.4 ms (0.984) | 0.975 |
| domain-model | 204.7 / 206.1 ms (0.993) | 0.982 |
| domain-model-large | 725.1 / 757.5 ms (0.957) | 0.981 |

**How we would know it is wrong.** A §5 loss under the patch; or a member read
through a generic reference whose type differs between the first request
inside an alias evaluation and one outside it (the admission rule), or one
whose declared object type gains a type-parameter edge after the first read
(rule 4).

## §3 C2 — structured property-name lists (`PerfLinks::structured_property_names`)

**Forcing measurement.** After C3, `get_property_names_of_type` was 9.41% and
`collect_structured_property_names` 8.59% of inclusive Ir. 18,589 walks per
run; each rebuilt the own-member list from the binder table, sorted it with
`compare_symbols` (3.15% + 1.03% in the sort alone) and recursed into every
base. Main callers: `Relater::is_pure_signature_type` (30,811 calls, 5.78%),
`properties_related_to_with_optionals` (2.49%), `infer_from_members`.

**Native operation.** `resolveStructuredTypeMembers` →
`resolveObjectTypeMembers` (`checker.go:19106`) publishes a class's or
interface's `resolvedProperties` once (own members, then
`addInheritedMembers` per base); `getPropertiesOfType` reads them.

**Identity and owner.** Key: the owning class or interface `SymbolId`
(`TypeData::Named { members: Some(owner) }`). Value: the name list, own
members in `compareSymbols` order then each base's, de-duplicated — exactly
what the top-level walk returned. Private `Checker`, Program lifetime.

**Publication.** Only from a top-level request (the `get_property_names_of_type`
Named arm), and only when: admitted by `memo_frames` over the owner's
declarations (bases need no check, r4-perf §2); the walk returned `true`
(every base resolved; `base_symbols_of_ex` answering `None` is a gap and is
recomputed); the walk met no cycle (`StructuredNamesWalk::cycle`, set when a
base re-enters); and `signature_links_publishable`.

**Read inside a walk.** A memoised walk reads a base's published list instead
of walking the base. Exact: a published list met no cycle, so no symbol
already on the walk can recur in the base's walk (that would be a cycle
through the base), and every symbol the base's walk would skip as already
visited has all its names in the list already; so appending the published
list with de-duplication yields the same names in the same order as walking.
An unadmitted request (`StructuredNamesWalk::uncached`) neither reads nor
publishes.

**Late-bound placeholder.** `late_bound_members_of` parks an empty list in
its own cache (`late_bound_member_names`) while it computes, so a walk
re-entered from inside that computation reads no late-bound names. Such a list
must not be published (the unit test
`active_late_bound_entry_does_not_complete_the_outer_name_list` pins this; the
first build of this memo failed it). The placeholder is indistinguishable from
a completed empty list, and that table is not this lane's, so the walk marks
itself unsettled when an owner's entry is already present and empty *and* the
owner declares a member with a computed property name
(`declares_computed_member_name`, the shapes `late_bound_members_of`
considers). An owner with no computed names cannot have late-bound members, so
its empty entry is final. Cost: nothing measurable (Ir 3,088,001,352 →
3,088,273,279).

**Work boundary.** The walk (binder table, late-bound members, sort, bases).
A hit is one hash lookup plus cloning the `Vec<String>` (1.18% Ir; consumers
take ownership, and changing `get_property_names_of_type`'s return type would
touch relater code this lane does not own).

**Measured** (§1):

| | Ir | Δ vs previous | Δ vs base |
|---|---:|---:|---:|
| C3 | 3,330,344,380 | — | −2.18% |
| + C2, first build (published under a live placeholder; not shipped) | 3,088,001,352 | −7.28% | −9.29% |
| + C2 as shipped | 3,088,273,279 | **−7.27%** | **−9.29%** |

`get_property_names_of_type` inclusive 313.5 M → 76.7 M; 1,235 walks remain
(18,589 before).

**Corpus.** Both dumps `cmp`-identical to the base (run with C3+C2 as
shipped).

**Wall and CPU** (§1, 21 samples, medians; `self` is C3+C2 against the C3
binary, `base` against `eec4b6b`, `tsgo` against native):

| Project | self wall | self CPU | base CPU | vs tsgo wall | vs tsgo CPU |
|---|---:|---:|---:|---:|---:|
| generic-imports | 1.022 | 0.991 | 1.034, re-run at 41 samples: **1.000** | 109.8 / 95.4 ms (1.152) | 0.610 |
| domain-model | 0.970 | 0.921 | 0.929 | 196.6 / 246.1 ms (**0.799**) | 0.504 |
| domain-model-large | 0.975 | 0.956 | 0.937 | 791.7 / 942.8 ms (**0.840**) | 0.526 |

generic-imports checks three files and does not reach this code; its row is
the noise floor.

**How we would know it is wrong.** A §5 loss; a class or interface whose
property list differs between a first enumeration inside an alias evaluation
and one outside it; or a class with computed-name members whose enumerated
names lack a late-bound member (the placeholder rule above).

## §4 Candidate 3 — `signature_candidates_of_named_type` per `(receiver, kind)`: refused

Built as a third table keyed by the reference `TypeId` and kind, under C7's
admission and publication gates (`r4-perf.md` §2), on top of C3+C2:

| | Ir |
|---|---:|
| C3+C2 | 3,088,001,352 |
| + receiver signature memo | 3,083,678,154 (−0.14%) |

181,625 requests, 6,993 published lists; the function stayed at 2.51% Ir,
of which `memo_frames` admission was 0.80% and the C7 memo read 0.46%. Most
receivers' lists are empty or short, so the receiver mapper this memo skips
was already cheap; what remains is the admission test every memo pays. A
0.14% gain does not pay for a third table and its contract. **What would
change this:** a cheaper admission test (§5), after which the remaining cost
is the lookup itself.

## §5 `memo_frames` admission cost: a hash-memo of its syntax facts refused

After C3+C2, `memo_frames` is 1.38% self Ir (C3 adds 18,589 + ~52,000
requests to C7/C9's). Tried: memoising each bound parameter's scope owners
(`FxHashMap<SymbolId, Option<Vec<NodeId>>>`) and each `(owner, anchor)`
enclosure answer (`FxHashMap<(NodeId, NodeId), bool>`). Ir rose: with the
candidate-3 table present, 3,083,678,154 → 3,101,327,170 (+0.57%), and
`memo_frames` grew from 44.9 M to 62.9 M. The parent walk is an array index
per step and the owners lists are one to three nodes, so two hash probes cost
more than the walk they replace. **What would change this:** a pre-order
node-range table (r4-perf §3), which makes the enclosure test two integer
comparisons without a hash.

## §6 Wall versus CPU (candidate 4; measurement only, no code)

**Question.** Why does TSR's median wall ratio against tsgo (0.80–1.15) lag
its median CPU ratio (0.50–0.61)? Measured at C3+C2 merged with the
integration branch at `0399578`; native tsgo from the pinned submodule.

**Measurements** (domain-model-large unless named; default mode = 4
checkers, both tools; medians):

| | TSR wall | TSR CPU | TSR CPU/wall | tsgo wall | tsgo CPU | tsgo CPU/wall |
|---|---:|---:|---:|---:|---:|---:|
| default, 21 samples | 792 ms | 1,713 ms | 2.16 | 943 ms | 3,253 ms | 3.45 |
| `--singleThreaded`, 9 samples | 1,684 ms | 1,663 ms | 0.99 | 1,048 ms | 1,313 ms | 1.25 |
| domain-model default | 197 ms | 391 ms | 1.99 | 246 ms | 776 ms | 3.15 |
| domain-model single | 355 ms | 351 ms | 0.99 | 252 ms | 325 ms | 1.29 |

`--extendedDiagnostics`, 7 default runs (medians): TSR Program 0.164 s
(parse 0.103, bind 0.036), Check 0.587 s; tsgo Parse 0.134 s, Bind 0.059 s,
Check 0.707 s. Three single-threaded runs: TSR Check 1.45–1.56 s, tsgo
Check 0.67–0.71 s.

**Finding 1: the CPU ratio is mostly tsgo's duplicated work, not TSR's
efficiency.** tsgo's four checkers burn 3,253 ms of CPU for work one checker
does in 1,313 ms (2.5x): each native checker resolves, privately, every
declaration its files reach, and on these projects every file reaches most of
the shared model types. tsgo's check *wall* barely moves with four checkers
(0.71 s default against 0.69 s single): its slowest checker does nearly the
whole job. TSR's four checkers cost 1,713 ms against 1,663 ms single (3%
duplication) and its check wall falls 2.6x (1.53 s → 0.59 s). On equivalent
work — the single-threaded run, where neither tool duplicates — TSR is
**1.27x tsgo's CPU** on domain-model-large and 1.08x on domain-model, and its
checker alone is about 2.2x slower than tsgo's (1.5 s vs 0.69 s). The
default-mode CPU ratio (0.53) compares TSR's split work against tsgo's
duplicated work and overstates TSR's lead; it should not be read as headroom.

**Finding 2: what TSR's default wall is made of.** 792 ms ≈ program
construction (~165 ms, serial apart from the parallel parse workers) +
the slowest checker (~590 ms) + process start and reporting. With
`TSR_WORK_TRACE` (feature `work-trace`; two runs, tracing inflates absolute
times about 5x, proportions only), the `i % count` assignment gives checker 0
51 files including `main.ts` (9–10% of its check time, the largest file);
the four checkers' summed `source_file_check` time is 3,435 / 3,101 / 3,063 /
3,115 ms and 3,351 / 2,931 / 3,048 / 3,059 ms: mean/max **0.92–0.93**.
Perfect balance would cut the check wall by about 7–8% (≈ 45 ms, wall ratio
≈ 0.84 → ≈ 0.79).

**What would have to change.**
1. *Per-checker work.* The release target is wall <= 0.50 of tsgo (≈ 470 ms
   here). With the serial program phase at ~165 ms the checkers get ~305 ms;
   at 4-way split and 0.92 balance that is ≈ 1.1 s of total check CPU,
   against 1.5–1.6 s today: about 30% less checker work, on top of everything
   in this note. Memo tables like C2/C3 are the route; balance alone cannot
   reach it.
2. *Balance.* Native assigns file `i` to checker `i % count`
   (`checkerpool.go:98`), and TSR mirrors it. Size-aware or work-stealing
   assignment would recover the 7–8%, but it is a deliberate deviation from
   native and **can change output**: a checker's type ids, and so union member
   order in printed types and the order in which lazily resolved declarations
   report, depend on which files that checker checked first. Diagnostics
   themselves are collected per owning file and sorted at reporting, so
   their order would not move, but their text could. It would need its own
   decision record and a corpus/bench check that every diagnostic text is
   unchanged.
3. *Program phase.* TSR's program time (~165 ms) is already comparable to
   tsgo's parse+bind (~190 ms); nothing to gain there relative to tsgo.

**How we would know this is wrong.** A tsgo build with `--checkers 1` that
does *not* match its `--singleThreaded` check time (the duplication
explanation predicts it matches), or a TSR single-threaded CPU that falls
below tsgo's single-threaded CPU while default-mode wall stays above 0.80.
