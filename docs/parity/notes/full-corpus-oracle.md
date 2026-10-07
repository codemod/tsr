# Full compiler oracle — tsr-2zk.47.3

Pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Release acceptance is **not met**. This work is not a speed win or a 99.9% parity claim.

## Population and execution

Recovered native overlay candidate `93a40491`. Discovery exercises the actual
pinned compiler runner, configuration expansion, both source trees, compiler and
conformance suites. Observed full manifest: **15,323 records**, including the eight
native configuration failures. No compilation was performed in this discovery run.
Native skips remain eligibility metadata, not exclusions. The machine-readable
[evidence record](full-corpus-oracle-evidence.json) preserves all eight failed IDs
and the five focused verdicts across Box destruction.

A separate process runs each native configuration and each TSR configuration.
Ten worker leases; 30-second compiler deadlines; a 180-second native build
deadline; file-backed stdout/stderr; timed
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
This Box exposes only read/bash/edit/write tools, not LSP or AST codemod devices.
Legacy suite comparison still zips results against expected file sections; that
comparison is not used by the full artifact oracle and cannot certify full parity.
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
The candidate notes commit `b6d2104f` is also applied separately as dependency
`cb99aa8d`; neither dependency is oracle-owned code.
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

Required integration-owner contract for `b6d2104f`:

1. `Diagnostic::reports_unnecessary() -> bool`, `reports_deprecated() -> bool`,
   `skipped_on_no_emit() -> bool`, backed by per-diagnostic state and real writers,
   not code-specific consumer overrides. The current producer uses message flags
   and lacks skipped-on-no-emit state; it is not lossless for those fields.
2. Signed byte location accessors preserving native undefined `(-1, -1)` globals;
   root and recursive nodes need the same location contract. Keep existing AST
   spans unchanged if the diagnostic model carries its own signed location.
3. Full Program/global diagnostics paired with optional canonical file identity,
   including option diagnostics such as witnessed TS5108; do not fabricate them
   in the oracle from expected rows or special-case the control source.
4. Pre/post-emit diagnostic collections and native option/configuration semantics;
   declaration/suggestion producers must run where the native harness runs them.

Existing chain/related APIs and Program source images are integrated already.
Diagnostic top rendering prefers each diagnostic's canonical source image,
including library/redirected files not among baseline input echoes; it must not
silently downgrade a file diagnostic to a global one.
These are correctness prerequisites, not permission to exclude configurations,
shrink the denominator, or mark the current producer as complete/lossless.

## Exercised evidence

Commands used offline release builds; Cargo/config/lockfiles unchanged.

- Built every tsr-conformance example after caller migration.
- Library tests: 137 passed.
- `full_oracle_boundaries`: 4 passed, including real native/TSR clean control,
  native Cartesian/span control, a hung-process deadline/reaping control, and
  config-only options with exact diagnostics/error/type artifacts.
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

- Additional real-process config controls: `tsconfigSimpleTest` exact; malformed
  non-object and rootDir/include controls differ in diagnostics/errors while type
  artifacts match. Config source images are now attached before native comparison,
  preserving identity even when JSON is not a Program source file. The simple
  control's strictNullChecks option exists only in tsconfig, not test directives.

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
