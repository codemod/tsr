# Parity lane `r5-checkperf3` — per-call and per-composite links for the check phase (round 5)

Lane: the checker's performance lane for round 5 of epic `tsr-2zk`
(`tsr-2zk.1091`), successor of [`r5-checkperf2.md`](r5-checkperf2.md) (whose
branch had not landed on the integration head when this lane measured; its
§7 and §6/§8 are the forcing items here). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on
equivalent complete work. Nothing here changes an answer: every diff keeps
both conformance dumps and the CLI output byte-identical.

Sections are numbered so code can cite them (`r5-checkperf3.md` §N).

The three changes edit main's files (`array_literals.rs`, `members.rs`) and
`index_signatures.rs` at their call sites only (a few lines each); the link
and memo storage and logic are an appended `r5-checkperf3` block in
`perf_links.rs`. They ship as three diffs, applied in this order (each applies on top of the
previous; each state builds and lints clean on its own):

1. [`r5-checkperf3-call-link.diff`](r5-checkperf3-call-link.diff) — §3
   (`array_literals.rs`, `perf_links.rs`);
2. [`r5-checkperf3-composite-names.diff`](r5-checkperf3-composite-names.diff)
   — §4 (`members.rs`, `perf_links.rs`);
3. [`r5-checkperf3-union-index-infos.diff`](r5-checkperf3-union-index-infos.diff)
   — §5 (`index_signatures.rs`, `perf_links.rs`).

They are diffs rather than commits because the `perf_links.rs` functions are
unused without their call sites, and an unused function fails the lint
gate.

## §1 Method and base

- **Base**: integration head `51d8da2` (batch AM). r5-checkperf2's commits
  and its enum-payload diff were not on the integration branch yet; nothing
  here depends on them (its §3 edits `signatures.rs`, its §4 the *inside*
  of `union_index_infos`, which §5 here calls unchanged).
- **Ir**: `valgrind --tool=callgrind` on the `profiling` build.
  Bench projects: whole process, `tsr -p benches/projects/<p>/tsconfig.json
  --singleThreaded --pretty false`, on `domain-model` ("dm"),
  `domain-model-large` ("dml") and `generic-imports` ("gi"). jsTyping:
  check phase only (`--toggle-collect='*Checker*check_source_file*'`) on
  r5-checkperf §8's scratch config
  (`src/jsTyping/tsconfig.perf.json` in the TypeScript submodule,
  `--singleThreaded`, removed afterwards).
- **Identity**: both unfiltered dumps (`diagverdictdump`, `verdictdump`)
  frozen from the base's release build before any edit; "identical" means
  `cmp` after cutting the `ms=`/`mib=` columns, for each stacked state
  (§3; §3+§4; §3+§4+§5). CLI output `cmp`-identical to the frozen base
  binary on dm, dml, gi and jsTyping for every state. `slowcases` on the
  final state's dumps (§6).
- **Wall**: §6.
- **Environment**: PyPI is blocked; a stdlib stand-in for the three
  `tomlkit` calls of `scripts/offline-cargo/assemble.py` on `PYTHONPATH`,
  outside the repository (`r5-operators3.md` §4). Native tsgo built from the
  pinned submodule by `scripts/offline-cargo/build-tsgo.sh`.

## §2 Where jsTyping's check phase went at the base

Check phase **96,561,689,803 Ir**. The brief's items, inclusive:

| inclusive Ir | function | note |
|---:|---|---|
| 13,678,726,944 | `Relater::is_pure_signature_type` (1.17 M calls) | §4, §5 |
| 9,565,725,353 | `literal_in_const_type_variable_context` | §3 |
| 8,150,949,870 | `property_names_of_type` | §4 |
| 5,389,312,068 | `union_index_infos` (116 k calls) | §5 |

