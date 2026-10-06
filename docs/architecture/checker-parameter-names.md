# Type-parameter name copying: rejected candidate

Task: `tsr-1yb.16.3.4`. Frozen Rust source:
`38b6e2b9a20d4a78558b46d718bd8c2eb7aac2d2`.
The borrowed-name candidate was rejected and its private source and three
ordinary target binaries were restored to the baseline. No runtime change
was shipped. [The receipt](checker-parameter-names.json) records source,
binary, input-reader and output hashes, the policy frozen before timing,
and the local raw receipt directory.

## Locating evidence

Sixteen fresh processes compared the ordinary baseline, probe disabled,
and two probe-enabled runs for each workload and mode. Explicit name-copy
counts repeated exactly; full outputs and loaded/reported scope were
preserved.

| Workload | Mode | Copy attempts | Text bytes |
| --- | --- | ---: | ---: |
| Real app | Default | 12,404,723 | 57,693,575 |
| Real app | Single | 8,114,401 | 37,567,000 |
| Public domain model | Default | 524,714 | 2,236,261 |
| Public domain model | Single | 524,621 | 2,236,168 |

The observer counts explicit copying at `local_type_parameter_types_of`.
It does not count allocator blocks or identify Checker owners; empty names
may allocate nothing. These counts establish repeated preparation, not a
causal wall-time share or a maximum possible speedup.

## Candidate and verification

[The unretained patch](checker-parameter-names-rejected.patch) borrows
immutable AST names through the Program lifetime in `declared.rs`,
`constraints.rs`, `members.rs`, `inference.rs` and `variances.rs`.
It keeps parameter symbol resolution, ordered traversal, declared TypeId
forcing and refusal paths. The parameter Vec and remaining projections
are still rebuilt on each query. There is no semantic cache, new key or
shared worker state. [The locating probe](checker-parameter-names-probe.patch)
is also preserved; neither patch is applied to production.

The full checker suite passed 1,492 tests with three existing ignores.
Strict release checker/execute Clippy passed. The final patch differs from
the full-test patch only in a documentation comment repaired for Clippy.
All 477,652 type payloads and 10,570 diagnostic-case payloads were
byte-identical to the baseline, with zero prior RIGHT losses. Initial
compile and documentation failures remain in the local raw receipts.

## Ordinary public confirmation

The frozen policy required a gain of at least 20ms against both the
baseline and identical-baseline A/A reference in each mode and workload,
with median RSS at most 1.05 times each reference. Two independent rounds
were required for retention. The first public round completed 42 fresh
processes, including five alternating triplets per mode.

| Mode | Baseline median | A/A median | Candidate median |
| --- | ---: | ---: | ---: |
| Default | 0.856806s | 0.864406s | 0.860050s |
| Single | 1.874035s | 1.857622s | 1.820667s |

Single mode met its gain floor. Default mode missed both comparisons.
The decision reader returned `revert` after correcting a gate-container
binding in its input; the invalid input and its `degenerate` output are
preserved locally. The threshold was unchanged. RSS passed.

App paired timing, the second round, prepared native focused controls and
the prepared direct checked-identity observer were not run after this
required gate failed. Loaded/reported scope equality does not prove actual
checked identities. No native ratio or 2x speedup is claimed.

## Follow-up boundary

Pinned native checker source
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` gathers local parameters across
eligible merged declarations and deduplicates declared type identities.
Rust currently selects the first declaration. This source discrepancy is
tracked in `tsr-1yb.1.1.2.1`; it is not a newly executed diagnostic failure.

The copy counts support investigating repeated native metadata/member
preparation. Any reuse needs a current owner, identity, publication and
receiver contract before caching. The rejection here does not establish
that all allocation work is unprofitable. The whole-project target remains
equivalent complete work with median TSR/pinned-tsgo wall ratio at most
0.50 and zero prior RIGHT losses; it remains unverified.
