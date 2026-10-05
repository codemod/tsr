# Reference-key construction coverage

All 19 production reference-cache calls are classified as empty, borrowed,
reused, moved, cloned, copied from a slice, or singleton construction. Nine
related declared target-pair constructors are separately counted. This closes
the coverage gap in the earlier [hash observer](checker-mapper-hash.md), without
selecting a production representation or claiming a speed improvement.

Measurements freeze TSR `67cae54a96113085b635b1d61b4f5c87bd15df68`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, and TypeScript fixture revision
`4d4f005c8541e0255a9d8791205fdce326e462bc`. The
[receipt](checker-reference-key.json) records the complete source ledger,
original expressions and lines, binary/module/harness hashes, options, input
and output fingerprints, per-owner counters, failures and restoration. Later
main changes do not refresh these measurements.

The observer invokes each literal original constructor once. It preserves
the vector's pointer and capacity on moves, uses the original clone/to_vec/vec!
operation on copies, and performs the original lookup or insertion in order.
Borrowed and reused keys have no constructor allocation. Disjoint allocator
tags measure requested/usable storage inside those constructors; they exclude
map bucket growth and unrelated consumers. Hashing and identity remain the
original operations. No production file is changed by this archive.

The schema has 563 counters: the previous 100 pool and 144 hash counters, plus
319 construction/site counters. Site lookups and inserts reconcile to the
table totals; constructor counts and storage reconcile to the existing query,
publication and target tags. Every owner reconciles to the global sums/maxima
in default and single mode. Unclassified production operations must be zero.

| Application lookup classification | Default | Single |
| --- | ---: | ---: |
| All reference lookups | 653,116 | 561,844 |
| Previously selected ordinary clone constructors | 609,040 | 523,001 |
| Additional borrowed lookups | 29,688 | 26,253 |
| Additional fresh lookup constructors | 14,388 | 12,590 |
| Additional fresh lookup storage requested | 57,552 bytes | 50,360 bytes |

The additional fresh constructors are deferred slice copies and NonNullable
singletons. Borrowed queries explain most of the former gap; counting the
entire gap as avoidable allocations would be wrong. Ordinary clones request
4,530,124/4,055,288 bytes. All query constructors together request
4,587,676/4,105,648 bytes; publication constructors request 368,444/288,072
bytes, and the nine related target constructors request 369,496/288,532 bytes.
These are cumulative original-construction requests, not live or avoidable
storage. A move retains its existing vector rather than requesting new storage.
Unused source sites remain explicit zeros in the ledger.

Both application modes retain all three diagnostics, 14,051 loaded files and
1,397 directly checked identities. Two normal, disabled and enabled processes
per mode preserve complete output; off/on owner assignment and check order,
and enabled non-clock counters repeat. Physical loaded/config snapshots are
stable. They omit embedded libraries, unsuccessful resolver queries and
transient inputs, so complete native project-work equivalence is unproved.

Four ordinary profiles capture the main thread and all four checker workers
in default mode, and the main thread in single mode. Member-name collection
is the largest named TSR owner across the four checker-worker rankings in
both default intervals (241/268 samples), and the largest named allocator
owner (111/124). Instantiation and binder lookup also appear prominently.
Unowned frames, truncated rankings and main-thread waits remain explicit;
the early single-mode interval also includes loading and omits later checking.
Generic HashMap frames do not isolate pure hash cost. These samples locate
work; they are not CPU percentages, allocation events or a saved-wall ceiling.
The first sampler attach refusal is retained separately from the four
successful profiles. No material pure-hash opportunity is established.

Validation passed 1,500 checker tests with three existing ignores, strict
checker/execute Clippy, formatting and 89 Python controls. Three new real Rust
tests exercise original constructors at empty/1/8/128 items, moved pointers,
borrowed/reused consumers, allocator requests and two-owner reconciliation.
The existing hash/identity/order/collision tests remain in the reconstructed
module. Removing the actual constructor or allocation counter makes two
focused tests fail; both mutations are restored.

Fresh ordinary/probe corpus outputs match byte for byte: all 476,787 type
rows and 10,570 diagnostic cases, with zero prior RIGHT losses. The type
result has 465,680 RIGHT rows; diagnostics have 3,459 RIGHT and 4,894
EMPTY_RIGHT cases. Upstream fidelity changes since the earlier hash snapshot
account for changed diagnostic verdicts; the observer adds no correctness win.
Corpus counters are disabled. Enabled behavior is covered by actual application,
public and pool runs: 140 public compiler children retain Rust outputs, with
24/28 native diagnostic matches, and 30 pool children have six native matches.
Existing Flatten (**tsr-6.58**) and private-brand explanation (**tsr-6.68**)
failures remain failed comparisons. Both abort controls refuse completion.

Replay into an isolated checkout at the frozen revision with
`python3 docs/architecture/replay_checker_reference_key.py --source /path/to/checkout`.
The helper verifies original files and archive hashes before applying the patch
and copying the constructor module, then checks all 11 reconstructed hashes.
Six actual replay controls cover shared/wrong/changed/existing/repeat refusals
and exact reconstruction. Enable `TSR_MAPPER_PROFILE=1` for the focused Rust
tests; `checker-reference-key-controls.py` reuses the existing actual-pool driver.
Full corpus examples run with `RAYON_NUM_THREADS=8`, without `TSR_*` flags.

All temporary primary/replay hooks and three prior target binaries are restored
and their checkouts are clean. Failed installation, compilation, lint, sampler
and summary-readback controls remain in the private raw archive. Constructor
coverage belongs to **tsr-1yb.16.1.2.1.1**; reference refusal/error/publication
states remain **tsr-1yb.16.1.2.2**, and the candidate decision remains
**tsr-1yb.16.1.4**. Mapper-context repair and the rejected mode-key gate are
unchanged. The required comparable whole-project TSR/tsgo wall ratio remains
at most 0.50 and is still unverified.
