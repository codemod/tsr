# Parity lane `r5-bind` — per-file front-end speed (round 5)

Lane: `tsr-2zk.1028` (= `tsr-2zk.1003`), the front-end wall of epic
`tsr-2zk` after [`r5-loader.md`](r5-loader.md). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Lazy JSDoc (`tsr-2zk.17.1`) and checker memos (`tsr-2zk.17`)
are main's and are not touched here. Release target (CLAUDE.md): TSR/tsgo
median wall ratio <= 0.50 on equivalent complete work.

Sections are numbered so code can cite them (`r5-bind.md` §N). Every number
names the source state and measurement it came from.

## §1 Method

- Source base `7b4029e` (branch head after merging `origin/main` at
  dispatch). 4-vCPU cloud container, Linux 6.18, glibc 2.39 `malloc`,
  `RUSTUP_TOOLCHAIN=stable`, transparent huge pages `madvise`. Native tsgo
  built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`).
- **Ir**: `valgrind --tool=callgrind`, release `tsr`, `--singleThreaded
  --pretty false`; line attribution from a separate
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` build.
- **Phase timeline**: a scratch build (not committed) printing a timestamp
  at loader start/end, around every parse in `load_task` and every serial
  bind in `bind_source_files`.
- **Wall/CPU**: `scripts/whole_project_perf.py`, default mode, 21 samples,
  against the frozen base binary (median child CPU) and against tsgo.

## §2 Where the front end's time goes

Callgrind, generic-imports, base (399.6 M Ir): `Program::from_root_files`
97.9%; parse 76.7%; bind 15.2%. Inside parsing, JSDoc parsing
(`parse_leading_jsdoc` and the JSDoc scanner entry points, plus
`jsdoc_ranges_in`) is ~44% of all Ir — main's lazy-JSDoc lane.

`lib.dom.d.ts` is 2,349,323 bytes, of which **1,560,839 (66%) are comment
text** and 144,281 whitespace outside comments. Every byte of comment text
crossed the trivia scanner's block-comment skip one decoded `char` at a time
(`bump` + `peek`, both out of line), which was the largest non-JSDoc
scanner self cost.

Phase timeline (scratch probe, base, one run each on a loaded box, so
proportions only):

| project | loader | of which `lib.dom.d.ts` parse | serial bind | of which `lib.dom.d.ts` |
|---|---:|---:|---:|---:|
| generic-imports | 79 ms | 55.6 ms | 21 ms | 17.6 ms |
| domain-model | 104 ms | 47.7 ms | 23 ms | 17.4 ms |

On both projects the loader's parse order is roots, then the lib walk
(`lib.esnext.full` → … → `lib.es5`, then `lib.dom` and the small DOM
companions), so `lib.dom.d.ts` finishes parsing at the end of the load, and
program order (libs sorted by priority, `collect_files`) binds it after the
`es` libs and before the roots.

## §3 Block comments skipped by bytes (`Scanner::skip_block_comment`)

