# Native concrete-member completion and re-entry

Pinned tsgo resolves structured members per concrete type and reuses a published
view. A published view can still be provisional or change later. TSR must preserve
those states before retaining a complete-member image or borrowing its projection.
This audit delivers that boundary; it retains no production optimization and does
not establish the full-project TSR/tsgo median wall target of <=0.50.

Native source is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`; the uninstrumented
TSR release binary is from `e1ec0a65cea397a581d8267f72e53f097f67f351`.
[The frozen report](checker-native-member-completion.json) records binary,
archive, probe, fixture/config and library hashes, every process result, complete
diagnostics, ordered loaded identities, and native actual checked-source traces.

## Observed work and output

All 22 public fixtures were attempted in five separate processes: clean native,
probe disabled, probe enabled, enabled again, and normal TSR. Native complete
output and loaded order agree in every fixture. Repeated native observations have
the same semantic counter/view shapes without requiring stable allocation IDs.
Each native checked-source trace contains the fixture's expected source files.
Per-checker file dumps are cumulative; only the last dump per checker is totaled.
During delivery, main advanced to `5976b8d6` with concurrent checker/flow changes.
Those changes are preserved; these observations remain frozen at `e1ec0a65` and
do not prove that every reported TSR gap still exists on the newer main.

| Control | Direct native observation |
| --- | --- |
| One versus 32 reads of `MC_Box<number>.value` | The concrete reference has one reference/object builder path in each; 32 reads include 31 resolved-flag hits. |
| Empty interface | One class/object builder path, then four resolved-flag hits with an empty final property view. |
| Different arguments and query order | Concrete references remain distinct; public positive/negative diagnostics and internal ordered-argument assertions exercise the distinction. |
| Shared-base diamond | The derived publication first contains `ownA, ownB`, then `ownA, ownB, base, left, right`. Each base has its own completion. |
| Own-member shadowing | Final order is `shared, own, base`; the inherited `shared` does not replace the own declaration. |
| Same-spelled namespace interfaces | Separate concrete types retain `left` versus `right` member views. |
| Function/namespace | Member publication records zero call signatures; the finished anonymous worker's view has a call signature. Setter publication alone is not an all-fields completion boundary. |

Class/reference counters are wrappers around the object worker. Adding both
counts would double-count one builder path. A resolved-flag hit shows the native
guard avoided that dispatch; it does not measure TSR forcing cost or saved wall.
The raw counter name `final_flag_hits` means only that `MembersResolved` was set
outside the tracked worker; it does not promise that no later reset can occur.
These controls add copies/JSON and their process timings are observer evidence,
not a performance comparison. No public control naturally observed an active
read, an active miss, or a reset of an already resolved member view.

TSR matches complete native diagnostics in 17 of 22 controls:

| Remaining difference | Issue |
| --- | --- |
| Generic assignment omits the native relation detail | `tsr-6.55` |
| Circular mapped alias silently succeeds instead of TS2456/TS2313 | `tsr-6.64` |
| Circular generic default aborts with stack overflow instead of TS2716 | `tsr-6.63` |
| Importing file cannot see module-augmentation members | `tsr-6.49` |
| Canonical mapped-alias missing-property case gives TS2322/expanded names instead of TS2741/aliases | `tsr-6.50` |

The recursive cross-file generic control matches, consistent with closed
`tsr-6.48`. A separate `MC_Copy` mapped-alias control matches; it does not close
the failing canonical `Keys` control. The crash was saved before the runner
stopped; the resumed run skipped completed measurements and attempted the missing
cases. The two final alias/shadow controls were then added without repeating the
original cases. The report contains all 110 children, including the failed one.

## State boundary and source anchors

Anchors below refer to the original pinned `internal/checker/checker.go`:

- `getPropertiesOfObjectType` (18854) obtains the type's structured property view;
  `resolveStructuredTypeMembers` (19062) dispatches only when `MembersResolved`
  is absent. It returns the existing structured data even during publication.
- `resolveTypeReferenceMembers` (19095) uses the target's ordered parameters and
  concrete arguments, appending the reference itself as receiver `this` when
  necessary. A declaration symbol or a property-name list cannot identify it.
- `resolveObjectTypeMembers` (19106) publishes own members before setting
  `UnresolvedMembers`, adds inherited properties/signatures/indexes, clears the
  unresolved flag, and publishes again. The first publication can have
  `MembersResolved=true` and `UnresolvedMembers=false` while the worker is active.
- `getBaseTypes` (19170) can clear `MembersResolved` after base resolution to
  discard partial results from default-type circularity. `cloneTypeReference`
  (25133) also clears the bit while preserving target and arguments.
- Anonymous (20650) and mapped (20894) builders have their own early-publication
  rules. Anonymous function/namespace signatures can change after the setter.
  Reverse-mapped publication is in `inference.go` (1099); it has no equivalent
  early-empty sequence proved by this suite.
- `getNamedMembers` (22049) keeps own declaration members ahead of inherited
  members, sorting within each group. `setStructuredTypeMembers` (25145)
  installs symbol table, ordered properties, signatures and indexes together.
- `getLiteralTypeFromProperties` (26717) constructs its cache key before forcing
  properties and includes `include`, `includeOrigin` and `UnresolvedMembers`.
  Alias/origin presentation and this projection key are separate from name reuse.

Three focused **synthetic native internal API-state tests** pass. They explicitly
inject published/active states or pre-resolved declaration tables; they do not
prove those states occur naturally in the public workloads. Actual native API
calls assert readable active empty/own views versus completed empty, symbol and
signature/index identity, the real `getBaseTypes` reset followed by one rebuild,
ordered reference arguments with appended receiver, clone rebuilding, and equal
numeric TypeIds in distinct checker domains. A previously returned structured
pointer observes later publication: native structured views are mutable backing
objects, not immutable snapshots.

The first package test attempt required uncached `gotest.tools`. No dependency was
added. The successful focused command compiles the exact native production `.go`
file list plus our internal test, excluding existing external tests. A map-key
compile correction preceded the passing run. This is not a full native test run.

## Implementation handoff

`tsr-1yb.4.2.1` can investigate one measured class/interface/reference builder
family using these native states. Its key must remain concrete and checker-local,
with ordered arguments and receiver identity. Preserve readable provisional
publication, failed/recoverable resolution and later resets; never permanently
memoize an empty or name-only answer as a completed image. An immutable image or
borrowed projection needs an explicit lifetime/publication policy compatible with
native mutation. All relevant fields and symbol identity must agree, including
signatures and indexes. Setter calls or a traversal-completeness boolean are not
sufficient proof of final completion.

This handoff does not validate mapped/anonymous/reverse-mapped/composite rollout,
all contextual mapper identities, augmentation, or cached failure recovery in
TSR. Their builder-specific and fidelity gaps remain unsupported for broad reuse.
Existing mapper, receiver, symbol-storage, private-checker ownership and worker
memory prerequisites remain in force. `tsr-1yb.4.2.2` retains the actual work,
storage, full-corpus preservation and two independent whole-CLI timing gates.
The name-projection API decision remains `tsr-1yb.7.5.1`.

TSR actual checked scope was not instrumented here. Fixture/config/binaries were
snapshotted before/after each child; 108 shared library files match the pinned
archive after the run. This is not complete cross-tool filesystem-query coverage
or a comparable full-project ratio. The audit makes no CPU ceiling, RSS-saving,
allocation-count, native-speed or >=2x claim.

## Replay and restoration

The [probe patch](checker-native-member-completion-probe.patch) includes native
instrumentation and the three internal tests; the
[fixtures](checker-native-member-completion-fixtures.py) contain all public input.
Apply the zero-context patch with `git apply --unidiff-zero` to a separate archive
of the exact native commit. Build clean and
patched `./cmd/tsgo` with Go 1.26.0, `-p 1 -buildvcs=false`, outside measurement.
Build the selected uninstrumented TSR source separately. Never edit shared vendor.

```sh
rtk proxy python3 docs/architecture/checker-native-member-completion-controls.py \
  --out /tmp/member-controls-replay \
  --clean /absolute/path/to/clean-tsgo \
  --probe /absolute/path/to/probe-tsgo \
  --tsr /absolute/path/to/uninstrumented-tsr \
  --tsr-source e1ec0a65cea397a581d8267f72e53f097f67f351
rtk proxy python3 docs/architecture/checker-native-member-completion-verify.py
```

The driver strips prior TSR/TSGO probe environment, uses explicit single-threaded,
no-emit/nonincremental/noncomposite options and resumes completed checkpoints.
Abnormal TSR exits remain failures in the report. A separate five-process
`repeat-32` smoke verified the shipped portable driver. The frozen-report verifier
checks fixture/config hashes, complete native output, checked source identities,
repeated counter shapes, concrete counts/order, artifact hashes and the five gaps.
It verifies saved evidence; it does not rerun compilers or prove the speed goal.

For the internal tests, run `go test -p 1 -count=1 -run '^TestMemberCompletion' -v`
on all production checker `.go` files plus `member_completion_controls_test.go` in
the patched archive. The frozen report records the exact successful argument list.

The owned native archive was restored byte-for-byte against all 55,103 pinned
source files and the patch passes `git apply --unidiff-zero --check` on that restored source.
The shared vendor was never modified. Only replay/evidence artifacts are retained;
no native or Rust probe is installed in production.
