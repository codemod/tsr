# Full compiler oracle recovery — tsr-2zk.47.3

Native pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Recovery checkpoint: `0e7824dd`. Source inspected: public
`box/parity-full-corpus-r2`, tip `319b1c2c`. Only conformance-owned
changes were extracted; the saved branch was not merged. Its automatically
committed snapshots are candidate source, not verified receipts.

## Current measurement

Full native discovery and isolated native/real-TSR production completed:

- Population: **15,323**; configurations: **15,315**; discovery failures: **8**.
- Exact: **4,813 / 15,323 = 31.4103%**. Release parity acceptance fails.
- Different diagnostics+errors only: 4,813.
- Different semantic diagnostics only: 3,028.
- Different all three artifacts: 1,408.
- Different types only: 950.
- Different diagnostics+types: 260.
- Native process failures: 30 (29 exits, one deadline).
- Actual process failures: 13 (10 exits, two deadlines, one abort).

These are artifact mismatch groups, not inferred checker root causes. Every
configuration and discovery failure remains in the denominator. Empty outputs
are actual artifacts, not absent-producer fallbacks. Native exclusions are
retained as metadata; they do not exclude configurations.

An initial campaign was interrupted by the command deadline and was **not**
reported as passing. A subsequent fresh full run settled all configurations.
The native worker now resumes only unpublished leases, validating source pin,
overlay contents and selection identity before reusing its manifest/binary.
Completed failures are settled records, not automatically retried successes.

## Expected-independent producer and boundaries

Go overlays expose the actual pinned compiler runner, configuration expansion,
pre/post-emit diagnostic collector, error baseline and type walker. No vendor
source is edited. Native outputs and TSR outputs execute in separate fresh
processes; expected artifacts are never passed to the TSR producer. Type output
is generated from source units and actual diagnostic presence. Every Rust caller
of the old expectation-driven assertion API was migrated to source units.

Native operation: `typeWriterWalker`, `generateBaseline`, `newCompilerTest`,
`CompileFilesEx`, `ast.CompareDiagnostics` and
`EqualDiagnosticsNoRelatedInfo` at the pin above.

Identity/owner: configuration identity is source tree + suite-relative path
including extension + native variation description. Native worker records must
match the leased identity. Each process owns its Program/Checker, node stores,
source images and options. No cross-configuration semantic cache is introduced.

Publication: discovery failure, unpublished lease, completed output, failed
process and unsupported semantic fields remain distinct. A native record becomes
completed only after successful process exit. File-backed output avoids pipe
blocking; deadlines kill and reap children; aggregate records fsync on publication.

Receiver/alias context: source-node queries use the existing Program-backed
private Checker and original parent/node sites. No presentation-name or raw
cross-store-ID equivalence is introduced. Native per-file checker affinity is
not proven equivalent to TSR's single private Checker.

Work boundary: parsing/checking/type traversal is per configuration; native
compilation includes its real emit/diagnostic phases. TSR currently checks the
loaded files and uses the available manual diagnostic collector; it does **not**
implement all native pre/post-emit Program/global/declaration/suggestion work.
No equivalent-work speed claim is possible from these runs.

## Unavailable diagnostic contracts

The checkpoint Diagnostic model has message, unsigned span and arguments only.
The producer explicitly serializes unavailable message chains, related
information and per-diagnostic reportsUnnecessary/reportsDeprecated/skippedOnNoEmit.
Native booleans/trees cannot compare exact to those markers. Message defaults,
code-specific heuristics and empty fabricated trees are not substitutes.

Required diagnostic-owner API:
`Diagnostic::{file,set_file,message_chain,related_information}`;
`reports_unnecessary/reports_deprecated/skipped_on_no_emit` backed by native
per-diagnostic writers; signed global location accessors; native compare/equality
and diagnosticwriter chain/related formatting.

Required compiler-owner result: actual pre/post-emit Programs and complete
config, Program, syntactic, semantic, global, declaration and enabled suggestion
diagnostics, including native sorting/count-mismatch behavior. Oracle consumers
must adopt that real result when supplied; no fake success fallback exists.

## Verification and RIGHT preservation

- Offline release build of every conformance example succeeded.
- Final `cargo test --offline --release -p tsr-conformance`: **587 passed**.
- Real native controls cover clean/config-only sources, Cartesian variations,
  same-start distinct diagnostic lengths, disabled type output retaining semantic
  failures, duplicate/orphan publication rejection and deadline reaping.
- An independently archived `0e7824dd` build produced **469,785 RIGHT keys**.
  Current unfiltered verdict dump preserves **all 469,785**: **0 losses**, checking
  missing keys as losses. Initial saved cutover lost/misaligned 221 keys; comparison
  now aligns produced source-unit identities after independent production instead
  of blindly zipping reordered sections.
- Full-oracle historical exact-ID ledger was not supplied; its certificate is
  explicitly unavailable. A missing/empty ledger cannot satisfy release acceptance.
- Equivalent complete-work median wall <=0.50 and hotpath timing are **unverified**.
  No checker hotpath files were changed.

## Durable receipts and reproduction

All receipts are repository-ignored under `target/recovery/oracle/`, not `/tmp`.
`current/` retains native manifest/results/binary/overlay, per-worker status/logs,
actual settings and all three artifacts, verdict ledger, summary and root groups.
`checkpoint-right.tsv`, `current-right.tsv`, `right-losses.tsv` and final test logs
retain the checkpoint preservation proof.

Transfer **before Box destruction**:
`target/recovery/oracle/current-artifacts.tar.gz` (72 MB), SHA-256
`96fc66da378bc8f57c8d427f6a0c1cd9bec6fa49bb7c13a6a43e49da165e8ab2`.
Ignored receipts are not transported by the commit fetch alone.

```sh
cargo build --offline --release -p tsr-conformance --examples
cargo test --offline --release -p tsr-conformance
target/release/examples/full_oracle_run target/recovery/oracle/new-run
# Optional third argument is a nonempty prior full-oracle Exact ledger.
# Successful release exit also requires that prior certificate.
```

The existing issue was queried with `bd prime` and `bd show tsr-2zk.47.3`;
this Box's database reports that issue absent. No duplicate issue was created.
Parent must update the existing issue with these results and unresolved contracts.
