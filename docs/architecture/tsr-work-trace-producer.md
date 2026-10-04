# TSR performed-work probe

`tsr-1yb.1.2.3` now has a bounded Rust producer behind the `work-trace` Cargo
feature. Ordinary builds contain no observer field, checker hooks, sidecar
environment reads or observer JSON dependency. A probe build can observe the
serial CLI without changing its diagnostic policy. This is instrumentation,
not a scheduler or a throughput improvement.

## Entry boundaries and ownership

The private checker attaches its observer after construction and applying
compiler options. Initialization before attachment is explicitly unobserved.
The producer records one actual `checker_created` event at that site; requested
worker flags are separate fields. Current TSR uses one serial checker even with
`--checkers 2`. `effective_serial_checker_limit: 1` describes that implementation;
`memory_admission_budget: null` leaves admission policy unsupported.
`requested_checkers_matches_actual_instances` records numerical agreement only;
`worker_options_applied_by_driver: false` records that the current driver does
not implement worker selection. A request for one can coincide with the actual
count without showing that worker policy ran.

| Operation | Entry | Meaning |
| --- | --- | --- |
| `source_file_check` | `check.rs`, `Checker::check_source_file` | Actual full-file entry/return, including TSR's unused checking within this call |
| `symbol_type_query` | `symbols.rs`, `get_type_of_symbol` | A query, including dispatches that return cached types |
| `declared_type_query` | `declared.rs`, `get_declared_type_of_symbol` | A query, including cached declarations |
| `variable_type_worker` | `symbols.rs`, `get_type_of_variable_or_parameter_or_property_worker` | Worker entry after the caller's cache lookup, including lazy JSON typing |

An owned RAII span records return or unwinding without borrowing the checker.
It only reads binder declaration identities and AST parents; it does not query
types, print types, change publication states or expose `TypeId`. Query target
files are the symbol's declaration-source set, not proof that every declaration
was recomputed. Nested spans describe observed call relationships; they do not
cover every forcing path or establish native cache ownership.

File IDs are indices in the complete ordered Program file table. The map from
Program-scoped source `NodeId` to those indices is local to this invocation.
Logical paths and source-node IDs remain in the sidecar, unmapped IDs are
explicit, and checker ID zero denotes this producer's sole attached instance.
None of these IDs can be compared across type stores, Programs or processes.

`program_file.full_check_eligible` and the CLI share `full_check_exclusion`
in `tsr-execute/src/compile.rs`. Its ordered exclusions follow native
`Program.SkipTypeChecking` / `canIncludeBindAndCheckDiagnostics`: noCheck,
declaration skipping, default-library skipping, a disabling file directive,
JSON, and explicitly unchecked JavaScript. JS with checkJs unset remains
eligible as native plain JS; an enabling file directive overrides checkJs false.
Default libraries use ordered Program membership, not a basename or the
no-default-lib pragma. Missing source nodes are explicit. Project-reference
redirects are unimplemented, so their native source exclusion is unsupported
and native eligibility certification remains false.
See [pinned native boundaries](native-check-trace-characterization.md), native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, for the corresponding full-worker,
lazy JSON, tracer/construction and sampled-event boundaries.

## Invocation and failure contract

The OS host reads `TSR_WORK_TRACE` only in a probe build. It opens a **new** file;
an existing path produces a stderr warning and is never overwritten. The
schema-version-1 NDJSON header includes actual PID, invocation nonce, arguments,
operation coverage and optional source/binary claims. The host flushes that
header immediately. Effective option debug text, show-config output, roots,
current directory and ordered Program files are separate records.

Normal control flow writes `invocation_end` and flushes the sidecar. `complete`
means the observer saw normal invocation control flow with no unfinished or
unwinding spans; diagnostic errors can still return normally. It does not mean
all semantic work was implemented or native-equivalent. Missing configuration
and list-files-only invocations explicitly distinguish no Program/no checker
from actual checks. Failed writes disable further records and warn separately,
preserving compiler output and status. Termination before the invocation finishes
leaves no normal completion marker. Consumers must also require a normal child outcome,
valid full records, the current invocation identity and no trace warnings;
compiler exit status alone never qualifies the sidecar. Reject a signaled child
even if it died after flushing end records but before exiting normally.

