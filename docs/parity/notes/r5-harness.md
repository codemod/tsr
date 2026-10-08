# r5-harness — the conformance gate's robustness

Harness box on epic `tsr-2zk`. Owns `crates/tsr-conformance/examples/*`, the
runner and harness code in `crates/tsr-conformance/src`, and this file. No
checker, binder, parser or scanner file is touched. Frozen base: `1252ab9`
(integration head when the box started).

## 1. `tsr-2zk.1041`: a case that runs away scored like a fast one

### The hole

The zero-loss gate (`box-protocol.md` §5.2, `scripts/parity_gate.sh compare`)
joins two dumps on key and verdict. A verdict compares diagnostics or type text
only. At `99b337b`, `varianceProblingAndZeroOrderIndexSignatureRelationsAlign`
grew without bound when run alone (r5-relater4 §1), yet both full dumps scored
it EMPTY_RIGHT. The cost of a case was not in the dump at all.

There were three ways a runaway case could disappear:

1. **It finishes, expensively.** It is scored on its text alone. This is what
   happened at `99b337b`: the case finished inside the full dump, which has
   more memory headroom than the other three workers happened to need at
   that moment.
2. **It kills the process.** That means the kernel's OOM killer (exit 137), an
   allocation failure, a panic or a stack overflow (all exit 134, see
   §2). Both dumps print only at the end, so the TSV is empty. `parity_gate.sh
   compare` reports every key as missing, which is loud. A hand-rolled `join`
   prints nothing for an empty file, so it passes silently.
3. **It panics.** See §2: under the release profile's `panic = "abort"` this
   is case 2.

### Measured at `1252ab9`: four cases are already over 10 s

The first guarded run (below) gave the per-case distribution on the full
diagnostics corpus, 12,238 cases, on this box (4 cores, 15 GiB):

| Case | Wall | Peak memory |
|---|---:|---:|
| `compiler/varianceProblingAndZeroOrderIndexSignatureRelationsAlign` | 61.5 s | 3,456 MiB |
| `compiler/relationComplexityError` | 46.0 s | 847 MiB |
| `compiler/varianceProblingAndZeroOrderIndexSignatureRelationsAlign2` | 45.1 s | 3,456 MiB |
| `compiler/performanceComparisonOfStructurallyIdenticalInterfacesWithGenericSignatures` | 14.3 s | 1,470 MiB |
| `compiler/ramdaToolsNoInfinite` | 1.3 s | 126 MiB |
| every other case | ≤ 0.53 s | ≤ 304 MiB |

The fix `25a8f62` stopped the *unbounded* growth, but the case is still the
most expensive in the corpus by an order of magnitude. Native checks it in
well under a second. §5 lists the issues to file for them. Their existence shaped the
checker below: a gate that fails on the base itself gates nothing.

### What the dumps now record

`crates/tsr-conformance/src/case_guard.rs` wraps every case of both dumps
(`diagverdictdump`, and `verdictdump` through `verdict::verdict_rows_guarded`):

- **Two trailing columns, `ms=<wall>` and `mib=<peak>`,** on every row, after
  the existing ones. Those keep their bytes. `cut -f1-4` of a guarded diag dump
  is `cmp`-identical to the unguarded one, and so is `cut -f1-4` of a guarded
  types dump (Overhead, below). The columns are tagged so a reader finds them from the
  right. A types row's `want`/`got` column can itself be a number (`>x : 1`),
  so an untagged trailing integer would be ambiguous between an old 4-column
  row and a new one. A types row repeats its case's numbers.
- **Peak memory per case** comes from a counting allocator
  (`examples/support/counting_alloc.rs`, `System` plus a thread-local
  counter). A case runs start to finish on one rayon worker: the checker
  spawns no threads, and the dumps start no nested parallel work. So the
  thread's high-water mark of bytes allocated and not yet freed, relative to
  the case's start, is the case's.
