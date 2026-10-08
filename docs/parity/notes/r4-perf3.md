# Parity lane `r4-perf3` — publication under resolution, index-info and name-list sharing (round 4)

Lane issue `tsr-2zk.943` (parent `tsr-2zk.17`). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Predecessors: [`r4-perf.md`](r4-perf.md) (C7, C9, the
`memo_frames` admission rule) and [`r4-perf2.md`](r4-perf2.md) (C3, C2, and
§6's finding that on equal single-threaded work TSR's checker is about 2.2x
tsgo's, so the wall target needs about 30% less checker CPU). Release target
(CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent complete work.

Sections are numbered so code can cite them (`r4-perf3.md` §N). Every number
names the source state and measurement it came from.

## §1 Method

- Source base `21b4368` (r4-perf2's C3+C2 and §6, merged with the
  integration branch; `origin/claude/beautiful-shannon-ar5gh0` had nothing
  newer at start). 4-vCPU cloud container, Linux 6.18,
  `RUSTUP_TOOLCHAIN=stable`.
- **Ir**: `valgrind --tool=callgrind` total instructions of the release
  `tsr`, `--singleThreaded --pretty false`, on
  `python3 scripts/generate_perf_project.py --modules 100` ("p100") and on
  `benches/projects/domain-model-large` ("dml"). Deterministic; every
  variant's complete CLI output is `cmp`-identical to the base's on both.
- **Corpus**: both unfiltered dumps (`diagverdictdump`, 10,570 rows;
  `verdictdump`, 477,979 rows) against the base, frozen from the `21b4368`
  build before any edit.
- **Wall/CPU**: `scripts/whole_project_perf.py`, medians of 21 samples
  (41 where a ratio exceeded 1.0), `--mode default` and `--mode single`;
  against the previous binary (self-comparison) and against native tsgo
  built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).

Base Ir at `21b4368`: p100 **3,314,126,626**; dml **7,078,767,940**.

## §2 Publication under an active resolution (`Resolutions::observations`)

**Forcing measurement.** r4-perf2 §2's probe: about 12.5% of C3's requests
were computed under an active resolution and never published, about half
of them repeating a key (≈1% Ir). The publication rule
(`signature_links_publishable`: no resolution open, no flow loop) refused
them because "under an active resolution a circular read answers
`errorType` somewhere inside the instantiation", and the port had no way
to see whether that happened.

**Native operation.** Native publishes resolved members and instantiated
symbol links unconditionally (`resolveObjectTypeMembers`,
`getTypeOfInstantiatedSymbol`); what makes an answer provisional is a
circularity: `pushTypeResolution` finding the entity already on
`typeResolutions` (`findResolutionCycleStartIndex`, `checker.go:18786`) and
failing the frames from the cycle start.

**The counter.** `Resolutions` gains `observations: Cell<u64>`, monotonic,
bumped on every answer the stack gives that depends on its open frames:
1. every cycle `push_with` finds (the place it marks frames failed);
2. every read-only probe answering `true`: `on_stack` (24 call sites, the
   §29 alias placeholder among them), `deferred_since`, `has_property_frame`,
   `has_active_return`, and `active_signature_keys` when it yields a frame.

The brief asked for (1) only. (2) is needed because this port reads the
stack in places native does not: each probe that answers `true` changes
the answer (a placeholder, a declined arm) relative to an empty stack, and
some (`has_property_frame`, `has_active_return`) answer `true` for any open
frame of a kind, cycle or not. A probe answering `false` answers what an
empty stack would. So a computation during which the count did not move
read nothing from the open frames, and its answer is the one it would have
computed at depth 0. The count is conservative: a cycle or probe hit wholly
inside the computation (frames it pushed itself) also refuses publication,
although the depth-0 rule would have published it.

**Publication rule** (`Checker::publication_mark` /
`Checker::publishable_since`, replacing `signature_links_publishable`): no
flow loop active, and either no resolution was open when the computation
began (the old rule, unchanged) or `observations` did not move during it.
Applied to all four memo tables — C7 `interface_signatures`, C9
`heritage_bases`, C3 `reference_member_types`, C2
`structured_property_names` — with every other condition (admission,
decidedness, C3's rule 4 and polymorphic-this check, C2's cycle and
unsettled flags) unchanged.

**Identity and owner.** The counter is per `Resolutions`, so per private
`Checker`, Program lifetime; it is never reset (u64 cannot wrap in a run).

**What it does not see.** State an open outer computation holds outside the
stack: a reserved object `TypeStore::complete_object` has not filled yet
(C3's rule 4 already refuses unchanged object answers for this reason),
`late_bound_member_names`' parked list (C2's `unsettled`, §3), and the
call re-entry guards (`resolving_signature_calls`). These were equally
reachable under the old rule from a depth-0 computation nested in such a
guard; the extension adds only computations nested in resolution frames.
The corpus gate is what certifies that no consumer reads such state
differently.

**Measured** (§1):

| | p100 Ir | Δ |
|---|---:|---:|
| base `21b4368` | 3,314,126,626 | — |
| C3 + C7 under the count | 3,286,852,003 | −0.82% |
| + C2 + C9 under the count (shipped) | 3,286,050,432 | **−0.85%** |

**How we would know it is wrong.** A §5-gate loss under the patch; or a
memoised member, signature list, base or name list whose first computation
ran inside a resolution and differs from the same request made at depth 0
in a fresh checker.

## §3 A distinct in-progress marker for `late_bound_members_of`

**Forcing constraint.** `late_bound_members_of` parks an empty list in
`late_bound_member_names` while it computes, so a re-entrant walk reads no
late-bound names. r4-perf2 §3's C2 had to tell that placeholder from a
completed empty list without one, and approximated it: an entry present and
empty, *and* an owner declaring a computed member name
(`declares_computed_member_name`, a syntactic scan of every member of every
declaration on each such read).

**Native operation.** `getResolvedMembersOrExportsOfSymbol` binds late
members under the symbol's links (`lateSymbol` / `resolvedMembers`);
its in-progress state is distinct from its completed state.

**Change.** `PerfLinks::late_bound_active: FxHashSet<(SymbolId, bool)>`
holds the keys whose computation is running: inserted with the placeholder,
removed when the completed list replaces it. C2's walk marks itself
unsettled exactly when its owner's entry is active; `declares_computed_member_name`
is deleted. The two unit tests that simulate the placeholder
(`active_late_bound_entry_does_not_complete_the_outer_name_list`,
`composite_enumeration_does_not_publish_active_late_bound_names`) now mark
it active too.

**Identity and owner.** Key `(owner, is_static)`, the same as
`late_bound_member_names`, private `Checker`. The set lives in
`PerfLinks` (this lane's file) rather than beside the cache in `checker.rs`
(a hub file); moving it there, or turning the cache value into an
`Active | Done(list)` enum, is the integrator's call.

**Difference from the approximation.** A completed empty list for an owner
with computed names (every computed name non-late-bindable) was refused
before and publishes now; an active entry for an owner without computed
names cannot occur (that computation does no re-entrant work). No answer
changes.

**Measured** (p100 Ir, on top of §2): see the table in §7. Cost of the set
operations is within noise (+0.005% when measured on the full stack:
3,012,115,892 → 3,012,266,004).

**How we would know it is wrong.** A completed late-bound list missing from
a published C2 name list — the unit tests above pin the active case.

## §4 Shared property-name lists (`get_property_names_of_type_shared`)

**Forcing measurement.** At §3's state, every C2 hit cloned the published
`Vec<String>` (r4-perf2 §3: 1.18% Ir), and the consumers then dropped it:
`Relater::is_pure_signature_type` alone asked 36,811 times on p100 (of
24,189 C2 reads in all) and freed 262,806 allocations doing so.

**Change.** `PerfLinks::structured_property_names` holds `Rc<[String]>`.
`get_property_names_of_type`'s body becomes `property_names_of_type`,
answering a private `PropertyNames::{Owned(Vec), Shared(Rc)}`;
`get_property_names_of_type` keeps its `Option<Vec<String>>` signature
(`into_vec`, a copy only for a shared list — what the clone cost before),
and `get_property_names_of_type_shared` answers `Option<Rc<[String]>>`
(`into_shared`: no copy for a shared list, one header move for a built
one). Same enumeration, same order, same side effects; only the return
representation differs. Callers are not migrated wholesale: the 100 call
sites are in 20 files owned by other lanes, and only one is hot —
`is_pure_signature_type`, migrated here. Others move when a profile names
them (the next is `properties_related_to_with_optionals`, 8,840 calls,
0.4% in `into_vec`).

**Identity, owner, publication.** Unchanged from r4-perf2 §3; `Rc` is not
`Send`, which is fine because `PerfLinks` is one checker's.

**How we would know it is wrong.** It cannot change an answer unless a
consumer mutated a list it got from the memo, which `Rc<[String]>` forbids.

## §5 Index infos per symbol (`PerfLinks::symbol_index_infos`)

**Forcing measurement** (p100 at base): `Relater::is_pure_signature_type`
was 7.86% of inclusive Ir (260.6 M, 36,811 calls). Of that,
`get_index_infos_of_type` was 157.5 M for 23,173 calls (the calls whose
type has no property names), and inside it `union_index_infos` 143.6 M for
1,814 unions: each `string` constituent reads `String`'s apparent
interface, and `index_infos_of_symbol` re-scanned every member of every
`String` declaration for index signatures, then `late_bound_index_infos`
scanned them again for computed names (7,124 calls, 128.2 M, about 18,000
Ir each). The property-name half was §4's.

**Native operation.** `resolveObjectTypeMembers` (`checker.go:19106`)
publishes a class's or interface's `indexInfos` once in its resolved
members (own declared, then late-bound `__index`, then inherited by key);
the static side's through `resolveAnonymousTypeMembers`. Consumer here:
`get_index_infos_of_type` (Named and class-static arms) and the inherited
loop in `index_infos_of_symbol` itself.

**Identity and owner.** Key `(owner SymbolId, static_side)`; value the
uninstantiated list exactly as the walk returned it (own declared, own
late-bound, inherited instantiated through each heritage reference's base).
Private `Checker`, Program lifetime. The receiver's mapper is applied after
the read (`get_index_infos_of_type`'s `instantiate_for_reference`), as
before; the memo stands for the declared type, not the reference.

**Publication.** Published when all hold: admitted by `memo_frames` over
the owner's declarations (heritage references inside are lexical; the
same argument as C2, r4-perf2 §3); the walk answered `Some` (`None` is a
gap or a heritage cycle, recomputed); no value is `errorType` (possibly a
provisional circular read); and §2's `publishable_since`.

**Read inside a walk.** The memo wraps `index_infos_of_symbol` itself, so
the inherited loop reads a base's published list. Exact for any
`visiting`: a published list met no cycle from an empty path, and every
symbol on the path has the base as an ancestor, so none can recur in its
walk (the C2/C7 argument).

**Context.** `late_bound_index_infos` mints an `IndexComponentsId` per call;
a hit now answers the first computation's id. The components it names are
the same declarations, and native creates the info once. The computed-name
`check_expression` and sibling `get_type_of_symbol` that it forces now run
once per owner instead of once per query; their own caches made the
repeats answer-identical already.

**Work boundary.** The worker is the declaration scan, the late-bound scan
and the heritage instantiation; a hit is the admission test, one hash
lookup and a clone of a short `Vec<IndexInfo>` (`Copy` fields).

**How we would know it is wrong.** A §5-gate loss; an index signature that
appears or disappears between a first read inside an alias evaluation and
one outside it; or a late-bound `__index` whose value changes after its
first read (a sibling member whose type was provisional at that read).

## §6 `Relater::is_pure_signature_type`: signature test first — refused

**Attribution** (p100, after §5): 66.3 M inclusive (2.16%), of which
`get_index_infos_of_type` 33.3 M (23,173 calls) and the name enumeration
27.6 M (36,811 calls). The function is a TSR gate with no native
counterpart; its answer is a conjunction of three facts native keeps in
resolved members (properties, index infos, signatures).

**Tried:** test `signature_types` (a table read) first and skip the two
enumerations when it fails. Same boolean answer. p100 Ir 3,066,808,905 →
3,012,115,892 (**−1.78%**), p100 CLI output identical — and the
diagnostics dump lost a correct line:
`conformance/mappedTypeRelationships` TS2322 at line 88 (`y = x` with
`x: Readonly<Thing>`, `y: Readonly<T>`), verdict WRONG both ways. Cause:
`get_index_infos_of_type` and `get_property_names_of_type` call
`resolve_mapped_type_members` on the receiver, and a later relation reads
the mapped members that call published. Skipping the enumeration skips that
side effect.

**Not shipped**, and per-`TypeId` caching of the answer was not tried for
the same reason plus a second one: `signature_types` entries move after
creation (`declared.rs` re-homes a resolved type's signatures to its
reserved id), so a cached `false`/`true` can go stale. **What would change
this:** mapped-member resolution moved to where native does it (a mapped
type's members resolved by whatever reads them, not as a side effect of
this gate), after which the reorder is exact.
