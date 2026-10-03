# Recursive relation proof publication

This records correctness prerequisites `tsr-6.51` and `tsr-6.52`, plus
bounded execution experiments under `tsr-1yb.15`. Relation-cache contract
`tsr-1yb.4.1.3` remains unfinished. The comparison starts at TSR
`738b1a797a65c62a418da34d27daad591b1599de`, against pinned tsgo
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

## The failed proof

The regression in `tests/relater.rs` compares mutually recursive `A/B` with
`C/D`. Their `value` properties conflict. A third type, `E`, has a matching
`value` but refers to `D` and therefore also fails against `A`.

Previously, `A -> C` parked a successful entry before recursing. `B -> D`
re-entered that entry and was recorded as a completed success. The later
`value` mismatch rejected `A -> C`, but left `B -> D` successful. The next
`A -> E` branch reused it and incorrectly accepted the union assignment.
Both `C | E` and `E | C` demonstrate this failure. Pinned tsgo rejects them
with TS2322; the previous TSR binary accepted them without diagnostics.

## Native publication and the port boundary

Native `internal/checker/relater.go::recursiveTypeRelatedTo` and
`resetMaybeStack` separate completed `Relation.results` from active
`maybeKeys`/`maybeKeysSet`. Re-entry returns `TernaryMaybe`. A conjunction
retains that state until an enclosing proof either succeeds or fails.
An independently true proof, or a successful recursive proof at depth zero,
publishes its dependent keys together. A failed branch discards its
assumptions; a failure under assumptions can be cached as a failure.

TSR now carries this distinction in a private `RelationResult`. Completed
answers and recursive assumptions have separate storage. Member, signature,
tuple, variance and union/intersection composition preserve `Maybe`, rather
than turning it into an independent success. Public `Ternary` remains the
existing three-state API.

The public port's `Unknown` covers unsupported work, depth refusal and an
uncomputed circular variance comparison. Native's `TernaryUnknown` describes
circular variance, rather than an unimplemented arm. TSR retains its existing
Kleene policy and discards an uncomputed scope without publishing it. Native
retains nested circular-variance keys with reliability flags; porting that
distinction and its reuse obligations remains part of the relation contract.
The depth-refusal regression checks that such a branch cannot supply a
successful recursive child to a later union branch.

This change retains per-top-level-walk result ownership. It does not establish
that ordered TypeId pairs suffice for checker-wide reuse. Native generic
equivalence keys, intersection context, diagnostic/reliability flags and
metadata completion remain obligations of `tsr-1yb.4.1.3`.

## Active variance and execution order

The correct eager proof-publication candidate exposed repeated structural
work: five alternating real-app pairs measured 4.857 seconds on the old
checker versus 11.040 seconds on the corrected candidate. It was not shipped.
A bounded CPU sample placed 1,486 of 2,316 checker samples beneath variance
measurement. Later signature/property/composite counters found 359,262 worker
executions, including 316,980 repeated pair executions within their own
relation walks. One `ResultAsync<__varianceSub, E>` to
`ResultAsync<__varianceSuper, E>` query accounted for 315,868 executions over
929 distinct pairs.

Native `getVariancesWorker` returns an empty slice for a recursively active
generic target. `structuredTypeRelatedToWorker` (`relater.go:3834`) stops with
`TernaryUnknown`. TSR already returned an empty vector from
`inference_variances` for that active target, but fell back to its members.
The fix stops only the active target with nonempty type arguments. It does
not cache the uncomputed result or treat an unsupported target as successful.
The regression fails without the rule; after measurement ends, it also checks
that real rejection and directional literal covariance recover.

Two isolated counter runs with the rule agreed on 43,395 worker executions,
2,041 repeated pair executions and 12 active-target refusals. These counts
compare two experimental variants that both included the later rejected
property exits; they are not final production counts. Temporary probes were
archived and production files restored byte-for-byte. No instrumentation or
process cache ships.

Signature comparison now follows native's direction order, skips an unneeded
second bivariant comparison, and stops after a failed `this` or parameter.
Union/intersection branches are evaluated lazily, preserving constituent order
and the port's policy that unsupported work must not hide a later decisive
branch.

