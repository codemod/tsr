# Full compiler oracle — tsr-2zk.47.3

Pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Release acceptance is **not met**. This work is not a speed win or a 99.9% parity claim.

## Population and execution

Recovered native overlay candidate `93a40491`. Discovery exercises the actual
pinned compiler runner, configuration expansion, both source trees, compiler and
conformance suites. Observed full manifest: **15,323 records**, including the eight
native configuration failures. No compilation was performed in this discovery run.
Native skips remain eligibility metadata, not exclusions.

A separate process runs each native configuration and each TSR configuration.
Ten worker leases; 30-second compiler deadlines; file-backed stdout/stderr; timed
out children killed and reaped. Nonzero native exit invalidates a completed row.
Native records are fsynced on publication and appended durably to the aggregate
as each process settles. Actual artifacts and verdicts are checkpointed per
configuration. Missing/crashed configurations stay in the denominator.

`full_oracle_run` requires the real sibling `full_oracle_actual` executable;
there is no optional/missing-producer mode. TSR consumes source plus the native
expanded merged setting map, builds a VFS-backed `Program`, checks source files,
collects diagnostics, and independently traverses/types source nodes. The expected
artifact never enters its process. Type source echoes, CRLF, headers, file order,
and only the pinned prefix replacements are retained. The actual writer guard
uses actual diagnostics, not reference-baseline existence.

The existing assertion APIs now accept `TestFile` source units instead of
`FileTypes` expectations. All Rust callsites were migrated, without adapters.
Legacy suites still have historical population and ordering rules; their passing
rates are not full-oracle acceptance.

## Identity, ownership and publication

Native identity is tree + suite + relative source path including extension +
native configuration description. Compiler tables/NodeIds belong to one Program.
The TSR diagnostic source-image map belongs to one producer invocation, keyed by
canonical Program filename. Related-file UTF-16 positions require the matching
source image; absent images fail rather than synthesize coordinates.

The type traversal shares the checked Program's private Checker; printed receiver,
alias and node-parent context remain the existing writer's. No cross-configuration
semantic cache is introduced. Existing bundled/test-library text caches are
process-local and immutable; parse/check work remains per isolated process.
A completed native row is not successful until the process exits successfully.
Unpublished, crashed, timed-out and failed discovery states remain failures.

## Diagnostic integration prerequisites

Fetched `box/parity-diagnostic-chains` at `b6d2104f`; its implementation is parent
`56eaaed3`. Dependency was cherry-picked separately for compilation. Integration
owner must not reapply that commit after merging the diagnostic owner.
The oracle consumes its real `message_chain()`, `related_information()`, `file()`,
`compare_diagnostics()` and `equal_diagnostics_no_related_info()` APIs.

Still missing from that model:

- Per-diagnostic `reports_unnecessary`, `reports_deprecated`, `skipped_on_no_emit`;
  message flags are not a substitute for diagnostic overrides. Native TS2695 is a
  witnessed mismatch. Current serialized flags expose this mismatch, not parity.
- Signed/undefined global diagnostic spans: native compiler diagnostics use -1.
- Native Program/global option diagnostics; `settingsSimpleTest` witnesses TS5108
  for removed Classic resolution. Compiler and diagnostics are owned elsewhere.
- Native pre/post-emit comparison and non-isolated declaration/suggestion producers.

Required owner contract: expose the three diagnostic flag getters; retain signed
undefined locations; supply Program/global/pre-post-emit diagnostics without
flattening chains or dropping related-file identity. These are correctness
prerequisites, not permission to exclude affected configurations.

## Exercised evidence

Commands used offline release builds; Cargo/config/lockfiles unchanged.

- Built every tsr-conformance example after caller migration.
- Library tests: 137 passed.
- `full_oracle_boundaries`: 3 passed, including real native/TSR clean control,
  native Cartesian/span control, and a hung-process deadline/reaping control.
- `assignment_declarations`: 10 passed.
- Focused executable run: five configurations, one exact (`2dArrays`). Type artifact
  bytes match native for all five: `settingsSimpleTest` strict true/false,
  `commaOperator1`, `2dArrays`, `genericParameterAssignability1`.
  `commaOperator1` errors also match byte-for-byte. Other semantic/error differences
  remain visible; the runner exits nonzero for failed acceptance.
- Completed all tsr-conformance tests successfully. An initial full-suite failure
  exposed that native selection orders roots before other files and filters unloaded
  files; ported those rules from source/Program facts without expected-file inputs.
  `commonjs_module_elements` then passed without re-pinning its native rows.

No full compilation run, prior-RIGHT preservation certificate, or whole-project
median performance measurement was performed. Prior exact-ID ledger comparison is
available, but historical line/diagnostic RIGHT mapping requires integration.
Missing exact IDs are losses. An empty prior ledger does not certify no losses.

## Run

```sh
cargo build --offline --release -p tsr-conformance --examples
target/release/examples/full_oracle_run OUTPUT_DIRECTORY [SELECTION_REGEX] [PRIOR_VERDICTS_TSV]
```

The prior file is a full-oracle ledger with `Exact` in column two. Artifacts must
be copied out of the temporary output directory to retain them across Box removal.
The full-run command must wait for the diagnostic/compiler prerequisites;
focused controls already prove
the real TSR producer is wired and its differences remain visible.
