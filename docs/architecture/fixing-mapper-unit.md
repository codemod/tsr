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


---

## Phase 0 — the transcription (2026-08-10, checker-1)

Read at the pinned submodule; every anchor is a real line.

### The priority lattice (`checker.go:299-318`), all twelve classes

`InferencePriority` is a BITSET, not a scalar rank — `n.priority`
carries the active flags during a walk, and candidates recorded under a
LOWER-valued active set displace higher ones (see `inferWithPriority`
uses at `inference.go:125/239`): `None=0` (plain positions),
`NakedTypeVariable=1<<0`, `SpeculativeTuple=1<<1`,
`SubstituteSource=1<<2`, `HomomorphicMappedType=1<<3`,
`PartialHomomorphicMappedType=1<<4`, `MappedTypeConstraint=1<<5`,
`ContravariantConditional=1<<6`, `ReturnType=1<<7` ("lower priority than
all other inferences", the `checker.go:9437` comment),
`LiteralKeyof=1<<8`, `NoConstraints=1<<9` and `AlwaysStrict=1<<10`
(behavior flags, not ranks), `MaxValue=1<<11` (the tracking seed,
`inference.go:59`), `Circularity=-1` (less than everything).
`PriorityImpliesCombination = ReturnType|MappedTypeConstraint|LiteralKeyof`
— candidates under those combine (union) rather than compete.

### The step-2 window (`inferTypeArguments`, `checker.go:9390-9495`)

1. **Return-side seed**: contextual type of the CALL feeds the
   signature's return type at `InferencePriorityReturnType`
   (`:9437`), skipped for binding-pattern-derived contexts
   (`:9405-9416` — those go ONLY into `context.returnMapper`); a
   separate `returnContext` pass (`:9447-9458`) builds the
   returnMapper used by `instantiateContextualType`.
2. **Implied rest arity** (`:9462-9474`): a type-parameter rest slot
   records `impliedArity` when no spread follows.
3. **this-argument** (`:9475-9479`) at `None`.
4. **THE ARGUMENT LOOP** (`:9480-9489`): per argument IN ORDER —
   `checkExpressionWithContextualType(arg, paramType, context, mode)`
   then `inferTypes(...)` at `None`. The context flows INTO the
   argument's own checking: this is where earlier arguments' fixings
   become later arguments' contextual parameter types.
5. Spread tail (`:9490-9493`), then `getInferredTypes`.

### Fixing (`inference.go:1251-1283`, `:1317-1412`)

The context carries TWO mappers minted at construction
(`:1280-1281`): `mapper` (fixing=true) and `nonFixingMapper`. The
fixing mapper's application marks `isFixed` and computes
`getInferredType` for the parameter; once `isFixed`, candidate
collection REFUSES new candidates (`:183`, `:962`, `:1644`).
`getInferredType` (`:1317`): covariant inference from candidates,
contravariant from contraCandidates, the preference rule at
`:1341-1354` (prefer covariant unless never/any, assignable to some
contra, and no conflicting constrained sibling); NoDefault flag →
silentNeverType wildcard; else the type-parameter DEFAULT
(instantiated under backreference+nonFixing mappers, `:1368`); nil →
any/unknown by flag (`:1376`); then the CONSTRAINT filter
(`:1378-1399` — pure ReturnType inferences may drop non-assignable
constituents to never, `:1385-1392`; a failing inference falls to
fallbackType-if-it-fits else the constraint).

### Ordered member sites (`inference.go:1285-1300+`)

`addIntraExpressionInferenceSite` collects context-sensitive
object/array literal MEMBERS during the omitted pass; the second pass
infers from earlier sites before contextually typing later ones — the
`foo([_a => 0, n => n.toFixed()])` example is upstream's own. This is
piece 3's exact mechanism: "This happens automatically when the arrow
functions are discrete arguments (because we infer from each argument
before processing the next)".

### Pass structure (the caller)

The skip/re-serve two-pass lives in `getSignatureApplicabilityError` /
`resolveCall`'s CheckMode plumbing (`CheckModeSkipContextSensitive`) —
transcribe its exact gates when Phase 3 opens; the window above is
complete for Phases 1–2.

**Phase 0 gate: MET** — every priority class named with its anchor;
fixing semantics and the ordered-sites mechanism transcribed. Phase 1
(the lattice in `inference.rs`'s existing engine) is build-ready.
