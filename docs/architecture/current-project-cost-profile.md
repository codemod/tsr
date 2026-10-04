# Current whole-project cost observations

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
