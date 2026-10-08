# Parity lane `r5-loader` — program construction wall (round 5)

Lane: `tsr-2zk.995`, the program-construction wall of epic `tsr-2zk`,
everything except lazy JSDoc (`tsr-2zk.17.1`, main's). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Predecessor measurement: [`r5-perf4.md`](r5-perf4.md) §7, which
found TSR's default mode *slower* than `--singleThreaded` on generic-imports.
Release target (CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent
complete work.

Sections are numbered so code can cite them (`r5-loader.md` §N). Every number
names the source state and measurement it came from.

## §1 Method

- Source base `a7d108d` (branch head at dispatch). 4-vCPU cloud container,
  Linux 6.18, `RUSTUP_TOOLCHAIN=stable`, glibc `malloc` (no global allocator
  is configured). Native tsgo built from the pinned submodule
  (`scripts/offline-cargo/build-tsgo.sh`).
- **Phase knobs.** A scratch build (not committed) read three environment
  variables: no dependency-parse pool, no parallel bind, and the old bind
  gate. Each variant's CLI output was compared with the base's on every run.
- **Wall/CPU.** Fresh processes, variants interleaved and order alternated
  each round, one warmup dropped; medians of wall, user and sys from
  `wait4` rusage. Single-run wall on this box varies by ±8 ms on an ~80 ms
  run (two copies of one binary measured 74.3 and 77.0 ms medians at 41
  samples; quartiles span ~17 ms), so CPU and repeated medians are the
  reliable columns.
- **Gates** (box protocol §5) on the committed code: both unfiltered dumps
  against the frozen base, the coverage run, tests, clippy/fmt, and
  `scripts/whole_project_perf.py` against the base binary and against tsgo.

**Native loader shape** (`internal/core/workgroup.go`,
`internal/compiler/filesparser.go:245-301`). `parallelWorkGroup.Queue` is
`wg.Go(fn)`: one goroutine per parse task, on the Go runtime's existing
GOMAXPROCS threads. Native has no admission threshold and no publication
copy: a goroutine parses straight into its own `SourceFile`, which *is* the
program's file. Binding is likewise one goroutine per file
(`program.go:472-486`). Nothing is cached across runs in the CLI: the
`ParseCache` (`internal/project`) is language-server only, so tsgo
re-parses every lib on every `tsc` run, exactly as TSR does. Lib re-parsing
is therefore equivalent work, not a gap.

TSR's parallel front end ([`parallel-front-end.md`](../../architecture/parallel-front-end.md))
differs in one structural cost: a worker parses or binds into *private*
tables, and the coordinator copies (AST) or relocates (symbols, flow) the
result into the canonical ones in file order. That publication is serial
and proportional to the file's size, so parallel work only pays when the
work it removes from the coordinator exceeds the publication it adds.

## §2 The dependency-parse pool and embedded lib texts

**Forcing measurement.** At base, on all three bench projects the pool ran
exactly 5 jobs on 4 workers, and every job was an embedded lib:
`lib.es5.d.ts` (219 KB) and four 18–40 KB `lib.es20xx` files. (`lib.dom.d.ts`,
2.35 MB of the 2.83 MB lib set — 83% — is above the pool's 1 MiB bound and
was parsed serially on the coordinator already.)

A pool job for an embedded text costs, beyond the parse it moves:
1. `read_static(..).to_owned()` — a copy of a `&'static str` into a `String`
   for the worker;
2. at the visit, `arena.alloc_str(prepared.source())` — a second copy, where
   the serial path borrows the static text (`load_task`);
3. `ParsedFile::publish` — a full-text equality check plus a copy of the
   whole AST into the canonical arena (the parallel-front-end probe measured
   publication at 0.4–0.75 of the private parse);
4. the pool itself: four threads spawned and joined, each with its own
   `malloc` arena (page faults on first touch).

And the overlap it buys is small: lib references are walked depth-first, so
a submitted lib is visited, and its ticket waited on, shortly after the
frontier that submitted it.

Medians, base `a7d108d` scratch build with knobs (21 samples; 11 for
domain-model-large):

| project | default | no dependency pool | no pool, no parallel bind | `--singleThreaded` | tsgo |
|---|---:|---:|---:|---:|---:|
| generic-imports wall ms | 85.7 | 76.8 | 73.8 | 71.5 | 72.4 |
| generic-imports CPU ms | 93.6 | 83.5 | 75.6 | 69.4 | 145.3 |
| domain-model wall ms | 188.1 | 169.9 | 161.3 | 307.3 | 188.2 |
| domain-model-large wall ms | 656.3 | 620.6 | 620.5 | 1374.2 | 711.0 |

**Change** (`FileLoader::prepare_dependencies`): a task whose text the host
serves as `read_static` is not eligible for the pool. It is not read ahead
either (no `Serial` copy); `load_task` borrows it at the visit, as the serial
path always did. Non-static texts — `node_modules` declarations, the case the
pool was built and measured for (parallel-front-end.md, "Dependency
confirmation": 768-file fixture −23% wall, API 477 jobs) — are unchanged.
Because pool creation counts *eligible* tasks, a program whose only
candidates are libs now never spawns the pool.

**Output.** Parse results are identical by construction: the pool's
publication contract already guaranteed byte-identical ASTs and node ids,
and the serial path is the reference it is tested against.

**How we would know it is wrong.** A lib-heavy program (many `--lib`
entries, or `libReplacement` packages, which are *not* static and stay
eligible) where the serial lib walk becomes the critical path while
workers sit idle; the measurement would be `Dependency parse work` near
zero with Parse time rising against the base.

## §3 The parallel-bind gate

**Forcing measurement.** The old gate was "the program has ≥10,000 nodes",
which every program with a default lib passes (lib.dom alone is 124,175
nodes). Node counts per bind (scratch instrumentation, base):

| project | files | nodes | largest | largest / total | bind serial → parallel (ms, extendedDiagnostics, 7 runs) |
|---|---:|---:|---:|---:|---|
| generic-imports | 66 | 155,256 | 124,175 | 0.80 | 14–16 → 17–19 (loss) |
| domain-model | 105 | 202,052 | 124,175 | 0.61 | 20–28 → 19–24 (neutral) |
| domain-model-large | 265 | 387,492 | 124,175 | 0.32 | 37–42 → 28–39, one run 49 (win) |

A model fits all three: with W workers the bind takes about
`max(largest, total/W)` of private work, then the coordinator publishes in
file order; publication measured at ~0.4 of bind per node here (generic-imports:
serial 14.5 ms for 155K nodes; parallel 18 ms ≈ 11.7 ms for lib.dom +
6.3 ms of publication). The parallel path therefore loses unless the work
*outside* the largest file is at least comparable to it.

**Change** (`Program::bind_source_files`): fan out only when the unbound
files hold ≥10,000 nodes **and** the largest file is at most half of them
(`2 * largest <= total`). generic-imports and domain-model now bind serially;
domain-model-large and the 192-declaration frontend fixture (lib.dom 124K of
876K nodes) keep parallel bind. The front-end test fixture (eight equal
files, `noLib`) still takes the parallel path.

Native binds one goroutine per file with no publication step, so native has
no analogue of this gate; it is a property of TSR's private-then-publish
design (parallel-front-end.md), and it goes away if publication does.

**How we would know it is wrong.** A program with largest/total just above
0.5 whose serial bind is measurably slower than the parallel one; the
threshold is the model's break-even, not a measured optimum, and the
neutral domain-model point (0.61) says the true break-even is near it.

## §4 Result (committed code)

`scripts/whole_project_perf.py`, default mode, 21 samples (domain-model-large
11), run back to back on an idle box. "Self" puts the base binary in the
`--tsgo` slot (ratio new/base); the vs-tsgo rows ran base and new against
the same native binary in the same session.

| project | self CPU | self wall | base / tsgo wall | new / tsgo wall | new / tsgo CPU |
|---|---:|---:|---:|---:|---:|
| generic-imports | **0.800** (97.4 → 77.9 ms) | 0.865 | 1.165 | **0.978** | 0.493 |
| domain-model | **0.937** | 0.952 | 0.929 | **0.865** | 0.559 |
| domain-model-large | **0.953** | 0.983 | 0.885 | 0.889 | 0.603 |

`diagnostics_match: true` on every pair. Process-level, generic-imports:
system time 29 → 8 ms and minor page faults 9,458 → 5,682 (one run each,
`wait4`); `strace -c` shows syscalls are not the remaining sys time — it is
first-touch page faults of the parse and bind arenas.

Corpus: both unfiltered dumps (`diagverdictdump` 12,238 rows, `verdictdump`
552,533 rows) are `cmp`-identical to the base. Coverage rows `checker_types`
8183/9538, `diagnostics` 4404/5502 (the checked-in snapshots lag the base
build; no dump line moved). `cargo test --workspace --release`: 3,240 passed,
0 failed, including the three `front_end::tests` and the
`parallel_front_end` integration tests; the eight-equal-file fixture still
exercises parallel bind under §3's gate.

## §5 Where generic-imports' wall goes now, and what is left

After this commit generic-imports runs the whole front end on the
coordinator, which is what `--singleThreaded` did (wall 75.6 ms new vs
77.7 ms `--singleThreaded` base at 41 samples — the same within noise).
`--extendedDiagnostics`: Parse ~45 ms, Bind ~14 ms, Check 1 ms, Program
~75 ms. `lib.dom.d.ts` is 83% of the parsed bytes and 80% of the bound nodes.

**Native gets nothing from parallelism here either.** tsgo default 71.6 ms vs
`--singleThreaded` 68.4 ms (r5-perf4 §7); its wall is lib.dom's parse+bind on
one goroutine. So the ratio on this project is per-file front-end speed on
one large file, and 0.50 needs TSR to parse and bind `lib.dom.d.ts` in about
half of Go's time. Levers, by size:

1. **Lazy JSDoc** (`tsr-2zk.17.1`, main's): 37% of Ir (r5-perf4 §7). Not this
   lane.
2. **Pipelined bind.** Bind is ~14 ms serial after the last parse. Binding
   file *k* while the coordinator parses file *k+1* needs the binder to read
   rows of a `NodeTable` the coordinator is still appending to (an
   append-only, concurrently readable table), or private-bind publication
   cheaper than ~0.4 of the bind (§3). Not attempted: it changes the
   `NodeTable` ownership contract (ADR-0003) and needs its own design record.
3. **First-touch page faults.** ~5,700 minor faults (~8 ms sys) for a 30 MB
   peak RSS. A larger initial arena chunk or `MAP_POPULATE` would move, not
   remove, most of them; unmeasured.

Not available as a lever: lib parse caching. Native's CLI re-parses every
lib per run (§1), so TSR doing the same is equivalent work.
