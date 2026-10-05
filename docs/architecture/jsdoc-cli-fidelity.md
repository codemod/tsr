# Program JSDoc transport and initializer checking

The CLI and private-checker helper need the same Program JSDoc tables already
supplied by conformance. A table-only experiment still accepts a string assigned
to `@type {number}`: the initializer reporter excludes JavaScript variables and
reads only syntactically written annotations. Both assignment checking and
initializer context must read the existing effective annotation selector.
Native's hosted reparser supplies these annotations to `Node.Type()` before
`checkVariableLikeDeclaration` and contextual initializer queries.

The candidate uses that selector in `assignreport.rs` and `contextual.rs`,
without changing its direct-versus-statement precedence or recovery behavior.
`symbols.rs` exports it only within the checker crate. Each private checker
receives borrowed immutable document tables before checking. The helper uses
the CLI's unchanged eligibility rule and complete Program file indices for
checked-file metadata and diagnostic source indexing, including libraries.

Exact source/binary/input identities, corpus transitions, process outcomes and
artifact hashes are in [the receipts](jsdoc-cli-fidelity.json). These are
correctness and locating controls. They do not qualify the <=0.50 native wall
ratio, complete resolver inputs, all lazy forcing or production workers.

Final integration is frozen at base `85cf4c50f8504f7c28d551aacbcd7365e367fb2e`
plus [this six-file candidate diff](jsdoc-cli-candidate.patch), preserving the
concurrent alias-naming/checker ports. Its unfiltered comparison gains 21
WRONG→RIGHT type assertions and one diagnostic case with no prior pass losses.
`checker_types` is 464,090/478,855 assertion lines (96.92%) and 7,365/9,538
complete cases (77.22%); diagnostics are 2,881/5,488 (52.50%). Two already-WRONG
diagnostic payloads change. These percentages describe different denominators.

## Whole-project failure found before retention

The transported initializer/context candidate at base `d74b5402` completes
comparisons over 475,538 aligned type rows and 10,570 diagnostic cases without
losing a previous pass. It gains 21 type assertions and one diagnostic case. That does
not establish real-project readiness: Next.js fails the original preflight
with a 180-second supervisor limit. That helper did not save the failed child,
so its exact outcome and duration are unavailable. An independent supervised
control is killed at 60.059 seconds with 39.605 seconds of user CPU. Both failed
invocations are rejected. The shared helper now persists show-config, preflight
and sample child receipts before validation; failure controls verify that no
completed summary is written.

An independently killed 45-second locating trace has a partial last JSON record
and no completion. Its complete prefix contains 53 returned source workers;
it is useful for locating, never equivalent-work or throughput evidence.

`get_type_from_type_reference` asks whether every zero-argument symbol is a
visible JSDoc alias. `jsdoc_alias_doc` previously iterated every supplied
document's tags even for ordinary TypeScript declarations. The missing tables
had concealed that work. Native's visibility switch selects typedef/callback
declaration kinds before following structural parents
(`internal/checker/emitresolver.go:128–135`, pin `5b1047d`).

An exact typedef-declaration discriminator now precedes the document scan.
The existing search matches a `JSDocTypedefTag` by its declaration node ID;
an ordinary declaration cannot match that association in the Program's node
domain. Genuine typedefs keep the same lookup, hosting and visibility behavior.
No document is deleted and no generic semantic cache is introduced.

The guarded candidate completes Next.js with the same 117 diagnostics,
14,050 loaded files, 1,397 reported checks, 14,746 parses, effective options
and ordered physical source bytes. Raw bundled-library names differ between
archive directories. The comparison qualifies only those known vendor prefixes,
while requiring identical realpaths, symlink targets and hashes; all project
paths and ordering remain exact. Individual timings overlap quality work and
are not performance samples. New setup maps and private retention need fresh
attribution under `tsr-1yb.9.2.2`.

## Rejected broader name-resolution change

A separate hosted lexical fallback at frozen `d4906082` recovers three of the
four native mixed-fixture errors, but loses 15 previously RIGHT type assertions
and a previously clean diagnostic case. It is rejected. The initializer-only
arm has no type losses but introduces a false tuple-array initializer error;
supplying the contextual annotation removes that regression.

The guarded candidate still reports zero errors on the stronger mixed fixture
where native reports four: generic identity, typedef member, overload with its
related message, and imported member assignment. Parent `tsr-6.65`, hosted
lexical ownership `.3`, and remaining initializer/reporting obligations stay
open. The diagnostic corpus also retains two already-WRONG cases: JSON missing
properties report TS2322 instead of the native TS2741, and a subclass case still
misses other errors. No fixture filtering or printed-name shortcut is accepted.

## Reproduce the public controls

Archive the receipt's exact source and apply its candidate diff, retaining
pinned vendor `5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Build baseline/candidate
release CLIs, the candidate `checker_workers` example and ordinary pinned tsgo
before running [the physical controls](jsdoc-cli-controls.py):

```sh
rtk proxy python3 docs/architecture/jsdoc-cli-controls.py \
  --baseline /absolute/baseline-target/release/tsr \
  --candidate /absolute/candidate-target/release/tsr \
  --helper /absolute/candidate-target/release/examples/checker_workers \
  --native /absolute/pinned-tsgo --output /tmp/jsdoc-cli-controls
```

Five projects exercise checked JS, `checkJs=false`, `noCheck`, a file no-check
directive and `skipLibCheck=false` libraries. Default/single TSR, native
default/single/2/4, and private 1/2/4 helpers must agree on complete diagnostics.
Checked/library projects report two expected errors; excluded modes report
zero. The tuple positive must remain clean. Helper identities are unique and
their executed worker totals agree with the CLI's reported check count.
This is helper validation, not proof of native admission or all semantic work.

Full unfiltered `verdictdump` and `diagverdictdump` gates compare exact row/case
identities and preserve multiline type payloads. Type totals must agree with
the producer summary; absent previous RIGHT rows cannot disappear from the
comparison. An initial archive with empty vendor directories produced zero
rows and was rejected before attaching the pinned checkout and using fresh
output. Release checker/execute/ownership tests, work-trace controls, strict
workspace Clippy, trace-enabled Clippy and formatting cover the candidate.
The three simplification lenses ran inline under repository instructions;
manual review is scoped to the candidate. Independent reviewer coverage is zero.
