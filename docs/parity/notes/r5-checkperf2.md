# Parity lane `r5-checkperf2` — the check phase after the r5-checkperf stack (round 5)

Lane: the checker's performance lane for round 5 of epic `tsr-2zk`, successor
of [`r5-checkperf.md`](r5-checkperf.md) (whose §1–§13 this continues; its
stack lands in integration batch AH). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on
equivalent complete work. Nothing here changes an answer: every commit and
diff keeps both conformance dumps and the CLI output byte-identical.

Sections are numbered so code can cite them (`r5-checkperf2.md` §N).

## §1 Method and base

- **Base**: integration head `451f474` with `r5-checkperf-stack.diff`
  (r5-checkperf §13) applied in the working tree, uncommitted — the state
  batch AH will produce. Every number below is against that base unless it
  says otherwise. The commits on this branch touch none of the stack's five
  files, so they apply with or without it.
- **Ir**: `valgrind --tool=callgrind` on the `profiling` build,
  `tsr -p benches/projects/<p>/tsconfig.json --singleThreaded --pretty false`
  (whole process), on `domain-model` ("dm"), `domain-model-large` ("dml") and
  `generic-imports` ("gi"). Base: dm **1,156,134,743**, dml
  **4,694,141,769**, gi **343,528,361**. CLI output of every variant is
  `cmp`-identical to the frozen base release binary's on all three.
- **jsTyping**: r5-checkperf §8's scratch config
  (`src/jsTyping/tsconfig.perf.json` in the TypeScript submodule, removed
  afterwards). Check-phase-only callgrind of the base:
  **97,934,259,216 Ir** (r5-checkperf measured 133.5 G before its stack).
  CPU is `--singleThreaded` user+sys of fresh processes, variants
  interleaved, stdout compared on every run.
- **Wall**: fresh processes, base / variants / native tsgo (built from the
  pinned submodule by `scripts/offline-cargo/build-tsgo.sh`) rotated each
  round, one warmup round dropped, medians of wall and `wait4` user+sys,
  `whole_project_perf.py`'s flags (default, multi-threaded mode), 31 rounds.
  On this container the base reads dm 0.753, dml 0.757, gi 0.885 of tsgo's
  wall (r5-checkperf's container read 0.673 / 0.659 / 0.790 for the same
  state: machine-to-machine spread, so ratios are compared within one run).
- **Corpus**: both unfiltered dumps frozen from the integration head's
  release build before any edit; "identical" means `cmp` after cutting the
  `ms=`/`mib=` columns. The stack itself reproduced both dumps identically.
- **Environment**: PyPI is blocked; a ~40-line stdlib stand-in for the three
  `tomlkit` calls `scripts/offline-cargo/assemble.py` makes (`parse` via
  `tomllib`, `inline_table`, `dumps`) on `PYTHONPATH`, outside the
  repository, as `r5-operators3.md` §4 describes. Nothing tracked changed.
- **Heap profile**: `valgrind --tool=dhat` on dml, allocation sites
  aggregated by the first frame in this repository's sources.

## §2 Where the stacked check phase goes

**jsTyping (97.93 G).** Inclusive, the shapes that stand out:

| inclusive Ir | share | function | note |
|---:|---:|---|---|
| 13,763,346,014 | 14.1% | `Relater::is_pure_signature_type` (1.72 M calls) | §4, §6 |
| 7,785,067,767 | 7.9% | `get_index_infos_of_type` — 6.49 G from `is_pure_signature_type` | §4 |
| 7,788,734,652 | 8.0% | `property_names_of_type` — 7.11 G from `is_pure_signature_type` | §6 |
| 9,386,556,561 | 9.6% | `literal_in_const_type_variable_context` → `resolve_call_signature_at` (750 calls) | §7 |
| 5,645,364,836 | 5.8% | `enum_member_value` (29.7 M calls, all from `is_simple_type_related_to`) | §5 |
| 2,722,372,710 | 2.8% | `get_regular_type_of_literal_type` (~50 M calls) | §8 |
| 18,243,988,408 | 18.6% | `narrow_type_by_type_predicate` → 2.67 M relations | contains the above |

Self cost is flat: the allocator (`_int_free`, `malloc`, `_int_malloc`,
`free`, `malloc_consolidate`, `realloc`) is 16.3%, `memcmp`/`memcpy` 4.6%,
then no checker function above 1.3%. The allocator is reached through the
items above (their per-call `Vec`s and `String`s), not one site.

`is_pure_signature_type` is asked up to three times per relation
(`is_related_to_with_flags`) and twice more in the structured walk. On
jsTyping its operands are mostly unions of enum members
(`SyntaxKind.A | SyntaxKind.B | …`): their property names are the common
names of the members' apparent type `Number`, certified per member and per
name, and their index infos are `union_index_infos` over ~45 members that
each map to `Number`.

