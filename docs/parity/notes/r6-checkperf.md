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
   (`calls.rs`, `perf_links.rs`);
2. [`r6-checkperf-jsdoc-deferral.diff`](r6-checkperf-jsdoc-deferral.diff) —
   §4 (the parser, `tsr-compiler`'s driver, three benches' option literals, a
   parser test, and the decision record ADR-0051 it needs). Independent of
   the first: it also applies alone on the base.

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

Wall/CPU, state/base (rotated with base, §4's state and tsgo, 31 rounds; 5
for jsTyping): dm 0.996 / 0.976, dml 1.009 / 1.034, gi 0.934 / 0.924,
jsTyping 1.043 / 0.930; dm, dml and gi re-run at 41 rounds, base and state
only: dm 1.018 / 1.028, dml 1.012 / 1.008, gi 1.014 / 0.999 — inside the
protocol's ±3% at the Ir change (+0.01%, +0.02%, −0.005%); jsTyping's 5
rounds swing ±7% on CPU around a 1.5% Ir fall.

*Correction.* The first wall run of this section (and the base row of §1's
table, which is unaffected) used binaries copied after
`cargo build --release -p tsr -p tsr-conformance --examples`, which builds
only the examples: the "state" binary was the base binary, byte for byte.
Those state/base numbers (dm 1.010 / 1.032 ...) measured nothing and are
replaced by the ones above, from `cargo build --release -p tsr` run on its
own; the CLI comparison was re-run on the real binary (identical). The dumps
and the Ir were never affected (the examples and the `profiling` build were
rebuilt).

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

## §4 dm and gi: the top self-cost is upstream-absent JSDoc work (diff + ADR-0051)

**Profile** (callgrind, whole process, §2's state). Top self-cost after the
allocator:

| | 1st | 2nd | 3rd | then |
|---|---|---|---|---|
| dm | `Scanner::scan` 35.6 M (3.3%) | `Scanner::bump` 35.4 M | `Scanner::peek` 31.5 M | `jsdoc_ranges_in` 25.3 M, `scan_jsdoc_comment_text_token` 22.3 M |
| gi | `Scanner::bump` 34.1 M (9.9%) | `Scanner::peek` 29.7 M | `Scanner::scan` 27.5 M | `jsdoc_ranges_in` 25.3 M, `scan_jsdoc_comment_text_token` 22.3 M |

The native operation is `scanner.Scan`, but the cost is not scanning tokens:
`Parser::parse_jsdoc_at` is **148.7 M Ir inclusive on dm (13.6% of the
process) and 148.9 M on gi (43.3%)** — the same number on both, because it
is the default lib `.d.ts` files' documentation (`lib.dom.d.ts` and the
ES libs), parsed on every compilation. Native does not do this work:
`withJSDoc` (`internal/parser/jsdoc.go:56`) only flags a documented node of a
non-JavaScript file and parses its comments eagerly only when one carries
`@see`/`@link`/`@linkcode`/`@linkplain`; everything else waits for a lazy
`Node.JSDoc()` read, and the checker's error paths read only `EagerJSDoc`
(`checker.go:2255`, `grammarchecks.go:916`). The two lazy readers in the
checker feed suggestions (`GetJSDocDeprecatedTag` from
`addDeprecatedSuggestionWorker`, `checker.go:14044`; `getAllJSDocTags` from
`checkUnmatchedJSDocParameters`, `errorOrSuggestion` — a suggestion in a TS
file). It is "work native caches" in the brief's sense in its strongest form:
native defers it until a reader asks, and on the CLI's path nothing asks.

