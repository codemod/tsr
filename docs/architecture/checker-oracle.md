# The checker's oracle (`checker_types`, `diagnostics`)

**Status:** the instruments exist; the subject does not.

```
checker_types    0/9538    0.00%   the type of every expression
                 0/478954  0.00%   ...and the gradient underneath it
diagnostics      80/5488   1.46%   every diagnostic, by code and position
```

The second `checker_types` row is the **per-assertion-line tally**
([ADR-0031](../adr/0031-a-gradient-beside-the-gate.md)): the 9,538 judged cases
carry 478,954 assertion lines between them, and how many of those match is
reported beside the case rate and never in place of it. The case rate is the
gate; a case passes only if all of its lines do, so the gradient is always the
more forgiving number. It exists because a binary gate over 60,269 lines of
upstream gives one bit of feedback per case and reads 0% for months.

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.

## Why this was built before the checker

`internal/checker` is 60,269 lines — a third of the core port, and 120–200
sessions by PLAN.md's estimate. Starting that without a gate means months of work
before the first honest number.

Nothing in this repository currently measures a type. `binder_symbols` reads
98.03% and that is *symbol tables* — name resolution, declaration merging, scope
walking. A checker can be arbitrarily wrong while every symbol resolves.

This repository keeps relearning that the instrument comes first:
`dts_reachable_target` sized the emitter's target before an emitter existed, and
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) committed to a
falsifier that could fire on the measurement alone. The same applies here at ten
times the scale.

## The target

Measured at the pin across `compiler/` and `conformance/`:

| | |
|---|---|
| `.types` baselines | **12,155** |
| positioned type assertions in them | **594,122** |
| cases this suite judges | **9,538** |
| assertions in the cases it judges | **478,954** |

The last row is new (2026-08-05) and is the gradient's denominator. It is smaller
than 594,122 for exactly the reason the case count is smaller than 12,155 — the
skips below — and the two shrink together, because a skipped case is out of both
denominators. Nothing was corrected here; the figure had simply never been taken.

(The `diagnostics` figure quoted above moved from 54/5,488 to 80/5,488 between
2026-08-05 sessions, as binder over-report fixes landed. That is the number
changing, not a number having been wrong.)

Upstream writes `.types` and `.symbols` in lockstep — 12,155 of each, at the same
positions. That pairing is worth knowing when the checker lands: `.symbols` tests
*resolution* and `.types` tests *inference*, so a failure in one localises against
the other.

The 12,155 baselines and 9,538 judged cases differ for the reasons the suite
prints, and the difference is files-versus-cases as much as exclusions — the same
reconciliation as the `.d.ts` denominator:

```
 1397  configuration-varied baseline (bd tsr-bb4.1)
  924  upstream recorded no .types baseline
  450  upstream records a known divergence from TypeScript
  135  the .types baseline has no assertions
```

## What a pass will mean

Every `>expression : type` line reproduced **verbatim, in order, for every file of
the case**.

Whole-line, not expression-and-type separately. A line is
`>{expression} : {type}` and **the expression can contain `" : "`** — a
conditional does:

```text
>Math.random() > 0.5 ? "abc" : "def" : "abc" | "def"
```

773 assertion lines in `compiler/` alone carry two or more, so there is no
unambiguous split. Rendering the line ourselves and comparing strings sidesteps
that and is the stricter check. `TypeAssertion::split` exists only to make a
failure readable, and its test demonstrates it getting that line wrong.

The first draft of this claimed the ambiguity came from type annotations and used
`>function (this: any) { } : (this: any) => void`. That is not ambiguous —
`this: any` has no space before the colon, so it never looks like the separator.
The unit test asserting the wrong split failed, which is the only reason the real
shape was looked up. Both forms are now tested.

## The suite reports 0%, deliberately

[conventions.md](../conventions.md): *a conformance suite whose subject does not
exist reports 0%, loudly, with the reason stated. It does not skip, and it does
not omit the row.* Every judged case comes back `Unsupported { "no checker
(bd tsr-4sc)" }`, so the row is in the summary table from today and the number
moves the first time a type is computed.

## A skip reason that was wrong in two suites

`checker_types` first checked for a plain `.types` file before checking whether
the case was configuration-varied. A varied case has **no** plain baseline — only
`case(target=es5).types` — so all 1,397 of them landed in the "no baseline" bucket:
the right exclusion under a label that said something else, which is how a skip
count stops meaning anything.

`binder_symbols` had the identical flaw and had had it since it was written. It
was never checking for varied `.symbols` at all.

**Its pass rate was not affected**, and that was checked rather than assumed: no
case in the corpus has both a plain and a varied baseline, so no case was ever
judged against an arbitrary configuration. 8,278/8,449 before the fix and after.
(The denominator has since moved to 8,457 as previously unparseable units entered the
suite; see ADR-0026 and ADR-0027.)
What was wrong was the breakdown — 2,321 cases reported as "upstream recorded no
baseline" when 1,397 of them were varied, which overstates how much of the corpus
upstream never ran.

Both suites now test varied first, and their skip lists are directly comparable.

## The diagnostics half

`.types` gates the types a checker computes; `.errors.txt` gates the errors it
reports, and that is the larger half — the baseline is what a user sees. Before
this, the only diagnostic comparison was `isolated_declarations`, filtered to the
`9000..9100` range: 20 codes out of TypeScript's ~1,600.

A pass is the **exact multiset of (file, line, column, code)**. Not a subset and
not "the codes we know about": a diagnostic we invent fails a case as surely as
one we miss.

