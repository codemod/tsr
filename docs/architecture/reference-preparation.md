# Reference preparation and pooled allocation origins

Issue: `tsr-1yb.16.3.1`; broader attribution remains `tsr-1yb.16.3`.
Frozen Rust: `ecacd9d04c48c9d0192510289be664b189634c05`.
Pinned native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Machine-readable results: [reference-preparation.json](reference-preparation.json).

## Decision

The integrated CLI admits four private checkers in default mode and one with
`--singleThreaded`. Both modes directly check the same 1,397 program-file
identities. All twelve completed ordinary/probe-disabled/probe-enabled app
children preserve 117 complete normalized diagnostics, the ordered 14,050
loaded names, and 14,746 reported parses. This establishes the selected pool's
activity on this source; it does not establish all required native lazy work or
complete query-input equivalence.

Reference spelling is a bounded candidate: the existing loop prepares text that
it sometimes discards. Its returned discarded payload is 2,349,765 bytes with
four checkers and 1,637,188 bytes with one. The larger reference-origin cost
includes argument resolution and type creation, so it cannot all be removed as
formatting. No production optimization or saved-wall claim is retained here.

## Ordinary runs and observer cost

Each cell is the median of two fresh children, in alternating mode/variant
order. Builds and owned checks were serialized. Other agents' host activity was
not controlled. Input snapshots include loaded physical sources, pinned physical
contents behind bundled library names, previously observed metadata/config paths,
and the binaries. Unobserved lookup candidates remain outside this snapshot.

| Mode / binary | Wall seconds | User seconds | System seconds | Peak RSS bytes |
|---|---:|---:|---:|---:|
| Default ordinary | 3.224 | 4.779 | 0.600 | 1,168,850,944 |
| Default probe disabled | 3.268 | 4.965 | 0.652 | 1,181,556,736 |
| Default probe enabled | 8.948 | 25.955 | 0.712 | 1,127,202,816 |
| Single ordinary | 4.988 | 4.173 | 0.598 | 1,158,111,232 |
| Single probe disabled | 5.211 | 4.378 | 0.644 | 1,104,388,096 |
| Single probe enabled | 5.384 | 4.461 | 0.623 | 1,129,299,968 |

Ordinary wall ranges are 3.214–3.234 seconds and 4.960–5.017 seconds. The archive
allocator adds a prefix even with attribution disabled; disabled median wall is
1.36% higher in default mode and 4.46% higher in single mode. Enabling counters
raises these observed medians by 177.56% and 7.94% against ordinary binaries.
The large pooled effect is consistent with contention on shared atomic counters;
its exact contributors were not independently isolated. Site elapsed times
include preemption, overlap across sites/threads, and this observer distortion.
They cannot predict ordinary saved wall or be added into a CPU percentage.

The existing production `work-trace` forces one checker. This archive does not
enable that feature or change pool selection. Receipts record constructor/private
lifetimes, actual synchronous `check_source_file` intervals, and program indices.
The reader reconstructs peak activity, verifies index affinity and unique checks,
and compares direct checked identities across both modes and repeats.

## Disjoint allocation origins

Requests below combine successful allocation and resize requests. Bytes are
requested Rust global-allocation layouts, not System usable size or RSS. Resize
and cross-thread free retain the allocation's original origin. A backing store
created in residual checker work stays residual when a nested operation resizes
it. Inner selected sites override outer sites for new allocations, so these four
origins form a disjoint accounting partition inside private workers.

| Origin | Default requests | Default cumulative bytes | Single requests | Single cumulative bytes |
|---|---:|---:|---:|---:|
| Reference getter/preparation | 3,699,715 | 134,711,703 | 2,653,458 | 86,321,150 |
| `instantiate_type_worker` | 5,786,075 | 282,974,620 | 4,356,729 | 205,837,437 |
| Member-name collection | 4,728,845 | 475,961,962 | 4,266,657 | 427,975,286 |
| Residual private checker | 23,983,676 | 1,288,658,854 | 21,132,043 | 994,594,243 |
| Union of the four origins | 38,198,311 | 2,182,307,139 | 32,408,887 | 1,714,728,116 |

Request counts and cumulative bytes repeat exactly within each mode. The
aggregate simultaneous requested-byte peak is 162,169,272 bytes in the first
default observation and 154,933,780 bytes in the first single observation; peaks
can vary with ordering and are retained per child in the JSON. They are measured
directly, rather than summing independent origin peaks. All tagged live requested
and padded-layout bytes are zero after private checker teardown.

Origin zero in the receipt is this aggregate. Origin four is residual work
inside private lifetimes. Shared Program/parser allocations, worker stacks,
non-Rust allocations and allocation-origin zero outside the selected partition
are unclassified. Their cost is not obtained by subtracting tagged requested
peaks from RSS. Separate alloc versus realloc event counts, full copy traffic,
and System retained pages remain unavailable; the broader parent stays open.

## Spelling handoff

