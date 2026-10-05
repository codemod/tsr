# Current whole-project cost observations

The refresh for `tsr-1yb.2.2.1` freezes current main
`d6acb2c4e186e891010784672d063587907a8356`, pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, and an ordinary release build.
Two normal refresh checks took 6.080 and 6.014 seconds. Checking takes
3.321–3.353 seconds and program construction 2.241–2.392 seconds in the
separate phase observations. These locate current work; they establish no
optimization gain or comparable TSR/native ratio.

After these observations, main received the native CLI checker-pool port
`e495d66c`, embedded libraries, lazy diagnostic-file indexing and concurrent
root preparation through `33c8e121`. The full pulled range changes runtime
behavior even though its latest commit is documentation. These measurements
remain qualified to the earlier `d6acb2c4` snapshot. Revalidate default and
single-worker phases, admitted checking and inputs on integrated main before
using this ranking to select a production change. The `.16.3` handoff records
that requirement; broader `.2.2`/`.11` stay open.

The [sanitized JSON](current-project-cost-profile.json) keeps the original c5
fields intact and adds this evidence under `current_refresh`. The rebuilt binary
SHA is `1e079f2cc610d27302b504ddb94c808cd59ca3f438e8764ee1443f0cba470aed`.
Release uses opt-level 3, LTO disabled, 16 codegen units and panic abort,
without the `work-trace` feature. An APFS clone reused build artifacts; the
frozen current source was rebuilt before any observation. Builds and observed
children run serially. Other agents' host activity is uncontrolled.

## Current phase accounting

| Disjoint top-level slice | First observation | Second observation |
|---|---:|---:|
| Configuration | 0.036 s | 0.052 s |
| Program construction | 2.392 s | 2.241 s |
| Checker initialization/registration | 0.013 s | 0.014 s |
| Checking | 3.321 s | 3.353 s |
| Reporting | 0.128 s | 0.111 s |
| Compilation total | 5.890 s | 5.770 s |
| Whole child wall | 6.162 s | 6.010 s |

The named compiler slices agree with compilation within the printed 1 ms
rounding precision. Whole child wall leaves 0.272/0.240 seconds outside that
timer. Startup, teardown and output outside the timer are not separately
attributed here. Checker initialization includes eligibility setup and JSDoc
registration; it is not just constructor execution.

Loader time is nested in program construction: 2.085/1.939 seconds, including
read 0.242/0.225, metadata 0.104/0.092, parse 0.773/0.750 and discovery
0.930/0.837 seconds. These loader slices leave 0.036/0.035 seconds. Resolver
0.805/0.716 seconds is nested in discovery. Do not add these nested timers to
the top-level rows.

## Current disjoint samples

The existing driver samples only its own launched, unreaped child PID.
Two early intervals request three seconds after a 0.2-second delay; two later
intervals request three seconds after a 2.5-second delay. Each main-thread
sample is assigned once to its nearest TSR frame. Inclusive ancestry and
allocator-library leaves are separate views, never additive costs.

| Nearest TSR owner, early interval | First | Second |
|---|---:|---:|
| Resolver `try_file_lookup` | 238 | 343 |
| Loader `load_task` | 184 | 252 |
| Resolver `get_package_json_info` | 92 | 102 |

These are the same top three in both early intervals. Each is already owned by
loader/resolver tasks `.2.1.3` and `.19`; `stat`/`open` samples do not establish
that another filesystem cache is useful or that repeated probes are present.

| Nearest TSR owner, later interval | First | Second |
|---|---:|---:|
| Member `collect_structured_property_names` | 179 | 161 |
| Inference `mentions_type_parameter_inner` | 121 | 113 |
| Binder `resolve_name` | 94 | 87 |
| Declared `get_instantiated_type_reference` | 59 | 74 |
| Inference `instantiate_type` | 77 | 69 |

The same first three lead both later intervals. Those intervals contain
2,543/2,539 main-thread samples, with 2,541/2,400 having checker ancestry.
Early intervals contain 2,499/2,543 samples with only 1,070/660 checker ancestry:
the early distribution is inadequate for ranking whole-checker work alone.
Nearest-owner reporting keeps the top 40; residual samples outside those rows
are explicit in the JSON. Deduplicated/unsymbolized leaves are retained.
Waiting, omitted threads, unsampled startup/end work and the selected interval
limit interpretation. These counts are not CPU percentages, allocation events,
wall shares or a saved-wall ceiling.

Allocator-library leaves in the later intervals are most often assigned to
member-name collection (70/77) and instantiated reference preparation (49/55).
General `instantiate_type` has 34/35. These locate origin probes without
establishing their physical allocation counts or whether semantic work repeats.

## Next operation and ownership

Member projections remain the highest named late owner. Existing `.16.2`,
`.7.5.1` and `.4.2` own forcing/copy evidence, consuming ownership and native
completed images. The rejected ordered-membership `.20` and temporary-vector
`.14` experiments are not revived by these samples. Concrete receiver, alias,
completion and publication gates remain necessary.