`is_pure_signature_type` (an instrumented run, 1,172,765 calls): its
operands are 683 k primitive/literal types (names `None`, no index infos —
cheap), 255 k named types (names found — cheap), and **118,230 unions on
only 3,996 distinct union `TypeId`s** (17,011 calls on one), plus 33 k
intersections on 469 ids. For the unions the composite name enumeration
declines (`None`) at its first constituent in 137 k of 138 k cases — the
constituent's apparent type has no certified declared table — after
which the predicate asks the union's index infos. Both answers are
recomputed on every call: `property_names_of_type`'s composite arm says so
in its own comment ("this traversal and type forcing run per query"), and
`get_index_infos_of_type`'s union arm calls `union_index_infos` afresh.

`literal_in_const_type_variable_context` (instrumented): 1,973 asks that
reach the re-resolution arm (no active inference context for the call, no
crossed function), on **960 distinct call nodes**; every answer is `false`
(jsTyping declares no `const` type parameter), and no call ever answered
differently on two asks.

## §3 The call's resolved-signature link for `literal_in_const_type_variable_context`

**Forcing measurement** (§2): each ask outside the call's own inference
re-checks the callee and resolves the call again (`resolve_call_signature`,
~12 M Ir each, through its argument checks) to learn one bit — whether the
chosen signature declares a `const` type parameter. About half the asks
repeat a call already asked.

**Native.** `getContextualTypeForArgumentAtIndex` (`checker.go:29786`)
reads `signatureLinks.resolvedSignature`: the `resolvingSignature` sentinel
answers on re-entry, anything else is `getResolvedSignature`
(`checker.go:8410`), which returns a cached non-sentinel link and otherwise
parks the sentinel, resolves, and publishes the result — except when a flow
loop is active (`len(c.flowLoopStack) != 0`: it restores the previous
value), and except that an inner resolution that published first wins
(`checker.go:8438`). Other writers: `resolveCall`'s pre-emptive store of the
overload-failure candidate (`checker.go:8940`), the IIFE parameter read
that parks `anySignature` around an argument check (`checker.go:29471`), and
the language service's `runWithoutResolvedSignatureCaching`
(`services.go:357`), which clears the links of every enclosing call-like
node for one query and restores them after — none of which this port's
consumer goes through: the IIFE arm returns before the resolution here, and
the checker has no service queries.

**This port.** There is no general per-call link; `resolved_call_signatures`
holds only calls with context-sensitive arguments, and the consumer
(`array_literals.rs`) resolved the call again under the existing
`resolving_signature_calls` sentinel (the reduction of native's
`resolvingSignature`, `checker.rs`). The change moves that arm into
`Checker::call_resolves_const_type_parameter` (`perf_links.rs`) and adds
the link `PerfLinks::call_const_type_parameters`. Convention record:

- **Native operation**: `getResolvedSignature` publishing
  `signatureLinks.resolvedSignature`, read by
  `getContextualTypeForArgumentAtIndex` (pinned `5b1047d`, lines above).
- **Key and owner**: the call's `NodeId`, private to one `Checker`, for its
  lifetime. Value: whether the resolved signature (as
  `resolve_call_signature(check_expression(callee), arguments)` answers it)
  declares a `const` type parameter — the only fact the consumer reads. The
  signature itself is not stored: nothing else reads it here, and a
  `Signature` copy is the allocation r5-checkperf2 §2 measured as the
  largest on dml.
