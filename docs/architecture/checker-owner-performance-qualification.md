# Owner port correctness and refcount cost

The selected owner migration now preserves every previously RIGHT legacy row
and every previously EXACT native case, but its runtime retention is refused by
the measured CLI regression. The checker-local lifetime holder reduces that
regression; it does not establish the release target. The six goal tickets stay
in progress.

All measurements here use frozen main
`2e47ffaad470f937bf9a71abdafb6402902f5fdf`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` and TypeScript corpus
`4d4f005c8541e0255a9d8791205fdce326e462bc`. Private source variants are separate
from canonical Rust. [Evidence](checker-owner-performance-qualification.json)
records source manifests, binary hashes, actual terminal receipts and samples.

## Source and preservation

[The owner replay](checker-owner-qualified-replay.patch) changes exactly 92
crate files from frozen main. It reconstructs all 1,056 guarded inputs exactly.
[The lifetime delta](checker-owner-local-lifetime.patch) changes only
`symbol_access.rs`, including the added lifetime/equality controls. Applying both
patches to a fresh private baseline copy also matches all 1,056 inputs exactly.
Neither patch is a retained production optimization. Temporary vendor symlinks
used to run the managed-worktree oracle are excluded.

The original owner variant, step111, passes the full unfiltered native gate:
14,960 population rows, including 12,797 compared cases, 45 listed skips and
2,118 native skips. Frozen main has 8,275 EXACT cases; the owner variant has
8,303, with 28 gains, zero losses and zero missing cases. The existing one
checker failure and one checker timeout remain in the compared denominator.
Separate halves preserve all 9,325 exact diagnostic halves and 10,776 exact type
halves, with nine and 37 gains respectively. Native skips are not successes.

The unfiltered legacy comparison retains all 552,533 unique type assertion keys
and 12,238 diagnostic keys: 124 type gains, nine diagnostic gains, zero prior
RIGHT losses and zero missing/added keys. Four changed remaining-wrong type rows
and six changed remaining-wrong diagnostic rows remain recorded. The workspace
test run has 3,286 passes, zero failures and 19 existing ignored tests; strict
workspace all-targets Clippy, formatting and work-trace compilation pass.
The workspace/corpus source differs from step111 only by the corrected Clippy
doc-markdown comment. The actual complete native run uses step111 itself.

The local `Arc` control variant, step133, passes all 230 checker library tests
and strict checker all-targets Clippy.
The controls require equal bound keys across independently allocated holders,
equal hash-map lookups, cross-checker reads on the same Program, foreign Program
rejection after index reuse, retained lifetime after both record store and
Checker drop, final holder release, and `Send + Sync`/thread transfer.
Its actual complete native run preserves all 8,303 EXACT cases against step111:
zero gains/losses/missing, zero diagnostic-half losses and zero type-half losses.
Against main it retains the same 28 case gains, nine diagnostic-half gains and
37 type-half gains. The one existing failure/timeout and native skips remain.
These gates do not substitute for broader runtime review or a measured keep gate.

## Confirmed contention and remaining regression

The original bound handle clones the same Program-stamp `Arc` from every Checker.
Native reads keep symbol pointers and publish IDs through `ast.GetSymbolId`
(`internal/ast/utilities.go`); reading a symbol edge does not increment a shared
lifetime refcount. The Rust representation adds this ownership cost to preserve
safe store validation and retained handles.

The corrected opt-in probe counts all seven bound-handle clone/lift sites from
before Checker construction through checking and diagnostic projection. Two
complete repetitions each observe **21,548,677** events across four owners and
**21,320,280** with one owner. These are handle operations, not expensive semantic
worker executions. Later CLI serialization is outside the counter interval.
The earlier 21,098,794/20,874,099 result omitted four raw-edge sites and remains
explicitly a lower bound. No production counters are retained.

The local holder is `Arc<SymbolStoreIdentity>` allocated once per Checker.
Clones touch that Checker's holder rather than the shared Program stamp.
Equality/hash delegate to the same inner Program identity; a holder allocation
is never a semantic identity. Existing publication states, aliases, receivers,
private Checker identities and native lazy ID publication are unchanged. This
adds no semantic cache, global registry, unsafe pointer or inferred epoch.

The isolated two-million-clone microprobe supports the contention mechanism,
but the full CLI comparison determines the benefit. In five interleaved fresh
process pairs, local `Arc` versus the original owner variant improves four-worker
wall time from **0.955339 s to 0.741066 s** (22.43%) and median user CPU from
2.359068 s to 1.503757 s (36.26%). Single-worker wall time is essentially
unchanged: 1.426215 s to 1.445456 s. The microprobe is not a saved-wall estimate.

The independent direct main/local-Arc/native batch still refuses the whole port:

| Mode | Frozen main | Owner + local `Arc` | Pinned native | Owner/main wall ratio |
| --- | ---: | ---: | ---: | ---: |
| Four checker workers | 0.597028 s | 0.722384 s | 0.317510 s | 1.209968 |
| Single checker | 1.183679 s | 1.375961 s | 0.487980 s | 1.162444 |

All sides have matching effective options, loaded input identities, complete
diagnostics and exit fingerprints; each emits the same TS2322 control. Each mode
has a warmup followed by five measured fresh children per tool. Builds, counters
and profilers run outside timing; incremental/composite state is disabled.
Source and project inputs are unchanged before/after. CPU/RSS distributions and
every sample are in the evidence. Do not combine medians from different batches.
Actual complete cross-tool performed work remains unverified, so the verified
native ratio is **null** and the <=0.50 release target remains unmet.

Replacing the holder with `Rc` is rejected. A direct five-pair local-Arc/Rc batch
gives Rc/Arc wall ratios **1.001725** with four checkers and **0.993764** with one.
There is no meaningful full-CLI benefit to justify changing standalone handle
`Send`/`Sync`. The retained safe `Arc` experiment and rejected Rc source/binary
remain distinct. Removing uncontended atomics is not the next supported target.

## Naming preparation attribution

The complete single-worker profiles after the local-holder change preserve
identical stdout/stderr. Disjoint nearest-boundary accounting attributes
**123/1,110 checker samples** to `best_name_ref`, including 64 allocator leaves;
main's naming boundary accounts for 8/964. These are sampled weights, not
allocation counts or an exact saved-wall ceiling. Raw profiles remain at the
source/binary-qualified local paths recorded in the evidence.

All **130 naming-ancestry leaf samples** are also inside
`check_object_literal_members` in that profile. This includes the 123 disjoint
naming-boundary samples plus nested resolution work. The allocation/preparation
handoff therefore also belongs to `tsr-1yb.16.3.10`: preserve native semantic
member construction and defer presentation where native does. Faster table
preparation does not complete the eager-rendering or member-builder acceptance.

[The naming probe](checker-owner-naming-probe.patch) adds opt-in counters to the
safe source and reports after checking/diagnostic projection. The probe's map
keys are actual Program `NodeId`s or validated `SymbolRef`s, plus one Globals
key per Checker. It does not use printed names, force native IDs, publish
semantic answers or supply a result to the checker. Each worker resets its
private thread-local observations before construction. The complete scope is
402 checked source files plus loaded declarations; loaded input agreement is
465 files. Resolved-export cache reuse is not implemented or certified here.

Two repetitions preserve exact diagnostics and counts in both worker modes:

| Table origin | Queries | Entries copied | Alias entries | Name payload bytes copied |
| --- | ---: | ---: | ---: | ---: |
| Locals | 5,200 | 865,904 | 516,704 | 11,536,320 |
| Scope raw exports | 400 | 800 | 0 | 4,000 |
| Globals | 400 | 891,600 | 0 | 15,219,600 |
| Alias raw exports | 516,704 | 0 | 0 | 0 |

There are no direct wins on these primary queries in this fixture. Of the raw
alias-export queries, 348,736 have absent tables and 167,968 initialized-empty
tables; that distinction survives the counter. Actual alias-export keys differ
by owner: 3,447 summed across default Checkers versus 2,015 single. Equal empty
tables are not interchangeable. The counts record copied payload length, not
allocator capacity, allocated bytes, expensive worker executions or cache hits.

Pinned native `trySymbolTable` performs keyed direct lookup before preparing
alias candidates. `getSymbolTableAliases` filters locals without caching,
excludes members and caches eligible globals/raw/resolved exports by native
table identity. The first selected handoff is `tsr-1yb.33.1.2`: preserve the same
selected owners and scope order while moving direct lookup ahead of copying and
preparing only aliases after a miss. Native table reuse, private mutation/reset
and raw/resolved publication remain distinct acceptance under `tsr-1yb.11.5`.
[The stamp-site probe](checker-owner-stamp-probe.patch) separately records all
seven bound ownership sites; it does not supply naming eligibility.

## Remaining acceptance

`tsr-1yb.11.6` owns the actual handle-site attribution and lifetime-holder
experiment; `tsr-1yb.11.5` already owns native alias-only naming-table scans.
The private naming preparation under `tsr-1yb.33.1.2` now passes the gates below,
but its whole-port main comparison remains red. The broad 92-file
runtime review remains incomplete. No correctness prerequisite is a speed win,
and no diagnostic/loaded-file agreement certifies complete equivalent work.
Canonical integration, member builder/publication, broad alias acceptance,
eager-rendering handoff, representative full-project attribution and verified
TSR/native median wall ratio <=0.50 remain under the original six goal tickets.

## Verified direct and borrowed preparation

[Direct preparation](checker-owner-direct-preparation.patch) applies to step133
and moves keyed lookup before alias snapshots, preserving the existing
ExportSymbol candidate competition. Only aliases acquire copied names and
selected handles. [Borrowed preparation](checker-owner-borrowed-preparation.patch)
then borrows immutable Program local/global table keys through the mutable
alias-resolution walk. Export and class display keys remain owned. Neither
variant changes alias forcing, receiver context, spelling, scope/declaration
order or the nested raw alias-export route, and neither adds a semantic cache.
Each production delta changes only `checker.rs`.

Both variants pass 230 library and 32 existing symbol-chain tests. Their actual
complete native runs retain all 8,303 EXACT cases, the one failure/timeout and
every non-timing payload field across all 14,960 rows against step111/133.
The unfiltered 552,533 type rows and 12,238 diagnostic cases are byte-identical
to the qualified owner variant. Borrowed preparation also passes 3,287 workspace
tests, zero failures, 19 existing ignores, strict workspace all-targets Clippy,
formatting and work-trace compilation. These are preservation results, not
evidence that the remaining WRONG cases are correct.

The separate anchor check exits 1 with exactly one inherited unresolved filename
in unchanged `full_oracle.rs`: its downstream native producer exists in TSR but
the checker interprets the filename as an upstream file. An independently built
frozen-main tool reproduces all 4,797 checks and the same failure. Sections pass
with zero dangling citations. `tsr-1yb.11.4.1` tracks the downstream reference;
the native pin is unchanged. This checkpoint does not claim a green anchor gate.

Two instrumented repetitions per worker mode preserve the ordinary complete
output and 402 checked files. Direct preparation reduces primary snapshot
entries from 1,758,304 to 516,704 and copied name payload from 26,759,920 to
7,993,920 bytes. Borrowed preparation removes the remaining snapshot name text
copies, while retaining those 516,704 selected alias handles. No direct hit
occurs in this fixture. The original absent/initialized-empty distinctions and
alias resolutions remain. Result-name copies, allocator capacity, worker
executions and completed cache hits are outside these snapshot byte counts.
[Direct probe](checker-owner-direct-preparation-probe.patch) and
[borrowed probe](checker-owner-borrowed-preparation-probe.patch) reproduce the
instrumentation. All four added patches replay exactly across 1,056 inputs and
pass reverse apply checks; they are private experiment sources.

Each ordinary batch has five measured fresh children per tool after warmup,
with matching effective options and loaded inputs. Builds, probes and profilers
are outside timing. CPU/RSS, complete outputs, binary/source hashes and every
sample are in the evidence. Rows in this table are separate batches:

| Mode and batch | Frozen main | Local Arc | Direct preparation | Borrowed preparation | Native |
| --- | ---: | ---: | ---: | ---: | ---: |
| Four workers, direct batch | 0.607222 s | 0.746497 s | 0.673110 s | — | 0.319402 s |
| Single, direct batch | 1.235464 s | 1.434398 s | 1.376946 s | — | 0.508454 s |
| Four workers, borrowed batch | 0.604817 s | — | 0.672601 s | 0.645021 s | 0.313315 s |
| Single, borrowed batch | 1.253729 s | — | 1.389183 s | 1.327131 s | 0.512727 s |

Direct preparation improves 9.83% against local Arc in its default batch.
Borrowing improves a further 4.10% against direct preparation in its own default
batch. Borrowed/main ratios remain **1.066474 default** and **1.058547 single**;
main integration remains refused. Do not combine medians from different batches.
The native observed default ratio is 2.058696, but equivalent complete performed
work remains unverified, so the verified native ratio remains null.

The next native boundary is `tsr-1yb.11.5.1`: `getAccessibleSymbolChainEx`
(`symbolaccessibility.go:441`) caches chains, including completed nil, on the
actual symbol owner with first relevant scope, meaning and external-aliasing
mode. This differs from alias-table slice caching. Current Rust routes do not
publish that matched completion; supported repeated keys and actual saved
workers remain unmeasured. Qualify them and the active/private/raw/resolved
publication boundary before reuse. Native nil-symbol/property-method refusals
precede the cache; a Rust unsupported/active decline is not a completed nil hit.
Eager semantic member rendering remains
`tsr-1yb.16.3.10`; faster preparation does not finish that task.