Source SHA (`TSR_WORK_TRACE_BUILD_SHA`, at build time) and executed binary hash
(`TSR_WORK_TRACE_BINARY_SHA256`, at run time) are **claims**, not self-verified
provenance. Missing claims remain null. A dirty tree/patch, actual binary and
complete input identities need a qualified external receipt. These records
always retain `all_forcing_observed: false`,
`complete_provenance_verified: false` and
`actual_work_equivalence_verified: false`. Native eligibility equivalence and
complete input equivalence are also false. No harness gate is enabled by this
producer.

## Running a probe

Build separately from the ordinary binary, then choose a sidecar path that does
not exist:

```sh
cargo build --release -p tsr --features work-trace --target-dir /tmp/tsr-work-probe
TSR_WORK_TRACE=/tmp/fresh-work.ndjson /tmp/tsr-work-probe/release/tsr --project /path/to/tsconfig.json --pretty false
```

The complete stream can be large: every selected query/worker entry is emitted.
The mutex protects trace bookkeeping/output, never semantic stores; the OS
writer buffers 64 KiB. A probe build with tracing disabled still has optional
observer branches. Keep probe-on, probe-off and native type-dump overhead outside
throughput acceptance timing; use ordinary builds for that timing.

## Eligibility alignment after the producer

The original evidence below exposed the forcing constraint: 68 equal loaded
identities concealed 4 versus 3 normal full checks, and 5 versus 67 with library
checking. The old driver claimed libraries never produced diagnostics and
excluded them before consulting options. JSON entered the full worker even
though native types imported JSON lazily. The driver now checks eligible files
from the complete Program table and shares its rule with directive/diagnostic
filtering and trace rows. Skipped files remain loaded, bound and queryable.

Pre-work `58cfa176` plus four qualified production files is captured in
`/tmp/tsr-eligibility-58cf/public-controls-final-314b/receipt.json`. Thirty-six
fresh children cover six requests in both tools, each off/on/repeat. All
preserve each tool's complete output/status across observer modes, 68 ordered
loaded identities, matching started/returned full-file identities, and complete
ordered native diagnostic lines:

| Request | Full checks, each tool | Intentional diagnostics, each tool |
| --- | ---: | ---: |
| Default / single / two requested checkers | 3 | 1 |
| noCheck | 0 | 0 |
| skipLibCheck false | 67 | 2 |
| skipLibCheck false, skipDefaultLibCheck true | 4 | 2 |

Seven native/candidate JS controls preserve plain-JS, explicit false/true and
last-file-directive distinctions. Eight native parameter/rest controls cover
omitted augmentation metadata, optional/required counts, later constraint/default
conflicts, distinct parameter names and never/scalar rest. The first candidate
exposed 62 false library errors: 61 TS2428 and one TS2370. Metadata omission and
optional-count handling remove those errors without discarding workers; never
is accepted as the bottom type. Constraint/default equality still uses the
pre-existing shallow written signature domain, not resolved-type identity.
`tsr-1yb.1.1.2.1` retains that broader fidelity boundary. No cache, mapper or
private-store identity is introduced. Diagnostics sort/deduplicate by canonical
file, span, code and arguments, the supported fields of `ast.CompareDiagnostics`.
Message chains and related information are not represented by this port.

Updating observer strings alone would leave the runtime omission intact.
Keeping the unconditional library skip would conceal the 62 errors and preserve
incomparable work. The accepted cost is performing the requested library work.
A native control with different eligible/started/returned identities, changed
lazy JSON diagnostics, a lost previous RIGHT row, or a real conflict hidden by
omitted metadata refutes the change. These controls do not certify all forcing,
input coverage, worker budgets, project references or a comparable speed ratio.

