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

## 3. TS2873's `undefined` arm compares against the seeded symbol

`getSyntacticTruthySemantics` reads `getResolvedSymbol(node) == c.undefinedSymbol`
(`checker.go:12907`). The port tested for *no* resolution, which stopped holding
once the binder seeded a global `undefined` (`declare_synthesised_globals`), so
every `undefined && x`, `undefined ? a : b` and `void 0 || void 0` was silent.
Now compares against `binder.undefined_symbol()`; a shadowing local still
resolves elsewhere and stays `Sometimes`. Seven cases converted, no losses.

## 4. TS2454 asks `getAssignmentTargetKind`, not "left of `=`"

`check.rs::is_definite_assignment_target` only recognised `x = …`. Upstream's
`checkIdentifier` returns before the flow section for any
`AssignmentKindDefinite` target (`checker.go:11109`), and `GetAssignmentTarget`
climbs array literals, spreads, parentheses and object-literal positions — so
`[...[a, b]] = …` and `for ([a] of …)` writes were reported as unassigned reads.
`expressions.rs` already ports the walk (`assignment_target_kind`); the rule now
asks it. TS2454 extras in the lane 23 → 1; twelve cases converted, no losses.
`is_write_only_access` (the `crate::unused` access-kind reuse) stays: it was the
earlier partial cover for the same gap and removing it is a separate question.

## 5. TS2774 / TS2801 / TS2845: `checkTestingKnownTruthyType` ported whole

`truthiness.rs` had a narrow stand-in for TS2774: an identifier condition of an
`if`, declared as a function or with a function-type annotation, whose `then`
branch does not mention the name. Ported instead as written
(`checker.go:3814`–`3953`): the three call sites (`if` → `then`, `?:` →
`whenTrue`, the left of `&&` always and of `||`/`??` inside an `if` chain), the
`||`/`??` left-operand walk, the enum-member arm (TS2845), call signatures *or*
a promised type (TS2801), `isSymbolUsedInBinaryExpressionChain` and
`isSymbolUsedInConditionBody` with its receiver-chain comparison.

**Declines (this port's, all toward silence).** A type whose call signatures
cannot be read, a promised-type lookup that hits an unported step (a `then`
with a non-`void` `this` parameter, whose subtype filter is not reproduced),
and a generic (`INSTANTIABLE`) type in that lookup all answer nothing.
`getSymbolAtLocation` is reduced to the two positions the rule compares
(property-access name → property of the receiver's apparent type; anything
else → its value resolution).

**Duplicate reports.** `if (a || b)` reaches `a` from both the `if` arm and the
`||` arm; upstream's diagnostic collection drops the identical second report.
`report` here does not deduplicate, so the rule checks for an identical
(file, span, code) before reporting.

**Perf (§5's ordering).** Every `if`, `?:` and `&&` reaches the rule. The first
version resolved call signatures and `then` before asking anything cheap and
read ~1.04 new/old on `domain-model` (swapped slots 0.96–0.965 old/new, i.e. a
real ~3–4%). An exact early-out — every constituent primitive ⇒ no signatures
and no promised type — moved ahead of those lookups brought four alternating
21-sample runs to 0.996 / 0.984 / 1.015 / 1.058 (mean ≈ 1.008).

**Measured.** TS2774/TS2801/TS2845 lines missing 92 → 18, extra 0 → 0; eight
cases converted. Left: `nanEquality`'s TS2845 is `checkNaNEquality` (operators
lane), and `truthinessCallExpressionCoercion2` line 116 reads
`window.console.error` through the DOM lib's `Window & typeof globalThis`.

## 6. TS2355 / TS2366 / TS2534 read end-of-body reachability

`check.rs::check_empty_body_returns_value` reported TS2355 for an *empty* body
only, because `functionHasImplicitReturn` was "reachability, this port's
standing refusal" (§440). That refusal is stale: `function_has_implicit_return`
(§743) reads the binder's end flow node through `is_reachable_flow_node`. The
whole of `checkAllCodePathsInNonVoidFunctionReturnOrThrow` (`checker.go:3728`)
is now ported as `flow.rs::check_all_code_paths_return_or_throw` and called for
function/method declarations, get accessors, function expressions and arrows.

**Not ported: TS7030 (`noImplicitReturns`).** The checker carries no
`no_implicit_returns` field and `apply_compiler_options` lives in the hub
`checker.rs`; adding one is reported to the integrator rather than made here.
That arm is 26 missing lines in six lane cases (`noImplicitReturnsExclusions`,
`noImplicitReturnsInAsync2`, `noImplicitReturnsWithoutReturnExpression`,
`reachabilityChecks5/6/7`).

**Declines.** JavaScript, generators (iteration return type), unannotated get
accessors (`getTypeOfAccessors` infers from the body), and error types. An
unannotated function returns *before* the reachability query: only TS7030 can
speak there, and the query types `never`-returning calls in the body, which
re-entered the function's own inferred return type and reported TS7023 on
`thisTypeInObjectLiterals2` in the first measurement (one EMPTY_RIGHT loss,
fixed before commit).

**Measured.** TS2355/2366/2534 lines missing 23 → 4, extra 2 → 0; seven cases
converted, no losses. Perf on this box has a slot bias of several percent that
flips sign between projects, so each project was run both ways (21 samples):
`domain-model` 0.955 / 1.052 direct and 1.051 / 0.946 swapped-inverted,
`generic-imports` 1.068 / 1.070 direct and 0.966 / 0.969 swapped-inverted —
geometric means ≈ 1.00 and ≈ 1.017.