**Change** (`r6-checkperf-jsdoc-deferral.diff`, with
[ADR-0051](../../adr/0051-jsdoc-deferred-in-checked-ts-files.md) in the
diff, since it changes ADR-0010's driver default): `ParseOptions` gains
`defer_ts_jsdoc`; with it, `parse_jsdoc_at` parses a construct's comments
only when one carries one of the four tags (`hasJSDocTag`'s test,
`scanner.go:372`). `ParseOptions::deferring_ts_jsdoc(name)` sets it for
every file that is not JavaScript by extension, and `tsr-compiler`'s loader
and `Program::parse` — the CLI's and the conformance harness's parse sites —
apply it. The option's default stays off. This is not a cache, so the
checker port convention's cache record does not apply; the decision is the
ADR's.

A first probe (not shipped) dropped JSDoc in every `.d.ts` file: gi
0.915 → 0.622 and dm 0.761 → 0.672 of tsgo's wall, CLI identical. Rejected
for having no upstream counterpart (ADR-0051, alternatives).

**Measured** (on top of §2):

| | §2 | §2 + §4 | change |
|---|---:|---:|---:|
| dm Ir | 1,092,791,384 | 974,688,306 | **−118.1 M (−10.8%)** |
| dml Ir | 4,366,533,878 | 4,248,089,430 | −118.4 M (−2.7%) |
| gi Ir | 343,673,020 | 225,414,551 | **−118.3 M (−34.4%)** |
| jsTyping check Ir | 67,672,682,180 | 67,707,544,263 | +0.05% |

The same ~118 M on every bench project is the lib files' share; jsTyping's
check phase does not parse.

Wall, rotated base / §2 / §2+§4 / tsgo, 31 rounds (5 for jsTyping):

| project | base/tsgo | §2/tsgo | **§2+§4/tsgo** | §2+§4 / base, wall · CPU |
|---|---:|---:|---:|---:|
| domain-model | 0.793 | 0.789 | **0.729** | 0.919 · 0.951 |
| domain-model-large | 0.774 | 0.781 | **0.751** | 0.970 · 1.009 |
| generic-imports | 0.967 | 0.903 | **0.662** | 0.686 · 0.685 |
| jsTyping | 2.783 | 2.902 | **2.899** | 1.042 · 0.939 |

(This run's base reads higher against tsgo than §1's: dm 0.793 against
0.745, gi 0.967 against 0.910 — the container's spread between runs; compare
within a row.)

**Identity and gates** (§2 + §4 stacked): both dumps identical to the base's
(12,238 + 556,303 rows), and the corpus's total case time fell from 724.8 s
to 484.4 s (summed `ms=` of the diagnostics dump), which shows the harness parses through the gated
path; CLI identical on dm, dml, gi and jsTyping; `slowcases` clean on both
dumps; `cargo test --workspace --release` 3,484 passed, 0 failed (with the
new `crates/tsr-parser/tests/jsdoc_deferral.rs`, 4 tests); `cargo clippy
--no-deps --all-targets -- -D warnings` clean on `tsr-parser`,
`tsr-compiler`, `tsr-binder` (the `Parser`'s two JSDoc bools are folded into
one `JSDocMode`, as `struct_excessive_bools` requires); `cargo fmt` clean.

**How we would know it is wrong.** ADR-0051's falsifiers: a conformance case
whose errors depend on a TS-file comment without `@see`/`@link` (none on
this corpus), or a checker reader of such a comment ported later.

**The checker phase of dm and gi has no recomputed link left to cache.**
Check-phase-only profile of dm (§2 + §4, 664.6 M Ir): no checker function
above 2.1% self; the allocator is 16%. The largest cluster is name
resolution (`BindResult::resolve_name` 3.5% inclusive): 20.0 M of it under
`resolve_identifier_memo`, which is r5-checkperf §4's port of native's
`links.resolvedSymbol` — 15,601 resolutions for 32,395 asks, i.e. one
resolution per `(identifier, meaning)`, native's own work. gi's check phase
is 5 M Ir (1.5% of the process). Nothing here is a links field the port
recomputes; the allocator share is `tsr-2zk.1092`'s (Signature copies,
r5-checkperf2 §2).
