# The fixing-mapper unit — the ranked top rock, specced for its opening window

Both lanes' 2026-08-10 windows converged on one verdict, from opposite
directions: the largest remaining single-mechanism mass in `checker_types`
sits behind upstream's inference **fixing** pipeline, and it cannot be
ported piecemeal. This document is the build's opener: what the unit is,
why every partial road measured negative, and the phase plan with gates.

## Why one unit, measured

Four independent refusals stand on this head, each from a different
direction, each measured whole (the requirement ledgers):

| attempt | direction | measured | record |
|---|---|---|---|
| callres2 §142 | predicate + fill-guard | net −21 | `checker-notes-callres2.md` |
| callres2 §143 | seam condition (memo-active detector) | 0 / +73 wrong | same |
| callres2 §144 | gate third-state (literal members ADOPT) | −16 / +23 wrong | same, **final for the piecemeal road** |
| narrow §146 | objects.rs whole-literal tolerance | +4/0, class exhausted; SS146p1: 232/420 error-members are arrows wanting contextual signatures | `checker-notes-narrow.md` |

The §144 close states the mechanism: *"in this port's economy the
whole-literal error is doing MORE work than upstream's member-wise typing
can replace piecemeal."* Every partial adoption of member-wise typing
loses to the existing whole-literal discipline until the full window
exists.

## The four pieces (upstream anchors)

Upstream's `inferTypeArguments` (`checker.go`, the step-2 window around
`inferFromTypes`/`getInferredType`) does four things this port's
`inference.rs` engine (real, partial — `check_generic_call` +
`InferenceInfo` buckets; its module doc: "no priority lattice and no
contravariant bucket") does not:

1. **Per-consumption fixing.** A type parameter FIXES (its inference
   resolves to a type) the first time a *consumption* needs it — not at
   the end of candidate collection. Later candidates against a fixed
   parameter are ignored. This is what lets an arrow's parameter type be
   read mid-call from inferences made by EARLIER arguments.
2. **InferenceInfo priorities.** Candidates carry priority classes
   (return-type, contravariant, literal-keyed, homomorphic-mapped…);
   a lower-priority candidate never displaces a higher class. The port
   currently walks every position covariantly and gaps on disagreement.
3. **Ordered member sites.** Object/array literal members contribute
   inference candidates in source order, member by member — the reason
   upstream can type `{ a: 1, b: (x) => x.length }` with `x` from `a`'s
   inference. Requires member-wise checking (the §142/§144 third-state
   machinery) — which only works WITH fixing, per the refusals.
4. **Pass-3 re-serve.** Context-sensitive arguments (unannotated arrows)
   are skipped in pass 1, inferred in pass 2 once other arguments fixed
   the parameters, and RE-CHECKED (re-served) with their now-known
   contextual types. checker-2's callres pass-3 hook exists dormant.

## Predicted reach (both censuses)

- The contextual generic/overload populations: 183 generic-callee + 102
  overload-set contextually-typed functions (`contextual.rs` module doc's
  table — the two "no" rows).
- `number|string-want-any-got` heads: contextualTypeWithUnionTypeMembers
  68, contextualTyping 51, inferFromGenericFunctionReturnTypes2 44,
  thisTypeInFunctions 40, objectLiteralGettersAndSetters 40 (§151).
- typeArgumentInference* families: ~86 lines (`unknown`-got pairs, §-tail
  census).
- badInference's head + intraExpressionInferences + the SS146p1 arrow
  population (232 error-member firings).
- Secondary unlocks: promisePermutations (480 gap) and variadicTuples
  need MORE than this unit (overload resolution depth, tuple spreads),
  but their first rungs consume its outputs.

Rough joint ceiling: 800–1,500 lines, the largest single build since the
member subsystem.

## Phase plan (each phase measured, bar-in-doc-first)

- **Phase 0 — transcription.** Read and transcribe upstream's step-2
  window into a design note: `inferTypeArguments`, `inferFromTypes`'s
  priority assignments, `getInferredType`'s fixing semantics, and where
  `checkExpressionWithContextualType` re-serves. No code. Gate: the note
  names every priority class with its upstream line anchor.
- **Phase 1 — priorities in the existing engine.** Add the priority
  lattice to `InferenceInfo` candidates under the CURRENT single-pass
  collection; disagreement within a class keeps the gap. Bar: zero
  regressions, any G→R is upside (typeArgumentInferenceWithConstraints'
  13 are the watch pool).
- **Phase 2 — fixing.** `getInferredType`-style per-parameter resolution
  with a fixed bit; consumptions read fixed types. This is the step that
  un-guards the §93-gate's grounded test for MENTIONING-parameter cases
  (custody: the gate's conditions are checker-2's — heads-up rule from
  the §152-era boundary agreement applies to whichever lane builds this).
- **Phase 3 — ordered member sites + pass-3 re-serve.** The §142/§143/
  §144 third-state machinery rebuilt ON TOP of fixing, plus the dormant
  pass-3 hook activated. The three callres2 refusal measurements become
  the falsifier set: if any of their adverse classes reappears, the
  phase is wrong, not the refusals.
- **Phase 4 — the whole-literal seam retired member-wise.** objects.rs's
  `return error` seam (SS146p1's 420 firings) converts to member-wise
  error tolerance where inference now grounds the members.

## Custody

One lane builds this; the other holds the baseline and reviews phase
gates. The §93 gate's conditions and calls.rs remain checker-2's to
approve; inference.rs/contextual.rs arms are checker-1-era surfaces.
Whoever opens the window claims it in STATUS §4 first.
