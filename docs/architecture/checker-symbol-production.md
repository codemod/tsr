# Production checker symbol ownership

The first production storage foundation supports checker-owned symbols without
inventing binder indexes. It establishes an ownership boundary used by real
unresolved type references. It does not establish a speedup, completed alias
resolution or node-symbol reuse. Storage task `tsr-1yb.7.7.1` remains open.

`SymbolRef` owns a domain stamp and an opaque bound/private index. Each bound
store has one stamp; each Checker has a separate private stamp. Access validates
the stamp before indexing, and retained handles keep stamps alive without
retaining symbol records or AST data. Two Checkers on one Program accept the
same bound handle and reject each other's private handles. No unsafe sharing,
process-global cache, new dependency or private-to-binder cast is introduced.

`CheckerSymbols` owns unknown, full-path unresolved records and single-hop
merged redirects. Private tables preserve absence. Clone accessors copy tables
and declarations independently, retain shallow symbol edges and explicit origin,
reset CheckFlags and export/declared-type links, and keep Program records
immutable. Raw parents remain separate from redirects; native merge workers,
late-bound parent forcing and alias stack publication remain separate ports.

The unresolved-reference consumer follows native entity-name AST recursion.
An identifier whose text contains a dot remains one segment; an empty right
identifier returns unknown before allocating parents. A full-path pool collision
retains the first record's name/parent, after performing required parent recursion.
The first candidate split printed text; final acceptance evidence belongs to the
corrected AST implementation. Existing presentation TypeIds and substitution work
remain after symbol selection. This adds the distinct native unresolved intrinsic
but does not memoize a type by node or change type arguments' ownership.

The [machine-readable evidence](checker-symbol-production-controls.json) stores
source/binary/probe hashes, library bytes, complete output fingerprints, every
timing sample and retained capacities. Private source and diagnostic text stay
outside the repository.

The initial frozen baseline is `b0475d3b2bdab8537c65f73d322e86b46d9ac510`;
the native corpus/library pin is
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Initial validation passes 133 focused
release tests, with one existing ignored test tracked by `tsr-6.57`, strict Clippy
for affected libraries/tests and the full CLI/verdict builds. Fifteen integration
controls exercise production domain operations; an internal weak-stamp control
and real Checker test cover retention and construction.

All 474,251 type-result rows are byte-identical to baseline: 459,505 RIGHT,
2,185 GAP and 12,561 WRONG. All 10,570 diagnostic cases, including empty cases
and complete expected/actual lists, are byte-identical: 4,797 EMPTY_RIGHT,
2,838 RIGHT, 2,650 WRONG and 285 EMPTY_WRONG. No previous RIGHT is lost.

The initial normal CLI retains the same options, 13,097 loaded identities/input bytes,
1,341 actually checked identities/input bytes and all 120 app diagnostics.
Normal, disabled-probe and enabled-probe outputs agree; baseline/enabled/repeated
checked identities agree. A public TS/JS control checks three files and preserves
all five diagnostics against both baseline TSR and pinned native. Loaded and
checked bytes were reverified after measurements.

The unchanged whole-project runner compared initial frozen baseline TSR with its
normal candidate in two serial sets of five alternating fresh-process pairs.
Its `tsgo` slot held baseline TSR, so these are ownership overhead observations,
not native performance ratios. Incremental/composite were disabled; filesystem
cache was warm.

| Observation | Baseline median | Candidate median | Candidate change |
|---|---:|---:|---:|
| First wall set | 4.457639 s | 4.484720 s | +27.081 ms (+0.608%) |
| Confirmation wall set | 4.448438 s | 4.431950 s | -16.488 ms (-0.371%) |
| First peak RSS set | 1,091,371,008 B | 1,121,173,504 B | +2.731% |
| Confirmation peak RSS set | 1,149,108,224 B | 1,109,262,336 B | -3.468% |

The opposite signs establish no confirmed wall/CPU/RSS improvement or regression.
They do not prove zero overhead or the whole-project native ratio target of 0.50.
The app retains 366 private records in 512 slots: 94,208 inline bytes,
4,253 owned-name bytes and 7,716 full-path key bytes. There are 365 unresolved
entries, no merged entries and no allocated member/export edges in this run.
Those figures exclude map buckets, stamps and allocator overhead. The 623 live
stamp references do not count handle clone events.

Three component observations report 52/54/52 ns for storage construction/drop
and 3/3/3 ns for private handle clone/drop; the handle is 16 bytes on this host.
These are bounded component loops, not full Checker construction, allocator
counts, a native comparison or a measured whole-project cost share.

To reproduce storage observations, use a scratch checkout with this production
change and apply the [candidate probe](checker-symbol-production-candidate-probe.patch).
Build release `tsr` and the `symbol_storage_cost_probe` checker example, using
the pinned `TSR_LIB_PATH`. Run the CLI with `TSR_SYMBOL_STORAGE_PROFILE=1`
and no-emit/nonincremental/noncomposite options; the probe writes actual checked
identities and final storage usage only to stderr. With the environment variable
absent it produces no counters. Run the example for component observations.
The [baseline probe](checker-symbol-production-baseline-probe.patch) applies to
the frozen baseline and emits the same checked identities with a storage-absent
sentinel. Use uninstrumented binaries for timing. The probes are archived patches
and are not part of the production CLI.

Bound SymbolStore currently cannot distinguish native absent tables from
allocated empty tables. `tsr-1yb.7.7.1.1` owns that representation/mutation audit;
table emptiness cannot recover absence or certify completed members. Full
consumer migration remains `.7.7.2`, native alias resolution/publication `.7.7.3`
and measured node completion `.7.7.4`, coordinated with `tsr-6.49` preparation
and `.3.2` Checker affinity. Their dependencies keep private symbols from reaching
bound-only consumers before those contracts are satisfied.


Before delivery, the foundation was rebased onto concurrent main commit
`9a44a194`. Native raw-parent review also added the empty-parent separator
control before building that candidate. Fresh checks pass 140 focused release
tests, retain the one ignored `tsr-6.57` control and pass strict affected Clippy
and the full CLI/verdict builds. The full 474,251 type rows remain byte-identical
to this updated baseline: 459,549 RIGHT, 2,183 GAP and 12,519 WRONG. All current
diagnostic case output is also byte-identical. Complete normal app diagnostics,
loaded scope, options and input bytes agree with this updated baseline.

`rebase_validation` in the evidence stores the separate current source, binary
and output identities. The timing/storage observations above belong to their
initial source snapshot; the rebased foundation has no new measured speed claim.
Actually checked identity traces above also belong to the initial snapshot; the
fresh normal app gate reports loaded identities and complete diagnostic equality.
