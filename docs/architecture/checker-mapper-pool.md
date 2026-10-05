# Mapper costs with actual checker pools

This locating measurement freezes Rust `9b494ca767baeb0129e74d3a099d0d3c9c3c6d5e`
and native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The [receipt](checker-mapper-pool.json) binds source files, binaries, options,
outputs, counters, resource observations and exact private restoration.
It refreshes the older [serial audit](checker-mapper-key-performance.md).
There is no production optimization or verified TSR/native <=0.50 result.

## Observer repair and coverage

The archived [module](checker-mapper-pool-probe.rs) and
[hooks](checker-mapper-pool-probe.patch) preserve the existing four
object/signature key producers, reference lookup path and containment predicate.
Hooks retain the native-derived pool size and file affinity. A guard starts
before each checker constructor, records setup completion and actual full-file
call intervals, then writes its receipt after that checker drops. The aggregate
is emitted once after every worker joins. All 100 counters reconcile against
owner totals; maximum key length uses the maximum, rather than a sum.

Active frames include the live checker identity, cache-table domain and exact
ordered key. Anonymous, literal and mapped producers share the object domain;
signature keys have their own domain. Addresses identify overlapping method
frames only; they are neither dereferenced nor retained as semantic identities.
Two Rust controls pass; deliberately collapsing the domains makes both fail.
The original bytes are restored before final builds.

The scope observer is independently enabled while key/allocation counters are
off. Each owner records its actual file index/order and follows `index % count`.
This establishes off/on scope equality without inferring it from diagnostics.
Normal binaries have no direct trace, so their complete output and reported
scope are checked separately. No instrumented single-worker run describes the
default pool, and counts are never duplicated by taking global snapshots once
per checker.

Publication success/refusal counters describe the four named Rust source blocks.
Nonactive hits do not certify native completed member images. The reference
path's nonactive answers are not separately classified as error values; its miss
block includes cheap early returns. Wider reference publication, inference/fixing,
conditional and emit work remains outside this qualification. Pure hashing is
not isolated: lookup clocks include the real hashing and equality operation.

## Whole-project observations

Thirty fresh compiler children complete: five rotated normal/counter-off/on
samples in each mode. All preserve complete **124 diagnostics**, **14,050 loaded
files**, **14,746 reported parses**, and **1,397 direct full-file checks**.
Actual admitted owners and peak overlapping full-file calls are **4/4** in
default mode and **1/1** in single mode. Global and per-owner non-timing counters
and retained capacities repeat exactly across all five enabled children.

| Path | Default requests / worker blocks | Single requests / worker blocks |
| --- | ---: | ---: |
| Captured anonymous object | 19 / 9 | 15 / 7 |
| Baked signature | 145,900 / 23,526 | 117,511 / 13,818 |
| Mapped type | 20 / 8 | 18 / 6 |
| Type literal | 16,238 / 8,944 | 12,139 / 5,041 |
| Reference after earlier branches | 556,519 / 49,292 | 471,610 / 35,311 |

Signature nonactive hits are 122,374/103,693. Signature workers succeed
23,442/13,742 times and refuse 84/76 times. Literal nonactive value hits are
7,294/7,097; single mode also has one nonactive error hit. The private caches
perform additional work across default owners, as their ownership requires.
This observation does not authorize sharing TypeIds or mutable caches.

The five key constructors request **6,548,144 / 5,528,584 bytes** in
default/single mode. Prelookup literal property copying requests
**8,646,010 / 5,913,538 bytes**; signature copying at that boundary requests
1,232 bytes in each mode. Both hits and misses contribute. None of these totals
can all be called avoidable. The next attribution is the hit-only partition
`tsr-1yb.16.1.3`, before the ordinary-work benefit decision `.16.1.4`.

Summed medians of the five constructor clocks are 85.35/59.52 ms; summed lookup
clocks are 18.34/15.40 ms. These include observer work and concurrent intervals.
Worker clocks include nested work. They are neither additive wall-time shares
nor a normal saved-wall ceiling. The new owner accounting changes observer cost;
these intervals must not be compared to the old serial probe as a regression.

| Mode | Normal median | Counter-off median | Counter-on median |
| --- | ---: | ---: | ---: |
| Default | 3.604524 s | 3.615708 s | 3.704566 s |
| Single | 5.497517 s | 5.550062 s | 5.732483 s |