| Current eligibility artifact | SHA256 |
| --- | --- |
| Frozen probe binary | `314b63a54848b7460e712265f1c07cb2476da49c414ad853ef0cfd3ef065ef22` |
| Ordinary final binary | `07bc9bebea7869e41fe78414a5733df651b0dc3aeece1eae1d8a639d619de361` |
| Public control helper | `2c0c0d515e99e47a54c016c55843630f7dd531bf2b382f2dc63c7dd102224ece` |
| Public control receipt | `e87f10acc414363274847f79198e7cf1f0384c05fa6511c6cfe26e10ff45fbd3` |

The same patch preserves all 474,852 aligned type verdicts byte-for-byte,
including 461,575 previously RIGHT rows. All 10,570 diagnostic cases retain
their identities: previously passing cases have zero losses, one WRONG case
becomes RIGHT and five EMPTY_WRONG cases become EMPTY_RIGHT. Four remaining
WRONG cases change only by removing unexpected diagnostics; none loses an
expected diagnostic. These are fidelity results, not throughput measurements.
The complete type and diagnostic dumps and comparison are retained locally in
`/tmp/tsr-eligibility-58cf/`. Source/diagnostic eligibility denominators and native
baselines are unchanged. Plain-JS diagnostic filtering beyond these controls
and resolved constraint/default identity are not certified by this patch.

Early follow-ups split producer forcing (`tsr-1yb.1.2.3.1`), actual worker
admission/ownership (`tsr-1yb.1.2.3.2`), artifact integrity
(`tsr-1yb.1.2.4.1`) and semantic-work comparison (`tsr-1yb.1.2.4.2`). The first
three can start independently; comparison waits for those specific prerequisites.
Matching internal operation counts is not required across different compiler
implementations, but complete required checking, inputs and output are.

Current patch verification passes 2,931 feature-enabled release workspace tests
(six existing ignores), 49 ordinary CLI/execution tests (one existing ignore),
strict workspace Clippy in both feature modes, formatting, 3,403 upstream
anchors and internal section citations. Eight fresh ordinary-build controls
preserve complete probe output/status with the observer environment absent or
set; the ordinary binary contains no observer string and creates no sidecar.
The issue-citation gate still reports 190 historical IDs missing from the
authoritative Dolt registry, unchanged pre-existing `tsr-10` debt; no citations
or issue records were deleted/fabricated to make it pass. Owned code/tests were
reviewed manually because unrelated local work was present. The explicit
simplification passes introduced no changes or new abstractions. No new
blocking review finding remains; broader resolved identity and work certification
remain the existing issues above, not completed by these tests.

## Original producer validation and remaining work

The original local probe is source-qualified by pre-work main `7762af68` plus an
exact per-file patch manifest in
`/tmp/tsr-work-trace-controls-published-7762/receipt.json`. The helper snapshots the
owned production/test files before running and verifies their hashes afterwards.
Rust is `1.96.0`, host `aarch64-apple-darwin`; the ordinary and probe binaries
are frozen separately. This local receipt does not turn the producer's claims
into complete build/input/work provenance.

Fifteen fresh CLI children run default, single-threaded, two requested checkers,
noCheck and skipLibCheck false, each off/on/repeat. All preserve complete raw
stdout/stderr, exit status and 68 ordered loaded identities. Enabled/repeated
records match after removing the per-process header, and all work spans balance.

| Request | TSR full checks | Pinned native full checks | TSR instances created | Intentional diagnostics |
| --- | ---: | ---: | ---: | ---: |
| Default | 4 | 3 | 1 | 1 |
| Single-threaded | 4 | 3 | 1 | 1 |
| Two checkers | 4 | 3 | 1 | 1 |
| noCheck | 0 | 0 | 1 | 0 |
| skipLibCheck false | 5 | 67 | 1 | 2 |