**domain-model-large (4.69 G whole process).** The allocator is 16.6% of
the whole run. DHAT (4.71 M blocks): the largest single sites are
`Parameter` name `String`s copied by whole-`Signature` clones (326,691
blocks; r5-checkperf §6, the representation `tsr-2zk.997` reserves),
`Signature` clones themselves (`signatures_of_type_kind`,
`complete_pending_signature_returns_of_type`, `instantiate_signature`,
`instantiate_signature_type`), `generic_heritage_member`'s declaration
list and path (206 k), `get_type_at_flow_branch_label`'s per-label vectors
(201 k), `evaluate_alias_body`'s lookup key (87 k) and
`binding_type_alias_body`'s argument copy and path (170 k). Inclusive:
`generic_heritage_member` 148.5 M (3.2%), `evaluate_alias_body` 115.3 M,
`is_pure_signature_type` 112.6 M (2.4%), `Signature::clone` 110.3 M,
`signatures_of_type_kind` 105.7 M.

## §3 `complete_pending_signature_returns_of_type`: read until the first writer (committed)

**Forcing measurement** (dml): the function cloned the type's whole
signature list on every call to iterate it (26,330 list clones, 5.6 MB; it
is inlined into `instantiate_type`, whose `[Signature]` clone is 10.8 M Ir
plus the drops). Most lists have no pending return at all.

**Change** (`signatures.rs`): iterate the stored list by reference; the
only arm that writes is `Pending` (it calls `complete_signature_return`,
which can rewrite slots of this very list). On reaching the first `Pending`
entry, copy the list from that entry on and run the original loop over the
copy. Before that entry every arm only reads, so the borrowed walk reads
exactly what the snapshot held; from that entry on the copy *is* the
snapshot, because nothing has written yet. Same returns, same calls, same
order. Not a cache.

**Measured.** dm 1,156,134,743 → **1,149,340,449 (−0.59%)**; dml
4,694,141,769 → **4,665,254,228 (−0.62%)**; gi 343,528,361 → 343,503,877
(−0.01%). Both dumps identical; CLI output identical. Interleaved (31
rounds) new/base: dm 1.008 wall / 0.997 CPU, dml **0.973 / 0.990**, gi
0.993 / 0.980; new/tsgo dm 0.758, dml 0.736, gi 0.878. jsTyping CPU
unchanged (19.92 s → 20.01 s, 5 rounds, noise).

**How we would know it is wrong.** A non-`Pending` arm that writes the
list (or anything `type_literal_key` reads): the borrowed prefix would then
see a write the snapshot did not.

## §4 `union_index_infos`: one query per primitive apparent type (committed)

**Forcing measurement** (jsTyping): `union_index_infos` 5.83 G inclusive
(117 k calls, 5.28 M constituents), of which `get_index_infos_of_type` on
the constituents 3.80 G and `apparent_type` 1.77 G. 5.08 M of the
constituents are primitives — enum members — whose apparent type is one of
`String`/`Number`/`BigInt`/`Boolean`/`Symbol`, so the same query ran ~45
times per union.

**Change** (`index_signatures.rs`): within one `union_index_infos` call, a
primitive constituent's apparent type is asked once; a later constituent
with the same apparent type reuses that answer. Convention record:

- **Native operation**: `getIndexInfosOfType(getReducedApparentType(t))`
  per constituent in `getUnionIndexInfos` (`checker.go`); native reads each
  apparent type's resolved members, computed once per type.
- **Key and owner**: the apparent `TypeId`; a local `Vec` owned by one
  call, dropped at its end. Not a cache across calls.
- **Publication**: an answer is reused only if the query that produced it
  was publishable — `publication_mark`/`publishable_since` (no flow loop
  active, no resolution frame observed; the rule every perf memo in
  `perf_links.rs` uses) — and holds no gap value. Unsettled answers are
  asked again, as before.
- **Context**: none enters; the apparent types are the global interfaces,
  instantiated by nothing. Mapper frames are unchanged within the loop.
- **Work boundary**: one `get_index_infos_of_type` per distinct apparent
  type per union instead of one per constituent; `apparent_type` still runs
  per constituent (its side effects stay where they were).

Why a reused answer is the answer the repeated query would give: a settled
answer reads only published state (binder tables, the published
`symbol_index_infos` and base lists), which only gains entries, and the
constituents between the two queries are other primitives whose own
queries only publish into the same write-once tables.

**Measured.** jsTyping: `union_index_infos` inclusive 5,825,672,311 →
**2,483,090,988 Ir** (−3.34 G, −3.4% of the phase; 5,083,850 constituents
reused, 93,635 settled first queries, 34,572 unsettled ones re-asked);
`--singleThreaded` CPU 20.01 s → **19.28 s (−3.6%)** against §3, 5 rounds.
Bench projects (on top of §3): dm 1,149,340,449 → 1,148,290,873 (−0.09%);
dml 4,665,254,228 → 4,656,043,001 (−0.20%); gi 343,503,877 → 343,521,956
(+0.005%, noise). Interleaved new/base (§3 + §4 against the base, 31
rounds): dm 0.992 wall / 0.987 CPU, dml 0.970 / 0.990, gi 1.011 / 0.994;
new/tsgo dm 0.746, dml 0.735, gi 0.895. Both dumps identical; CLI output
identical.

**How we would know it is wrong.** A writer that rewrites a published
index-info list or base list of a global interface, or a primitive whose
apparent type is not a global interface (a new arm in `apparent_type`): the
reused answer could then go stale within one loop.