Only cases expecting **at least one** diagnostic are judged. Reproducing a clean
file by reporting nothing is real conformance, but it is already measured by
`parser_typescript` and `scanner_clean_files`, and folding those 5,082 cases in
here would bury the number that matters — of the diagnostics upstream reports, how
many do we? The false-positive direction survives anyway: a case expecting three
and getting four fails.

### It found two defect classes on its first run

0.98% is the checker's absence, and expected. What was not expected: **865 of the
5,434 failures are cases where this port emits a diagnostic upstream does not.**
Those are unblocked by the checker and ours to fix today.

| code | cases | |
|---|---:|---|
| `TS2300` duplicate identifier | 395 | **a binder diagnostic** (`bd tsr-y4u.18`) |
| `TS1005`, `TS1003`, `TS1012`, `TS1109`, `TS1125` | ~470 | parser codes (`bd tsr-pum.11`) |

Both were invisible to the suites that ostensibly cover those components, and for
structural reasons rather than by oversight:

- `binder_symbols` (98.03%) compares symbol **resolution** against `.symbols`. A
  binder can resolve every name correctly and still invent 395 duplicate-identifier
  errors, because it never raises them into that comparison.
- `parser_typescript` (99.38%) judges whether a file parses **cleanly** and skips
  the 7,413 cases that legitimately contain parse errors — so *which* errors we
  report in exactly the cases designed to produce errors was never compared.

That is the argument for this suite in one paragraph: a component-shaped gate
measures the component's model, and only an output-shaped gate measures what
ships.

## Three things to know before trusting either number

Learned 2026-08-05, while using both suites to fix five binder defects. All three
are properties of the instruments, not of the compiler.

**1. `checker_types`'s judging path has been proved, 2026-08-05 — before it was
used.** The rest of this note is what that took, and it is left in place because
the *reason* for doing it applies to the next suite as much as it did to this one.

The problem as it stood: all 9,538 cases were classified `Unsupported`, so the
comparison code had run zero times. Worse than untested — there was no comparison
code at all, only the `Unsupported` return. The `.types` baseline *parser* had four
unit tests; the suite consuming it had none. A suite that has only ever reported 0%
is as unproven as one that reads 100% on its first run — the failure this project
already hit with `file_loader`, which read 76/76 until four deliberate mutations were
applied to the code under test.

So `types_suite::compare` was written and proved before anything measured with it.
Eight tests, and each was checked against a **deliberately weakened judge** rather
than assumed to bite. The seven mutations and what each turned red:

| mutation of `compare` | tests that went red |
|---|---|
| sort both sides — multiset, not positional | `the_right_types_in_the_wrong_order_fail` |
| drop the assertion-count check | `a_missing_assertion_fails`, `an_extra_assertion_fails…` |
| drop the file-name check | `output_for_the_wrong_file_fails…` |
| stop after the first file | `a_multi_file_case_fails_when_only_its_second_file_is_wrong` |
| count lines, do not compare their text | `a_wrong_type_with_the_right_count_fails`, `the_right_types_in_the_wrong_order_fail`, `a_multi_file_case…` |
| take the denominator from our output, not the baseline | `a_missing_assertion_fails`, `an_extra_assertion_fails…`, `producing_nothing_fails_and_the_denominator_survives` |
| never match a line — the over-strict direction | 6 of 8, including `identical_output_passes_and_the_tally_is_full` |

Every mutation turned at least one test red, **and every test was turned red by at
least one mutation** — including the positive control, which the seventh mutation
exists to check. A control that no mutation can break is the no-op test this project
has already shipped once (see [checker.md](checker.md) on the memo counter).

Two of these earn their place beyond ceremony. The multiset mutation covers a real
defect class — every right type attached to the wrong expression — that a
set-based comparison would pass. And the denominator mutation is the one that
matters for [ADR-0031](../adr/0031-a-gradient-beside-the-gate.md): taking `total`
from our own output makes a checker that produces nothing read **100%**, which is
the most dangerous possible failure of a gradient.

What is still unproven is the *producer*: nothing yet renders our checker's types
in baseline form, so `compare` is exercised only by its unit tests and by cases
where our side is empty. When the producer lands, the same discipline applies to
it — the first non-zero number is not evidence until something deliberately wrong
has been fed through the whole path.

**2. `diagnostics` is capped at 90.9%, permanently, until the parser is fixed.** 500
of its 5,488 judged cases carry a diagnostic *we* emit and upstream does not — 2,675
parser/scanner over-reports and 126 binder ones, measured by
`examples/over_reports.rs`. The checker cannot remove those: a false positive from an
earlier stage sits underneath the checker's output. So a rising number will look
better than it is, and ~9 points of it are unreachable. `bd tsr-pum.11`.

**3. `binder_symbols` is a weak instrument for symbol-table defects.** It sat at
8,278 through three separate defects that merged unrelated declarations into a single
symbol — a class's type parameters, static versus instance members, and block-scoped
declarations. Its `.symbols` baselines do not distinguish those. **A flat
`binder_symbols` is not evidence that a symbol-table change is safe.** What caught all
three was a diagnostic they happened to produce, bucketed by declaring construct
(`examples/ts2300_constructs.rs`); what pins them now is unit tests in
`crates/tsr-binder/tests/bind.rs`, each verified to fail with its fix reverted. See
[binder.md](binder.md).

## What is still missing for Phase 4

- **Per-configuration runs** (`bd tsr-bb4.1`), which would return 1,397 cases to
  `checker_types` and 793 to `diagnostics`.
- **Message text.** Both suites compare codes and positions, never the rendered
  message. Two diagnostics with the same code and different arguments are equal
  here, and `.errors.txt` records the full text. That is a real gap and a
  deliberate one: the localised message tables are `bd tsr-5e7.6`.
