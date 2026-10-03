# Binder scope-walk experiment

Measured 2026-10-03 for `tsr-1yb.7.4`, against main
`a697028a53aa641d72e1a1bd36d93feda21c0d11` and pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

The presence-bitmap candidate was reverted. Its final Clippy-clean build
improved the whole-project median by 17.909 ms, inside the existing 20 ms
comparison threshold. Earlier candidate measurements were promising, but do
not establish a confirmed gain for the final code. Production binder sources
and the rebuilt CLI are byte-identical to the reference. The JSDoc scope
regression control remains in `crates/tsr-binder/tests/jsdoc_scopes.rs`.

## Locate the work

The local Codemod Next.js workload used fresh compiler processes, warmed
filesystem state, and explicit `--noEmit --incremental false --composite false
--pretty false`. Builds and locating instrumentation were outside the timed
comparisons; this agent ran no builds or CPU-heavy tests during timing.
Complete effective options, loaded identities and input contents matched between
the two TSR binaries in each comparison.

A macOS `sample` of the reference TSR captured 2,953 main-thread self samples.
Disjoint nearest TSR owners included `resolve_name_excluding` at 92 and
`lookup_scoped` at 81. These are sampled-stack observations, not exact phase CPU
or whole-CLI wall percentages. They justify examining scope lookup, but cannot
predict its wall-time benefit.

A temporary opt-in query probe counted the actual walk:

| Counter | Count |
|---|---:|
| Resolution queries | 3,788,288 |
| Ancestor visits | 30,271,006 |
| Longest walk | 52 |
| Visits with a locals table | 6,177,990 |
| Visits without a locals table | 24,093,016 |
| Local name/meaning candidates | 2,787,087 |
| Distinct complete query keys | 623,630 |
| Repeated complete query keys | 3,164,658 |

Keys include the immutable BindResult domain, start NodeId, exact name, meaning
bits and excluded declaration. Repetition is an opportunity count; it is not
proof of completed semantic reuse or saved expensive work. Candidate name hits
also precede the walk's visibility and exclusion checks.

The probe used a mutex-protected aggregate and allocated query names; its single
enabled/disabled locating runs took 5.397/3.724 seconds. Those runs establish
instrumentation overhead, not compiler throughput. They preserved all 121
complete diagnostics and the same 1,341 checked-file identities in order.
All temporary instrumentation was removed.

## Candidate and ownership

Native `internal/binder/nameresolver.go:48` reads `location.Locals()` before
looking up the name. TSR's sparse outer map instead checks every AST ancestor.
The experiment stored one presence bit per node, set by a single table-creation
helper. A negative bit avoided the outer-map lookup; a positive bit still used
the same symbol table and canonical IDs. BindResult carried both representations
through binding additional files, then exposed them immutably to checkers.

Syntax-kind filtering alone needs more than `HAS_LOCALS`: conditional `infer`
declarations and JSDoc typedef templates create their own scopes. Declaration
destinations can also create tables lazily. The rejected bitmap covered all
creation paths and retained empty tables; it never skipped ancestor updates,
member/export lookup, static visibility, meaning or declaration exclusion.

The candidate's temporary assertion probe compared every absent read against
the actual outer map and checked all table-presence bits:

| Inventory | Count |
|---|---:|
| AST nodes | 6,261,734 |
| Locals tables | 158,566 |
| Empty locals tables | 48,823 |
| Absent reads validated against the map | 24,113,376 |
| Bitmap payload bytes | 782,720 |
| Bitmap retained capacity bytes | 1,565,040 |

The absent-read count includes other binder consumers; it is distinct from
resolution-walk visits. Bitmap bytes are exact vector payload/capacity counts,
not net process RSS or allocator overhead. RSS ranges overlapped.

## Fresh-process comparisons

Each round had one untimed warmup pair and five measured pairs in alternating
order. Both harness slots contained TSR; the slot named `tsgo` held the candidate.
None of these ratios is a TSR/native speed comparison.

