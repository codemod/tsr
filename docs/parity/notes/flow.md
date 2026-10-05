# Lane `flow` notes (tsr-2zk.7)

Control-flow narrowing, definite assignment, truthiness and comparison overlap.
Baseline for every number below: `06f25e0` lane lists, measured on the box
branch head `0d996e8` with `diagverdictdump` (3,427 RIGHT diagnostics cases).

## 1. TS2367 asks the comparable relation

`crates/tsr-checker/src/comparison_overlap.rs` substituted assignability for
`isTypeComparableTo` and carried three declines to keep that substitution from
over-reporting: a union/intersection veto (`either_is_composite`), the enum veto
inside `pair_is_reportable`, and a syntactic "an earlier `if` in this block
already tested this name" test (`narrowed_away_by_an_earlier_test`).

**Forcing fact.** `Relation::Comparable` has existed since
`checker-notes-diag2.md` §750 and TS2352 already uses it. The substitution's
own premise (no comparable relation) had gone stale.

**What changed.** The rule now asks `relate_ternary(_, _, Comparable)` in both
directions, as `isTypeEqualityComparableTo` (`checker.go:12861`) is written, and
prints `getBaseTypesIfUnrelated`'s pair (`checker.go:12745`). The error-type
gate stays (`assignability_pair_is_reportable`: an unbuilt type is `any`
upstream). `Unknown` in either direction is still silence.

**Measured.** Corpus-wide TS2367/TS2678 lines: missing 141 → 72, extra 175 → 0.
Twelve cases converted, no verdict lost. Each decline was then removed one at a
time; removing the narrowing test changed **no** case's output (byte-identical
dump), so it was deleted rather than kept as dead insurance — the operands'
flow types already carry the narrowing it approximated.

**Would be wrong if** a future TS2367 extra traces to a comparable-relation
`NotRelated` that upstream answers `Related`. That is a relater bug (relate
lane), not a reason to restore a decline here.

**Remaining TS2367 misses** are type-parameter pairs
(`comparisonOperatorWithTypeParameter`, `…NoRelationshipTypeParameter`,
`compareTypeParameterConstrainedByLiteralToLiteral`): `T === U` with both
unconstrained. Upstream's comparable relation answers `False`; this port's
relater does not reach a confident `NotRelated` for the pair (the arm that
declines is not yet pinned down). Owned by `relater.rs` — reported, not changed.

**Perf.** `domain-model` 1.018 (9 samples). `generic-imports` read 1.045/1.065
at 21 samples, but the project contains no `==`/`!=`/`===`/`!==` at all, the
baseline binary against itself reads 1.027 in the same slot arrangement, and
the swapped arrangement (old in the `--tsr` slot, new in `--tsgo`) reads 0.998.
The excess is slot bias on this box, not the change.

## 2. TS2678 is `checkSwitchStatement`'s comparable arm, not a class test

`check.rs::check_switch_case_comparable` reported TS2678 only for a switch on
an intrinsic primitive with a `case` naming a class (§414), with an empty
source type in the message. Ported instead as written (`checker.go:4188`), in
`comparison_overlap.rs::check_switch_case_comparability`:
`!isTypeEqualityComparableTo(expr, case)` then `checkTypeComparableTo(case,
expr)`. Note the asymmetry with the equality operator: only a nullable *case*
type takes the flag disjunct.

**One decline, and why.** A fresh object-literal case type fails the
comparable relation in `hasExcessProperties` first, whose reporter emits
TS2353 on the property rather than TS2678 on the clause (`switchStatements`,
line 35). That reporter is the excess-property one, so the clause stays
silent here; the TS2353 stays missing. It would be wrong if a case with a
fresh literal *and* no excess property were expected to report TS2678 — none
in the corpus does at this commit.

**Measured.** TS2678 missing 47 → 0, extra 0 → 0; 17 cases converted.
