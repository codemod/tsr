# Current member construction and copy costs

The next bounded algorithm experiment is ordered membership testing during
member-name collection (`tsr-1yb.20`). Consumer cloning is not the leading
copy opportunity in this app. Native concrete-member completion remains a
separate workstream (`tsr-1yb.16.2.2`/`.4.2`); these observations do not certify
completed member images or establish a production speed gain.

The [sanitized counts](checker-member-cost-counts.json),
[initial temporary patch](checker-member-cost-initial-probe.patch),
[expanded patch](checker-member-cost-probe.patch), and
[control runner](checker-member-cost-controls.py) preserve the measurement.
Neither patch belongs in production.

## Identity and controls

The source is `359a2789`; full revision and original/patched source-file hashes
are in the counts. Normal, scope-only, initial probe and expanded probe binaries
were separately frozen. Each build used ordinary release settings, one Cargo
job, and the same pinned vendor source. Builds and measured processes ran
serially; other agents' host activity was uncontrolled.

Both probe snapshots use explicit project, no emit, incremental false,
composite false, pretty false, list files and extended diagnostics. Every full
run preserves all 122 normalized diagnostics and ordered 14,015 loaded files.
Effective configurations agree across normal/scope/probe builds. Input snapshots
remain equal before/after each child, outside child timing. Historical query
paths plus currently loaded files are partial input coverage, not complete
cross-tool coverage.

The scope-only binary enables the existing checked-file trace independently of
member counters. Its two runs, disabled probe runs, enabled counter runs and
timed counter runs have exactly the same ordered 1,364 checked-file identities.
The ordinary binary has no direct checked-identity trace; identical outputs and
loaded scope do not add that missing telemetry. No full-app native timing or
verified native wall ratio is claimed.

## Work and copied payload

Non-timing counters repeat exactly in both counter modes and both snapshots.
Query frames charge walks to the immediate query, propagating guard risk to
parents without double-counting walks.

| Observed boundary | Count / payload |
|---|---:|
| Member-name queries / distinct private TypeIds | 352,603 / 13,358 |
| Successful / unsupported results | 196,727 / 155,876 |
| Identical successful repeats outside detected guards | 186,700 |
| Structured walks / walks on such repeats | 197,815 / 99,428 |
| Member table entries examined | 2,710,466 |
| Structured duplicate tests / actual String comparisons | 2,645,861 / 83,152,851 |
| Owned names copied before duplicate elimination | 2,607,302 |
| Payload copied for those owned names | 25,272,494 bytes |
| Returned name payload / payload on successful repeats | 10,216,883 / 9,949,112 bytes |
| Late-bound worker entries / cached-result reads | 5,008 / 753,368 |
| Payload copied by late-bound cached-result reads | 2,591,849 bytes |

The returned payload overlaps construction payload: returned names are moved,
not necessarily copied again. Do not add those two totals as distinct copies.
String payload/capacity counts exclude vector buffers, allocator metadata and
usable allocation sizes. They are not allocator events or total heap traffic.

The late-bound worker count measures actual entry into its source miss block;
completed versus active cache reads retain the existing sentinel semantics.
It does not count native `resolveStructuredTypeMembers` executions. Equal
successful name lists are not native-completed member tables. There are 26,093
failed-base events and three visited prunes (including possible diamonds), no
observed active query/late-bound reentries, and no changed ordered answers.
These observations cannot authorize caching guarded or unsupported answers.
The legacy alias-force field was not instrumented and is excluded from the
exported query totals.

The expanded probe adds the two existing `intersection_names.clone()` consumers
in `properties_related_to_with_optionals`. The object-target consumer executes
235 clones; the missing-property consumer executes 31,598. Both copy **zero**
string payload in the app. Their `Option` inputs do not contain name vectors on
those requests. Optimizing request count alone would target negligible copy work.
The public intersection control exercises a nonempty consuming copy: four
strings/four payload bytes, proving this boundary is observed when present.

The observer retains previous answers solely to compare ordered results:
267,590 bytes of string capacity and 778,200 bytes of vector-element capacity
at report time. This excludes its hash-table storage and temporary/reporting
allocations. No observer data serves semantic queries.

## Timing limits and next experiment

Two initial narrow-site observations measure structured duplicate tests at
59.342/59.547 ms and owned-name copying at 69.224/68.961 ms. Late-bound cached
result copies take 17.068/16.754 ms; base queries take 18.840/18.049 ms. Timers
include clock cost. Base queries may contain other measured work; do not sum
their intervals with contained sites. Uninstrumented normal savings remain
unknown. Constant synthetic names, mapped literal-key construction and other
copy boundaries are outside these selected sites.

| Mode | Initial two-run median | Expanded two-run median |
|---|---:|---:|
| Normal | 4.009 s | 3.949 s |
| Scope only | 4.059 s | 4.060 s |
| Disabled probe | 4.016 s | 4.153 s |
| Count enabled | 4.593 s | 4.605 s |
| Count and site clocks | 4.674 s | 4.835 s |

These observations quantify material observer overhead and ordinary variation;
two samples are not a confirmed throughput comparison. Raw per-child CPU/RSS
is exported. Observer copies, table bookkeeping and reporting belong to the
probe, not a production candidate.

The actionable new evidence is 83 million membership comparisons. Task `.20`
can test a query-local index while preserving encounter order, original guard
behavior and `Option<Vec<String>>` ownership. It must measure data-structure
overhead, collision/equality correctness, the public wide/duplicate-heavy cases
and normal whole-CLI benefit before retention. The previous temporary-vector
experiment `.14` was rejected; removing that vector alone is not this handoff.
Concrete image reuse and immutable projection APIs still need the
[native member contract](checker-member-cache-contract.md), actual native
completion/re-entry controls and concrete receiver/mapper context.

## Public fidelity and replay

The expanded runner executes eleven public fixtures as ten TSR full checks and
one pinned-native check per fixture, plus configuration/listing controls.
Every TSR variant preserves complete diagnostics and loaded scope; every scope
variant preserves the single checked root, and non-timing counters repeat.
One versus 32 repeated assignments grows name queries/walks from 3 to 96,
without pretending those walks are native cache misses.

Eight fixtures match native complete diagnostics, including positive member,
diamond, receiver, mapped and computed cases and deliberate cycle/failed-base
errors. Three fidelity controls fail and remain explicit:

- Generic argument rejection omits the native property/type diagnostic chain;
  this extends `tsr-6.55`.
- Namespace target naming differs (`Left.Shape` versus `Shape`).
- An intersection missing a required property emits TSR TS2322 instead of
  native TS2741. The latter two are tracked in `tsr-6.62`.

Both patches reverse cleanly, restore all four original source-file hashes,
and replay against their exact pinned source. Main compiler and native source
were untouched. Use the separately frozen binaries with the runner's required
`--normal`, `--scope`, `--probe`, and new `--output` arguments. Add `--tsgo` for
public controls; or `--project` plus `--input-manifest` for a real project.
Native source is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`, and its clean
binary hash is recorded. Setup/builds stay outside observations; raw private
paths/diagnostics remain local.

The bounded cost decomposition supplies `.16.2.1`'s implementation handoff.
Broader allocation/completion audits and `.2.2`/`.16.2` remain open. No new best,
semantic cache, corpus coverage change, or verified wall ratio of 0.50 is claimed.
