# Current type-parameter graph walk costs

The current walk makes 1,027,823 scratch allocation/reallocation requests in default mode and 1,286,021 in single mode. Their cumulative requested bytes are 19,958,208 and 26,283,360. Most root queries that create cycle markers use at most four visited identities. This selects a bounded query-local inline-storage experiment under `tsr-1yb.7.3.1`; whole-CLI benefit remains unproven.

All measurements freeze Rust `1b8821a498114ba60bf841bd17875ac181f6f116` and the pinned native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`. They are not measurements of a later main. Runtime hooks exist only in a private archive; the four original files and pre-session CLI were restored exactly. [The receipt](checker-graph-walk.json) records binaries, captured source, source replay, raw receipts, all counters and gates. The normal release binary has no observer hooks.

## Current ordinary CPU locator

Two single-worker ordinary-release main-thread intervals contain 1,676 and 1,687 samples. The nearest TSR owner `mentions_type_parameter_inner` accounts for 73 and 87 disjoint samples, including 24 and 16 allocator self samples. It is the largest sampled nearest owner in both intervals. Those are sampled intervals, not whole-process CPU shares or a savings ceiling; default worker threads are not represented. Three ordinary children preserve complete output and the observed physical input snapshots.

## Exact observed work

| Counter | Default, four checker owners | Single, one checker owner |
| --- | ---: | ---: |
| `roots` | 1,438,396 | 1,676,187 |
| `visits` | 5,505,464 | 7,377,167 |
| `unique_visits` | 4,758,810 | 6,164,557 |
| `duplicate_visits` | 746,654 | 1,212,610 |
| `explicit_parameter_comparisons` | 11,179,694 | 12,997,378 |
| `visited_comparisons` | 14,133,837 | 22,656,552 |
| `visited_hits` | 169,491 | 292,059 |
| `markers` | 2,087,946 | 2,804,150 |
| `terminal_leaves` | 2,883,475 | 3,978,664 |
| `fallbacks` | 299,768 | 419,496 |
| `empty_names_fallbacks` | 189,275 | 326,961 |
| `fallback_payload_bytes` | 3,900,786 | 5,423,163 |
| `scratch_allocations` | 1,027,823 | 1,286,021 |
| `scratch_requested_bytes` | 19,958,208 | 26,283,360 |
| `printing_allocations` | 299,768 | 419,496 |
| `printing_requested_bytes` | 3,900,786 | 5,423,163 |
| `printing_usable_bytes` | 6,391,408 | 8,834,736 |
| `max_visited` | 50 | 50 |
| `max_capacity` | 64 | 64 |
| `roots_visited_0` | 555,058 | 627,649 |
| `roots_visited_1_4` | 771,803 | 862,848 |
| `roots_visited_5_16` | 109,375 | 182,061 |
| `roots_visited_17_64` | 2,160 | 3,629 |

Two enabled snapshots per mode repeat exactly globally and per owner. Independent global atomics reconcile against each owner's totals, using maxima for maximum fields. In all twelve app children, complete diagnostics have 46 entries and fingerprint `a39b0cfead1232d06fc2ec8d43fe8f864964f75a6e59e462265d0d5cb6b2beec`; 14,050 ordered files are loaded, 14,746 parses are reported, and 1,397 full-file checks are reported. Direct disabled/enabled traces preserve all 1,397 checked indices/names and file affinity. Earlier 09b65ead hit-copy observations had 73 diagnostics; concurrent parser/property changes are part of this newer frozen source, so the counts are not interchangeable.

The before/after physical snapshots contain 13,992 records per mode and agree. They start after the first ordinary child and omit embedded library bytes, extends/package queries, directory state and transient reads. Normal runs lack direct checked traces. These observations do not establish full input-query or native performed-work equivalence.

## What the counts mean

Every original parameter test precedes visited membership. The explicit parameter slice and checker registry are counted separately; instrumented scalar comparisons preserve the original membership answers and short circuits. Marker pushes stay after mapping/template detection and before structural traversal, retaining the shipped terminal-leaf placement. The observer changes no graph edge, fallback or semantic cache. Unique visits are per-root TypeId-set cardinalities, including terminal leaves and parameter hits; duplicate visits are not all expensive repeated structural work.

The distinct-request ledger uses checker-local root identity, mode, ordered explicit parameter/name images and registry length. It intentionally does not certify evolving graph metadata or exact registry contents. Its retained sets are observer storage. Repeated shapes must not be treated as native completed answers or safely memoized booleans: executable controls demonstrate changed answers after registry/metadata updates.

The archive allocator delegates unchanged requests to `System`. Thread-local tags surround only original `Vec::push` and fallback printing, and nested tags restore their parent. Root deltas exclude observer query/visited HashSets, trace storage and key copies. Successful alloc/alloc_zeroed/realloc events, cumulative requested bytes and macOS usable size-class bytes are distinct; they are not retained/live heap, copied bytes, malloc calls or saved wall time. Realloc can grow in place. Peak RSS is recorded separately in each child.

The 771,803/default and 862,848/single roots with one through four markers each need one initial vector allocation in this observed implementation. With four-byte TypeIds, their initial requested capacity is 12,348,848/default and 13,805,568/single bytes. This is a bounded allocation opportunity for inline scratch, not a normal-work wall ceiling. Comparisons remain small enough in this workload (maximum 50 markers) that a whole-store bitset or unconditional HashSet is not justified by these counts.

## Qualification and limitations

Eight Rust controls execute the actual method on deep/wide/diamond graphs, cycles, same-name distinct identities, registry changes, later metadata, fallback rendering, predicates/this/constraints/defaults, deferred/indexed/mapped/conditional/object/tuple/union edges, and nested/thread-isolated allocation tags. Removing the visited-comparison increment fails three of the original six controls. Restoration is byte-exact and the final comparison body matches that mutation baseline. The final eight controls pass.

Forty-eight additional TSR children cover the existing repeated-signature, ordered-argument, fresh-shadow and mapped negative fixtures in both modes: normal/disabled/enabled/repeat outputs and counters agree. Eight qualified native children match the complete diagnostics in all eight variants. The initial eight native children used a symlinked configuration/cwd and six comparisons differed only in diagnostic path prefixes; those raw failures remain preserved. Physical-path reruns qualify the final matches. This is four supported public families, not full app parity or native expensive-work proof.

The complete checker suite passes 1,498 tests with three existing ignored tests across 101 result blocks, with the observer enabled and tests serialized. Strict release checker/execute library/tests Clippy, repository-config formatting, eleven Python receipt controls and five-file byte-exact patch replay pass. Canonical compiler/runtime/vendor files are untouched.

Two rotated observations per app role record wall/user/system/RSS in the receipt. Default wall ranges are wide (normal 5.682–6.520 seconds, disabled 3.156–3.218, enabled 3.244–7.050); single ranges are normal 5.043–5.096, disabled 4.853–4.870, enabled 5.635–5.957. Uncontrolled host variation and two observations do not support a causal overhead estimate or a speed claim. There are 79 semantic children plus ten showConfig children, with builds/tests outside those measurements.

## Reproduce the archive

Use a private checkout of the exact Rust source above with the pinned vendor revision. Copy [the module](checker-graph-walk-probe.rs) to `crates/tsr-checker/src/graph_walk_probe.rs`, then apply [the four-file patch](checker-graph-walk-probe.patch). Build the normal binary before applying hooks and the observer binary afterward; record exact executable hashes in bindings with `normal`/`probe` path, SHA-256 and source fields.

Run `python3 docs/architecture/checker-graph-walk-controls.py --bindings /absolute/bindings.json --project /absolute/tsconfig.json --output /absolute/new-output`. `TSR_GRAPH_TRACE` records direct owner checks independently of `TSR_GRAPH_PROFILE`, which enables counters. The reader rejects unfinished/failing children, missing counters, wrong owner affinity, duplicate checks, impossible partitions and mismatched global/owner sums. Restore all four runtime files, remove the temporary module, and restore any reused target CLI afterward. The allocator observer is macOS-specific and cannot be a production implementation.

`tsr-1yb.7.3.1` owns the next candidate. Preserve original ordered marker semantics, exact identity, parameter precedence, fallback behavior and query-local lifetime with a bounded spill path. Keep no new dependency or semantic memoization. Full type/diagnostic preservation and two independent same-source ordinary-release five-pair whole-CLI confirmations under fixed wall/RSS gates remain required. The equivalent-work TSR/native median wall target of at most 0.50 remains unverified.
