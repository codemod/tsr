# Resolver trace argument experiment

The experiment against source `533f50dc38872dca24acf8507eacf6a7812b311b`
was rejected. Guarding formatting and copying used only by disabled resolution
traces did not establish a repeatable whole-project improvement. Production
resolver code was restored; this evidence does not establish the required
TSR/pinned-tsgo median wall ratio of 0.50.

## Boundary and oracle

[resolver-trace-candidate.patch](resolver-trace-candidate.patch) contains the
source-exact rejected candidate. It adds guards around six trace-only regions:
module-kind display, resolution-result/package-ID display, condition formatting,
URI messages, nonrelative lookup messages and invalid import-specifier messages.
Semantic lookup, condition storage, cache warming, unresolved returns and cache
publication remain outside the new guards.

Pinned `internal/module/resolver.go` at
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` guards the corresponding trace work
with `tracer != nil`, including `resolveNodeLike`'s condition formatting.
Its source SHA-256 was verified against the pinned source manifest:
`bfd63b170178ec30977bf9c3bbf1bef574184901b12fc5eaebbbef2f41ac4ff1`.
This makes the candidate a faithful optimization boundary, but does not prove
that its cost is material.

## Measurements and decision

[resolver-trace-performance.json](resolver-trace-performance.json) records the
frozen source/binary identities, all process samples, diagnostic fingerprints,
resource observations, controls and limitations. Private project paths and
diagnostic text are omitted.

Each round used one warmup per role and five alternating fresh-process pairs.
Builds and corpus runs did not overlap the timing runs. Both binaries used the
same pinned libraries and `--noEmit --incremental false --composite false
--pretty false`. The comparison is between two TSR binaries, not TSR and tsgo.

| Round | Baseline median wall | Candidate median wall | Candidate minus baseline |
| --- | ---: | ---: | ---: |
| 1 | 3.813750 s | 3.862525 s | +48.775 ms |
| 2 | 3.831332 s | 3.808447 s | -22.885 ms |

Wall and CPU ranges overlap in both rounds. Median RSS was essentially unchanged
in round one and higher for the candidate in round two; this is not evidence of
a memory improvement. The opposite timing signs fail the requirement for two
independently confirmed gains. Selecting only round two would discard contrary
evidence.

All 24 processes produced the same complete normalized 118 diagnostics. The
root configuration hash remained unchanged. Full loaded-input and actually
checked-file identity controls were **not captured during these timing rounds**;
the root config hash and equal diagnostics are insufficient substitutes. The
samples therefore support rejecting retention, not a comparable speed claim.
No full checker type/diagnostic corpus was run for this rejected candidate.

## Enabled trace controls

The candidate passed all 49 release `tsr-module` unit tests, including query-cache
identity and enabled-trace bypass tests. Existing conformance suites passed
95/95 `module_resolution` cases and 96/96 `file_loader` cases; both resulting
snapshots were byte-identical to the measured source's committed snapshots.
Other discovered cases are outside these suites' supported trace populations.

The resolver suite checks the native step-by-step trace for requests taken from
the baseline. The loader suite separately checks the requested resolutions and
their order. The canonical sanitizer normalizes compiler version and
package-json cache messages, so this is canonical suite equality rather than
unsanitized native CLI trace equality. It does not prove full-project checked
scope, every failed-lookup field or full checker parity.

## Reproduction and next action

Build the baseline at the measured source, apply the exported patch, and build
the candidate using the same release flags/toolchain. Save each binary separately
before the next build. Use fresh processes and alternating role order through
`scripts/whole_project_perf.py`'s `process` helper; retain both rounds and exclude
warmups. The recorded driver SHA identifies the original local measurement
script; it is not a portable public workload. Any new retention decision must
also capture complete input, effective options, loaded/checked scope and full
correctness evidence before and after measurement.

`tsr-1yb.2.1.4` closes as a measured rejection. Do not reattempt trace formatting
solely because one sample improves. The next loader attribution is
`tsr-1yb.2.1.3.1`: identify origins of the previously observed unique metadata
probes and discovery work. The existing filesystem/query caches remain useful;
another duplicate cache cannot explain the unique-path floor. Dynamic loader
replay and byte-budget controls remain `tsr-1yb.19.1` and `tsr-1yb.19.2` before
production concurrency. The full 2x target remains unmet and unverified.
