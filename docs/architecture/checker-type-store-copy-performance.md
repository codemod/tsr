# TypeStore lookup copying

Measured for `tsr-1yb.16` and `tsr-1yb.7.8` against TSR
`39fcbfc1366d97b6f46d9ee42cbfc293de4929ee`. The change moves the existing
`(TypeFlags, TypeData, fresh)` key into the map entry and clones its payload only
when a type is created. It changes neither the key nor the lifetime of a type.
No new cache, dependency or shared TypeId domain is introduced.

## Locating evidence

The previous `TypeStore::intern_literal` cloned TypeData for every lookup and
again to store a newly created type on a miss. Four workloads were measured with
the original CLI, disabled probe, enabled probe and a repeated enabled run:
the real Next.js app and public literal, generic and mapped/recursive controls.
Complete diagnostics, effective options and loaded inputs matched. Enabled and
disabled probes checked exactly the same files; repeated accounting matched.
All three public controls also matched complete pinned-native diagnostics,
including their deliberately invalid assignments.

The app made **244,303** intern calls: **209,821** hits and **34,482** misses.
Lookup clones copied **6,700,371** owned payload bytes. Misses copied another
**1,904,785** bytes into the type store; that copy remains necessary in the
current representation because the map key and the stored type own their data.

| Lookup payload | Copied bytes |
|---|---:|
| Union text and constituent vectors | 4,017,555 |
| String literals | 1,365,896 |
| Intersection text and constituent vectors | 1,293,288 |
| Number, enum and bigint payloads | 23,632 |

These are counts at specific clone sites, not total allocator bytes or the CPU
share of allocation. Scalars, struct copies, allocator bookkeeping, construction
before interning, mapper keys, member projections and other checker tables are
excluded. Nonempty-buffer counts estimate standard-library allocation requests;
they are not hooks on the allocator. Broader attribution remains `tsr-1yb.16`.

The probe also observed 9,035 reserved-object completion clones with 975,774
payload bytes, a separate site excluded from the lookup-copy saving. The
retained type store had 184,603 types and 34,482 interned entries. Owned payload
capacities were 15,962,354 bytes in types and 1,909,621 in keys. These figures
exclude the Vec elements, hash table/control storage and other checker state.

## Native boundary and identity

Pinned native source is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
`getStringLiteralType`, `getNumberLiteralType` and `getBigIntLiteralType` reuse
checker-local types by their values and allocate on misses. Fresh/regular forms
remain distinct through `getFreshTypeOfLiteralType`. Native union/intersection
factories also intern completed constituent/alias identities.

This change preserves TSR's complete existing enum-based key, including flags,
freshness, constituent order and nominal/alias symbol identity. It does not
replace keys with printed text or claim that the renderer's existing
representation divergences are fixed. TypeId creation order stays unchanged;
hits do not reserve an unused type. Intrinsic and symbol-owned named creation
remain outside the interning table.

## Whole-CLI comparison

Both comparisons use immutable saved TSR binaries from the same source, with
only the interning change between them. The harness's `tsgo` slot contains the
saved TSR reference for these isolated experiments; these numbers are **not** a
TSR/native ratio. Each set has five fresh-process alternating pairs after one
warmup pair, with incremental/composite checking disabled and warmed files.
Builds and corpus tests ran outside the timing samples.

| Independent set | Reference median | Candidate median | Reduction |
|---|---:|---:|---:|
| First | 3.731048 s | 3.709803 s | 21.246 ms / 0.57% |
| Confirmation | 3.785392 s | 3.714625 s | 70.767 ms / 1.87% |

Both pass the existing 20 ms absolute comparison threshold. Complete 120 app
diagnostics, effective options and loaded-file identities matched throughout;
inputs were rehashed after each set. Matched separate probes established the
same 1,341 actual checked-file identities and exactly equal type counts, Vec/map
capacities and retained payload capacities on all four workloads.

Across all ten samples per side, median total user+system CPU was
3.752867 s before and 3.707159 s after. RSS medians moved in opposite directions
between sets; the combined medians were 1.082294 GB before and 1.089667 GB after
(+0.68%). Reference RSS ranged 1.046544–1.164231 GB and candidate RSS
1.034158–1.169834 GB. Retained store capacity is unchanged. These RSS
observations do not establish a memory saving or an exact zero peak-RSS change.
An identical-binary fresh-process control is recorded separately in the artifact
to characterize this variability; do not turn a noisy point estimate into a
memory claim. Ten runs of identical reference bytes ranged 1.059946–1.204273 GB;
alternating five-run groups differed by 1.36% in median RSS. This control
demonstrates variability, not an exact zero peak-RSS change from the candidate.

## Validation and reproduction

The current-source full type corpus is byte-identical before/after: **474,251**
rows, **459,493 RIGHT**, **2,194 GAP**, **12,564 WRONG**, with output SHA-256
`597675021541281a78f7c0c7df08e36915247d41d4c4024a6d82b6cd947e5aea`.
The first baseline capture completed successfully; its artifact parser initially
failed on a multiline printed payload. Parsing was corrected and the completed
bytes were reused, with no timing reconstructed for that capture.

The focused release suites pass **164** tests, including three direct positive/
negative interning identity controls and the existing literal, union and
intersection tests. Checker library/affected-test Clippy, formatting and diff
checks pass. Temporary accounting and checked-file instrumentation are removed
from production source and the normal CLI is rebuilt.

[Aggregate counts and comparison identities](checker-type-store-copy-counts.json)
include source/binary/input hashes, all variant rows, public control sources,
complete-diagnostic fingerprints, actual checked-scope fingerprints, all timing
samples and resource observations. Private app sources and diagnostic text are
not exported.

Apply [the locating probe](checker-type-store-copy-probe.patch) to the recorded
baseline source, build the release CLI, and set `TSR_TYPE_STORE_PROBE=1` for
counts and `TSR_ALLOCATION_SCOPE_PROBE=1` for checked identities. The two patched
source hashes are verified by replay. This probe is a measurement artifact;
its Drop-time accounting and opt-in maps are not part of the shipped checker.
For the matched candidate capacity/scope control, apply
[the retained-state probe](checker-type-store-retained-probe.patch) to the
candidate source and set `TSR_TYPE_RETAINED_PROBE=1` and
`TSR_ALLOCATION_SCOPE_PROBE=1`. Its two source hashes are also verified by replay.

The full-project release target remains verified TSR/pinned-native median wall
ratio **at most 0.50**, with equivalent work and diagnostics. These isolated
gains do not establish that target. Future changes must retain the full native
identity contract; the symbol-completion representation and generic alias
fidelity follow-ups remain separate work.
