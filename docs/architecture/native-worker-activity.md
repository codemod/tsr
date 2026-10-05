# Pinned native worker activity producer

`tsr-1yb.1.2.3.2.1` archives an opt-in producer against native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. The canonical vendor and production
Rust checker remain unchanged. The [patch](native-worker-activity.patch) contains
three native integration changes, a small observer package and focused Go tests.
The [public driver](native-worker-activity-controls.py) produces fresh-process
controls; the [qualified summary](native-worker-activity.json) records hashes and
observations. These are lifecycle mechanics, not a throughput acceptance run.

## Why a separate producer

Native generateTrace supplies full-file events but uses synthetic trace PIDs and
samples other forcing. A count of type tracers cannot prove simultaneous activity,
exclusive access, constructor intervals or file affinity. Reusing its PIDs as
process evidence would be false. Deriving activity from configured counts would
also conceal idle instances and nested queries.

The archived producer brackets actual constructors and mutex acquisitions and
observes selected semantic entries without changing their results. It uses a
separate stream so type dumping and display forcing cannot contaminate this
bounded observation contract. The accepted cost is synchronous observer I/O and
bookkeeping: measured probe-on/off overhead stays outside ordinary speed timing.
An upstream complete lifecycle observer could replace this patch; it must expose
these same boundaries and preserve output under the controls below.

## Native anchors and event meanings

All anchors name the unmodified pinned tree, not line numbers after this patch.

| Native boundary | Evidence emitted |
| --- | --- |
| `internal/compiler/checkerpool.go`, `newCheckerPoolWithTracing` | Actual selected count, requested count, singleThreaded and complete Program file array |
| `createCheckers`, around each `checker.NewChecker` call | Constructor begin/return per pool slot; attachment after return records native checker ID |
| `createCheckers`, actual file association publication | Program array file ID and assigned slot, after the association is stored |
| `GetChecker` / `getCheckerForFileExclusive` | Lease starts after mutex acquisition and ends before actual unlock; existing OnceFunc release stays intact |
| `forEachCheckerParallel` / `forEachCheckerGroupDo` | Exclusive lease around each callback/group, including idle groups |
| `getCheckerNonExclusive` / `getCheckerForFileNonExclusive` | Explicit access record with exclusive ownership unproved |
| `internal/checker/checker.go`, `checkSourceFile`, under `!links.typeChecked` | Actual first full-file worker, including the function's unused-check tail; cached calls produce no full span |
| `getTypeOfSymbol` / `getDeclaredTypeOfSymbol` | Query entry/return, including cached answers; these are not expensive-worker counts |
| `cmd/tsgo/main.go`, CLI `runMain` | Actual process PID, random invocation nonce, arguments and normal return status; LSP/API routes are outside coverage |

Native selection defaults to four, singleThreaded selects one before an explicit
count is considered, then clamps to at most the complete file count and 256, with
a minimum of one. Affinity uses the complete Program array index modulo that
selected count. The observer follows the stored association rather than assigning
workers itself. Pool IDs and slots are invocation-local; the recorded native
checker ID is a private checker identity, never a semantic TypeId or reusable key.

The version-1 NDJSON header declares monotonic elapsed nanoseconds. Every record
is timestamped under the observer mutex, so recorded chronology is monotonic;
concurrent ordering may differ between children. `pool_selected` and
`program_file` can exist before instances are constructed. `file_affinity` follows
actual publication. `checker_created` follows its successful constructor return.
`span_begin` and `span_end` carry a unique token; kinds are constructor, lease,
source_file_check, symbol_type_query and declared_type_query. Query file ID -1
means unattributed: the probe does not force declaration resolution to find one.
Full-file IDs come from the checker's complete file-index map.

End peaks count distinct `(pool, slot)` identities: constructing, covered semantic,
full-file, leased and the union of construction/covered semantic activity. Leases
are a separate category; holding a mutex alone does not prove checking work.
Nested queries increase span counts without increasing the distinct checker peak.
Idle created instances are separate from activity. These clocks include observer
work and scheduling delay; they are neither CPU utilization nor memory admission.

## Failure and coverage limits

`TSR_NATIVE_WORK_ACTIVITY` is read only by the archived CLI probe. It opens a new
0600 file exclusively; stale paths are never overwritten. Write failures warn on
stderr and disable further records while compiler results remain intact. A direct
deferred span end records panic abortion and rethrows the original panic. Pending
spans, aborted spans, failed writes or absence of normal CLI return make completion
false. A canceled CLI context does not emit a qualified normal return. External
supervision must still reject a signaled or timed-out child, even if it had already
written an end record.

