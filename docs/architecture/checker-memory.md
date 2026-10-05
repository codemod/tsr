# Private checker memory and admission

`tsr-1yb.3.1.1.3` measures simultaneously live private requested allocations on
frozen Rust `4faf1cbd1c9c520e361174b69febfbd93e032dab`, with native pin
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. No safe automatic memory bound is
established. Keep the production CLI serial; `tsr-1yb.3.1.1.4` owns admission
before private stores grow. The existing ownership, augmentation and emit/query
prerequisites still apply. This evidence does not establish TSR/native <=0.50.

## Results and boundaries

[Machine-readable receipts](checker-memory.json) contain source/archive/patch
and binary hashes, all sample metrics and owner records, physical-input and
diagnostic fingerprints, actual checking counts, and private raw artifact hashes.
Private app diagnostics/source are not exported. The normal helper and observer
helper have byte-identical patched source; only the observer feature differs.
Canonical Rust and vendor source are unchanged.

Three alternating off/on pairs per worker count and per project qualify: 18
pairs total. Every complete Next.js child preserves 117 complete diagnostics,
1,397 actual completed file checks and 14,050 ordered loaded payloads. The
ordinary frozen TSR CLI independently preserves the same complete diagnostics
as the helper. Public small, skewed mapped-member and cross-file generic-cycle
controls also match pinned native diagnostics at one worker; helper 1/2/4 modes
preserve complete checking/output scope. This is partial observed input coverage
(loaded payloads plus root config), not all resolver queries/manifests, nor proof
of every lazy forcing operation or equal required work across tools.

Next.js private stores are held alive together at the checked rendezvous:

| Workers | Live requested bytes | Direct global peak bytes | Padded live layout bytes | Allocation/reallocation requests |
| --- | ---: | ---: | ---: | ---: |
| 1 | 145,855,984 | 146,007,995 | 168,485,424 | 32,416,622 |
| 2 | 167,069,892 | 167,220,692 | 192,032,840 | 35,461,511 |
| 4 | 208,035,249 | 208,229,173 | 235,760,080 | 38,209,091 |

All three repetitions have these same checked totals. One-worker construction
retains 13,781 requested bytes after 1,440 requests. Each mode retains zero
private requested/padded bytes after checker release. Four stores add about
62.2 MB over one on this workload, rather than multiplying one-store bytes by
four. That observation is not a bound on another project or forcing order.

The expanded public fixture checks 21 files and forces actual mapped object
assignments, with an eight-times-wider file and an A/B generic import cycle.
Its 1/2/4 requested live totals are 1,588,906 / 1,729,386 / 1,872,906 bytes.
At four workers the largest owner retains 1,105,968 bytes versus roughly
254–259 KB for each other owner. The small fixture totals are 11,642 / 19,007 /
36,373 bytes. Worker type counts and computation counters in the receipts are
entries/events, not retained capacity or bytes, and do not cover all forcing.

## Timing and observer cost

Next.js measurements below are seconds except RSS, which is bytes. The feature
off mode still installs the prefixed System allocator, but disables private
owner tagging. Separate ordinary-helper observations are also preserved.

| Workers | Off wall median [range] | Off user CPU [range] | Off system CPU [range] | Off peak RSS [range] | Paired on-minus-off wall median |
| --- | --- | --- | --- | --- | ---: |
| 1 | 4.646512 [4.635437, 4.718471] | 4.160242 [4.149345, 4.170318] | 0.476887 [0.475151, 0.519920] | 1,143,455,744 [1,106,395,136, 1,159,266,304] | 0.045994 |
| 2 | 3.583215 [3.562969, 3.618640] | 4.471967 [4.452702, 4.510928] | 0.487917 [0.487565, 0.499149] | 1,118,011,392 [1,109,311,488, 1,120,010,240] | 1.104722 |
| 4 | 2.963705 [2.963661, 2.973212] | 4.905144 [4.875993, 4.916658] | 0.541718 [0.539041, 0.550420] | 1,157,890,048 [1,146,814,464, 1,183,809,536] | 3.955155 |

On medians are 4.698117 / 4.713858 / 6.928367 seconds. Shared accounting atomics
are a substantial observer cost at 2/4 workers; do not infer scheduling
scalability from these instrumented times. The off helper four-worker wall is
36.2% below its one-worker wall in this round, but this is not a production
change, independent confirmation or a native speed ratio. Small-fixture wall
outliers near 0.108 seconds make its timing inconclusive. An earlier ordinary
baseline ranged 5.66–7.94 seconds; it is not a reliable winning comparison.
All on/off CPU/RSS ranges and single growth-control timings remain in the JSON.

