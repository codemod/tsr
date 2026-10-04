# Compact bound-table storage experiment

The optional boxed-map candidate was reverted. It reduced memory use on the
observed application but did not improve full CLI wall time. The production
`SymbolTableField` remains `Option<SymbolTable>`.

The measured source is `63ca73e43773309cfa1786e05bbdc612e527ed31`, with the
one-file experimental diff in [the candidate patch](checker-compact-table-candidate.patch).
The opaque field API, native absence/initialized-empty/populated states, borrowed
lookup/index/iteration and independent cloning stayed unchanged. Local/global
scope maps, checker ownership and semantic cache boundaries were untouched.

## Storage and actual allocations

The full app contains 1,082,855 bound symbols in 2,097,152 reserved slots.
Boxing reduces each record from 128 to 80 bytes: inline reserved capacity goes
from 256 to 160 MiB. There are 145,798 present member fields and 26,324 export
fields. This reserved-slot difference does not predict committed process RSS.

The diagnostic allocator observes 172,122 new initialization allocations,
5,507,904 requested bytes and the same macOS usable bytes. Cloning adds 1,206
map-object allocations and 38,592 bytes; the original 1,206 bucket-copy allocations
remain. Insertions retain 201,822 bucket allocation events, 40,797,576 cumulative
requested bytes and 42,828,304 usable bytes in both representations. Retained
member/export bucket capacities are unchanged at 700,506/186,254. The candidate
retains 5,507,904 usable bytes of separately allocated map objects.

Counts come from temporary scoped allocation tags and a `System`-delegating
`GlobalAlloc`, with macOS `malloc_size` on valid live allocations. This is the
diagnostic-only unsafe exception described by ADR-0011; none of it remains in
production source. Usable sizes exclude allocator metadata and page fragmentation.
Cumulative allocated bucket bytes are not retained memory. Normal process RSS
captures the net effect without treating the reserved-slot subtraction as a
memory forecast.

## Normal CLI comparison

Two independent serialized alternating five-pair rounds use frozen normal
executables, one warmup per executable, disabled incremental/composite reuse,
identical libraries/options/inputs, and complete diagnostic comparison. Builds
and instrumented runs are outside timed samples. The harness's `tsgo` slot holds
the saved TSR baseline for this isolated comparison.

| Round | Baseline median wall | Candidate median wall | Difference | Median RSS reduction |
| --- | ---: | ---: | ---: | ---: |
| Initial | 4.566939 s | 4.624383 s | +57.443 ms | 39,501,824 bytes |
| Confirmation | 4.604289 s | 4.617515 s | +13.226 ms | 41,091,072 bytes |

The unchanged absolute wall threshold is 20 ms. `decide.mjs` returns `revert`
for the initial round and `inconclusive` for confirmation. Neither round is
eligible to retain the candidate. CPU, RSS ranges, every wall sample and exact
source/binary/probe hashes are in [the controls](checker-compact-table-controls.json).
The RSS observations apply to this source/workload; they do not establish a
general allocator or worker-memory policy.

## Work and correctness scope

Enabled/repeated/disabled profiles and both normal timing rounds preserve all
118 complete app diagnostics. Both profiles check the same 1,341 identities in
the same order with unchanged input hashes. They perform 36,353,228 bound-record
reads and 6,009,379 table lookups, with the same hit/miss distribution and table
publication/insert counts. The candidate passes 114 binder unit/bind/program
tests and 16 checker symbol-domain tests; strict affected Clippy and formatting
pass. The exact owned production file is restored after rejection.

Full type/diagnostic corpora were not run for this unshipped candidate, so these
controls do not claim full-corpus preservation. The pinned-native class prototype
and anonymous signature-content gaps remain under `tsr-1yb.7.7.1.1.1`, and the
private-consumer/alias/node-completion gates remain open. The verified comparable
TSR/pinned-tsgo median wall target of 0.50 remains unproved.

## Replay

Check out the stated source. Apply the candidate patch only for its normal
build, and freeze the resulting executable before profiling. Each diagnostic
patch applies to its corresponding baseline or candidate source; build release
`tsr`, run the app with `TSR_TABLE_PROFILE=1`, then restore every patched file.
The patches are [baseline](checker-compact-table-baseline-probe.patch) and
[candidate](checker-compact-table-candidate-probe.patch). Disabled, enabled and
repeated enabled outputs must match; do not time instrumented executables.

Use `scripts/whole_project_perf.py` with the frozen candidate and baseline as
`--tsr`/`--tsgo`, `--samples 5 --warmups 1 --require-comparable`, twice serially.
Rebuild/setup stays outside samples. A future retry needs new measured evidence
that changes the allocation/indirection tradeoff; reduced record size alone
does not justify reviving this candidate.