- **Publication**: absent = unresolved; `resolving_signature_calls`
  containing the call = native's sentinel (unchanged: a re-entrant ask
  still answers `false` before the link is read); an entry = resolved.
  Published only when the computation was `publishable_since` its mark
  (no flow loop — native's own rule — and no resolution frame observed) and
  the context was clean at both ends (`call_link_context_clean`).
- **Context**: `call_link_context_clean` keeps out the frames under which
  this port resolves a call against bindings native never has open at a
  call: an active inference context, a higher-order or uninstantiated
  contextual read, an alias-evaluation or mapped-template frame. A hit is
  read only under the same clean context. Native publishes under any
  checkMode and contextual state (`getResolvedSignature` has no such
  guard), so this is narrower than native, not wider. An outer call's
  callres2 inference window (`call_inference_signatures`) is *not* kept
  out: native caches there too, and 508 of the 1,973 jsTyping asks are
  inside one.
- **Work boundary**: one resolution per call node instead of one per ask:
  jsTyping 1,973 asks → 964 resolutions (1,009 hits); the corpus's
  `diagverdictdump` run 4,629 asks → 1,553 resolutions.

Before relaxing the context guard to the rule above, a validation build
resolved on every hit as well and compared: no mismatch on jsTyping (1,009
hits) or on the whole `diagverdictdump` corpus (3,076 hits).

**Measured** (§3 alone against the base):

| | base | §3 | change |
|---|---:|---:|---:|
| jsTyping check Ir | 96,561,689,803 | 90,057,393,303 | **−6.50 G (−6.7%)** |
| dm Ir | 1,118,504,743 | 1,117,888,950 | −0.06% |
| dml Ir | 4,508,819,635 | 4,509,451,027 | +0.01% |
| gi Ir | 343,533,796 | 343,517,486 | −0.005% |

Both dumps identical; CLI output identical on all four projects. The
bench projects have few literal arguments outside inference; dml's
+0.6 M is the link's hash traffic.

**How we would know it is wrong.** A call whose resolution, asked twice
under a clean context, picks signatures that differ in const-ness: the
second ask would see the first answer. The validation build is the probe;
re-run it (resolve on every hit, compare) if a writer starts changing
callee types or candidate lists after a call was first resolved.

## §4 Composite property names: one enumeration per union or intersection

**Forcing measurement** (§2): `is_pure_signature_type` asks
`get_property_names_of_type_shared` of 118 k unions on 3,996 ids; each ask
re-runs the composite arm (`binding_type_alias_body`, `apparent_type`,
`declared_property_table` per constituent until it declines, or the
whole certification walk when it does not). r5-checkperf2 §6 refused
skipping repeated apparent constituents *within* one enumeration
(−0.1 G) and named this memo as the lever.

**Change.** `members.rs`' `property_names_of_type` becomes a wrapper: for a
union or intersection it asks `Checker::composite_property_names`
(`perf_links.rs`), whose miss runs the unchanged enumeration
(`property_names_of_type_worker`, the old body renamed). Convention record:

- **Native operation**: `getPropertiesOfType` →
  `resolveStructuredTypeMembers` → `getPropertiesOfUnionOrIntersectionType`
  (`checker.go`, pinned `5b1047d`), which publishes the composite's
  `resolvedProperties` once per type.
- **Key and owner**: the composite's `TypeId` (a union's or intersection's
  identity carries its alias/origin in this port), private to one
  `Checker`. Value: the names as the shared `Rc<[String]>`, or the
  enumeration's decline (`None`).
- **Publication**: both a list and a decline are completed answers — the
  decline is this port's "unsupported", which the enumeration reaches only
  after every table it read was complete (its own comment: "temporary name
  lists publish only as a complete Some, never a recursive assumption";
  the decline sites are unresolved/error bodies, mapped metadata, private
  origins, `Unknown` relations and uncertified tables). Published only
  when `publishable_since` holds for the mark taken at the miss (no flow
  loop, no resolution frame observed), so a decline caused by an active
  resolution's placeholder is not published.
- **Context**: admitted only with no alias-evaluation binding, mapped
  template or identity-unmapped frame open — the frames
  `binding_type_alias_body`/`evaluate_alias_body` and the mapped metadata
  read. Not admitted means the old enumeration runs. No receiver, `this`
  or relation kind enters: the enumeration's own relation
  (`relate_ternary`, assignable) is fixed.