Observed on-minus-normal differences are about 100/235 ms. CPU, RSS, ranges and
individual fresh PIDs are retained in the receipt. The default normal range
includes a 5.513 s outlier; external host activity is uncontrolled. This is an
overhead observation, not a causal timing conclusion or speed/memory win.

The allocator delegates unchanged allocation/deallocation arguments to `System`.
Constant TLS tags count successful allocations and requested/macOS usable bytes,
including reallocations. These are cumulative events, not live heap or RSS.
Retained vector capacities are recorded separately per checker and summed only
after owners complete; bucket storage, values and other caches are excluded.
Unsafe allocator observation is confined to the owned archive under ADR-0011.

Complete effective Rust configuration fingerprints and the supplied project
config are bound. Observed loaded-file/config snapshots stay equal after the
first ordinary child. Unobserved queries, directory entries, transient changes
and physical embedded-library bytes are not covered. Complete cross-tool input
and lazy-work equivalence remain unproved.

## Public, failure and quality controls

The [driver](checker-mapper-pool-controls.py) reuses the existing 14 public
mapper fixtures in both modes. All **140 children** complete and preserve TSR
diagnostics, loaded order and off/on direct scope. **24/28** mode variants match
native complete diagnostics. The same-spelled nominal negative still omits the
private-brand explanation (`tsr-6.68`); recursive readonly-array `Flatten` still
incorrectly accepts its negative assignment (`tsr-6.58`). Both stay failed
fidelity controls, with no unsupported-work timing pass.

Another **30 children** cover one-file admission, skewed work with an explicit
two-worker override, single precedence and `noCheck`. All six native diagnostic
comparisons match. `noCheck` records constructor/setup owners with zero full-file
calls. Two injected failures abort with SIGABRT and produce no completed
aggregate; the reader rejects both. Release uses `panic=abort`, so absent owner
Drop receipts establish no cleanup claim.

The [12 Python controls](test_checker_mapper_pool_controls.py) reject wrong
PID/mode, stale epoch, failure/timeout, missing completion, partial records,
duplicate checks/counters, invalid affinity/intervals and inconsistent owner
sum/max accounting. The complete archived release checker suite passes
**1,492 tests across 101 result blocks**, with three existing ignored tests.
Strict release checker/execute library/test Clippy and full archive formatting
pass. No full-corpus delta or coverage gain is claimed for diagnostic tooling.

Both preliminary and final runs remain in private scratch; the exported
qualification uses the final probe hash only. All five temporary runtime files
are restored byte-for-byte, the observer module is removed, and the shared
private build target's ordinary CLI is restored. Frozen normal/probe binaries
and raw evidence remain available. Canonical checker and vendor code were never
modified. `.16.1.2` stays in progress for remaining hash/reference-state
qualification; the broader audit, context repair and allocation implementation
are not closed by this package.

## Reproduction

Use an owned clean checkout at the frozen Rust pin and the pinned native vendor.
Copy the module to `crates/tsr-checker/src/mapper_key_probe.rs`, then apply the
hooks in that private checkout. Build normal and probe release binaries
separately, keeping all compilation outside sampling. The receipt lists exact
pre/post source hashes and the delivered patch reconstructs those observed bytes.

A binding JSON supplies `normal`, `probe` and `native`, each with an absolute
binary `path`, `source` SHA and `sha256`. The reused native baseline's Go metadata
has no VCS revision; its source qualification is the existing
`native-worker-activity.json` build receipt and matching baseline binary hash.

```sh
python3 -B docs/architecture/checker-mapper-pool-controls.py --bindings /absolute/bindings.json --output /absolute/new-public
python3 -B docs/architecture/checker-mapper-pool-controls.py --bindings /absolute/bindings.json --output /absolute/new-pool --pool-controls
python3 -B docs/architecture/checker-mapper-pool-controls.py --bindings /absolute/bindings.json --output /absolute/new-project --project /absolute/tsconfig.json --repeats 5
python3 -B docs/architecture/test_checker_mapper_pool_controls.py
```

Counters require `TSR_MAPPER_PROFILE`; direct scope/lifecycle requires a fresh
`TSR_MAPPER_TRACE` path. These switches and the failure-injection switch exist
only in the archive observer, not the shipped CLI. Every replay uses fresh
output paths so prior receipts cannot be overwritten or reused as completion.