The now-unblocked `.16.3` allocation-origin task should distinguish three
boundaries, consuming existing receipts where available:

- `declared.rs::get_instantiated_type_reference` (line 4006 at the frozen
  source): argument resolution/default filling and written spelling preparation
  versus expensive reference construction. The function prepares argument
  renderings before knowing whether a composed written spelling is needed.
  Measure discarded versus retained rendering/copy work and any lazy forcing
  caused by rendering before considering a change. Written NodeId state,
  ordered type arguments, symbol identity and alias context must survive.
- `inference.rs::instantiate_type` (line 5132): mapping and generic-predicate
  checks versus `instantiate_type_worker`. Native
  `instantiateTypeWithAlias`/`instantiateTypeWorker` keep mapper identity,
  alias identity and active mapper cache lifetime distinct; pop/reset and
  inference-changing answers cannot become persistent vector-key reuse.
- `members.rs::collect_structured_property_names` (line 3015): coordinate the
  existing projection receipts and concrete-member consumer contract. Preserve
  encounter order, cycles, late-bound members, failed bases and receiver state.

The instantiation and graph costs also retain `.16.1`, `.4.1.2.2` and `.7.3`
owners; scope-query work retains `.7.7`. Exact allocation/copy origins and
executed-worker counts are the next cheapest evidence. A benefit ceiling is
still unknown. Broad `.2.2` and `.11` remain open.

## Output, scope and capture qualification

All ten qualified full checks (two baseline, six early-driver full checks,
two later samples) preserve 117 complete normalized diagnostics. All six
list-files full checks preserve the same ordered 14,050 loaded identities and
report 1,397 checked and 14,746 parsed files. Reported check counts are not a
fresh direct checked-identity trace. The 14,112-path observation snapshot is
stable across the six early full checks and both later samples; it contains
historical loaded paths plus newly listed sources and root configuration.
Complete cross-tool query coverage and required lazy-work equivalence remain
unverified. No native timing is collected.

The clean baseline's normal checks took 6.288/6.336 seconds; the subsequent
normal refresh checks took 6.080/6.014. This variation cannot be interpreted as
a runtime change: all use the same frozen binary and option flags. Profiled
children took 6.478/6.473 early and 6.182/6.066 later seconds. Sampling impact
and host variation are not independently estimated by these observations.

An initial baseline overlapped an advisory repository scan; it is excluded and
retained locally. The scan was stopped through its owned live handle before
repeating the baseline. The narrower advisory inspected only the script and
identity files. An initial later child completed but used different logical
binary/library spellings; its cross-run input fingerprint was rejected and its
raw output preserved. The qualified later pair normalizes those spellings to
the existing driver's paths. The JSON includes hashes for excluded receipts
and qualified raw evidence, without publishing private application source or
diagnostic payloads.

## Reproduce the current intervals

Build a clean immutable ordinary release snapshot before measuring. Supply a
schema-version-1 input manifest accepted by `benchmark_inputs.load_manifest`.
Its paths remain partial observed inputs, not a complete query-coverage proof.
Use a fresh output directory for each protocol:

```sh
python3 docs/architecture/current-project-cost-controls.py \
  --binary /absolute/path/to/frozen-tsr \
  --project /absolute/path/to/tsconfig.json \
  --source-root /absolute/path/to/pinned-checkout \
  --input-manifest /absolute/path/to/observed-inputs.json \
  --delay 0.2 --duration 3 --output /tmp/new-early-observations

python3 docs/architecture/current-project-cost-controls.py \
  --binary /absolute/path/to/frozen-tsr \
  --project /absolute/path/to/tsconfig.json \
  --source-root /absolute/path/to/pinned-checkout \
  --input-manifest /absolute/path/to/observed-inputs.json \
  --delay 2.5 --duration 3 --output /tmp/new-late-observations
```

Both commands use the public driver's normal/phase/sample protocol. This run's
later pair called its same `profiled_process`/`parse_sample` helpers directly,
with source, binary, options, inputs and full output checks; the local wrapper
hash is recorded separately. The earlier c5 reproduction text below referred
to schema 2; the existing loader accepts schema version 1 as shown here.

# Historical c5ddf73e observations

Two normal release checks at `c5ddf73edffdd28ef15d827da338703f964bc08c`
took 4.025 and 4.037 seconds. Separate phase observations put checking at
2.233–2.246 seconds and program construction at 1.546–1.642 seconds. These
observations locate work for subsequent experiments; they establish no speed
gain or comparable TSR/tsgo ratio.

The [sanitized observations](current-project-cost-profile.json) pin the compiled
binary, native source, harness versions, observed inputs and effective options.
The binary used ordinary release defaults: opt-level 3, LTO disabled, 16 codegen
units, panic abort, built with rustc 1.96.0. Builds and observed children ran
serially. Other agents' host activity was uncontrolled.