- **Work boundary**: one enumeration per composite id instead of one per
  query; a hit clones one `Rc`. Callers that want a `Vec`
  (`get_property_names_of_type`) copy the list as they did from the
  structured-names memo (`r4-perf3.md` §4).

**Measured** (on top of §3):

| | §3 | §3 + §4 | change |
|---|---:|---:|---:|
| jsTyping check Ir | 90,057,393,303 | 82,634,379,393 | **−7.42 G (−8.2%)** |
| dm Ir | 1,117,888,950 | 1,116,799,001 | −0.10% |
| dml Ir | 4,509,451,027 | 4,500,218,431 | −0.20% |
| gi Ir | 343,517,486 | 343,531,001 | +0.004% |

Both dumps identical; CLI output identical on all four projects.

**How we would know it is wrong.** A writer that changes what a published
composite's enumeration would read — a constituent's declared table or
apparent type gaining or losing a member after the composite was first
enumerated outside any resolution — would leave the memo stale. The
existing structured-names and declared-property memos rest on the same
property of the tables (they only gain settled entries).

## §5 Union index infos: one computation per union

**Forcing measurement** (§2): after a declined name enumeration,
`is_pure_signature_type` asks `get_index_infos_of_type` of the union, whose
union arm runs `union_index_infos` over every constituent each time
(116 k calls, 5.39 G). r5-checkperf2 §4 makes each call cheaper by asking
each primitive apparent type once *within* a call; this is the memo
*across* calls, at the arm's call site, and composes with it.

**Change.** `index_signatures.rs`' union arm calls
`Checker::composite_index_infos(id, |c| c.union_index_infos(&types))`
(`perf_links.rs`). `apparent_mapped_type` and `resolve_mapped_type_members`
still run before the arm on every ask. Convention record:

- **Native operation**: `resolveUnionTypeMembers` publishing the union's
  `indexInfos` from `getUnionIndexInfos` (`checker.go`, pinned `5b1047d`),
  once per type.
- **Key and owner**: the union's `TypeId` after `apparent_mapped_type`,
  private to one `Checker`. Value: the infos (`Vec<IndexInfo>`, small
  `Copy` records) or `None` (incomplete index work).
- **Publication**: `Some` and `None` are both published, and only under
  `publishable_since` — a `None` from a constituent whose own index work
  was cut short by an active resolution observes that resolution and is
  not published; a `None` computed on settled state is the same `None` the
  next ask computes.
- **Context**: admitted only with no alias-evaluation binding, mapped
  template or identity-unmapped frame open (as §4); otherwise the old
  computation runs.
- **Work boundary**: one `union_index_infos` per union id instead of one
  per ask; a hit clones the (usually empty) vector.

**Measured** (on top of §3 + §4):

| | §3 + §4 | §3 + §4 + §5 | change |
|---|---:|---:|---:|
| jsTyping check Ir | 82,634,379,393 | 77,763,565,480 | **−4.87 G (−5.9%)** |
| dm Ir | 1,116,799,001 | 1,112,051,742 | −0.43% |
| dml Ir | 4,500,218,431 | 4,476,498,794 | −0.53% |
| gi Ir | 343,531,001 | 343,532,232 | +0.0004% |

Both dumps identical; CLI output identical on all four projects.

**How we would know it is wrong.** A writer that rewrites a published
index-info list of a union constituent (or its apparent type's) after the
union was first asked; the same property r5-checkperf2 §4 rests on.

## §6 The stack, gates and wall

**Identity and gates** (the stack, §3 + §4 + §5):

- Both dumps identical to the base's after cutting `ms=`/`mib=` (12,238
  diagnostics rows, 556,291 type rows), for each of the three stacked
  states; CLI output `cmp`-identical on dm, dml, gi and jsTyping.
- `slowcases`: the full-corpus run flagged 76 diagnostics and 2 type cases
  `SLOWER`, all from that dump sharing the box with a test build (e.g.
  `resolutionModeTripleSlash2` 47 → 2,489 ms). Re-run on an idle box, base
  and new binaries over the same flagged cases (`TSR_FILTER`, 1,220 cases
  matched): **0 SLOWER / OVER_BUDGET / FAILED / MISSING** on both dumps.