**Change.** The skip scans bytes until the first line break (only `*/`,
`\n`, `\r` and the UTF-8 lead of U+2028/U+2029 matter, none of which a
UTF-8 continuation byte can equal), then finds each `*` with `str::find`
(the standard library's word-at-a-time `memchr`) and tests the next byte
for `/`. Same terminator, same "crossed a line" answer, same unterminated
diagnostic span (`start..limit`), so the token stream is identical by
construction; `block_comments_end_at_their_first_terminator_wherever_it_is`
covers `*` runs, non-ASCII text, CRLF and both Unicode terminators before
and after the first break, and unterminated comments.

Upstream (`scanner.go` `Scan`, the `/*` arm) also loops per character; the
port's byte loop is an implementation choice of this scanner, like the
trivia loop beside it.

**Measured** (callgrind, generic-imports, `--singleThreaded`):
399,573,555 → **373,783,521 Ir (−6.45%)**; CLI output `cmp`-identical.

## §4 Pipelined bind (lever 1): measured, not built

The brief's lever 1 is binding file *k* while file *k+1* parses. The
timeline in §2 bounds what it can buy:

- Program order is only known when the walk ends (`collect_files` sorts the
  libs), and the bind must run in program order (symbol ids, global merges).
  A serial pipelined binder can therefore only overlap the files that both
  precede `lib.dom.d.ts` in program order *and* finish parsing before it
  does: the `es5`/`es20xx` libs, 3–4 ms of bind.
- `lib.dom.d.ts` itself (≈80% of the bound nodes) can only be bound after
  its own parse, which is the last large parse of the load, so its ~13 ms
  bind stays on the critical path either way.
- Binding the roots ahead of `lib.dom.d.ts` needs private binds and
  publication, which r5-loader §3 measured at ~0.45 of the bind, so a root
  bind of ~5 ms on domain-model saves under 3 ms.

The structural cost is an append-only, concurrently readable `NodeTable`
(new `unsafe` outside `arena.rs`, ADR-0011) or a two-level row lookup on the
checker's hottest read path (`parent`/`kind`). Against a 3–4 ms ceiling this
is refused; no ADR is written because nothing in ADR-0003's ownership
contract changed. **What would change this:** a program whose largest file
is parsed early and bound late (the opposite of the lib walk), or a lib walk
that parses `lib.dom.d.ts` first — the second changes `NodeId` order and so
needs its own identity argument.

**Gates** (commit `r5-bind: skip block comments by bytes`): both unfiltered
dumps `cmp`-identical to the base (`diagverdictdump` 12,238 rows,
`verdictdump` 552,533 rows); `cargo test --workspace --release` 3,253
passed, 0 failed; clippy/fmt clean on `tsr-scanner`.

Whole-project, this box (which ran ~30% slower than during r5-loader, so
compare ratios, not milliseconds):

| measurement | generic-imports | domain-model |
|---|---:|---:|
| `whole_project_perf.py` self, median CPU new/base (21) | 1.009 | **0.952** |
| interleaved, median CPU base → new (31) | 110.9 → **102.2** ms | 461.7 → **455.5** ms |
| interleaved, wall / tsgo base → new (31) | 1.063 → **0.974** | 0.785 → **0.734** |
| interleaved, CPU / tsgo base → new (31) | 0.533 → 0.491 | 0.498 → 0.491 |

`diagnostics_match: true` on every pair. The sequential (non-interleaved)
harness runs against tsgo drifted by more than the change (generic-imports
base 101 ms vs new 106 ms CPU in back-to-back sessions, against −8% when
interleaved), which is why the interleaved rows are the ones reported.

## §5 Where domain-model's wall goes (after §3)

`--extendedDiagnostics`, medians of 9 runs of the §3 binary on this box:

| phase | domain-model | generic-imports |
|---|---:|---:|
| loader (parse 75 / 57 of it) | 87 ms | 63 ms |
| serial bind | 31 ms | 24 ms |
| program total | 119 ms | 92 ms |
| checker init | 1 ms | 1 ms |
| check (4 checkers) | 129 ms | 1 ms |
| compilation | 252 ms | 94 ms |

So domain-model is about half front end, half check; the check half is
main's lane (`tsr-2zk.17`). Inside the loader (scratch probe, three runs):
reading the 42 roots 1.2–1.5 ms; root parse preparation 4.5–8 ms on the
pool plus 7.8–9 ms of publication; serially the same roots parse in 11.8–13.7
ms. Both orders reach the lib walk at 17–23 ms, which is r5-loader §6.1's
neutral result again; the lib walk then runs to the end of the load
(`lib.dom.d.ts` ≈ 45–50 ms of it).

## §6 Measured and refused

1. **Arena chunks on transparent huge pages** (2 MiB-aligned 4 MiB chunks
   after the first 1 MiB, `madvise(MADV_HUGEPAGE)`; THP is `madvise` here).
   Minor faults fell 5,652 → 4,312 (generic-imports) and 11,330 → 9,405
   (domain-model), but sys time did not: the kernel still zeroes the same
   bytes. Interleaved, two sessions: generic-imports wall 101.8 → 98.1 ms
   (31 samples) then 98.0 → 101.0 ms (41); domain-model 230.2 → 225.8 then
   217.5 → 220.6. The sign flips between sessions, so it is noise. The
   other ~4,300 faults are spread over `Vec`/hash-table growth, and
   `GLIBC_TUNABLES` mmap/trim/top-pad thresholds left the fault count
   unchanged (they are first touches, not re-faults). Not built.
2. **Pipelined bind** (§4): ceiling 3–4 ms, needs a concurrently readable
   `NodeTable`. Not built.
3. **Overlapping the lib walk with root parsing** by parsing libs on
   workers: this is r5-loader §6.2's refused design (publication of
   `lib.dom.d.ts` ≈ 15 ms on the critical path); §5's timeline adds
   nothing that changes its arithmetic.

## §7 What is left for the front end, by measured size

Callgrind, generic-imports after §3 (372.9 M Ir):

1. JSDoc (main, `tsr-2zk.17.1`): `bump`+`peek` 63.8 M (≈90% from
   `scan_jsdoc_comment_text_token`/`scan_jsdoc_token`), `jsdoc_ranges_in`
   25.3 M, `scan_jsdoc_comment_text_token` 22.3 M self, and
   `mentions_tag` 6.3 M (two `@`-tag sweeps per `/**` comment in
   `classify_block_comment`, 7,411 comments) — together ≈ 40%.
2. Binder 60.7 M (16%): `bind_inner` 18.2 M self, the child visitor
   (`push_children`) 10.9 M, symbol-table inserts ~12 M. Binding semantics
   are not this lane's.
3. Non-JSDoc scanning is now small: `scan` 27.6 M self, identifiers 10.1 M,
   keyword lookup 3.0 M.
