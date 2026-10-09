# Parity lane `r6-checkperf` — the check phase after round 5 (round 6)

Lane: the checker's performance lane for round 6 of epic `tsr-2zk`
(`tsr-2zk.1131`), successor of [`r5-checkperf3.md`](r5-checkperf3.md) (its §7
is this lane's brief), [`r5-checkperf2.md`](r5-checkperf2.md) and
[`r5-checkperf.md`](r5-checkperf.md). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on
equivalent complete work. Nothing here changes an answer: every diff keeps
both conformance dumps and the CLI output byte-identical.

Sections are numbered so code can cite them (`r6-checkperf.md` §N).

The lane owns `perf_links.rs` and `index_signatures.rs`. Every change that
needs a hook in main's files (`calls.rs`, `members.rs`, ...) ships as a diff
in this directory, applied in this order (each on top of the previous; each
state builds and lints clean on its own):

1. [`r6-checkperf-main-park.diff`](r6-checkperf-main-park.diff) — §2
   (`calls.rs`, `perf_links.rs`).

They are diffs rather than commits because the `perf_links.rs` functions are
unused without their call sites in main's files, and an unused function fails
the lint gate (the same reason as `r5-checkperf3.md`).

## §1 Method and base

- **Base**: `claude/beautiful-shannon-ar5gh0` @ `b18aec06` (main `17265fac`
  plus round-6 bookkeeping), frozen before any edit. r5-checkperf3's three
  diffs are on it (`perf_links.rs`' `r5-checkperf3` block).
- **Identity**: both unfiltered dumps (`diagverdictdump`, `verdictdump`) from
  the base's release build, run alone; "identical" means `cmp` after cutting
  the `ms=`/`mib=` columns (12,238 diagnostics rows, 556,303 type rows). CLI
  output `cmp`-identical to the frozen base binary on dm, dml, gi and
  jsTyping (`--pretty false`). `slowcases` on both dumps of every state.
- **Ir**: `valgrind --tool=callgrind` on the `profiling` build (the base's
  from a worktree at `b18aec06`). Bench projects whole process,
  `tsr -p benches/projects/<p>/tsconfig.json --singleThreaded --pretty false`:
  `domain-model` ("dm"), `domain-model-large` ("dml"), `generic-imports`
  ("gi"). jsTyping: check phase only
  (`--toggle-collect='*Checker*check_source_file*'`) on r5-checkperf §8's
  scratch config (`src/jsTyping/tsconfig.perf.json` in the TypeScript
  submodule, `--singleThreaded`, removed afterwards).
- **Wall**: fresh processes, the binaries rotated each round, one warmup
  round dropped, medians of wall and `wait4` user+sys, `whole_project_perf.py`'s
  flags (`--noEmit --incremental false --composite false --pretty false`,
  default multi-threaded mode). 31 rounds on the bench projects, 5 on
  jsTyping; native tsgo built from the pinned submodule by
  `scripts/offline-cargo/build-tsgo.sh`. Ratios are compared within one run
  (machine-to-machine spread is larger than the changes).
- **Environment**: PyPI is blocked; a stdlib stand-in for the three
  `tomlkit` calls of `scripts/offline-cargo/assemble.py` on `PYTHONPATH`,
  outside the repository (`r5-operators3.md` §4).

**Base measurements** (this container):

| | dm | dml | gi | jsTyping |
|---|---:|---:|---:|---:|
| Ir | 1,092,635,723 | 4,365,790,760 | 343,689,167 | 68,710,735,572 (check) |
| TSR wall (s) | 0.1731 | 0.6623 | 0.0698 | 6.761 |
| tsgo wall (s) | 0.2323 | 0.8456 | 0.0766 | 2.307 |
| **TSR/tsgo wall** | **0.745** | **0.783** | **0.910** | **2.931** |
| TSR/tsgo CPU | 0.504 | 0.558 | 0.459 | 2.187 |

Against round 5's closing numbers (0.708 / 0.666 / 0.817 / 3.203, measured on
r5-checkperf3's container) the bench ratios read higher and jsTyping lower;
they are a different machine, and jsTyping's check phase fell from 77.8 G
(r5-checkperf3 §6) to 68.7 G Ir with main's round-5 merge.

## §2 The call's own resolution parks the call link

**Forcing measurement** (`r5-checkperf3.md` §7): the call link
`call_resolves_const_type_parameter` still resolved each call once, and an
ask made while the call's own candidates are checked resolved the call
again, nested. At the base it costs **2.89 G inclusive** of jsTyping's
68.7 G check phase.

**Native.** `getResolvedSignature` (`checker.go:8407`) stores
`resolvingSignature` in `signatureLinks.resolvedSignature` before
`resolveSignature` runs and replaces it with the result afterwards; a read
during the resolution (`getContextualTypeForArgumentAtIndex`,
`checker.go:29783`) sees the sentinel and does not resolve again.

**This port.** `check_call_expression_worker` (`calls.rs`) now parks the call
around `resolve_call_signature_at`, and `call_resolves_const_type_parameter`
(`perf_links.rs`) answers `false` on the park — the answer it already gives
on its own `resolving_signature_calls` re-entry. Convention record:

- **Native operation**: `getResolvedSignature`'s `resolvingSignature` store
  around `resolveSignature` (`checker.go:8427`), read by
  `getContextualTypeForArgumentAtIndex`.
- **Key and owner**: the call's `NodeId` in
  `PerfLinks::main_resolving_calls`, private to one `Checker`; an entry
  lives exactly as long as the call's main resolution.
- **Publication states**: absent = not resolving; present = native's
  sentinel. A call already parked (by a re-entrant check of the same call)
  resolves nested, as native's re-entry does, and the outer frame unparks.
  Nothing is published: the call link's publication rules
  (`r5-checkperf3.md` §3) are unchanged, and an ask answered by the park
  does not publish.
- **Context**: the park is kept **apart from** `resolving_signature_calls`.
  Parking that set itself (the faithful shape: native has one link) was
  measured first and changes answers: `contextual.rs`' argument readers
  answer `any` on it, and this port's argument checks reach those readers
  during the call's own resolution where native reads the pushed contextual
  type instead. Diagnostics: `correlatedUnions` and
  `intersectionSatisfiesConstraint` EMPTY_RIGHT → EMPTY_WRONG (a TS2345 and
  a TS2322); types: RIGHT lines 549,853 → 549,835 net, with
  `doYouNeedToChangeYourTargetLibraryES2015:0:329` moving the other way
  (e.g. `correlatedUnions:0:194` prints `keyof DocumentEventMap` for `K`). So only
  the call link reads the main park; the contextual readers keep their
  current park. Unifying them waits on the contextual readers getting their
  answer from the candidate being checked, as native's do (main's
  `contextual.rs`).
- **Work boundary**: one resolution fewer per ask made inside the call's own
  resolution.

**Measured** (against the base):

| | base | §2 | change |
|---|---:|---:|---:|
| jsTyping check Ir | 68,710,735,572 | 67,672,682,180 | **−1.04 G (−1.5%)** |
| `call_resolves_const_type_parameter` incl. | 2,888,873,334 | 1,575,277,677 | −1.31 G |
| dm Ir | 1,092,635,723 | 1,092,791,384 | +0.01% |
| dml Ir | 4,365,790,760 | 4,366,533,878 | +0.02% |
| gi Ir | 343,689,167 | 343,673,020 | −0.005% |

Wall/CPU (state/base, same rotated runs): dm 1.010 / 1.032, dml 0.983 /
1.012, gi 1.020 / 1.030, jsTyping 1.040 / 1.022 at 31 rounds (5 for
jsTyping); dm and gi re-run at 41 rounds, base and state only: dm 0.980 /
1.002, gi 1.008 / 0.996. The bench projects move by the park's hash traffic
(+0.16 M / +0.74 M Ir); jsTyping's wall at 5 rounds is inside its noise
(the Ir fall is 1.5%).

Both dumps identical; CLI output identical on all four projects; `slowcases`
clean on both dumps; `cargo test --release -p tsr-checker` 1,762 passed.

**How we would know it is wrong.** A call whose nested re-resolution, asked
during its own resolution, chose a signature with a `const` type parameter:
the park answers `false` where the nested resolution answered `true`. The
dumps are the probe (no such call in the corpus); jsTyping declares no
`const` type parameter at all, so its CLI output cannot see it.

## §3 The composite name enumeration's decline (refused: 0.06 G)

**Brief**: the composite enumeration declined for 137 k of 138 k union asks
(`r5-checkperf3.md` §2, `members.rs:3233` then), because a constituent's
apparent type has no certified declared table; port
`getPropertiesOfUnionOrIntersectionType`'s real enumeration.

**Measured at the base**: the number no longer holds. r5-checkperf3 §4's memo
publishes the decline once per composite id, and `is_pure_signature_type`
now tests the signature list first. On jsTyping the whole composite
enumeration is **515 worker runs, 61 M Ir inclusive (0.09% of the check
phase)**, plus 26,533 non-composite runs at 15 M. An instrumented build (not
committed) counted **431 declines** at the certified-table test in the whole
run, nearly all `Named` interface types from `types.ts` (`Identifier`,
`ElementAccessExpression`, `StringLiteral`, ... — the AST node interfaces
the union constituents are).

So the port of the real enumeration is a fidelity change with at most
0.06 G (0.09%) to win, and it would make those unions answer names where
they now decline, which `is_pure_signature_type` and every other reader of
`get_property_names_of_type` would see — not a byte-identical change. Not
attempted in this lane. The fidelity half stays with main's `members.rs`
(why `collect_declared_properties` cannot certify the `types.ts` node
interfaces is the open question).