The recorder is installed before CLI workers begin and cleared after they join.
Its process-local global pointer supports one CLI invocation, not concurrent
embedded requests. It never owns or shares mutable semantic stores. Standalone
NewChecker callers, nonexclusive safety, global diagnostics without a covered
query, initialization forcing inside NewChecker, unused-only cached-file work and
other lazy paths remain unqualified. The header keeps initialization/all forcing,
complete provenance and actual work equivalence false; memory admission is null.

The [TSR interval producer](tsr-work-trace-producer.md#direct-activity-intervals)
and this native stream have separate schemas. The sibling
`tsr-1yb.1.2.3.2.3` still owns independent cross-tool worker qualification, and
`tsr-1yb.1.2.4.2` owns required-work comparison. Broader initialization/lazy hooks
remain `tsr-1yb.1.2.3.1`; private-store admission remains `tsr-1yb.3.1.1.3`.
These controls do not clear scheduler, fidelity, memory or comparable median
TSR/native <=0.50 gates.

## Reproduction and observed controls

Apply the patch to a fresh archive of the exact pin, outside the canonical vendor.
Build an unmodified binary before applying it, then build the probe from the same
copy. Native requires Go 1.26; this receipt used go1.26.0 darwin/arm64.

```sh
# From the TSR repository; keep this task's source and artifacts isolated.
TASK_NATIVE_COPY=$(mktemp -d /tmp/tsr-native-worker-replay-XXXXXX)
git -C vendor/typescript-go archive 5b1047d10d32e7d5b446be4de56b126ff42f82bb | tar -x -C "$TASK_NATIVE_COPY"
(cd "$TASK_NATIVE_COPY" && go build -o native-off ./cmd/tsgo)
(cd "$TASK_NATIVE_COPY" && git apply /absolute/path/to/tsr/docs/architecture/native-worker-activity.patch)
(cd "$TASK_NATIVE_COPY" && go test ./internal/workactivity ./internal/compiler ./internal/checker ./cmd/tsgo)
(cd "$TASK_NATIVE_COPY" && go test -race ./internal/workactivity ./internal/compiler -run 'Test(Activity|WorkerActivity)' -count=1)
(cd "$TASK_NATIVE_COPY" && go vet ./internal/workactivity ./internal/compiler ./internal/checker ./cmd/tsgo)
(cd "$TASK_NATIVE_COPY" && go build -o native-probe ./cmd/tsgo)
python3 docs/architecture/native-worker-activity-controls.py --baseline "$TASK_NATIVE_COPY/native-off" --probe "$TASK_NATIVE_COPY/native-probe" --output "$TASK_NATIVE_COPY/controls"
```

The example keeps binaries and controls inside the new task-owned directory;
the helper requires a fresh output directory.

Pre-work TSR `1dfcdb93` locates the development session; the observed compiler is
native pin plus the archived patch, not that TSR revision. The original native
binary fails the new control because no sidecar exists, before implementation.
Focused observer/pool tests pass, including empty/small selection, count clamping,
exclusive/repeated release, idle instances, nested activity, pending construction,
panic preservation and writer failure. Complete affected Go package tests and
focused race tests pass. Fresh patch application reproduces all six tested source
files exactly.

Eight public modes each run original, probe-off, probe-on and repeat: 32 normal
children preserve complete stdout, stderr, status and ordered loaded identities.
All 16 traces independently recompute their peaks and exclusive leases. Three
additional children prove stale-file refusal, missing-parent failure and live
SIGKILL after the current header appears. The stale sentinel is unchanged; the
killed child exits -9 and has no invocation_end.

| Mode | Created checkers | First full-file workers |
| --- | ---: | ---: |
| Default | 4 | 3 |
| singleThreaded | 1 | 3 |
| checkers 2 | 2 | 3 |
| checkers 8 | 8 | 3 |
| noCheck | 4 | 0 |
| skipLibCheck false | 4 | 86 |
| skipLibCheck false / skipDefaultLibCheck | 4 | 4 |
| listFilesOnly | 0 | 0 |

The library count belongs to this NodeNext/default-target fixture; it must not be
substituted for the earlier 67-file fixture's evidence. Per-child wall, CPU and
peak RSS are retained separately for observer overhead. Internal query chronology
and activity peaks can vary under parallelism; stable full-file affinity/output
are checked. None of these observations measures a TSR speed improvement.

Raw receipt: `/tmp/tsr-native-worker-activity-_vxx_snw/public-final/receipt.json`.
The qualified JSON summary records original/probe/patch/helper/receipt hashes and
all mode results without publishing raw fixture paths. The native vendor remains
at the exact clean pin. The proof is falsified by changed outputs, inconsistent
slot assignment, overlapping exclusive leases, unmatched spans, false completion
under failure, or an observer that introduces semantic forcing.
