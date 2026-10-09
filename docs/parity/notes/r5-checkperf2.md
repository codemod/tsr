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
  (r5-checkperf §13) applied in the working tree — the state batch AH
  produced. §2–§6 are measured against that base. Batch AH then landed and
  the branch was rebased onto integration head `1301779`; §9 re-measures
  everything there.
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

## §5 Enum members compared on borrowed payloads (diff for `relater.rs`)

**Forcing measurement** (jsTyping): `is_simple_type_related_to` asks
`enum_member_value` for both sides of every simple relation: 29,719,162
calls, 5.65 G inclusive, each member answer allocating its `n:`/`s:` key
(`enum_value_key` 0.95 G self plus its frees), only to compare two keys or
test a prefix.

**Change** (`r5-checkperf2-enum-payload.diff`): `literals.rs` gains
`Checker::enum_member_payload(member) -> Option<(SymbolId,
&EnumLiteralValue)>` — `enum_member_value`'s lookup without the key — and
`enum_value_has_key(value, key)`; `relater.rs`'s enum arm in
`is_simple_type_related_to` compares the borrowed payloads. Equivalences:
two keys are equal iff the two payloads are (`EnumLiteralValue`'s derived
`PartialEq` compares variant and text, and the prefix encodes the
variant); `key.starts_with("n:")` iff the payload is `Number`;
`plain_literal_key(t) == key` iff `enum_value_has_key(payload,
&plain_literal_key(t))`. `is_enum_type_related_to` still runs only when the
values are equal. `enum_member_value` stays for its other callers. The
helper ships inside the diff rather than committed alone: nothing else
calls it, and an unused function fails the lint gate. The diff applies to
the integration head with or without this branch's commits.

