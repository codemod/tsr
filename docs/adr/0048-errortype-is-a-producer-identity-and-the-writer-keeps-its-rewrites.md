# ADR-0048: Upstream's `errorType` is a producer identity; the writer keeps its gap rewrites for now

- **Status:** Accepted. Built: the split (`Intrinsics::native_error`,
  `Checker::is_error`/`included_error`, `types_producer::render`), the
  `ceiling` instrument, and five `checkIdentifier` producers. **Not built:**
  the remaining producers (held as diffs, listed below) and narrowing the
  writer's rewrites (a decision left open, below).
- **Date:** 2026-10-08
- **Issue:** `bd tsr-2zk.944` (step 1), tsr-2zk.960 (step 2)
- **Supersedes:** [ADR-0038](0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md).
  [ADR-0039](0039-the-any-that-upstream-prints-is-the-baseline-writers-decision.md)
  already corrected 0038's reasoning; its finding (the `any` is the baseline
  writer's decision) is what this record builds on, and it stands.
- **Measurements:** [`docs/parity/notes/r4-errorsplit.md`](../parity/notes/r4-errorsplit.md)
  (step 1), [`docs/parity/notes/r5-errorsplit2.md`](../parity/notes/r5-errorsplit2.md)
  (step 2).

Step 1's code and notes cited this record as "ADR-0047" before it existed;
that number belongs to r4-variants. Corrected in the error-split owned files;
see the r5 notes.

## The forcing constraint

ADR-0038 made one intrinsic serve two meanings: upstream's `errorType` (a
type upstream *computes*: an unresolved name, a TS2563 bail, an assignment
to a constant) and this port's gap (the port computed nothing). It printed
both `error` so that a gap stayed distinguishable from a wrong answer, and
accepted a ceiling for the lines where upstream's own `errorType` prints
`any`.

Two things that record could not see are now measured:

1. **The ceiling it accepted barely exists any more.** ADR-0039 moved the
   `any` rendering to where upstream decides it, the baseline writer. At
   the frozen base (`ac56208`) the `any`-vs-`error` bucket is **36 lines**
   (ADR-0038 measured 35,508 at its time).
2. **The false credit it refused exists anyway.** The writer's rewrites
   (SS180 `hadErrorBaseline` and the positional guards) print the *gap*
   `any` as well as upstream's `errorType`. Counted by identity with the
   step-1 split: **4,644 matched lines at the base whose top-level type is
   the gap** — the population ADR-0038 refused to create, created by the
   ADR-0039 rewrites and invisible until the identities were split.

So the question is no longer "render `errorType` as `any` or not" but "which
identity does each producer answer", because the writer's rewrite should
apply to upstream's identity and not to the port's.

## The decision

1. **Two intrinsics.** `Intrinsics::error` stays the port's gap (the
   579 existing uses keep their meaning with a zero-line diff);
   `Intrinsics::native_error` is upstream's `errorType` (`checker.go:979`):
   `TypeFlagsAny`, printed `any` by the checker, `error` by the writer's
   intrinsic-name fast path (`types_producer::render`).
2. **A producer answers `native_error` only where native answers
   `errorType`, by identity, verified against a native build.** Each switch
   cites the `checker.go` line and was confirmed with a probe on
   `GetErrorType()` identity (r5 notes §1, §6). Built in step 2:
   `checkIdentifier`'s §475 type-parameter arm (`:11048`), its
   non-variable-assignment arm (`:11094`) and its readonly-assignment arm
   (`:11102`), plus upstream's `requireSymbol` (`nameresolver.go:322`,
   `anyType`) — found as the first loss of the wider switch. Measured: +1
   type line, zero losses on both dumps; credited gap 4,644 → 4,494,
   `native_error` lines 0 → 171, all matched.
3. **The writer's guards treat both identities alike**, since they are
   upstream's conditions on an any-flagged type; only the gap sets the
   "port failed" flag.