| Build and round | Reference median | Candidate median | Improvement | Decision |
|---|---:|---:|---:|---|
| Initial reference-argument build, 1 | 3.666627 s | 3.572763 s | 93.864 ms, 2.560% | Promising |
| Initial reference-argument build, 2 | 3.629212 s | 3.586481 s | 42.732 ms, 1.177% | Promising |
| Final value-argument build, 1 | 4.518488 s | 4.500579 s | 17.909 ms, 0.396% | Inconclusive; revert |

The private four-byte NodeId argument changed to pass by value to satisfy
Clippy; the unused lifetime on the retained-memory helper was also removed.
The final build was measured separately. All earlier pairings remain in the
experiment history. Absolute runtimes varied; no cause is established here.
`decide.mjs` returned terminal `inconclusive` for the final comparison, so no
additional confirmation was run and the production candidate was restored.

Final-round wall samples, in measurement order:

- Reference: 4.518488, 4.484048, 4.556877, 4.579401, 4.517419 seconds.
- Candidate: 4.490022, 4.546912, 4.516253, 4.500579, 4.466927 seconds.

Final-round user/system CPU medians were 3.449871/0.484985 seconds for the
reference and 3.410258/0.479467 seconds for the candidate. Peak RSS ranges were
1,097,859,072–1,152,090,112 and 1,036,189,696–1,167,785,984 bytes respectively.
Each round loaded 13,097 files with identical complete diagnostic fingerprints.

## Controls and next action

Shallow/deep generated controls used 2/40 nested blocks, a successful outer
reference, an intentional TS2322 assignment, a missing-name TS2304, namespace
exports and a conditional `infer` reference. Reference, candidate, instrumented
reference and pinned native produced the same complete two diagnostics.

| Scope control | Queries | Visits | Longest walk | Table hits | Repeated keys |
|---|---:|---:|---:|---:|---:|
| 2 blocks | 50 | 293 | 9 | 120 | 17 |
| 40 blocks | 50 | 901 | 47 | 728 | 17 |

The existing binder controls cover shadowing, meanings, exports, conditional
infer branches, static type parameters and merged symbols. The new control
checks same-spelled JSDoc typedef templates resolve to different symbols and
remain invisible outside their comments. It passes against the restored binder.
The initial candidate passed 151 binder controls; the final candidate passed
all-targets Clippy after its private argument and lifetime adjustments.
Full-corpus candidate binaries were prepared but not executed after the terminal
timing decision; no full-corpus preservation claim is made for the discarded
candidate. Production source and CLI byte identity prove the restoration.

`tsr-1yb.7.7` owns the next measured investigation: attribute repeated queries
to the native checker-owned `symbolNodeLinks.resolvedSymbol` boundary.
Native `getResolvedSymbol` (`checker.go:13890`),
`getReferencedValueOrAliasSymbol` (`:13907`) and `getSymbolFromTypeReference`
(`:23077`) have distinct completion and diagnostic semantics. TSR's existing
`node_types` expression memo is a separate mechanism; type-reference resolution
also calls the binder directly. Caller counts must establish the applicable
opportunity before adding reuse. A generic name cache is not selected.

## Artifact identities

SHA-256 identities:

- Reference/restored CLI: `9787054f832ca8d6680eaee29b1d03bb12d2e104de901fed0e9e66736bed67c9`.
- Initial candidate CLI: `ab9db55831e197cb832785c96b02e757c14ffd512aef15da8e22af3d660a3131`.
- Final discarded CLI: `42ac3393ed1addfac2b54427be68fe61cf62edeafe7999b8d1238d88d28de69f`.
- Input contents/loaded identities: `395a32b083501c1cac01a17c6585385849f997fc10a3faab37b2f3d2d7bd9b16`.
- Sorted complete app diagnostics: `fa0e14ebbc31d7fb9f24f996a93bfb2888f16755748a89785f00309b0b33a190`.

Local investigation files are under `/tmp/tsr-scope-walk-a697`: `profile.rs`,
`probe-results.json`, `telemetry-controls.json`, three paired reports and decision
payloads, source snapshots and copied binaries. They are scratch evidence;
this report and the Beads issues preserve the findings across machines.

The release requirement remains a verified comparable whole-project TSR/native
median wall ratio ≤0.50 with equivalent performed work and no previously RIGHT
assertion losses. It remains unmet and unverified. See
[whole-project performance](whole-project-performance.md) for the benchmark
contract and comparability limits.