| Unchanged preparation loop | Default | Single |
|---|---:|---:|
| Preparations | 127,968 | 113,864 |
| Rendered argument strings | 254,105 | 235,388 |
| Cloned written argument strings | 132 | 78 |
| Returned argument-string bytes | 2,884,009 | 2,165,053 |
| Preparations whose text is retained | 58,488 | 57,787 |
| Preparations whose text is discarded | 69,480 | 56,077 |
| Discarded returned argument-string bytes | 2,349,765 | 1,637,188 |

These values repeat exactly. Returned capacities equal lengths in this workload.
This excludes vector storage, base-name text, composed/joined text, intermediate
rendering allocations, and every other copy. It neither attributes all reference
allocation to spelling nor forecasts the time to avoid it.

The exact consumer is `get_instantiated_type_reference` in `declared.rs`: after
resolving argument types, it fills `spelled` from written-node text or rendered
arguments, then publishes composed text only if `any_written || partially_written`.
`qualified_written_text` belongs to the private Checker and is keyed by source
`NodeId`; it preserves written annotation spelling while computed references can
carry expanded default arguments. It is not a semantic type-identity key.

Current Rust `Checker::type_to_string(&self)` calls
`printing::type_to_string(self.store.get(id))`. That helper only formats an
existing `Type` and does not resolve members or mutate semantic state. The
reference argument resolution and default substitution occur separately and
must remain. Native `printer.go::typeToStringEx` explicitly can force lazy
members, while `checker.go::getTypeFromTypeReference` caches reference resolution
without an unconditional success-path serialization. Do not generalize this
Rust helper's purity to context-sensitive `type_to_string_at` or native printing.

`tsr-1yb.16.3.2` owns the executable native written-node/display and rendering
boundary contract; `tsr-1yb.16.3.3` owns a single measured consumer change after
that contract and this attribution. Full corpus preservation and independently
confirmed ordinary whole-CLI benefit remain requirements for a retained change.

## Public controls and remaining fidelity

[reference-preparation-public-controls.py](reference-preparation-public-controls.py)
retains all source texts and complete child output for four cases. Ordinary,
disabled and enabled TSR outputs and performed scope agree in every case.
Partial/dependent defaults and nested bare defaults are exercised alongside
same-spelled namespace declarations. `--singleThreaded --checkers 2` admits one;
explicit `--checkers 2` admits two. A primitive assignment error matches complete
native diagnostics; `--noCheck` removes its error and records zero full checks.
The latter is a deliberate skipped-work control, not a speed result.

The positive namespace/default receiver case fails native fidelity in both
default and single mode: native accepts `Left.Box` with default `string` and
`Right.Box` with default `number`; ordinary TSR instead compares each value to
unsubstituted `T`, producing two TS2322. This predates the observer and is tracked
as `tsr-6.66`, related to `tsr-6.23` and the receiver-boundary contract. It remains
explicitly failed rather than changing the fixture or weakening its comparison.

The public controls are not an exhaustive declaration/display contract, and
the real app's complete native diagnostics remain unequal. No full-project
TSR/native ratio or 2x success is qualified by these observations.

## Reproduction and checks

In a clean archive of the frozen Rust source with the pinned vendor contents,
apply [reference-preparation-hooks.patch](reference-preparation-hooks.patch)
with `git apply --unidiff-zero` (the context-free patch is checked against that pin).
Copy [reference-preparation-probe.rs](reference-preparation-probe.rs) to
`crates/tsr-core/src/reference_origins.rs`; create its `reference_origins/` module
directory and copy [jsdoc-setup-meter.rs](jsdoc-setup-meter.rs) there as `meter.rs`.
Remove that copied meter's four-line test-only allocator declaration: the parent
module already supplies the archive's global allocator. Canonical production
code and allocator remain unchanged. The JSON hashes every resulting source.

Build the archive with `cargo build --release --offline -p tsr --bin tsr
--features reference-origins`. The process-selected `TSR_REFERENCE_ORIGINS` path
enables attribution. The normal binary has no observer feature; ordinary,
disabled and enabled variants run the same full-check flags. The driver clears
all other `TSR_*` variables before every child, avoiding library/trace overrides.
Its `--help` lists identity, baseline, output and optional ledger inputs.

Four archive allocator tests cover alignment/zeroing, nested origins, shrink,
cross-thread free, reallocation ownership/payload preservation and refused layout
behavior. Strict Clippy passes for the archive core/checker/execute/CLI libraries
and binaries. Three receipt-reader tests include mode/process/PID/epoch, unknown
or duplicate rows, wrong affinity, out-of-lifetime checks, overlap, missing checks,
impossible aggregates, partial final records and JSON round-trip controls.

Raw receipts remain under the local scratch directory named in the JSON's
`raw_proof` list, with 26 SHA-256 hashes. The first app batch was excluded after
its verified write caught integer-map keys becoming JSON strings; the corrected
reader was tested and the complete batch repeated at a distinct destination.
The first public batch stopped on the native-positive defect; its full output is
retained, and the final batch records that mismatch while completing all cases.
These exclusions and the initial repaired documentation lint failure are explicit.

Delivery against a later main must inspect the full source delta and keep this
evidence qualified to its frozen SHA. The unchanged release requirement remains
equivalent complete-project fresh-process median TSR/pinned-tsgo wall <=0.50;
it is unmet and unverified.
