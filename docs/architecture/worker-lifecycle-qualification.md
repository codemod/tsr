# Cross-tool worker lifecycle qualification

`tsr-1yb.1.2.3.2.3` consumes the shipped TSR interval producer and the
[archived pinned native producer](native-worker-activity.md). The public
[supervisor](worker-lifecycle-controls.py) and the independent
`scripts/checker_work_trace.py::validate_worker_activity` qualify their bounded
lifecycle mechanics. [Frozen observations](worker-lifecycle-qualification.json)
bind the source, binaries, native patch, reader, helper and raw receipt.

The forcing constraint is that constructed instances and configured counts are
different from simultaneous work. A nested query is still activity on one
private checker. Native's pool selection and file affinity also differ from
TSR's current serial driver. Treating their requested counts as equal execution
would conceal that difference before production scheduler APIs are introduced.

## What the reader establishes

The explicit `--worker-producer tsr|native` seam selects a known schema. Ordinary
artifact validation remains the default. Both modes rehash the supervising
receipt's binary, qualified source/patch files and partial input snapshots;
bind the actual child PID, invocation identity, arguments and process outcome;
and reject trace warnings, signals, timeouts, changed bytes and malformed JSON.
Receipt authenticity and binary build provenance remain outside the reader.

Records stream through strict monotonic-clock validation. The reader retains
Program identities and active spans rather than all query records. Each category
tracks nesting depth per private checker, so nested calls contribute one owner.

| Category | Meaning |
| --- | --- |
| constructing | Actual constructor brackets |
| semantic | Covered full-file and named query/worker spans |
| full | First full-file worker intervals |
| leased | Native exclusive mutex occupancy; TSR reports null because it does not observe leases |
| observed | Union of construction and covered semantic activity |

Each category reports distinct-owner peak, `wall_ns` (the temporal union with
any owner active), and `checker_ns` (the integrated distinct-owner count).
Summing nested span durations would count the same checker repeatedly and is
rejected as the model for this summary. These elapsed intervals include observer
bookkeeping and scheduling; they are not CPU utilization or throughput timing.

TSR activity requires additive schema 1, valid constructor start/return order,
one private owner, balanced returned spans, and independently recomputed peaks.
The existing work-policy reader also validates its complete Program inventory,
eligible full-file scope and effective options. Explicit listFilesOnly accepts
the inventory with zero constructed checkers and zero workers; the ordinary
semantic reader retains its existing requirement for completed eligible work.

Native checks the exact pinned selection formula, complete ordered Program
inventory, unique native-checker-to-private-slot mapping, all constructor returns
before affinity publication, complete modulo affinity, non-overlapping exclusive
leases, unique balanced tokens and full-file ownership. Query entries include
cache answers and are not recomputation counts. Nonexclusive access is recorded
without certifying safety. This validates named mechanics, not native eligibility
or every possible checker access path.

All required-work, complete-input, complete-provenance, safe-memory and speed
flags remain false. Neither valid artifacts nor matching public full-file counts
clear the semantic comparator `tsr-1yb.1.2.4.2`, initialization/lazy forcing
`tsr-1yb.1.2.3.1`, private-store admission `tsr-1yb.3.1.1.3`, production workers
or comparable TSR/pinned-native median wall <=0.50.

## Real process controls and limits

Frozen TSR source is `c30db42a`; native is
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` plus the archived patch. A fresh
task-owned TSR archive build enables work-trace. Canonical vendor and production
Rust source are unchanged by this unit.

Fifteen public mode/project pairs run both compilers: default, singleThreaded,
explicit 1/2, single precedence, file and 256 caps, noCheck, library modes,
listFilesOnly, small no-lib and skewed programs. Public minimal globals let the
no-lib fixtures reach actual checking rather than stop on missing global types.
Cyclic type imports, lazy JSON and deliberate TS2322 diagnostics remain in the
cycle fixture. Unsupported zero is rejected by CLI option validation; private
pool zero/negative boundaries stay in the native producer's focused Go tests.

135 normal children include showConfig, off/on/repeat and original-native
baselines. All 60 traces qualify; complete stdout, stderr, exit status and ordered
loaded identities match within each compiler. Full-file affinity is stable across
repeat; parallel chronology and overlap can vary. Each child's wall, own CPU and
peak RSS are retained separately. No cross-tool diagnostic or total-work
equivalence is inferred from these within-tool controls.

Six additional real children prove stale-file refusal, missing-parent failure
and SIGKILL for each producer. The signal control stops its own live child,
captures an open full-file span in the frozen sidecar prefix, then kills and
reaps that exact PID. An actual signal overrides any recorded end marker.
TSR buffers 64 KiB, so the query-heavy fixture exposes records during checking;
a short trace can stay invisible until normal completion. An open buffered
prefix cannot prove the exact physical checking state at signal delivery. The
control proves process failure rejection, not live scheduler utilization.

Eight separately rehashed corruption artifacts fail for replayed PID, bad clock,
false peak and duplicate completion. Focused reader tests additionally exercise
duplicate native identities, wrong affinity, overlapping leases, partial pools,
aborts, token replay, corrupted constructor clocks and initialization-only work.
The proof-first TSR controls fail on the earlier reader's missing worker seam.

Raw receipt: `/tmp/tsr-worker-qualification-public-final/receipt.json`.
The summary records hashes and redacts raw project paths. The result is falsified
by changed outputs, an accepted malformed/failing child, double-counted nesting,
wrong ownership, or a coverage flag promoted beyond the observed operations.

## Replay

Use Python 3.9+ on POSIX with wait4 and Go 1.26 for the native archive recipe.
Build a TSR work-trace binary from an isolated source archive. Build native
baseline and probe from the [exact patch recipe](native-worker-activity.md#reproduction-and-observed-controls).
Use fresh output paths; signal controls require permission to signal their own
compiler children. Existing artifacts are never overwritten.

```sh
python3 docs/architecture/worker-lifecycle-controls.py \
  --tsr-probe /absolute/task/target/release/tsr \
  --native-probe /absolute/native/native-probe \
  --native-baseline /absolute/native/native-off \
  --tsr-source /absolute/task/source \
  --native-source /absolute/native/source \
  --tsr-sha c30db42a \
  --native-sha 5b1047d10d32e7d5b446be4de56b126ff42f82bb \
  --output /absolute/fresh/controls

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH=scripts python3 -m unittest discover -s scripts -p 'test_*.py'
```