Property exits were rejected: two previously RIGHT rows in
`arrayDestructuringInSwitch1` lost a callable union constituent at
`operands.every` and `every`. Saved binaries isolate the losses to property
stopping; signature stopping alone preserves them. Property traversal was
restored, and `tsr-6.53` tracks the missing member-forcing or identity behavior
before retrying. There is no fixture-specific exception.

## Verification

The original regression fails before the fix with `Related` instead of
`NotRelated`. Afterward, all 74 relation tests pass, including valid recursive
pairs, a valid later union branch, branch ordering and depth refusal. An
overload regression also selects native's fallback for both invalid unions
and the concrete overload for a valid recursive union. Pinned native
declaration output independently confirms these three selections.

The checker/execute release suite passes 1,298 tests with four existing ignored
tests. All four structured overload tests pass. Checker/execute library and
test Clippy, formatting and diff checks pass. Local review covered the public
API projection, proof promotion/discard scopes, native comparison order,
active-target guard and regression fixtures.

The unfiltered before/after corpus has 474,251 rows, byte-for-byte identical:
459,451 RIGHT, 2,195 GAP and 12,605 WRONG. There are no previously RIGHT losses.
The real Next.js workload retains all 123 complete diagnostics and the same
counts: 13,097 loaded, 1,341 checked and 13,560 parsed files.

The negative CLI controls match native's rejection, code and location. TSR
still omits native's nested `next.next.value` diagnostic elaboration; complete
diagnostic fingerprints therefore differ on these controls. This is not a
claim of complete diagnostic parity or the required 2x throughput.

The retained variant excludes early property exits. Its full corpus, CLI
controls, gates and source/binary hashes are preserved in
`/tmp/tsr-relation-no-property-{corpus,controls,gates,source}.json`. The corpus TSV
fingerprint is
`f01585f8cafc7fd2fc198b22350c097d1baaa0a6037421369613895e34db55fb`.

The retained CLI SHA-256 is
`34e81bb8eb340a985ddd0deff9ddbb0774355ba5473ca05d7543d2d62a4c812e`.
Counter and bisect artifacts are `/tmp/tsr-relation-counter-results.json`,
`/tmp/tsr-relation-counter-hot-results.json`,
`/tmp/tsr-relation-active-counter-results.json`, and
`/tmp/tsr-active-variance-bisect.json`.

## Whole-project confirmation

Each row below is five alternating fresh-process pairs on the same real app.
The second run is an independent confirmation. Builds and quality gates ended
before timing; incremental/composite reuse was disabled and the filesystem
was warmed. The unchanged keep threshold was 20 milliseconds.

| Comparison | First-run median | Confirmation median |
| --- | --- | --- |
| Corrected eager proof → retained variant | 10.763 → 4.808 s | 10.782 → 4.685 s |
| Previous main → retained variant | 4.836 → 4.747 s | 4.877 → 4.769 s |

The complete package improves main by 89 and 108 milliseconds (1.84% and
2.21%). Main-comparison user CPU medians decrease from 3.752 to 3.636 seconds
and from 3.772 to 3.658 seconds. Median peak RSS changes from 1.124 to 1.097 GB
and from 1.123 to 1.085 GB. This does not attribute a separate timing gain to
each signature/composite edit.

All samples preserve the 123 complete diagnostic fingerprint
`cdb777a1930fee9c86ab978511539234e820c77a69a4586d522409488e074074`,
13,097 loaded, 1,341 checked and 13,560 parsed files. Before/after loaded
identities hash to
`7ffb6eb6de272445848f5afd7442ed0f50fc9517a9a8ceb1017467fa108015d4`;
loaded input contents hash to
`395a32b083501c1cac01a17c6585385849f997fc10a3faab37b2f3d2d7bd9b16`.
Configuration and lockfile hashes also remain stable. Raw individual samples,
CPU/RSS, binary hashes and scope checks are in
`/tmp/tsr-relation-no-property-paired.json`.

The full-project TSR/pinned-tsgo median wall target remains ≤0.50 on equivalent
semantic work. These are TSR before/after comparisons; they do not establish
the required native ratio or erase the remaining diagnostic-parity gaps.