## What is accounted

[The archived meter](checker-memory-meter.rs) prefixes System allocations with
their original owner. A thread-local guard begins before each private Checker
constructor; realloc/free retain that owner even on another thread. Program
construction happens outside owner regions. Output clones happen after each
guard drops. The coordinator observes all checkers alive after initialization
and after checking, then after release; quiescent owner sums match the direct
global live total. The global high-water mark is measured directly, never made
by summing independent owner peaks or subtracting process RSS maxima.

This attributes allocation origins, not individual reflected checker fields.
Requested layouts include private stores and temporary Rust allocations;
padded layouts include observer prefix/alignment overhead. Neither measures
allocator usable size/retention, stacks, stacker C allocations, other unclassified
allocations or shared Program storage. Zero tracked bytes after release does not
mean RSS returns to zero. Four allocator controls cover alignment, zeroing,
cross-thread frees, nested guards, realloc preservation/failure and Vec shrink.
Unsafe permission is scoped to this archived GlobalAlloc module; no production
allocator or unsafe Sync is introduced.

## Executable failure policy and recommendation

The archival helper keeps native count selection separate from admission:
default four, explicit override, singleThreaded precedence and complete-Program
file-count/256 clamp. Existing selection controls match 200 pinned-native pool
observations. A physical tiny program requested at four with singleThreaded
selects one and completes three checks. Affinity remains complete-Program index
modulo selected count, before current CLI eligibility filtering.

`TSR_CHECKER_MEMORY_BUDGET` is an **experimental soft stop**, checked after
initialization and after each completed file. A zero budget exits 2 before any
file check. On the growth fixture, a 20,000-byte budget passes initialization
(14,804 bytes), completes seven of 21 files, then exits 2 with
`MEMORY_BUDGET_EXCEEDED`. At the checked barrier it has 1,269,287 live bytes and
a 1,512,523-byte peak; all private allocations are released. These failed runs
are explicitly unqualified for complete-process/output and speed acceptance.
They terminate rather than waiting for unavailable credits, and never report
partial checking as success. Budget absence is exercised by every normal pair.

The large overshoot is decisive: post-file observation cannot admit a single
large operation. Constructor/file internals are currently infallible, and the
release panic profile aborts; this helper does not promise arbitrary unwind
cancellation or OS memory exhaustion recovery. `tsr-1yb.3.1.1.4` must identify
fallible reservation/growth boundaries and a bounded failure contract before
recommending automatic production admission. Keep the serial default while
those and `tsr-1yb.3.2` controls remain unfinished. Explicit native count
semantics do not justify silently changing requested counts using an unproved
per-worker estimate. The 32.4 million single-worker requests are handed to the
existing `tsr-1yb.11` profiling owner; they are not malloc calls or a proof of
which source location dominates time.

## Reproduction and verification

Apply [the four-file patch](checker-memory-probe.patch) to a clean archive of
the exact Rust commit above, with pinned vendor available. The replayed four
source hashes match the receipts. Build release `checker_workers` twice into
separate targets, once normally and once with `--features checker-memory-probe`.
The ordinary helper also has the same eligibility/aligned-index patch, so the
comparison does not confuse a scope change with observer cost.

Generate controls outside measured samples:

```sh
python3 docs/architecture/checker-memory-controls.py fixtures /tmp/memory-small
python3 docs/architecture/checker-memory-growth-fixtures.py /tmp/memory-growth
python3 docs/architecture/checker-memory-round.py \
  --normal /absolute/normal-target/release/examples/checker_workers \
  --probe /absolute/probe-target/release/examples/checker_workers \
  --project /tmp/memory-growth --output /tmp/memory-pairs --repeats 3
python3 docs/architecture/checker-memory-controls.py measure \
  --binary /absolute/probe-target/release/examples/checker_workers \
  --project /tmp/memory-growth --output /tmp/memory-budget.json \
  --workers 4 --repeats 1 --probe --budget 20000
```

The driver supervises fresh PID/start/outcome, bounds each child at 180 seconds,
checks immutable binary/observed inputs, preserves complete diagnostics and
actual file checking, and rejects malformed/stale/incomplete memory records.
Eight reader tests, ten archived example tests, strict release archived Clippy
and meter rustfmt pass. CLI comparisons canonicalize only diagnostic file paths
through physical identity (macOS `/var` versus `/private/var`), preserving
positions, codes and complete messages; raw outputs remain hashed locally.
Independent review coverage is zero. Full native-work, corpus and production
concurrency acceptance are unchanged because this unit changes no runtime code.