4. **A producer whose switch exposes a consumer outside the owning lane is
   held as a measured diff, not landed.** Held: the empty-name arm (diff A,
   zero losses with its one `signatures.rs` consumer fixed); the
   final-`else` unresolved-name arm (diff B, one residual type loss after
   `members.rs` and `array_literals.rs` fixes); TS2563 in `flow.rs` (r5 notes
   §7); P4 in `symbols.rs` (r5 notes §6, §7). P11 and P12 are **not**
   `errorType` natively (the unresolved-reference type, printed by name) and
   must not switch.
5. **The writer's gap->`any` rewrites are not narrowed in this step.**

## The alternatives, taken seriously

**Rename: `error` becomes `gap`, `errorType` takes the old name.** Tidier.
It rewrites 579 lines across 40 files, most of them in hub files every lane
edits, in one commit. Rejected for the merge cost, not on the merits; it is
the right spelling once the producers have moved and the remaining `error`
uses are few.

**Narrow the rewrites now** (print the gap `error` everywhere, keep the
rewrites for `native_error` only). This is the faithful end state: the
rewrites are upstream's, and upstream never applies them to a type it did not
compute. Measured at step 2's commit: **5,063 RIGHT→GAP lines in 1,028
cases** (SS180 3,531; the position guards 1,119; statement names 312;
access/qualified parents 78; `declare global` 23), plus 1,488 WRONG→GAP. It
fails the zero-loss gate by construction: every one of those lines is
currently matched only because the port's gap prints like upstream's
`errorType`. It is therefore a gate decision, not a box's, and the decision
is the integrator's:

- *Narrow now*, accept −5,063 on `checker_types`, and get a gradient whose
  every matched line was computed. The 5,063 then come back one producer at
  a time as each moves to `native_error`, each counted honestly.
- *Narrow per producer*: keep the rewrites; as each producer switches, its
  lines leave the table (r5 notes §4's producer rows) and the residual
  falls. The gradient stays inflated by the residual until the end.

The producer table says where the residual sits: 870 lines are unresolved
references the §31 gate keeps as the gap, 544 are aliases to modules this
port does not type (P4), and the rest is spread thin. The first two are
diff B and the P4 diff, so *per producer* has a short path for about a
third of the population and a long tail for the rest.

**Make `Checker::is_error` mean the gap only.** Most of its 201 call sites
mean "decline, nothing was computed", which is wrong for `native_error`.
Measured with diff B: type losses 1 → 0 but diagnostics losses 0 → 2, because
`check.rs`' TS2564 arm and `index_access_reports.rs`' TS2538 arm use it as
upstream's flag test. Neither definition is right at every site; it stays
step 1's (both identities), and auditing the call sites is diff B's
precondition.

## Consequences accepted

- The gradient keeps crediting 4,494 gap lines (3,944 of them through a
  writer rewrite; 550 because the baseline itself prints `error`) until the
  rewrites are narrowed. `ceiling` prints the number on every run, so it can
  no longer be mistaken for computed work.
- Two producers that native answers `errorType` for (the empty name and the
  final `else`) still answer the `any` stand-in, with comments saying so.
- `is_error` answers true for an identity that most of its callers treat as
  "nothing computed". It is unreachable from the switched arms' consumers
  at commit time (zero losses), but every new switch must be measured on
  both dumps.

## How we would know this was wrong

- If a switched producer's lines turn out to be `anyType` natively, the
  native-identity rule is being applied by reading code rather than by the
  probe. The `require` case is the example that came close.
- If narrowing per producer stalls, with the residual in r5 notes §4 not
  falling across rounds, the inflation becomes permanent and *narrow now*
  wins.
- If the `is_error` audit finds the decline sites are a small minority, the
  right fix is to give upstream's `isErrorType` its own helper and let
  `is_error` mean the gap, the opposite of what step 1 chose.