**Measured.** jsTyping check phase (callgrind, profiling build, on top of
§3–§4): `is_simple_type_related_to` 9,299,496,666 → **2,215,055,710 Ir**;
the phase 94,404,242,448 → **87,289,310,600 (−7.5%)** (that run also
carried §6's change, whose own effect was −0.1 G). `--singleThreaded` CPU
18.91 s → 18.48 s (5 rounds, −2.3%; this container's CPU readings move ±3%
between rounds). Bench projects on the integration head `1301779`
(which contains the stack) with §3–§4 and this diff: dm 1,157,117,660 →
1,148,975,790, dml 4,703,886,656 → 4,661,996,285, gi 343,551,796 →
343,528,009 — almost all of that is §3–§4 (§9); the bench projects relate
few enum members. Both dumps identical; CLI output identical;
`tsr-checker` tests pass.

## §6 Union property names: repeated apparent constituents (refused, −0.1 G)

**Hypothesis** (jsTyping): `property_names_of_type` is 7.79 G inclusive,
7.10 G of it from `is_pure_signature_type` (1.72 M calls), and its
composite arm certifies every candidate name against every constituent's
apparent type (`get_type_of_property_with_this_argument` 508 k calls,
4.01 G). If the unions were enum-member unions (as in §4), ~45
constituents would repeat the same apparent type `Number`.

**Tried**: skip a constituent whose apparent type an earlier one already
has while `publishable_since(mark)` holds for a mark taken at the loop's
start, and hold the names as the shared `Rc<[String]>` instead of copying
them per constituent (`members.rs`).

**Measured and refused.** jsTyping `property_names_of_type` 7,774,522,557
→ 7,669,236,740 Ir (−0.1 G, −0.1% of the phase): the composite arm sees
only 181,033 constituents in all, so its unions are small and rarely
repeat an apparent type; the 501 k property-type reads are the
certification itself. The cost is that the composite arm is recomputed on
every call — it has no memo (the arm's own comment says the traversal runs per query) — while native
resolves a union's properties once (`getPropertiesOfUnionOrIntersectionType`,
`checker.go`). A publication-ruled memo of the composite answer per
`TypeId` is the lever, in `members.rs` (main's file); not attempted here.
What would make the refused shape win: unions of many constituents sharing
an apparent type reaching this arm (the count above would show it).

## §7 `literal_in_const_type_variable_context` re-resolves the enclosing call (write-up)

**Forcing measurement** (jsTyping): 9.39 G inclusive (9.6% of the phase)
from 1,661 + 137 calls, almost all of it `resolve_call_signature_at` (750
calls, 9.30 G, ~12 M Ir each): for a literal argument of a call whose
inference context is not active, the function re-checks the callee and
resolves the call again to learn whether the chosen signature has a `const`
type parameter (`array_literals.rs`, main's file).

Native reads the call's resolved signature from `links.resolvedSignature`
(`getContextualTypeForArgumentAtIndex`, `checker.go`), computed once per
call. The faithful fix is that link — a per-call resolved-signature
publication with native's `resolvingSignature` sentinel — which belongs to
the owner of `calls.rs`'s resolution and is not a local change. A cheaper
exact shortcut is not available here: skipping the re-resolution skips its
side effects (argument checks, type creation order), and whether those
change later answers is not decidable from this function. Recorded with
its size for the owner.

## §8 What is left

- `get_regular_type_of_literal_type` (jsTyping ~50 M calls, 2.72 G): one
  `enum_member_regular` hash probe plus a store read per call. A dense
  per-`TypeId` twin column in `TypeStore` would make it an array read, but
  the map's writers live in `declared.rs` (r5-declared3's) and a reader in
  `flow.rs`; a flag pre-test (every key today is fresh, enum-like or an
  object) would silently break when a new writer adds another kind. Not
  attempted.
- `is_pure_signature_type` (10.4 G after §4): its last test (the stored
  signature list) is a pure read, but the first two force property and
  index resolution whose side effects later answers may depend on, so they
  are not reordered. The composite-names memo (§6) and the relater's own
  per-relation overhead (`Relater::new` and two `relate_ternary` calls per
  `is_applicable_index_type`, 4.1 G) are the remaining parts, in
  `members.rs` and `relater.rs`.
- The relater under narrowing (§2: 2.67 M relations from
  `narrow_type_by_type_predicate`): r5-relater7's file.
- The allocator on the bench projects: `Signature` copies (§2; the
  representation is `tsr-2zk.1092`, reserved for one owner between
  rounds), then per-call vectors in `generic_heritage_member`,
  `get_type_at_flow_branch_label` and `binding_type_alias_body`.

## §9 The lane on the integration head

Rebased onto integration head `1301779` (batches AH–AJ, so the stack is in
the base), dumps re-frozen from that head's release build. Commits: §3
(`signatures.rs`) and §4 (`index_signatures.rs`); diff: §5
(`r5-checkperf2-enum-payload.diff`, applies with or without the commits).
Both dumps of the head with §3, §4 and §5 are identical to the new base's
(`cmp` after cutting `ms=`/`mib=`); CLI output identical on all three bench
projects; `tsr-checker` tests pass.

| | dm Ir | dml Ir | gi Ir |
|---|---:|---:|---:|
| base `1301779` | 1,157,117,660 | 4,703,886,656 | 343,551,796 |
| §3 + §4 (committed) | 1,149,036,145 (−0.70%) | 4,665,771,171 (−0.81%) | 343,526,431 (−0.01%) |
| plus §5 | 1,148,975,790 (−0.70%) | 4,661,996,285 (−0.89%) | 343,528,009 (−0.01%) |

Interleaved fresh processes (default mode) against the base and tsgo:

| project | rounds | base/tsgo | §3+§4 / base | §3+§4 / tsgo | +§5 / base | +§5 / tsgo |
|---|---:|---:|---:|---:|---:|---:|
| domain-model | 31 | 0.758 | 1.020 wall / 1.002 CPU | 0.773 | 1.007 / 0.977 | 0.764 |
| domain-model-large | 41 | 0.742 | 1.017 / 0.993 | 0.755 | 0.997 / 1.005 | 0.740 |
| generic-imports | 41 | 0.870 | 1.005 / 1.004 | 0.874 | 1.021 / 1.022 | 0.888 |
| jsTyping | 5 | 3.87 | **0.970 / 0.970** | 3.75 | **0.929 / 0.921** | **3.59** |

A control (a second copy of the base in the rotation) read 1.006 / 1.001
on dml and 0.997 / 0.997 on gi at 41 rounds. On the bench projects the
wall and CPU ratios stay inside the noise: their check phases move under
1%, and gi's Ir does not move at all (its 1.02 with §5 is layout, not
work: §5 changes no instruction on gi's path). jsTyping, where the
changed functions are hot, moves 7–8% in CPU and wall. Native tsgo on
jsTyping: 2.77 s wall, 7.07 s CPU; TSR with §3–§5: 9.93 s, 20.24 s.

**How far from the target.** dm, dml and gi sit at 0.74–0.89 of tsgo's
wall on this container, jsTyping at 3.6. The bench projects' check phases
are flat after the stack (no checker function above 1.5% self in dml);
jsTyping's remaining multiples are §7 (9.6%), the composite-names memo
(§6), `is_pure_signature_type`'s relater overhead, and the narrowing
relater (§8).
