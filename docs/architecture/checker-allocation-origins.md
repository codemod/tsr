# Current checker allocation origins

The `tsr-1yb.16.3` refresh measures ordinary TSR
`2bf3601b5a7382f8adc31d588f31069bce0d0f2f` and an isolated allocator observer.
Pinned native is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The observer separates allocation traffic at three operation boundaries from
their nested parameter walks and instantiation workers. It preserves all 40
ordinary/probe output comparisons, including 24 complete Next.js diagnostics
and 14,051 ordered loaded files in both worker modes. No runtime optimization
or comparable TSR/native speed result is retained.

The [receipt](checker-allocation-origins.json) binds sources, binaries,
drivers, controls, full public outputs, private-output fingerprints and
restoration. The [eight-file probe](checker-allocation-origin-probe.patch)
applies to the frozen TSR source above. It is an archival measurement tool:
its feature, allocator and public module are absent from production.

## Why the ranking changed

The previous [cost profile](current-project-cost-profile.md) predates the
checker pool and loader changes. This refresh samples four fresh compiler
children: early and later intervals in default and single-worker modes.
The sampler attaches only to its own unreaped child. Per-thread nearest-owner
accounting includes all sampled threads rather than treating the main thread
as the checker. Default reports five sampled threads; single reports one.
Those are sampled thread roots, not verified checker admission counts.

In the later intervals, allocator-library leaves most often have
`instantiate_type` as their nearest TSR owner: 293 default and 96 single
samples. `instantiate_for_reference_with_this` has 180/67, and
`collect_structured_property_names` has 169/56. Receiver/property operations
and heritage preparation also remain material. Main-thread waiting dominates
some default stacks. These interval observations include waits, omit unsampled
work and are not CPU percentages, allocation counts or saved-wall ceilings.

That ranking selects three boundaries in the private observer:

- `instantiate_for_reference_with_this`: receiver-mapper preparation.
- `instantiate_type`: requests, including direct substitutions, signature
  admission, parameter checks and early refusals.
- `collect_structured_property_names`: member-name projection walks.

Nested `instantiate_type_worker` entries and parameter walking inside receiver/
instantiation requests have separate origins. A nested origin replaces its
parent for traffic accounting; its bytes are not added to a parent's exclusive
row. Uninstrumented work remains explicitly unclassified.

## Complete-project counts

Each mode has two fresh enabled runs. Entries and successful allocation/
reallocation counts and cumulative requested bytes repeat exactly for every
selected origin. Unclassified traffic and concurrent peaks vary slightly.

| Exclusive origin | Default entries | Default allocation/reallocation events | Default requested bytes | Single entries | Single events | Single requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Receiver mapping | 3,078,581 | 17,663,058 | 475,831,206 | 1,360,718 | 6,015,823 | 160,808,563 |
| Instantiation request | 7,806,956 | 9,958,571 | 827,636,641 | 4,510,706 | 3,749,184 | 306,997,277 |
| Member-name walk | 761,537 | 15,830,768 | 1,697,533,701 | 632,422 | 12,691,534 | 1,362,720,617 |
| Instantiation worker | 3,134,220 | 38,405,994 | 1,917,675,834 | 1,376,498 | 16,771,070 | 867,626,153 |
| Parameter walk within measured requests | 5,281,687 | 5,401,821 | 96,131,637 | 2,796,359 | 2,474,510 | 41,040,176 |
| Unclassified | — | 150,399,966 | 8,604,911,607 | — | 100,274,061 | 6,737,499,006 |

These are Rust global-allocation layout requests at successful allocation/
reallocation hooks, not malloc calls or copied payload bytes. Reallocation
traffic belongs to the executing origin, while live storage retains its
original allocation origin. Allocation failure, startup before enabling,
non-Rust allocation, stacks, usable sizes and retained system pages are excluded.

