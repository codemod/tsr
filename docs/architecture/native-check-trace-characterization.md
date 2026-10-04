# Pinned native check-trace characterization

Beads `tsr-1yb.1.2.3` owns performed-work producers. At TSR inspection revision
`8209086f3d466ed404ab4fa1586e300e440e567a`, clean native revision
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` already provides an unsampled
full-source-check boundary through `--generateTrace`. Reuse that boundary before
adding a native full-check patch. This does not complete either compiler's
eligibility, lazy forcing, worker-budget or benchmark-validation contract.

## What the existing trace means

- `internal/checker/checker.go:2196`: `checkSourceFile` pushes its trace inside
  the `!links.typeChecked` guard. Begin marks first full-worker entry; end is
  deferred until the entire function returns, including unused checking when
  requested in that call. Later unused-only calls produce no full-worker event.
  Neither end nor `typeChecked` certifies uncanceled checking or diagnostic
  success.
- `internal/tracing/tracing.go:267`: `Push(..., true)` emits every begin/end;
  `Push(..., false)` samples events at 10 ms boundaries. Sampled relation or
  expression events cannot count all expensive work or lazy cross-file forcing.
- `internal/checker/tracer.go:29`: the wrapper attaches `checkerId`, a pool
  index. Native trace `pid` is the synthetic constant 1 and `tid` is a synthetic
  visualization identity, not an OS process/thread receipt.
- `internal/compiler/checkerpool.go:42,95`: the native pool defaults to four,
  honors single-threaded/explicit checker options and clamps capacity. Each
  construction task creates a type tracer before `NewChecker`; affinity uses
  the full Program file index modulo pool size. Observed full-check IDs, tracer
  indices, requested capacity and measured concurrent check intervals are
  different evidence. None alone proves a memory admission budget.
- `internal/compiler/program.go:713`: eligibility includes noCheck,
  declaration/default-library skips, project references and script kind/check-JS
  rules. JSON is excluded from full-source checking. Its imported value is typed
  in `getTypeOfVariableOrParameterOrPropertyWorker` at `checker.go:16578`.
- `internal/tracing/tracing.go:440`: stopping dumps types before closing the
  trace. `tracer.go:330` may call `TypeToString` for display and suppress display
  panics. Tracing can force additional work; it is not a transparent general
  worker/recomputation counter. Keep observation overhead outside acceptance
  timing and distinguish post-check type dumping from ordinary checking.
- `internal/execute/tsc.go:27,43`: trace start/stop I/O errors print warnings
  rather than changing the compiler's exit contract. A normal exit cannot prove
  that tracing started or completed.

## Public controls and receipt

The local helper `/tmp/tsr-native-trace-characterization-8209.py` creates five
public inputs: `index.ts` imports numeric JSON, a cyclic A/B interface pair and
an exported declaration with an unresolved type. Assigning the JSON value to
`string` deliberately produces TS2322; checking the declaration adds TS2304.
Options use ES2022, ESNext/bundler, strict, resolveJsonModule, esModuleInterop,
no emit and incremental/composite false. Only `index.ts` is a configured root.

Each case runs a fresh clean native process without tracing, with tracing, then
with repeated tracing. All 15 children preserve complete stdout/stderr, exit
status and ordered loaded identities. Enabled/repeated traces have balanced
full-worker begin/end records and the same `(checkerId, source path)` multiset.
Parallel global chronology is not required to be identical; it is not a
canonical diagnostic or ownership order.

| Request | Loaded | Full checks | Type-tracer indices | Full-check IDs | Peak active full-check intervals | Diagnostics |
| --- | ---: | ---: | --- | --- | ---: | ---: |
| Default | 68 | 3 | 0/1/2/3 | 0/1/3 | 3 | 1 |
| Single-threaded | 68 | 3 | 0 | 0 | 1 | 1 |
| Two checkers | 68 | 3 | 0/1 | 0/1 | 2 | 1 |
| noCheck | 68 | 0 | 0/1/2/3 | none | 0 | 0 |
| skipLibCheck false | 68 | 67 | 0/1/2/3 | 0/1/2/3 | 4 | 2 |

The default three full checks are index/A/B; the declaration and 63 libraries
are skipped, while JSON supplies the actual numeric assignment error lazily.
Disabling the declaration skip adds 64 checks, leaving JSON outside this phase.
The zero-check case still records four type tracers. Do not collapse these
different populations into one "checked files" or "workers" field.

A sixteenth child passes a regular file as the trace directory. It prints
`Warning: Failed to start tracing`, exits 1 for its intentional TS2322, and
creates no trace. The producer must reject this trace evidence independently
of compiler diagnostic completion.

The receipt retains actual PIDs, commands, per-child wall/CPU/RSS, full output,
loaded order, begin/end identities, trace/legend hashes and pinned-source file
hashes. Single off/on/repeat observations are not paired overhead estimates:
for example declaration-check wall times are 109.0/256.4/260.4 ms. These only
show why tracing stays outside throughput acceptance, not a project speed claim.

Local receipt: `/tmp/tsr-native-trace-characterization-8209/receipt.json`.

| Identity | SHA256 |
| --- | --- |
| Clean native binary | `6340b230f887095fb7c82fedb41eecadd7dc49e4baa65a05ef4abb8ab4b74bc4` |
| Public fixture contents | `15ac0a1449e8c2a98b4594899dfe3883c0726fe1433ef87d9afa8a4122288d9a` |
| Local characterization helper | `35a82b9f6267412e97af589332c2ae3e2f1d71d4d0c9df2ee8d76f18e5661000` |
| Receipt | `85680c173cb3a80b2e111cabdfcedabe41a22e9df07b6eb75f3166da026fd651` |

Next producer work remains explicit eligibility and lazy JSON/cross-file forcing,
actual construction/admission evidence, source/binary/options/input provenance,
and partial/canceled/crashed/stale trace rejection. TSR needs its own matching
opt-in boundaries. `.1.2.4` owns harness validation; this characterization sets
neither work comparability nor any verified worker-budget flag. The comparable
full-project TSR/native median wall target <=0.50 remains unmet and unverified.
