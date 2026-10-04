# Ordered member-name indexing: no retained CLI gain

Keep the existing production vector. The query-local membership experiment
(`tsr-1yb.20`) improves wide standalone lists, but both independent five-pair
normal CLI rounds fail the unchanged 20 ms wall-time gate. The candidate and
temporary scope trace are restored byte-for-byte. No semantic cache, member
completion, worker policy or dependency change ships.

The [report](checker-member-index-performance.json) preserves all 105
microcontrol rows, capacities, public diagnostics, raw scored wall/CPU/RSS
samples and the two `decide.mjs` receipts. The
[candidate patch](checker-member-index-candidate.patch) is rejected evidence,
not an implementation to apply to production. It is a zero-context patch;
replay against the recorded base with `git apply --unidiff-zero`. The replay
reconstructs the exact measured candidate source hash.

## Candidate and controls

The base is `8d822d94fff05ef68010ae7516fb77c447505b16`. Ordinary release
baseline/candidate binaries are separately frozen; hashes identify their
actual bytes. Builds use one Cargo job, outside observations. All measured
compiler processes run serially and disable incremental/composite reuse.
Other host activity is uncontrolled.

The candidate preserves the original owned-name batches and traversal/guard
order. It keeps a vector until 32 distinct names, then moves keys into an
existing FxHashMap with their first encounter positions. The next insertion
can promote storage even if that name is a duplicate. Final assembly moves
keys into their recorded positions. Static-name collection stays unchanged.
It publishes no completed member image and does not reuse semantic answers.

All 13 candidate property-name tests pass, including forced hash collisions,
empty/case/Unicode-distinct names and growth. After restoration, all 11
original property-name tests pass. The public driver checks 19 cases twice,
including widths 16/32/33/256 and shared-base degree 1/4. Baseline/candidate
complete diagnostics and loaded order agree in every case; 16 agree with
pinned tsgo `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

Three existing fidelity differences remain explicit: the generic argument
error chain (`tsr-6.55`), namespace target naming and intersection missing
property diagnostic (`tsr-6.62`). Every new wide/shared-base case matches
native clean output. Replay resolves the temporary output directory first;
only the original `/tmp` alias's leading diagnostic location prefix needs
normalization between captures. Source positions and entire message chains
remain identical.

## Local cost versus full CLI

The [standalone control](checker-member-index-micro.rs) clones identical owned
batches for all algorithms and checks the complete returned vector. Five
in-process rotations give these median nanoseconds per complete query:

| Case | Linear | Hybrid |
|---|---:|---:|
| 4 names | 220 | 222 |
| 32 names, four batches | 8,263 | 5,954 |
| 256 names, four batches | 159,825 | 40,930 |
| 1,024 names, four batches | 2,565,777 | 161,294 |

These are data-structure observations, not fresh-process project speed gains.
The [extracted helper control](checker-member-index-storage.rs) reports actual
vector/map element capacities. At 32 names/four batches, linear vector capacity
is 32; hybrid index capacity is 56 and final vector capacity is 32. Map/output
buffers overlap during finish. Both retain the original name construction;
indexing does not eliminate those payload copies. The equality reference
also clones unique strings solely to compare outputs. Capacity, logical entry
size and payload are distinct from allocator events, bucket bytes and total
heap traffic; those physical allocation totals are unobserved.

| Five-pair round | Baseline median | Candidate median | Candidate minus baseline |
|---|---:|---:|---:|
| 1 | 4.130 s | 4.158 s | +28.362 ms |
| 2 | 5.413 s | 5.572 s | +158.847 ms |

Both auxiliary retention decisions are `revert`, with no further measurement
requested. CPU/RSS are in the report; RSS medians decrease, but there is no
confirmed throughput benefit. Round two varies widely and a late host-load
observation is elevated. These deltas are not a precise causal estimate of
index overhead, and no cause is assigned to another agent or process.

Every full check retains 122 complete normalized diagnostics and 14,015
ordered loaded identities. Separate scope-only variants agree on 1,364
actually checked identities before and after both rounds. The baseline scope
binary reuses the earlier `359a2789` build: its hash is verified and its
compiler/Cargo tree has no diff from `8d822d94`. It is not described as a new
same-path build. The normal timed binaries have no checked-identity trace.
Historical query paths plus current loaded/config/binary snapshots are stable
before/after each child, outside timing; coverage remains partial. These checks
cannot establish complete native work/input equivalence or the required
TSR/tsgo median wall ratio of 0.50.

Full checker corpora were not run for the discarded candidate. Production
source in the isolated experiment is restored exactly. Main advanced to
`51a6b63f` during delivery; those concurrent changes are preserved. Measurements
remain qualified to the recorded base and no runtime candidate is integrated.
Keep native concrete-member completion (`.16.2.2`/`.4.2`) separate from this
name algorithm. Revisit indexing only with new representative cost evidence,
not the standalone comparison count alone.

## Replay

The [public driver](checker-member-index-controls.py) accepts separately frozen
`--baseline`, `--candidate`, `--native`, `--source` and a fresh `--output` path.
The [paired driver](checker-member-index-paired.py) accepts those TSR binaries,
`--project`, `--input-manifest`, `--source`, `--candidate-patch` and a fresh
`--output`; use `--pairs 5 --rounds 2` for the confirmation shape. Supply both
`--baseline-scope` and `--candidate-scope` for independent checked controls.
Scope provenance must be verified separately. Private inputs and raw app
output remain local; the tracked report contains fingerprints and aggregates.

For standalone replay, put the two `.rs` files in separate Cargo binary targets
with existing `rustc-hash = "2"` (measured 2.1.3), edition 2024 and ordinary
release settings: opt-level 3, lto false, codegen-units 16, panic abort. The
microcontrol includes returned-vector construction/drop; the storage control
is capacity/equality evidence only. Do not count its reference clones as the
production algorithm's allocation events.
