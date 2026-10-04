# Observational whole-project CI reports

The Performance workflow publishes `whole-project-results/` alongside the existing
parse/bind and conformance artifacts. It runs the public `generic-imports` smoke
project in default and single-threaded modes, serially under the existing
`self-hosted-exclusive` group. This exposes incomplete evidence and tool failures
while the faithful checker port continues.

The smoke fixture validates reporting plumbing. It cannot establish representative
throughput, activate a timing ratchet, or verify the release requirement that TSR
take at most half the wall time of pinned tsgo. The JSON always sets
`release_target_verified` to false; the summary says the target is not evaluated.

## Local use

Build both release CLIs before invoking the reporter:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/report_whole_project_perf.py \
  --project benches/projects/generic-imports/tsconfig.json \
  --tsr /absolute/path/to/release/tsr \
  --tsgo /absolute/path/to/pinned/tsgo \
  --output-dir /tmp/whole-project-observations --samples 5 --timeout 60
```

The adapter reuses `whole_project_perf.py`: one warmup, alternating fresh-process
pairs, no emit, incremental/composite reuse disabled, warmed filesystem, and
per-child wall/user/system CPU/peak RSS. Builds, input snapshots, and configuration
and loaded-file discovery remain outside measured checks. Default worker requests
and `singleThreaded=true` are recorded separately; actual worker counts and budgets
remain unavailable. Prior TSR/TSGO probe environment is removed before launches.

`observations.json` embeds each mode's raw report. `default.json` and `single.json`
retain complete diagnostics, child commands/identities, binary/input fingerprints,
effective options, loaded scope and comparability reasons. `summary.md` shows
completed-sample counts, median/range/standard deviation, CPU and RSS. Harness logs
are retained per mode. Source/oracle checkout revisions do not independently prove
which source produced a caller-supplied binary; CI builds from its checkout, while
local users must preserve their build provenance.

The harness requires POSIX `wait4` and Python 3.8 or newer. It converts ordinary
and signaled child statuses with `WIFEXITED`/`WEXITSTATUS` and
`WIFSIGNALED`/`WTERMSIG`; it does not require Python 3.9's
`waitstatus_to_exitcode`. The timer and `wait4` still own termination and reaping,
so the resource receipt belongs to that child.

## Failure handling and gate boundaries

Normal compiler exits 0, 1 and 2 can represent completed checks with diagnostics.
The adapter distinguishes `completed_incomparable`, `timed_out`, `tool_failed`,
changed/invalid inputs, `harness_failed`, and `setup_failed`. Failed warmups persist `rejected_measurement`; failed configuration/listing
preflights persist `rejected_preflight` before aborting the harness. Partial and failed runs
publish their evidence without a completed-run ratio. A matching loaded list or
fast result cannot bypass the missing actual-work or complete-query-input gates.

The workflow builds whole-project CLIs in a separate step before measurement.
That step uses `continue-on-error`, preserving the existing parse/bind gate if a
reporting-only build fails. The reporter checks the build step's `outcome`, which
records failure before `continue-on-error` changes its conclusion. See the
[GitHub steps context](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#steps-context).
On failed/skipped setup, `--builds-ready false` prevents compiler launches and
removes prior raw mode reports. Every invocation replaces its aggregate report,
summary and logs so a stale successful artifact cannot stand in for a new run.

Reporting control tests run after the existing Compare step. Observation failures
remain visible in artifacts and do not introduce a throughput gate. Result upload
and job summaries run with `always()`, including after compiler or gate failures.

## Validation and remaining evidence

All 45 script tests pass, including real subprocess controls for accepted/error
checks, options/scope/full-diagnostic mismatches, input mutation, warmup crash and
timeout, missing binaries, failed setup, and stale successful output. YAML syntax,
step ordering and the exclusive concurrency group were checked locally.
The new child-process control removes the Python 3.9 wait helper and verifies
exit codes 0/2/5, SIGTERM, timeout/SIGKILL and per-child resources. The reporting
suite has 34 tests; cache-isolation and phase-cost controls add seven and four.

A release TSR binary built from `02fafbc0f8b6ad9274ee68a752c1e480ba587b54` and
clean native `5b1047d10d32e7d5b446be4de56b126ff42f82bb` completed five pairs in
each mode through the adapter. Both reported 66 loaded files and the same one
intentional diagnostic in every sample. Both reports are correctly incomplete for
equivalent-work verification: actual checking/worker budgets and complete
cross-tool query-input coverage are unverified. The local receipt is
`/tmp/tsr-ci-observations-final-02fa/observations.json`; these observations qualify
reporting behavior at the frozen source, not throughput on a later main revision.

The first remote run, [37231887059](https://github.com/codemod/tsr/actions/runs/37231887059)
at `95e676791866234ddfa220a85847eb88c30fd76e`, published its artifact but failed
the controls: its Python 3.8 interpreter lacked `os.waitstatus_to_exitcode`.
Downloaded `whole-project-results/observations.json` confirms both modes were
`harness_failed`, with zero samples and `release_target_verified=false`. The
portable status conversion addresses that confirmed failure; a later successful
remote run still must verify complete publication. Beads `tsr-1yb.8.1` tracks the
publication deliverable. Parent `.8` retains broader integration; `.1.2` owns
performed-work evidence, `.1.3` owns the representative public suite, and `.13`
owns the later regression ratchet. The verified native wall ratio <=0.50 remains
unmet and unverified.