- **A watchdog thread** (100 ms poll) prints `SLOW <case> <s>s <MiB>MiB` to
  stderr once a case passes `TSR_CASE_WARN_S` (10 s). If a case passes
  `TSR_CASE_MEM_MIB` (6,144 MiB) or `TSR_CASE_TIMEOUT_S` (600 s), or the
  in-flight cases together hold more than `TSR_DUMP_MEM_MIB` (default three
  quarters of `MemTotal`, which blames the largest case), the watchdog writes
  that case's `OOM` or `TIMEOUT` marker row to stdout, names the other
  in-flight cases on stderr, and exits with status 3. 6,144 MiB is 1.8× the
  largest legitimate case (3,456 MiB). It is a tripwire for unbounded growth;
  the gate's own memory budget is `slowcases`' 1 GiB.
- `TSR_CASE_INJECT=panic:<case>` or `grow:<case>` injects a fault into one
  case, so the markers can be tested in the build the gate actually runs.
- **A panic** writes the case's `PANIC` marker row before the abort (§2).
- `TSR_CASE_TRACE=1` prints `START`/`END` per case to stderr. A death that no
  hook sees (a stack overflow, the kernel's OOM kill) then leaves its in-flight
  cases in the log: the `START`s with no `END`.

`scorepair` reads `verdict::verdict_rows`, which runs every case under the
same guard but without the columns, because it stores its rows as its
baseline. It prints a `PANIC ⚠` line for any panic marker, whose `case:*:*` key
matches nothing in the baseline.

Marker rows keep the join key's shape: `case<TAB>OOM<TAB>-<TAB>detail<TAB>ms=…<TAB>mib=…`
for diagnostics, and `case:*:*<TAB>…` for types, which folds back to the case
name.

### Alternatives, and what they cost

| Option | Catches | Cost | Taken? |
|---|---|---|---|
| Wall-time column | slow cases that finish (hole 1) | an `Instant` per case | yes |
| Peak-memory column via a counting allocator | memory-heavy cases that finish fast; attributes growth to one case | one thread-local load/store pair per allocation (Overhead, below); one `unsafe impl GlobalAlloc` in tooling, the exception ADR-0011 already makes for `alloc_profile.rs` | yes |
| Process RSS polling (`/proc/self/statm`) | the process as a whole | no attribution: four workers share one RSS, and the diag dump's RSS already peaks at 4.7–7.9 GB on a clean run | no |
| `setrlimit(RLIMIT_AS)` per process | turns the kernel OOM into an allocation failure | still an abort, still unattributed; the limit applies to all four workers | no |
| A subprocess per case | everything, with exact attribution and per-case RSS | 12,238 process spawns, each re-reading and re-binding the lib files that a worker now caches once (`types_producer::LIBS`); far over the 2% budget | no |
| `catch_unwind` per case | panics, in an unwind build | nothing in a release build: the profile is `panic = "abort"` (§2) | kept for unwind builds (tests); the panic hook does the release work |
| A watchdog that marks `TIMEOUT` and carries on | hangs | a thread cannot be stopped from outside, and the stuck worker's case never yields a row; carrying on just waits forever | it ends the run instead (exit 3, marker row) |
| Treat every case over 10 s as a loss | slow cases | four cases at the base are already over (table above), so every gate would fail | over-budget-in-base is `KNOWN_SLOW`, not a loss, unless it also got 3× slower |

What would change these choices: a checker cancellation point (a budget the
relater polls, as native's `relationCount`/complexity limits do) would let
`TIMEOUT` and `OOM` mark one case and continue. If the counting allocator ever
costs more than the 2% budget, the memory column goes to an opt-in env var and
the wall-time column alone remains.

### Overhead

Measured against the base binaries (`1252ab9`, built by the same toolchain),
on this 4-core box:

| Measurement | Base | Guarded | Ratio |
|---|---:|---:|---:|
| callgrind `Ir`, `diagverdictdump`, `TSR_FILTER=compiler/ca` (67 cases), one thread | 25,334,787,870 | 25,496,171,096 | **1.0064** |
| median user CPU, 4 alternating pairs, `TSR_FILTER=compiler/c` (857 cases), `RAYON_NUM_THREADS=1`, `diagverdictdump` | 49.15 s | 49.40 s | 1.005 |
| same, `verdictdump` | 47.65 s | 48.55 s | 1.019 |
| same, median wall, `diagverdictdump` / `verdictdump` | 52.68 / 52.48 s | 53.47 / 50.63 s | 1.015 / 0.965 |
| full-corpus wall, 2 alternating pairs, default pool, `diagverdictdump` | 241.9, 235.6 s | 271.6, 231.4 s | noise |
| same, `verdictdump` | 215.6, 234.7 s | 214.9, 235.1 s | noise |

The instruction count is the number to trust: **+0.64%**, under the 2%
budget. Full-corpus wall varies ±12% between two runs of the *same* binary.
It is set by when the 45–62 s cases happen to start on the four workers, so
it cannot resolve a 2% effect. The single-thread filtered runs are within ±2%
of each other in both directions. The cost is the thread-local load/store pair
per allocation. Wall-time measurement, the registry lock (twice per case) and
the 100 ms watchdog poll do not register. Peak process RSS is unchanged
(4.3 GB types, 4.7–7.7 GB diagnostics in both).

Output: with the two trailing columns stripped
(`sed -E 's/\tms=[0-9]+\tmib=-?[0-9]+$//'`), both guarded full dumps are
`cmp`-identical to the base dumps, twice each. `cut -f1,2` is identical too.
Note that `cut -f1-4` is **not** a valid way to strip the guard columns from
a types dump: 203 types rows have tabs inside their type text (up to 118
fields), and so did the unguarded dump.

### How the integrator uses it

1. **Freeze the base with guarded binaries.** The first time, the base is an
   unguarded dump. Every case then has no base time, so `slowcases` reports
   the four slow cases as `OVER_BUDGET`. Re-freeze once after this lands.
2. Run both dumps as now. **Check their exit status**: 3 means the watchdog
   stopped a runaway case, 134 means a panic or abort. The last stdout line
   names the case either way, and stderr says what else was in flight. Do not
   gate on a dump that exited non-zero.
3. The key/verdict loss checks are unchanged. They read columns 1 and 2, and
   `scripts/parity_gate.sh compare` already fails on a missing key.
4. Add the cost check, once per dump:
   ```bash
   cargo run -q --release -p tsr-conformance --example slowcases -- $B/diag.tsv  $A/diag.tsv
   cargo run -q --release -p tsr-conformance --example slowcases -- $B/types.tsv $A/types.tsv
   ```
   Exit 0 is clean. Exit 1 lists the offenders:
   - `OVER_BUDGET`: over 10 s or 1 GiB, and within budget in the base;
   - `SLOWER`: more than 3× the base's wall time and over 1 s, or more than 3×
     its memory and over 256 MiB;
   - `FAILED PANIC|OOM|TIMEOUT`: a marker row;
   - `MISSING`: a case with rows in the base and none in the new dump.

   `KNOWN_SLOW` lines are informational. Flags: `--budget-ms`, `--budget-mib`,
   `--ratio`, `--floor-ms`, `--floor-mib`.
5. Wall time is measured with four workers sharing the box, so it carries
   scheduling noise. The 3× ratio and the 1 s floor are set so that two runs
   of the same binary never trip them (Overhead, above). Memory is per thread and does not
   depend on scheduling, apart from allocator noise.

A proposed one-line addition to `scripts/parity_gate.sh compare`, which this
box does not own: run `slowcases` on both pairs after the awk checks and OR its
status into `status`.

### Verified at `99b337b`

The harness files were copied onto a `99b337b` worktree (same vendor pin),
built, and run:

- **The case alone** (`TSR_FILTER=varianceProblingAndZeroOrderIndexSignatureRelationsAlign`,
  which also selects `…Align2`): `SLOW` for both at 10 s. At 82 s the
  watchdog stopped `…Align` at 6,159 MiB, printed its `OOM` row, and exited 3.
  `…Align2` was in flight at 6,160 MiB. Process RSS had reached 13.6 GB of
  15, close to the kernel's limit, which is why the whole-process budget
  (`TSR_DUMP_MEM_MIB`) was added.
- **The full diagnostics dump**: `SLOW` for the four known cases. At 79 s:
  `OOM compiler/varianceProblingAndZeroOrderIndexSignatureRelationsAlign` at
  6,159 MiB, with 11,015 MiB live across the in-flight cases. Exit 3, and the
  marker row is the dump's only row. `slowcases` against the base reports
  `FAILED OOM` for the case plus every other case `MISSING`, and exits 1.
  Before this change, that run scored the case EMPTY_RIGHT.
- **Fault injection on the guarded `1252ab9` build** (release profile, so
  `panic = "abort"`): `TSR_CASE_INJECT=panic:compiler/castTest` gives one
  `compiler/castTest<TAB>PANIC<TAB>…` row (types: `compiler/castTest:*:*<TAB>PANIC`)
  and exit 134. `grow:` with `TSR_CASE_MEM_MIB=512` gives an `OOM` row and
  exit 3. `grow:` with `TSR_DUMP_MEM_MIB=300` gives the same row through the
  whole-process budget.

## 2. `tsr-2zk.46`: intermittent SIGABRT in corpus runners

What §1 needed from this item: `[profile.release]` in the workspace
`Cargo.toml` sets `panic = "abort"`. Every corpus runner is built with it, so
a panic in any case aborts the process with SIGABRT (exit 134) before any row
is printed. The same goes for a stack overflow, which Rust reports from its
SIGSEGV handler and then aborts, and for an allocation failure
(`handle_alloc_error`). This is the reported symptom: an empty TSV and exit 134.
It is also why per-case `catch_unwind` alone cannot make a panicking case
visible in the dumps. The panic hook does.

## 3. `tsr-2zk.37`: `source_variable_initializer::javascript_keeps_its_existing_decline`

**Root cause: a stale expectation, already corrected on main. Nothing to
change.** The test asserted that a `.js` file with `allowJs` + `checkJs`
reports no TS2322/TS2739 for annotated initializers. `a0fa106e` ("supply JSDoc
annotations to CLI checking and initializer context") made the port report
them, and the test failed on main. That is `tsr-2i2`, closed 2026-10-05 with
this evidence: the pinned native `TestLocal` with `allowJs` + `checkJs`
reports the same initializer errors in `.js` as in `.ts`, beside each TS8010,
because a type annotation in JavaScript is still the declared type. Main
replaced the test with `javascript_reports_native_initializer_errors`, which
asserts the nine native occurrences.

The js box reported `tsr-2zk.37` at its base `0d996e82`. That base still
carries the old test (`git show 0d996e82:crates/tsr-conformance/tests/source_variable_initializer.rs`
has `fn javascript_keeps_its_existing_decline`) and the new behaviour, so it
fails there. The parity branch took main's corrected test in `638b0ae7`
(2026-10-05 23:10 UTC, after the report). At `1252ab9`, `cargo test
--workspace --release` passes 3,260 tests with 0 failures, including all nine
`source_variable_initializer` tests. No assertion was changed here. `tsr-2zk.37`
is a duplicate of `tsr-2i2` and can be closed.

## 5. Issues for the integrator to file

`bd` cannot be installed in this container (`box-protocol.md` §1). These are
written for the integrator to file:

- **The two `varianceProbling…` cases still take 45–62 s and 3,456 MiB at
  `1252ab9`**, after `25a8f62` removed the unbounded growth. Native checks
  them in well under a second. Same relater expansion as the one r5-relater3 hit with
  its held discriminated-type port, and r5-relater4 §1: `Either<L, (a: A) => B>` through discriminated
  decomposition. Checker lane (relater).
- **`performanceComparisonOfStructurallyIdenticalInterfacesWithGenericSignatures`:
  14–16 s and 1,470 MiB.** Checker lane.
- `relationComplexityError` (46 s, 847 MiB) is `tsr-2zk.1017`.
