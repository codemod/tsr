# Fixed-plan leased-read admission

Task `tsr-1yb.19.2.2.1`, base `17c4db04` plus hashed opt-in sources, supplies
bounded progress controls for the [leased preparation API](loader-read-reservation-api.md).
It changes no production VFS, parser, checker, loader or scheduler. The
[complete evidence](loader-read-budget-pool.json) records source/binary/input
hashes, physical controls, resource observations and accounting qualifications.

## Policy and ownership

`read_budget_pool` accepts a fixed manifest and direct/1/2/4 modes. Workers
receive a path and the shared logical budget, and return owned `PreparedText`.
Parsing and consumption run on the coordinator in manifest order. Each worker
has one outstanding read/result; the next batch starts after current callbacks.

Reservations are fail-fast. A worker never waits for credits while holding an
incomplete read. The coordinator drains the entire current batch before any
callback or retry decision. If any reservation failed and every input in that
batch was an opened regular file, it drops **all** speculative results, then
retries the entire batch once in canonical serial order. This avoids a queued
later result retaining credits needed by an earlier retry. Each retry result is
consumed before the next serial read; a second refusal returns a typed resource
error immediately. There is no indefinite retry, file skipping, uncharged
fallback or conversion of resource refusal into missing I/O.

Regular-file classification uses metadata on the opened file only to decide
whether repetition is permitted; sizes never authorize allocation. Repeating
reads assumes stable re-readable inputs. This prototype does not freeze files,
validate inode identity or support changing filesystem hosts. It adds metadata
queries and can repeat successful reads, so it is not selected as an I/O win.
FIFOs and other non-regular streams are never reopened for implicit retry;
their successful output is preserved or a failure is returned explicitly.

A consumer can retain prepared text, and its charge remains live across moves.
If held text prevents another preparation, serial retry can still fail; the
policy cannot manufacture credits. Dropping the consumer's retained values
releases their charges. All pending batch results are dropped on early failure
or consumer unwind. Endpoints live inside the scoped-thread closure, so unwind
drops them before the scope joins idle workers. No next batch is in flight.

The private fault-test reader seam catches worker panic in an unwind build and
returns a worker failure. Repository release builds use `panic=abort`, which
does not permit such recovery. The CLI explicitly rejects its controlled
`unwind` action before reading a manifest in that profile. Panic cleanup is
proven with the separate Rust test runtime, not asserted for an aborting CLI.
Blocked OS open/read calls remain uncancellable; these finite controls do not
establish shutdown for an indefinitely stalled device or producer.

## Accounting and controls

The unchanged lower-level API charges raw, UTF-16 temporary and decoded
requested layouts, conservatively overlapping old/replacement layouts during
growth. Queued/consumer-held text retains its lease. Exact reported capacity is
qualified to inspected Rust 1.96.0/default Global sources; the public reserve
method does not promise it on every allocator. An excess-capacity guard observes
allocation after it happens and cannot establish a portable preallocation bound.
The model excludes usable-byte rounding/retention, thread/stack/channel/error
storage, manifest bookkeeping, and parser/arena allocations. Logical reserved
peak is not an observed simultaneous allocator peak or an RSS ceiling.

Two complete driver runs each cover 12 physical cases and 48 mode audits:
empty/tiny plans, skew and encodings, forced low-budget contention, oversized
and stale-size files, UTF-16 expansion, missing/directory failures, FIFO
success/refusal and consumer retention. Successful outputs match the same-build
ordinary reader's complete text/parser/node/JSDoc/reference/diagnostic images.
All modeled reservations balance after drop and stay within the stated limit.
This parser witness creates a fresh per-file arena; it does not prove a shared
program node domain or dynamic loader task identity.

Native byte checks reuse a reader with all 53 compiled native module source
files verified against pinned `5b1047d1`. Supported physical bytes agree;
malformed UTF-8 passthrough versus Rust replacement remains `tsr-34z`.
Ten release example tests pass: five inherited API controls and five pool
controls. Four fault tests also run as separate processes under 30-second
timeouts, covering forced queued-result contention, consumer unwind, worker
panic/non-repeatable refusal and partial-read failure/unwind. Finite FIFO
writers have a two-second join bound. Four release CLI controls prove an unwind
request is explicitly unsupported with `panic=abort`.

Workspace formatting, strict all-target Clippy and 2,860 tests pass (six tests
remain ignored). All 3,405 upstream anchors and 16,630 section citations resolve.
The aggregate gate stops at the existing 190 historical dangling issue IDs
tracked by `tsr-10`; none of this unit's issue references are dangling. Section
verification was therefore run separately. The aggregate gate is recorded as
failed, not relabeled green.

## Cost observation

Three fresh-process samples per mode read/drop 128 stable regular files
(11,534,336 raw bytes), under an 8 MiB logical budget. No retries occurred.
The confirming run observes:

| Mode | Wall median | CPU median | Peak RSS range | Startup median | Logical reserved peak |
|---|---:|---:|---:|---:|---:|
| direct | 11.808 ms | 10.731 ms | 2,129,920 B | 0 | 524,288 B |
| 1 | 12.347 ms | 11.349 ms | 2,981,888–4,292,608 B | 29.042 µs | 524,288 B |
| 2 | 10.349 ms | 11.755 ms | 4,292,608–5,111,808 B | 31.542 µs | 557,056 B |
| 4 | 9.322 ms | 12.696 ms | 4,620,288–5,177,344 B | 37.875 µs | 622,592 B |

Process wall includes fresh startup, argument/manifest loading and shutdown.
Pool startup covers channel/thread creation through worker-ready messages.
Pool wall also includes callbacks; these resource samples use only read/drop,
while payload controls include parser/printing cost. Thread count, finite input
hashes and successful consumption are verified. Host activity and warm OS file
caches are uncontrolled; these small observations are not a retained normal
whole-project speed comparison. Four readers increase CPU/RSS; one-reader
queue overhead also appears. No winner or native ratio follows from this table.

## Reproduction and remaining gates

```sh
CARGO_BUILD_JOBS=1 cargo build --release -p tsr-compiler \
  --example read_budget_pool --example read_allocations
CARGO_BUILD_JOBS=1 cargo test --release -p tsr-compiler --example read_budget_pool
python3 docs/architecture/loader-read-budget-pool-controls.py \
  --pool /absolute/release/examples/read_budget_pool \
  --reference /absolute/release/examples/read_allocations \
  --native-reader /absolute/pinned-native-reader \
  --test-binary /absolute/release/examples/read_budget_pool-TEST_HASH \
  --output /absolute/new-controls-directory
```

Use the test executable path printed by Cargo, and the native-reader wrapper
from the [allocation characterization](loader-read-allocation-contract.md).
The driver requires an unused output directory and writes/verifies results
after each observation. Scratch logs retain the first failed CLI unwind audit;
the corrected control classifies the profile explicitly.

`tsr-1yb.19.2.2.2` owns leased reads under dynamic canonical discovery replay
after `tsr-1yb.19.1`. Parent `.19.2.2` remains open for that integration.
Production `.5` retains full work/output, node/host ownership, native replay and
independently confirmed real-project benefit gates. No full checker corpus was
rerun for these isolated examples. The comparable fresh-process whole-project
TSR/pinned-tsgo median wall ratio <=0.50 remains unproved.