The first runs' total peak requested-byte counters are 1,877,162,082 default
and 1,919,185,224 single. They describe the observer's accounting events,
not physical-memory enforcement or process RSS. Per-origin peaks occur at
different times and cannot be summed. Final selected-origin live bytes are
zero; remaining unclassified buffers are 1,634/1,702 bytes. Quiescent live and
padded-byte sums match their total counters.

Default invokes about 2.28 times as many instantiation workers as single.
This locates duplicated private-checker work; it does not prove which calls
are eligible for reuse or whether native executes equivalent work. Worker
entries may still contain internal cache answers and refusals. Completion,
publication, saved expensive computation and native work remain separate gates.

## Controls and observer cost

Five allocator controls pass against the final reporter, including aligned
zeroed allocations, cross-thread frees, resizing under a different executing
origin, rejected layouts, nested owners and unwind restoration. Two mutations
are detected: charging reallocation traffic to the original storage owner,
and restoring the wrong origin after a nested guard.

Four public generic/mapped/receiver/cyclic-import projects and Next.js run with
ordinary, observer-disabled, observer-enabled and repeated-enabled binaries in
default/single modes: 40 distinct completed compiler children. Full stdout,
non-observer stderr, exit status and ordered loaded outputs are preserved.
The receiver fixture positively enters all selected origins. Native parity
and full unfiltered corpus qualification are not claimed from these controls.

The first reporter fails its own live-sum assertion: formatting adds 64 bytes
between an origin snapshot and the total. That failed enabled child and its
binary remain archived. The corrected reporter disables collection before
formatting, after scoped checker workers join; prefixes still allow subsequent
deallocation to debit the correct original owner.

Observer overhead is substantial. The default Next.js sequence takes 14.988 s
ordinary, 15.256 s disabled and 45.449/43.633 s enabled. Single takes
16.322 s ordinary, 17.402 s disabled and 18.846/18.683 s enabled. These are
unpaired observations under uncontrolled host activity. Other fresh ordinary
checks range from 9.247 s default to 16.582 s single. No throughput gain,
precise overhead ratio or native target success follows. Acceptance timings
must use ordinary binaries with this observer removed.

The first sampler attempt returns nonzero without a report; its child metadata
was not saved by the initial driver, so it is excluded from qualified counts.
The resumed driver retains completed observations and persists subsequent raw
sample results before validation. Caller-supplied historical query inputs remain
partial. The source-qualified loaded-input fingerprints and complete output
controls do not certify actual checked-file or native semantic-work equivalence.

## Implementation handoff and restoration

`tsr-1yb.16.3.5` investigates borrowing names in
`local_type_parameter_types_of` from the existing immutable AST lifetime.
The helper currently creates owned strings while resolving every parameter's
declaration type. Borrowing may remove those copies without another cache or
skipping declaration forcing. First measure that copy site: the receiver-origin
row is an upper bound, not its isolated benefit. Preserve declaration order,
same-spelled distinct TypeIds, defaults, JSDoc ownership and failure behavior.

tsr-1yb.16.3.6 separates temporary worker payload copies from published TypeStore
ownership before selecting another borrow/move candidate. Its broad origin
count does not establish copied bytes or saved wall time.

Member completion remains with `.16.2`/`.4.2.1` and the published-symbol
prerequisites `.32`/`.33`; mapper publication remains `.16.1`/`.4.1.2`.
Parameter-walk work remains `.7.3`. These counts do not qualify a broad cache.
Any retained candidate still needs complete current-source fidelity and
changed-diagnostic replay, then independently confirmed ordinary whole-CLI
benefit with CPU/RSS controls. The complete-work TSR/pinned-tsgo median
wall target remains **<=0.50**, unmet and unverified.

All 665 baseline Rust/manifest/lock files, including 643 Rust files, are restored
byte-identically with no added Rust files. The 108 pinned library build inputs
remain intentionally present in the archive. Rebuilding the ordinary CLI after
restoration reproduces the original binary hash exactly. Canonical runtime
source is unchanged. Private app source and diagnostic text are not published.
