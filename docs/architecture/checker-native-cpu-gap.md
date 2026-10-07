# TSR / pinned tsgo CPU gap

Diagnosis for `tsr-1yb.11.2`, measured on 2026-10-06. The important native
advantage is completed, lazy semantic state: a lookup can return a retained
answer without reconstructing its preparation, member image, signature vector
or display text. The useful optimization boundary depends on the workload.

This report changes no compiler code. The [receipt](checker-native-cpu-gap.json)
contains every final timed child, warmups, CPU/RSS summaries, source and binary
identities, sampled stacks, and explicit limitations. Raw local artifacts are
in `/tmp/tsr-native-gap-profile`.

## Source and measurement controls

- TSR checkout: `8817b95b1e36efda18d1124977b45b707af4c36f`. Its compiler sources,
  Cargo manifest and lockfile are byte-equivalent to
  `5dd3bad84d12991e1ba169d2d5687321e1989740`.
- Pinned native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`, independently built
  clean binary, rather than the installed native-preview package.
- Frozen TSR binary SHA256:
  `b25267b075ace8b87c8e3ded256261bc59773692adecff56c3cd4aaf32e5f3a6`.
- Frozen native binary SHA256:
  `6340b230f887095fb7c82fedb41eecadd7dc49e4baa65a05ef4abb8ab4b74bc4`.
- Every compiler invocation is a fresh process with `--noEmit --incremental
  false --composite false --pretty false`; both receive the same positive
  `--checkers` override. `--checkers 1` leaves frontend parallelism enabled.
- The final timing runner interleaves both compilers and both checker counts in
  each cohort, alternates ordering, and excludes one full warmup cohort. It uses
  the existing `whole_project_perf.py` wait4 CPU/RSS and input-snapshot controls.
  Input snapshots run outside the child interval. All measured children complete
  with stable observed inputs and the expected complete normalized diagnostics,
  exit status and stderr.
- Builds and profiling are outside the final timing cohorts. Concurrent external
  builds affected this host: observed load averages reached roughly 198. Earlier
  non-interleaved batches remain in the archive and are not combined with the
  final cohorts. These are descriptive measurements, not release qualification.
- Warmed copies start promptly: TSR `--version` takes 4.9 ms and native 8.5 ms.
  This does not characterize a new executable's first macOS security scan; see
  the existing fresh-launch investigation.

Observed source/config/binary snapshots do not cover every resolver query,
environment input or bundled-library byte. Equal diagnostics and file names do
not establish equal performed lazy semantic work. The verified complete-work
wall ratio remains unset, and the release target remains unmet.

## Ordinary end-to-end timings

The generated project has 400 modules plus `core.ts` and `main.ts`, and one
intentional TS2322 diagnostic. Both tools load 465 files including libraries.
Five measured cohorts follow one warmup cohort.

The real API project loads 9,861 files in each tool. Three measured cohorts
follow one warmup cohort. TSR still reports four TS1005 errors on `function:`
parameters in ioredis declarations; native reports none. Effective-config
serialization also differs. Its timing is therefore not a fidelity or
equivalent-complete-work acceptance result.

| Workload | Checkers | tsgo median wall | TSR median wall | TSR / tsgo |
| --- | ---: | ---: | ---: | ---: |
| Generated 400 modules | 1 | 0.538 s | 3.519 s | 6.54 |
| Generated 400 modules | 4 | 0.370 s | 1.809 s | 4.89 |
| Real API | 1 | 3.906 s | 13.501 s | 3.46 |
| Real API | 4 | 3.557 s | 9.760 s | 2.74 |

For the generated project, median total CPU is 3.578 s versus 0.829 s with one
checker, and 3.371 s versus 1.282 s with four. There is substantial extra CPU
work even without checker scheduling. The four-checker wall ranges are
1.778–2.452 s for TSR and 0.345–0.501 s for native; the host was settling during
that batch. Do not turn the median into an exact universal multiplier.

On the API project, median total CPU with four checkers is 21.324 s versus
15.983 s. The CPU ratio is smaller than the wall ratio. Worker tails, frontend
parallelism, Go runtime/GC activity and scheduling can therefore matter alongside
extra checker work. Whole-process CPU divided by wall time is not a count of
active checker workers.

## Running-thread CPU samples

TSR was recorded separately with Instruments Time Profiler. Its exported schema
has `record-waiting-threads=0`, `all-thread-states=NO`, and a configured 1 ms
sampling period. Both targets exit with the expected diagnostics and status;
neither recording ends at its time limit. The reader resolves XML reference
IDs, retains running-state weights, counts a method family at most once per
stack, and preserves per-thread weights and first/last observed CPU samples.

The generated recording has 11,412 running samples; the API recording has
37,512. Their weighted CPU estimates are **observer-on measurements**, not
ordinary-process CPU durations. They must not be inserted into the wall table.

| TSR sampled path | Generated | API |
| --- | ---: | ---: |
| Allocation/free routine at the sampled leaf | 26.7% | 39.9% |
| `get_effects_signature` | 17.5% | 0.18% |
| `type_to_string_at_worker` | 16.4% | 2.0% |
| Alias-resolution family | 14.6% | 3.8% |
| `instantiate_for_reference_with_this` | 7.8% | 18.9% |
| Type-instantiation family | 7.7% | 22.7% |
| `mentions_type_parameter_inner` | 3.4% | 6.6% |
| Structured member-name collection | 2.2% | 2.0% |

Except the physical allocation/free leaf bucket, these are inclusive stack
shares. They overlap and cannot be added, interpreted as cache misses, or
converted to saved wall time. Optimized/inlined or truncated stacks can hide
work. Running-state syscall frames alone do not measure blocked duration. The
allocation bucket is an attribution heuristic, not an allocation counter.

Native's separate built-in `--pprofDir` runs comprise five generated four-checker
children, three generated one-checker children, and one API four-checker child.
The CPU profile period is 10 ms. Profile banners are retained in raw output;
profiling stop/write overhead never enters ordinary timing. The accompanying
`memprofile` is `pprof.Lookup("allocs")`, not RSS or a live-heap measurement.
Sampled allocation data can lag publication and includes profiling/runtime work.

In native's generated four-checker aggregate, `getEffectsSignature` occupies
1.62% of sampled CPU, `resolveAlias` 1.18%, and
`couldContainTypeVariablesWorker` 0.19%. No `typeToString` frame is sampled.
On API, `instantiateTypeWithAlias` remains substantial at 19.19%; native still
performs generic work. These profiles also spend about 27–28% in
`runtime.madvise` under host contention, plus GC work. Their denominators and
sampling conditions differ from Instruments, so cross-tool percentages are
locating evidence, not normalized operation-cost ratios. No sampled frame is
not proof of zero execution.

## The missing native boundaries

### 1. Per-call effects completion, including a completed negative

Pinned `internal/checker/flow.go:2047–2095` reads
`signatureLinks[node].effectsSignature`. It computes the callee/signature only
for an uncomputed slot, then stores either the effects signature or
`unknownSignature`; a completed unknown answers nil on subsequent requests.

TSR `flow.rs:2721` explicitly recomputes this path, including callee typing and
materialized signature lists. The existing pure-function negative shortcut is
useful but does not replace per-call completion. Both implementations already
have reference-local shared-flow memoization: their outer reference query marks
and then truncates its shared-flow region. A different reference can revisit a
call after that region is discarded. That is a different reuse boundary from a
completed effects answer owned by the call node.

The generated long function makes this path important; the API profile does
not. `tsr-1yb.11.3` owns actual request/worker/publication counts, native query-order
controls and a bounded port. Deferred or active TSR work must not be promoted to
a completed negative just because an Option currently returns None. Existing
`resolved_call_signatures` is a separate cache and already exists.

### 2. Semantic construction without eager display work

Pinned `checker.go:13144–13355` checks object-literal properties and constructs
an anonymous semantic member table; `newAnonymousType` at line 25090 publishes
structured members. It does not run the type node builder to render every
member during ordinary checking.

TSR `objects.rs:2025` calls `member_text_at`, which can enter
`type_to_string_at_worker`; line 2154 renders the complete object text. In the
generated recording, essentially all sampled callers of that rendering worker
are under `check_object_literal_members`. One diagnostic therefore does not
bound the amount of display work performed by the checker.

`tsr-1yb.16.3.10` owns this specific object-literal consumer contract. It is
distinct from the already-characterized generic-reference spelling boundary and
the existing empty-name fallback candidate. Baked text has semantic consumers
and rendering can force types or signatures, so deleting strings or substituting
empty text is not a faithful optimization. The handoff must preserve presentation
site, alias/origin, written-node reuse, semantic slots and pending-return order.

### 3. Retained instantiated member images and lazy value slots

Pinned `checker.go:19106` resolves declared members once, creates instantiated
symbols using a mapper when needed, incorporates instantiated base types, and
publishes structured members. `getTypeOfInstantiatedSymbol` at line 16528 and its
write-type counterpart fill separate private symbol-link slots lazily.

TSR `members.rs:1944` still applies receiver substitution at the consumer. Member
lookups, signature lists and structural walks also produce temporary copies.
The API's receiver and instantiation stack shares make this the strongest
real-project semantic-state lead. Reusing only argument/name preparation did
not deliver a retained four-checker gain in the previous experiment; that result
does not price the whole member-resolution boundary.

Use the existing structured-member and concrete-receiver contracts and owners
`tsr-1yb.33.1` / `tsr-1yb.4.2.1`. Raw/reduced/apparent views, concrete `this`,
read/write slots, static/instance side, alias frames and active/error/completed
publication remain separate. The CPU shares do not qualify a broader cache key.

### 4. Alias targets retained independently of alias types

Pinned `checker.go:16266` uses private `aliasSymbolLinks.aliasTarget`, its
AliasTarget resolution stack, transitive alias/type-only propagation, and
failed-pop recovery to unknown. TSR `symbols.rs:1040` intentionally recomputes
target dispatch. Caching an alias's resulting type does not avoid every separate
target request.

This is the already-open `tsr-1yb.7.7.3`, now with refreshed CPU locating
evidence. Its symbol-ownership and publication prerequisites remain intact;
an ad-hoc resolving bit or a name-keyed cache would change native cycles.

### 5. Cheap type-variable gates and mapper-scoped reuse

Pinned `checker.go:22184` caches the conservative
`CouldContainTypeVariablesComputed` answer in object flags. At line 22104 the
instantiation entry uses that gate before expensive work, then consults a cache
owned by an active mapper and keyed by type plus alias. `popActiveMapper` clears
the cache while retaining reusable map storage.

TSR `inference.rs:5397` checks mapped identity, completes pending returns, and
uses a parameter-specific graph walk (`:6304`) before its worker. Object,
signature and reference instantiation caches already exist elsewhere. The
remaining difference is not a license for a global TypeId cache: native's
conservative bit differs from exact parameter membership, and mapper lifetime,
alias/display mode, metadata mutation and fixing context affect safe reuse.
Continue the existing `tsr-1yb.4.1.2` and graph/copy attribution work.

## Worker policy and rejected explanations

Both compilers use the complete Program file index modulo the selected checker
count, private checker state, and source order within each worker. There is no
native work-stealing advantage in the pinned pool implementation.

Additional normal file-list controls verify **identical ordered identities**, not
just identical sets: all 465 generated and all 9,861 API files match after
normalizing bundled-library prefixes. At four checkers, there are zero owner
assignment differences. Generated `main.ts` is file 464, belongs to checker 0,
and comes after the model files in both tools. Checking dependencies first is
already happening in this workload.

In the TSR generated CPU recording, checker 0 accounts for 6.573 s of sampled
running weight; the other checkers account for roughly 1.45–1.47 s each. Its last
observed CPU sample is at 23.15 s, versus 12.37–12.79 s for the other workers.
On API, checkers 0 and 1 carry 12.568 / 12.242 s versus 3.645 / 5.281 s for 2 and
3. These are observer-on samples, not exact worker start/end durations, but they
show expensive tails in the selected assignment.

The qualified archived native activity producer confirms four private checkers,
four simultaneously observed full-file workers, and 402 first full-file checks
on the generated project, with unchanged complete diagnostics/file listing.
Its 1,642,089 query/span events inflate wall time from 0.630 s off to 15.789 s
on. Those worker durations are unusable for throughput attribution. The API
query observer was stopped and its partial trace is explicitly unqualified.
Do not compare those logging spans to ordinary TSR timing.

Fresh-process runs disable both incremental and composite reuse. The existing
[cache-isolation proof](benchmark-cache-isolation.md) establishes the native full
compilation path. Filesystem warming remains intentional. The relevant native
caches in this diagnosis are private state built within one compilation.

## Reproduction and next work

The verbatim [timing runner](checker-native-cpu-gap.timing.py) consumes an existing
`whole_project_perf.py` preflight receipt with the project, binary paths, loaded
files and expected complete output. For example, with task-owned frozen binaries:

```sh
python3 scripts/whole_project_perf.py --project "$TASK_PROJECT/tsconfig.json" --tsr "$TASK_TSR" --tsgo "$TASK_TSGO" --samples 1 --output "$TASK_OUTPUT/preflight.json"
python3 docs/architecture/checker-native-cpu-gap.timing.py --repo "$TASK_REPO" --preflight "$TASK_OUTPUT/preflight.json" --output "$TASK_OUTPUT/interleaved.json" --cohorts 5
```

For native attribution, add `--pprofDir "$TASK_PROFILE_DIR"` to a separate
ordinary compiler invocation and read its CPU profile with `go tool pprof`.
This host's Go installation lacked the pprof executable; its bundled
`cmd/pprof` source was built offline into the task directory. Merge matching
profile files when the generated check is too short for useful sample counts.

For TSR, use `xctrace record --template 'Time Profiler' --no-prompt`, launching
only the task's compiler child. Inspect `xctrace export --toc` before selecting
the `time-profile` schema. The [CPU reader](checker-native-cpu-gap.cpu.py) consumes
that exported XML. Sampling settings and profile hashes remain in the
local archive/receipt. Never include either profiler in the timing cohorts.

Prioritize completed effects and object-display boundaries for the generated
project, and instantiated-member/copy boundaries for API. Existing alias,
mapper, ownership and fidelity work remains independently tracked. Actual
avoided worker executions, full previous-RIGHT preservation, memory bounds and
ordinary whole-CLI benefit are still required before retaining a runtime change.
