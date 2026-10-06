# Intersection admission copying: measured, rejected candidate

`tsr-1yb.16.3.6` qualifies one allocation site at frozen TSR
`4cfe23401a346189756099437091a2c5f5656cf0`. Native remains
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, with TypeScript fixtures
`4d4f005c8541e0255a9d8791205fdce326e462bc`.
The `tsr-1yb.16.3.7` ordinary candidate failed its first required public
confirmation and was restored. Production code is unchanged.

The [receipt](checker-intersection-clone.json) binds source, binaries, input
guards, controls, the frozen policy and restoration. The
[probe](checker-intersection-clone-probe.patch) and
[rejected candidate](checker-intersection-clone-rejected.patch) are complete
replays against that source, not production features.

## Exact work boundary

After its reference, mapped and union branches, `instantiate_type_worker`
matches an intersection against `self.store.get(id).data.clone()`.
Cloning precedes the variant test. A non-intersection therefore copies its
owned payload before the pattern rejects it. An intersection also copies its
cached display text, although substitution consumes only constituents and symbol.

This differs from the shipped interner lookup-copy optimization `.7.8`.
It changes no interning key, mapper, publication state or cache eligibility.
Native `instantiateTypeWorker` (`internal/checker/checker.go:22220`) selects
the union/intersection branch before reading constituent types. Its existing
alias and mapper semantics remain the fidelity authority.

The isolated observer adds one exclusive origin around this expression only.
The guard ends before recursive substitution. Its successful allocation
layout requests are not all malloc calls, copied payload bytes or saved wall
time. Origin IDs aggregate across private Checkers rather than identifying
individual Checkers or admitted workers.

## Repeated site counts

| Workload | Mode | Clone entries | Allocation events | Requested bytes |
| --- | --- | ---: | ---: | ---: |
| Next.js | Default | 2,086,614 | 2,200,623 | 176,690,825 |
| Next.js | Single | 926,473 | 1,032,946 | 70,372,217 |
| Domain model | Default | 74,974 | 74,974 | 5,236,512 |
| Domain model | Single | 74,346 | 74,346 | 5,198,783 |

Each selected count repeats exactly in two enabled fresh processes. The
clone performs no reallocations in these runs. Its final live bytes are zero;
concurrent peak counters vary and cannot be added to other origins' peaks or
interpreted as RSS. These totals include admitted intersection copies and
rejected variant copies; they are not all removable by the candidate.

Seven projects run ordinary, observer-disabled, enabled and repeated-enabled
binaries in default/single modes: 56 completed compiler children, plus seven
initial full listings. Complete stdout, exit status and non-observer stderr
are identical within each comparison. Source hashes and input snapshots are
checked before and after each comparison. Next.js retains its 24 diagnostics
and 14,051 ordered listed files.

Public controls cover intersections, signatures, tuples, unions and receivers.
The intersection control records four entries/eight allocations. Signature
and tuple controls record three entries/three allocations each, demonstrating
copying on rejected variants. The union control records zero entries because
its branch returns earlier. Five existing allocator controls pass against the
extended collector. No new allocator algorithm was introduced.

Enabled Next.js observations take 44.670/45.589s default versus 9.358s
ordinary, and 16.785/18.750s single versus 13.331s ordinary. These unpaired
observations show substantial observer overhead; acceptance uses ordinary
binaries after removing the observer.

## Ordinary candidate and refusal

The candidate selects borrowed `TypeData::Intersection` first, then copies
only the constituent Vec and symbol before recursive queries can grow the
store. This preserves the owned constituent snapshot, source order, alias
symbol, error return and existing constructor. It introduces no borrowed
state across mutable recursion and no semantic cache.

The policy was frozen before execution: two independent rounds, five
alternating baseline/A/A/candidate triplets per mode and required workload,
at least 20ms median wall improvement against both references, and median
CPU/RSS at most 1.05 times each reference. A/A uses a byte-identical baseline
binary. Full output/input/source controls passed for all 36 first-round public
children, including warmups.

| Domain model mode | Baseline median | A/A median | Candidate median | Result |
| --- | ---: | ---: | ---: | --- |
| Default | 1.109090s | 0.889130s | 1.124708s | Wall gain fails both references; CPU/A/A 1.212264 also fails |
| Single | 1.728314s | 1.705651s | 1.675303s | Wall/CPU pass; RSS/baseline 1.050321 exceeds 1.05 |

RSS passes in default and against A/A in single. The two identical baseline
default medians differ by about 220ms. Host activity is uncontrolled, so this
is a failed retention gate, not proof of a causal candidate slowdown. The
single RSS threshold was not relaxed for its narrow miss. No app candidate
timing or second confirmation follows a failed required public workload.

Before the candidate, unfiltered eligible baseline dumps completed with
477,652 aligned type rows (467,435 RIGHT) and 10,570 diagnostic cases. The
type dump has 477,658 physical lines because payloads can contain newlines.
Only the baseline corpora ran: candidate full corpora, package tests, lint,
format/anchor gates and native replay were not executed after the performance
refusal. No candidate corpus preservation or new coverage score is claimed.
The 69,388 corpus-input hashes were captured during the first baseline run
and still match at restoration; these are not per-child corpus input guards.

## Restoration and next action

All 665 baseline source/manifest/lock files, including 643 Rust files, are
restored byte-identically with no added Rust files. All three ordinary target
binaries are restored, and rebuilding the ordinary CLI reproduces its frozen
baseline hash. The 99 controlled CLI children exclude build, collector-test
and baseline-corpus processes. Private app source, diagnostic text and input
paths are retained locally only. Actual checked identities and equivalent
native semantic work remain unverified.

Follow-up `.16.3.8` measures a different site: anonymous property payloads
copied before an existing instantiation-cache lookup. It must distinguish
actual complete hits from misses/refusals before proposing copy-after-lookup;
the mapper/print-context contracts still govern existing cache semantics.

Task-cut correction: `.16.3.5` duplicated the already rejected `.16.3.4`
[borrowed-name experiment](checker-parameter-names.md), discovered by searching
the actual helper in repository documentation. It is closed without repeating
that experiment. The complete-work TSR/pinned-tsgo median wall target remains
**<=0.50**, unmet and unverified.