## Phase accounting

| Measured slice | First observation | Second observation |
|---|---:|---:|
| Program construction | 1.546 s | 1.642 s |
| Checker initialization | 0.001 s | 0.001 s |
| Checking | 2.246 s | 2.233 s |
| Reporting | 0.110 s | 0.104 s |
| Compilation total | 3.912 s | 3.990 s |
| Whole child wall | 4.098 s | 4.158 s |

Compilation totals leave 0.009 and 0.010 seconds outside the named top-level
slices. Another 0.186 and 0.168 seconds separate compilation telemetry from
child wall; this includes work outside that compiler timer and is not attributed
to a specific teardown operation.

Loader time was 1.300/1.386 seconds, including read 0.178/0.179, metadata
0.034/0.041, parse 0.637/0.655 and discovery 0.423/0.483 seconds. These named
loader slices leave 0.028 seconds in each observation. Resolver time
0.326/0.383 seconds is nested in discovery. Neither resolver nor loader should
be added to the top-level program time. Indexing, binding and configuration
also belong to program construction.

## Disjoint stack observations

The driver sampled its own unreaped child PID after a 0.2-second delay, for a
requested three-second interval. It assigned each main-thread sample to its
nearest TSR stack frame once. Inclusive parent stacks were not summed.

| Nearest owner | First interval | Second interval |
|---|---:|---:|
| `collect_structured_property_names` | 125 | 55 |
| `mentions_type_parameter_inner` | 88 | 71 |
| `resolve_name_excluding` | 83 | 86 |
| `get_instantiated_type_reference` | 41 | 24 |

The intervals contain 2,319/2,328 main-thread samples; 1,823/1,479 have checker
ancestry. The different interval distributions include varying loader exposure.
Allocation-library leaves beneath member-name collection account for 63/24
samples, and instantiated references for 33/19. These are stack observations,
not allocation events, copied bytes, CPU percentages or a saved-wall ceiling.
Waiting, unsampled startup/end work and omitted threads limit interpretation.
Profiled child wall was 4.268/4.421 seconds; observer impact and ordinary run
variation are not separately estimated from two observations.

## Scope and remaining evidence

All six full checks have the same 122 normalized diagnostics. All four
list-files checks have the same ordered 14,015 loaded identities and report
1,364 checked files and 14,617 parsed files. Direct checked-file identities
were not captured. The 51,020-path input snapshot is stable before/after each
full child, but combines historical query paths with currently loaded files;
complete cross-tool query coverage remains unverified. No native timing was
performed here.

Member completion/projection is the next bounded checker candidate: both
intervals locate work there, and its ownership boundary affects subsequent
ports. Selection for production still needs expensive-worker counts, copy
costs and native-positive controls. Use the concrete checker-local type,
receiver and mapper context, following pinned
`resolveStructuredTypeMembers`/`resolveTypeReferenceMembers`/
`setStructuredTypeMembers` and the
[member contract](checker-member-cache-contract.md). Declaration names alone
do not establish reusable completed results.

Beads `tsr-1yb.16.2.1` owns current forcing/copy attribution;
`tsr-1yb.16.2.2` owns native completion/re-entry controls. After both,
`tsr-1yb.7.5.1` owns the measured consuming API decision. Existing `.4.2` and
`.7.5` retain production implementation and fidelity/timing gates. The earlier
temporary-vector experiment `.14` was rejected and should not be repeated
without new cost evidence. Resolver and scope-query work retain their existing
owners. Broad audits `.2`, `.11` and the unresolved evidence in `.2.2` stay open.

## Reproduction

Build an immutable ordinary release binary from the recorded clean source
outside measurement. Supply a schema-2 observed-input manifest to the
[opt-in driver](current-project-cost-controls.py):

```sh
python3 docs/architecture/current-project-cost-controls.py \
  --binary /absolute/path/to/frozen-tsr \
  --project /absolute/path/to/tsconfig.json \
  --source-root /absolute/path/to/pinned-checkout \
  --input-manifest /absolute/path/to/observed-inputs.json \
  --output /tmp/new-cost-observations
```

macOS `/usr/bin/sample` must be available and permitted to attach to the owned
child. Raw outputs remain local. `--resume` checks source, binary, shared
harness, options and input identity before reusing saved observations; it
records each driver version. The initial JSON readback failure was a tuple/list
serializer mismatch, repaired before resuming the saved measurements. The
delivered driver additionally removes an unused import; its separate hash is
recorded, without claiming those earlier runs used the delivered bytes.

The [parser controls](../../scripts/test_current_project_cost_controls.py)
check nested exclusive accounting, omitted worker threads, JSON readback,
unowned waiting samples and rejection of child overcounts. Existing benchmark
input controls remain responsible for snapshot correctness.
