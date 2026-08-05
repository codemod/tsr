# ADR-0031: A gradient beside the gate, not instead of it

**Status:** accepted, 2026-08-05
**Context:** `bd tsr-4sc`, `bd tsr-4sc.2`
**Supersedes:** nothing. Extends the measurement discipline in
[conventions.md](../conventions.md) and [ADR-0021](0021-isolated-declarations-is-not-a-port.md).

## The forcing constraint

`checker_types` judges 9,538 cases and its verdict is binary: a case passes only
if **every** `>expression : type` line of **every** file matches verbatim, in
order. That strictness is correct and is not in question here.

What it costs is feedback. `internal/checker` is 60,269 lines and PLAN.md
estimates 120–200 sessions for it. A binary per-case gate over a component that
size yields **one bit per case**, and it will read `0/9538` for months — the
whole of that time producing an unchanged number while the work is going well or
badly, indistinguishably.

PLAN.md §4 already recorded this failure mode and rejected the usual answer:

> defer to a final tuning phase

Measured at the pin, the 9,538 judged cases carry **478,954 assertion lines**
between them (594,122 across all 12,155 baselines, before the 2,906 skips). That
is a signal fifty times finer than the gate, sitting unused inside the same
files the gate already reads.

The precedent is in this repository: `dts_reachable_target` sized the emitter's
target before the emitter existed. This is the same move a level down — sizing
the checker's target *within* each case rather than across cases.

## The decision

`Suite` gains one defaulted method:

```rust
fn judge(&self, case: &CaseEntry) -> Judgement {
    Judgement { outcome: self.run(case), lines: None }
}
```

A `Judgement` is a verdict plus an optional `LineTally { matched, total }`. The
tally is summed across cases into `SuiteResult::lines` and printed as a
`lines` column in the summary and an `Assertion lines:` row in the snapshot —
**under** the pass rate, and labelled *"a gradient, not a pass rate"* in the
snapshot text itself, because a snapshot gets read out of context.

The case rate remains the gate. The two can never disagree about what correct
means, because the gradient counts the very lines the gate requires: a case
passes iff all of its lines match.

## The alternatives, taken seriously

**A new `Outcome` variant** carrying the tally. Rejected on a specific ground,
not on blast radius: the tally has to attach to `Outcome::Unsupported` as well
as to `Passed`/`Failed`. A case the checker cannot yet run still owes a
denominator — 478,954 lines are owed *today*, when nothing runs — and a variant
that only some verdicts can carry would zero the denominator precisely while the
number is `0/N`, which is the only period the gradient exists for. That is a
correctness objection to the shape, and it stands independently of cost.

The blast radius was also measured rather than assumed, since it was the reason
to hesitate: `Outcome::` is constructed in 11 files but **matched in exactly
one** (`suite.rs:121`), so a variant would have been cheaper than it looks. It
was still the wrong shape.

**A second `Suite`.** Rejected because a `Suite` yields one `Outcome` per case
and cannot express a ratio at all. "Cases where every line matched" is the gate
again under a second name.

**A separate binary**, as `examples/over_reports.rs` does for the diagnostics
over-report analysis. Rejected because a number nobody runs is a number nobody
reads: the gradient's whole purpose is to move in the same output, on the same
run, as the gate it sits beside.

## The consequences accepted

- **The tally is positional, so it is a strict lower bound.** A missing
  assertion in the middle of a file costs every line after it, because position
  *i* of ours is compared against position *i* of the baseline. A
  best-alignment measure (LCS) would read higher and be kinder. Positional was
  chosen because it can understate progress and never overstate it, and because
  it is the same rule the gate uses — a gradient computed by a different rule
  than the gate is a second definition of correct.
- **A full tally does not imply a pass, and must not be read as one.** Produce
  every expected line plus one extra and `matched == total` while the case is
  wrong. The gate is `Comparison::mismatch`, never `matched == total`; there is
  a test named for exactly this
  (`an_extra_assertion_fails_even_though_every_expected_line_matched`).
- **One extra number to misquote.** The mitigation is in the output rather than
  in convention: the snapshot row carries its own disclaimer, and the summary
  column is headed `lines` rather than anything resembling a rate.
- Suites with no gradient print `—` and their snapshots are byte-unchanged.
  Verified: of 16 snapshots, exactly one line in one file moved.

## How we would know this was wrong

- **If the gradient and the gate ever tell different stories about the same
  change.** They are computed from one comparison over one population — skips
  are excluded from both — so a divergence means a bug in the accounting, not an
  insight. Specifically: `matched` can never exceed `total`, and a suite whose
  case rate is 100% must read 100% on lines.
- **If the gradient climbs while the gate does not, for many sessions.** That
  would mean the checker is getting a growing fraction of lines right in cases it
  still fails, i.e. the remaining errors are spread thin across cases rather than
  concentrated. That is real information, not a falsifier — but if it persists
  past the point where the case rate should have started moving, the positional
  rule is probably masking a systematic offset (one missing assertion early in
  every file), and the tally should be re-derived with an alignment before
  anyone concludes the checker is nearly there.
- **If it is quoted as a pass rate in this repository's own documents.** Then
  the labelling failed and the column should be removed rather than explained.