- `cargo test --release -p tsr-checker` passes; `cargo clippy -p
  tsr-checker --all-targets -- -D warnings` reports nothing in the touched
  code (it reports pre-existing stable-toolchain lints elsewhere, e.g.
  `index_signatures.rs:247`); `cargo fmt --all` clean.
- On the integration tip `c75e4af` (batch AP): the three diffs apply
  cleanly, and both dumps of the tip with the stack applied are identical
  to the tip's own (`cmp` after cutting `ms=`/`mib=`; 12,238 diagnostics
  rows, 556,300 type rows), each pair produced by binaries built from that
  tip.

**Ir** (callgrind; bench projects whole process, jsTyping check phase):

| | dm | dml | gi | jsTyping check |
|---|---:|---:|---:|---:|
| base `51d8da2` | 1,118,504,743 | 4,508,819,635 | 343,533,796 | 96,561,689,803 |
| §3 | 1,117,888,950 | 4,509,451,027 | 343,517,486 | 90,057,393,303 |
| §3 + §4 | 1,116,799,001 | 4,500,218,431 | 343,531,001 | 82,634,379,393 |
| §3 + §4 + §5 | **1,112,051,742 (−0.58%)** | **4,476,498,794 (−0.72%)** | 343,532,232 (−0.0005%) | **77,763,565,480 (−19.5%)** |

**Wall** (fresh processes, base / stack / native tsgo rotated each round,
one warmup round dropped, medians of wall and `wait4` user+sys,
`whole_project_perf.py`'s flags, default multi-threaded mode; tsgo built
from the pinned submodule):

| project | rounds | base/tsgo | stack/base wall | stack/base CPU | **stack/tsgo** |
|---|---:|---:|---:|---:|---:|
| domain-model | 31 | 0.721 | 0.982 | 1.007 | **0.708** |
| domain-model-large | 31 | 0.676 | 0.984 | 0.983 | **0.666** |
| generic-imports | 31 | 0.830 | 0.984 | 0.995 | **0.817** |
| jsTyping | 5 | 3.746 | **0.855** | **0.833** | **3.203** |

jsTyping medians: base 9.49 s wall / 19.61 s CPU, stack 8.11 s / 16.33 s,
tsgo 2.53 s / 6.55 s. On the bench projects the stack's wall and CPU sit
inside this container's noise (their check phases move under 1%); on
jsTyping, where the three answers are hot, wall and CPU fall 15–17%.

## §7 What is left

- r5-checkperf2's branch (§3, §4 commits and the enum-payload diff) had not
  landed when this lane measured; its §4 and §5 here touch the same
  `union_index_infos` call path from opposite ends and were not measured
  together.
- `literal_in_const_type_variable_context` still resolves each call once
  (964 resolutions on jsTyping). Native's answer during the call's own
  resolution comes from the candidate being checked (the argument's
  contextual type is the candidate's parameter); this port's main
  resolution does not park `resolving_signature_calls`, so an ask made
  while the call's own non-generic candidates are checked resolves the
  call again, nested. Parking the sentinel in `calls.rs`' resolution would
  remove those resolutions but also changes what `contextual.rs`' readers
  of the sentinel answer during resolution; not attempted (main's file).
- `is_pure_signature_type`'s remaining cost is the 683 k primitive and
  literal operands (cheap each) and the relater's per-relation overhead
  (r5-checkperf2 §8, `relater.rs`, r5-relater7's file).
- The composite name enumeration declines for 137 k of 138 k union asks
  because a constituent's apparent type has no certified declared table
  (`members.rs:3233` at the base); native has no such decline. Making the
  enumeration answer for those unions is a fidelity change in main's file
  and would *add* certification work per union (now once per id).
