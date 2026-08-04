# ADR-0021: `.d.ts` emit is written against TypeScript's `isolatedDeclarations` rules, not ported from typescript-go

- **Status:** accepted
- **Date:** 2026-08-04
- **Upstream pin:** `vendor/typescript-go` @ `5b1047d10`
- **Relates to:** [ADR-0001](0001-idiomatic-rewrite.md) (the "port, don't
  reinvent" default this departs from), [ADR-0004](0004-oxc-inspiration-not-dependency.md)
  (inspiration vs. dependency), [ADR-0006](0006-conformance-oracle.md) (what a
  baseline is allowed to prove)
- **Scope:** `tsr-dts`. What its rules are anchored to, what upstream-anchoring
  and drift tracking mean for a component that has no upstream counterpart, and
  what its conformance oracle is.

## The forcing constraint

Phase 3.5 exists for one reason: to ship something usable roughly a year before
the checker lands (PLAN.md §4). Its premise was that `.d.ts` emit needs no
checker. That premise is true of oxc. **It is not true of typescript-go**, and
the difference is not a detail at the edges — it is the whole design.

Measured at the pin:

| Fact | Evidence |
|---|---|
| The declaration transform is 2,986 lines | `internal/transformers/declarations/transform.go` |
| It threads `printer.EmitResolver` throughout | `ensureType` at `transform.go:1629` calls `resolver.CreateTypeOfDeclaration` at `:1676`; 20+ further `getSymbolAccessibilityDiagnostic` sites |
| The sole implementation of `EmitResolver` is the checker | `internal/checker/emitresolver.go`, 1,326 lines over the 60k-line checker |
| `isolatedDeclarations` is a *flag on shared state*, not a separate path | `transform.go:105` — `SymbolTrackerSharedState{isolatedDeclarations: …, resolver: resolver}` |

The last row is the one that decides this. It would be reasonable to hope that
`isolatedDeclarations` selects a checker-free branch that could be ported alone.
It does not. It selects *whether the same checker-driven transform errors or
infers*.

**The error path is checker-driven too.** This is the part that is easy to get
wrong from a distance, so it is worth tracing precisely. Only five of the twenty
`TS9xxx` diagnostics are raised under an `isolatedDeclarations` guard inside
`transform.go` (lines 128, 587, 591, 2278, 2562). The other fifteen — including
the four highest-frequency ones, `TS9007` function return type, `TS9010`
variable, `TS9012` property, `TS9011` parameter — are raised from
`tracker.go:92`, inside the `SymbolTracker` the checker's node builder calls
*when it fails*:

```go
// internal/transformers/declarations/tracker.go:250
tracker := &SymbolTrackerImpl{…, getIsolatedDeclarationError: createGetIsolatedDeclarationErrors(resolver)}
```

and `createGetIsolatedDeclarationErrors` (`diagnostics.go:683`) itself calls
`resolver.RequiresAddingImplicitUndefinedUnsafe`. So in typescript-go the
question "does this declaration need inference?" is answered by *running the
inference and seeing it fail*. There is no syntactic predicate to port.

Therefore a faithful port of `transformers/declarations` cannot ship before
Phase 4, which defeats the only reason Phase 3.5 exists.

## The alternative that makes the phase possible

oxc answers the same question syntactically, and the crate is small:

| | |
|---|---|
| `oxc_isolated_declarations` `src/` | **3,550 lines** across 14 files (`oxc-project/oxc` @ `main`, v0.143.0, fetched 2026-08-04) |
| Dependencies | `oxc_allocator`, `oxc_ast`, `oxc_ast_visit`, `oxc_diagnostics`, `oxc_ecmascript`, `oxc_span`, `oxc_str`, `oxc_syntax`, `bitflags`, `rustc-hash` |
| Checker or semantic-analysis dependency | **none** — not even `oxc_semantic` |

**Correction to the record.** PLAN.md §Phase 3.5 states 4,066 LOC, citing the
oxc pin `5e5178b` recorded in PLAN.md §0. That commit **does not resolve on
GitHub** (`No commit found for the ref 5e5178b`), so the 4,066 figure cannot be
reproduced at the pin it claims. The number above is a fresh measurement at
`main`. The load-bearing claim — a few thousand lines, no checker — holds; the
specific figure and its anchor did not. PLAN.md is corrected in the same commit
as this ADR. The oxc pin should be re-recorded as a resolvable SHA before
anything else rests on it.

## Decision

**Write `tsr-dts` against TypeScript's `isolatedDeclarations` *specification* —
the `TS9xxx` diagnostic family and the rules that produce it — rather than
porting `internal/transformers/declarations`.** Syntactic, checker-free,
erroring wherever inference would be required.

This is a deliberate departure from [ADR-0001](0001-idiomatic-rewrite.md), which
says port rather than reinvent. ADR-0001's rationale is that upstream is actively
developed and its fixes must remain re-derivable. That rationale applies to code
with an upstream counterpart. `tsr-dts` will not have one.

### What upstream-anchoring means for a component that is not a port

[conventions.md](../conventions.md) requires every ported item to name its
typescript-go counterpart, because that is what makes drift tracking mechanical.
`tsr-dts` cannot satisfy that rule as written, and pretending otherwise — naming
`transform.go` functions in doc comments on code that does something structurally
different — would be worse than not naming them, because the drift tracker would
then file issues against a correspondence that does not exist.

The rule for `tsr-dts` is therefore different, and narrower:

1. **Anchor to the diagnostic, not to the function.** Each rule names the
   `TS9xxx` code it implements and the upstream message constant in
   `internal/diagnostics/`. Diagnostic codes and messages are a stable public
   contract; `transform.go`'s internal structure is not. This is a correspondence
   that will still be true in a year.
2. **The drift tracker watches `internal/diagnostics/` for the `TS9xxx` range**,
   not `internal/transformers/declarations/`. A new or reworded
   `isolatedDeclarations` diagnostic upstream is a real signal for us; a
   refactor of `ensureType` is not.
3. **Where a rule is taken from oxc's shape rather than derived independently,
   say so by file** (`oxc_isolated_declarations/src/class.rs`), the same way
   [ADR-0004](0004-oxc-inspiration-not-dependency.md) already permits for
   designs. Inspiration, cited; still not a dependency.

### What this does *not* decide

This ADR covers the **rules**, not the **text**. Producing `.d.ts` output needs a
printer, and no printer exists here (upstream's is 6,280 lines,
`internal/printer/printer.go`). The printer is a separate decision and a separate
slice, and it is Phase 5 machinery earned early rather than borrowed — nothing
about the printer is `isolatedDeclarations`-specific, so it should be ported
faithfully under ADR-0001's default. See `bd tsr-49v.4`.

## The oracle, and a correction to the epic's premise

`bd tsr-49v.1` recorded that 21 corpus cases set `@isolatedDeclarations: true`
and only 6 have `.d.ts` output, and concluded the oracle is "far too small for a
subsystem this size". That is right about `.d.ts` *output* and wrong about the
subsystem, because it measures the wrong half.

The corrected count, measured at the pin:

| | |
|---|---|
| Cases setting `@isolatedDeclarations` | **22** (not 21) |
| Cases whose `.errors.txt` baseline carries `TS9xxx` diagnostics | **16** |
| Distinct `TS9xxx` codes exercised | **20** |
| Positioned diagnostics across those baselines | **~172** |

Those 172 positioned diagnostics are an oracle for exactly the checker-free half
— the predicate "does this declaration require inference?" — and they carry
positions and codes, so a wrong answer is localised rather than merely counted.
That is a better gate than six text comparisons, and it is available before a
printer exists.

The `.d.ts` output oracle stays small and stays a separate gate: 6 cases with
`@isolatedDeclarations`, plus 1,974 `.js` baselines repo-wide that embed a
`//// [x.d.ts]` section (`bd tsr-bb4.4` builds the reader).

## Consequences accepted

**We will diverge from `tsc` and from `tsgo` in ways no baseline catches.** The
16-case error oracle covers the rules it covers. TypeScript's actual
`isolatedDeclarations` behaviour is larger than 22 test cases, and we have no
baseline for the rest. Divergence found by a user rather than by CI is the
expected failure mode of this decision, and it is the price of shipping a year
early.

**The reachable denominator is an upper bound, not a target.** `bd tsr-49v.3`
measures how many `@declaration` cases have every export explicitly annotated.
Even for those, byte-matching upstream's `.d.ts` needs visibility analysis that
upstream does through `EmitResolver.IsDeclarationVisible`. Concretely:
`compiler/declareFileExportAssignmentWithVarFromVariableStatement` annotates
everything, and its `.d.ts` baseline still drops `var x = 10` entirely, because
`x` is not visible from the `export = m2`. A syntactic emitter can approximate
that with reference reachability from the exports — oxc does, in `scope.rs` — but
"approximate" is the operative word. The denominator suite must say this in its
`describes()`, or it will be read as a promise.

**When the checker lands, we will have two answers to the same question.**
Phase 4 gives us `CreateTypeOfDeclaration`'s equivalent, at which point full
declaration emit becomes portable and `tsr-dts` becomes the `isolatedDeclarations`
path specifically — which is what it is in TypeScript proper. That is a
convergence, not a rewrite, but the two must be reconciled deliberately rather
than left to drift apart.

**`tsr-dts` is a fork with no upstream to merge from.** ADR-0001's three
containment mechanisms (anchored doc comments, drift tracker, baseline ratchet)
degrade here to one and a half: the ratchet works, the drift tracker works only
against the diagnostic range, and anchored doc comments do not apply. This
component is less contained than the rest of the port, on purpose.

## How we would know this was wrong

Three falsifiers, in the order they would show up:

1. **The reachable denominator comes in small.** If `bd tsr-49v.3` finds that
   fewer than a few hundred of the 1,414 `@declaration` cases are annotated
   enough for a checker-free emitter, then the artifact this phase ships is not
   usable on real TypeScript, and the phase should be cut rather than built. This
   number is measured *before* any emitter is written precisely so this
   falsifier can fire cheaply.
2. **The error rules do not converge on the 16-case oracle.** If the analysis
   plateaus well short of the 172 diagnostics and the residue is cases where
   upstream's answer genuinely depended on inference, the syntactic predicate is
   not expressible and option (a) was right after all.
3. **Reconciliation at Phase 4 turns out to be a rewrite.** If, once
   `CreateTypeOfDeclaration` exists, `tsr-dts` shares nothing with the ported
   declaration transform, then the year of early shipping cost us a subsystem's
   worth of throwaway work, and future "ship before the checker" phases should be
   judged against that.

## Alternatives

**(a) Port `transformers/declarations` faithfully.** Wins if Phase 4 turns out to
be much shorter than the ~120–200 sessions estimated for the checker, because
then the year of early shipping is not worth a forked subsystem. Rejected
because at the current estimate it makes Phase 3.5 empty: there is nothing to
ship until Phase 4 completes, and the phase's entire justification is that there
is.

**(c) Depend on `oxc_isolated_declarations` directly.** Rejected by
[ADR-0004](0004-oxc-inspiration-not-dependency.md) for the standing reason (it is
pre-1.0 and moves fast), and by a specific one: it is written against `oxc_ast`,
which per [ADR-0002](0002-own-ast.md) is deliberately not TypeScript's AST. The
adapter would be the expensive part.

**(d) Wait for the checker and skip Phase 3.5.** The honest alternative to this
whole phase, and the one falsifier 1 above would select. Not chosen now, because
the cost of finding out is one measurement rather than a subsystem.