Native counts come from the separately pinned characterization, not a new
cross-tool timing comparison. TSR's fourth normal full check is JSON; its
fifth declaration-enabled check is the imported declaration. All 63 libraries
remain omitted from TSR's full-file loop. Normal cases also observe three
symbol queries, two declared-type queries and one actual variable worker for
JSON. These historical counts are superseded for full-check eligibility by the
alignment above (`tsr-1yb.1.1.2`); recording the original mismatch does not
prematurely enable a comparable ratio.

The ordinary binary contains no `TSR_WORK_TRACE` environment string, ignores
that environment variable without creating a sidecar, and preserves the same
complete public output. Two additional five-pair rounds per normal/declaration
case measure probe-on versus probe-off cost with warmed OS reads. All 40 measured
children preserve output; CPU/RSS and variability remain in a separate receipt.
These small-fixture observations do not estimate large-project instrumentation
cost or qualify native throughput.

| Case/round | Off median wall ms | On median wall ms | On p95 wall ms |
| --- | ---: | ---: | ---: |
| Default 1 | 30.755 | 31.421 | 32.381 |
| Default 2 | 30.490 | 31.575 | 33.909 |
| Declaration 1 | 30.463 | 31.971 | 49.501 |
| Declaration 2 | 31.399 | 32.167 | 48.107 |

The off/on/repeat ordering is not a paired overhead estimate; an earlier frozen
prototype had a cold first observation around 166 ms. The separately warmed paired rounds
retain their outliers and do not claim a constant observer cost.

| Artifact | SHA256 |
| --- | --- |
| Final probe binary | `8e3e4cadfbf8431a688b71f29b1bf2ddb0b77eab6e04ecf51e9b92c1d443e415` |
| Ordinary binary | `de989d4024dc2ab11934f8726ed134daa7eeeeedadbd5370a5dbdafcdb69e465` |
| Public fixture contents | `15ac0a1449e8c2a98b4594899dfe3883c0726fe1433ef87d9afa8a4122288d9a` |
| Final control helper | `ec59d99f2f28785177c6a7b1e4764347bf29194ed95d124c05edc3e3ad1612b7` |
| Final control receipt | `cc5b6862bb53463f11a6f6a71bfb7c70ba1416bbc925d477f12db2796044487f` |
| Paired overhead helper | `30496a34434df7ec9995e18d3b5bdd7be4864a58f701238d0e7dfd77b7035e3c` |
| Paired overhead receipt | `03b21379fb45a459fe95cfc9c260ee6edc6bcb5c5e5ea08cac97a03a413e7802` |

The paired receipt is
`/tmp/tsr-work-trace-controls-published-7762/overhead-quiet-receipt.json`; the
ordinary-build control is `default-control.json` in the same directory.
Figures and source identities were refreshed after separating numerical worker
agreement from policy application; earlier prototype receipts remain local.

The final feature-enabled release workspace passes 2,924 tests across 258 result
blocks, with six existing ignores. Ordinary CLI/execution tests and strict
workspace Clippy in both feature modes pass; formatting is checked. These are
test results, not a new corpus/diagnostic verdict measurement.

Seven virtual-host controls preserve complete CLI output/status and repeated
trace semantics for cyclic imports, deliberate JSON assignment errors, imported
declarations, noCheck, skipLibCheck, worker requests, missing configuration,
list-files-only, injected sidecar write failure and final flush failure. A flush
failure can leave parseable end records; its separate warning still invalidates
that trace. Two real CLI controls verify
actual PID/new-file behavior and reject killed invocation completion.

This slice deliberately leaves producer `.1.2.3` in progress. Remaining work
includes broader/native eligibility and lazy-forcing hooks, qualified build/
binary/patch/input provenance, cancellation/failed-check distinctions and
native actual-construction/admission evidence. `.1.2.4` owns strict trace
validation and benchmark integration; `.1.2.2` owns complete query input
coverage. The comparable whole-project TSR/pinned-tsgo median wall target
<=0.50 remains unmet and unverified.
