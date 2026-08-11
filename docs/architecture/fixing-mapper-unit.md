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


### Phase 1 pre-read correction (2026-08-10, checker-1)

The opener's "engine partial" line UNDERSTATES the port:
`check_generic_call` already carries much of the step-2 window from
checker-2's SS135-SS141 arc — per-argument candidate BUCKETS merged in
index order (SS140), deferred context-sensitive arguments SERVED from
pass-1 inferences with consumed parameters marked fixed (the SS75
uninstantiated-serve semantics), the intra-expression harvest for
object-literal members IN ORDER with per-site so-far flattening
(SS137 slice 2), array-element tuple harvest (SS138), and
reference-member instantiation (SS141). What Phase 1 adds is
PRECISELY the lattice and nothing else: `InferenceInfo` candidates
carry no priority tag, so the three merge sites — the buckets merge,
the `return_mapper` separation (structurally ReturnType-priority
already), and the `so_far` flatten — cannot yet express "a
lower-priority-value class displaces". Slot the tag into
`add_candidate` and the merge points; do NOT restructure the pass
shape (the SS135 interleaving already measured −610 once). The
§142/§143/§144 refusals were measured ON this engine — their adverse
classes remain Phase 3's falsifiers, and any Phase-1 change that
moves intraExpressionInferences or typeArgumentInferenceWithObjectLiteral
in EITHER direction is out of scope and reverts. Builder must read
`inference.rs` whole plus checker-notes-callres2.md SS133-SS144
before writing the first line.


### Phase 1 slice 1 — MEASURED +4/9 AND REVERTED (2026-08-10, checker-1)

The minimal lattice (priority on InferenceInfo, displacement in
add_candidate, NakedTypeVariable tagged in the union arm's
constituent recursion, ReturnType on the return seed) built clean
and measured NET NEGATIVE: +4 G→R (unionTypeInference 2,
recursiveTypeReferences1 2) against 9 G→W (unionTypeInference 6,
nestedTypeVariableInfersLiteral 3) — the zero-regression bar
fires. The mechanism: displacement without `inferToMultipleTypes`'
FULL structure (`inference.go:700+` — the matched-count strike,
the single-naked-variable condition, the source-constituent
distribution) resolves previously-gapping disagreements to the
WRONG survivor. The port's existing strike rule (the assignability
early-out) and the naive lattice compose incorrectly — the same
interlock lesson as §142/§143/§144 one level down. AMENDMENT to
the phase plan: Phase 1's true unit is lattice + inferToMultipleTypes
TOGETHER (transcribe :700-800 before rebuilding); the slice's code
shape (the two-fn split, the merge-site threading, the const
names) was correct plumbing and is preserved in the scratchpad
(phase1s1.py) for the rebuild. The four-refusal table gains a
fifth row, and the thesis sharpens: not even the LATTICE lands
alone.


### Phase 0 extension — `inferToMultipleTypes` transcribed (`inference.go:448-552`)

The structure my slice-1 lacked, and WHY +4/9 happened:

1. **Union target, per-source distribution** (`:453-457`): a union
   SOURCE distributes into constituents; `matched[i]` tracks per
   source constituent.
2. **The matched test is INFERENCE-QUALITY, not assignability**
   (`:469-478`): for each non-variable target constituent, infer
   source[i] against it under `inferencePriority = MaxValue`; if the
   walk's resulting `inferencePriority == n.priority` (candidates of
   equal quality to a naked-variable inference were recorded),
   `matched[i] = true`. Circularity (`-1`) is tracked. Our port's
   assignability strike is an APPROXIMATION of this — right on
   primitives, wrong exactly where slice 1 misfired.
3. **The common case makes a PLAIN-priority inference and RETURNS**
   (`:495-506`): exactly ONE naked variable + no circularity → the
   UNION OF UNMATCHED sources infers to the variable via plain
   `inferFromTypes` — NOT at NakedTypeVariable priority — and the
   function returns. My slice tagged THIS road naked, which is the
   +4/9's root: real candidates got demoted and displaced.
4. **NakedTypeVariable priority marks only the FALLTHROUGH**
   (`:519-529`): multiple naked variables, or single-variable-but-
   everything-matched, or the intersection single-variable case
   (`getSingleTypeVariableFromIntersectionTypes`, `:532` — every
   target an intersection containing the SAME single variable →
   infer whole source at naked priority). The doc comment's example:
   `Promise<string>` to `T | Promise<T>` infers `string` for T, not
   `Promise<string> | string`.
5. Non-union multi-target (`:507-518`): non-variable targets first
   (ordering as soft priority), then the fallthrough.

**Phase 1 rebuild spec (amended):** port `inferToMultipleTypes`
whole INTO the union arm — per-source matched tracking with the
quality test (which needs the walk to REPORT the priority of what
it recorded: thread an `inference_priority` out-param like
upstream's `n.inferencePriority` min-tracking), the unmatched-union
plain road, the naked fallthrough, and the intersection
single-variable rule — WITH the slice-1 lattice plumbing
(preserved in scratchpad phase1s1.py). The two land together or
not at all; the existing assignability strike retires in the same
commit.


### Phase 1 iteration 2 — the WHOLE inferToMultipleTypes port measured net −41 and REVERTED (2026-08-10, checker-1)

The amended spec built complete (per-source matched tracking with
record-into-live-context, unmatched-union plain road, naked
fallthrough, multi-variable arm, union-source distribution, the
assignability strike and union-source bail retired): own-lane
~+24 G→R (promiseType 12) against 57 G→W + 8 R→GAP — the
promiseType/promiseTypeStrictNull families SPLIT (+16/−44), and
unionTypeInference lost its §-era wins. Two findings for the
rebuild: (1) the matched-quality approximation ("recorded any
candidate" vs upstream's priority-equality test) is NOT exact even
single-priority — the walk must report WHAT it recorded, i.e. the
`inferencePriority` min-tracking out-param is LOAD-BEARING, not
bookkeeping; (2) union-SOURCE distribution (new behavior — the old
arm bailed on union sources) interacts with the T|PromiseLike<T>
pattern: per-source-constituent inference into structured
constituents records nested-variable candidates my matched test
then counts, where upstream's quality test does not. The two
probes now BOUND the rebuild: slice-1 (+4/9, lattice alone) and
iteration-2 (−41, structure without the quality out-param). The
Phase-1 build that lands carries: the lattice + the structure +
the priority-report out-param, together. Fresh window, full
inference.rs read, the promiseType pair as the first comparator.


## Fixing-mapper unit — the FOURTH piece found, build reverted [checker-1]

Built lattice + `inferToMultipleTypes` + the recorded-priority
channel TOGETHER (the three pieces the two earlier probes had taken
separately). Measured +31/57, then +15/21 with a conservative gate.
Reverted.

**What the third attempt bought: the missing piece is now named.**
The promiseType/promiseTypeStrictNull split is not a lattice or a
structure problem — it is that upstream matches a source REFERENCE
against a STRUCTURALLY RELATED target reference. `resolve(value: T |
PromiseLike<T>)` given `Promise<never>`: upstream infers T:=never
through `Promise`-vs-`PromiseLike`, marks the source MATCHED, and
drops it from the unmatched union. This port's reference arm
requires IDENTICAL target symbols (`ts == ss`), so the source stays
unmatched and poisons the union —
`Promise<number | Promise<never>>` where `Promise<number>` is
wanted, which is exactly the 26+18 adverse both attempts produced.

So the unit is FOUR pieces, not three: lattice, inferToMultipleTypes,
the recorded-priority channel, and **reference matching across
related targets** (variance-based inference through base types /
`getTypeReferenceIfMatched`). The first three are written and
preserved in the scratchpad (fm1.py, fm2.py); the fourth is
untranscribed and is the next window's opener.


### CORRECTION to the entry above — the fourth piece is STRUCTURAL MEMBER inference, not reference matching

The entry above named the fourth piece "reference matching across
related targets (variance-based inference through base types /
`getTypeReferenceIfMatched`)". **That was a guess and it is wrong.**
Read at `inference.go:700-702` and `:232-234`: upstream's reference
arm requires `source.Target() == target.Target()` (or both array
types) exactly as this port's does — so `Promise<never>` against
`PromiseLike<T>` does NOT match there either.

Where upstream actually records it is the STRUCTURAL tail of
`inferFromObjectTypes` (`inference.go:822-825`):
`inferFromProperties`, then `inferFromSignatures` for call and
construct, then `inferFromIndexTypes`. `inferFromProperties`
(`:828-836`) walks the TARGET's properties, looks each up on the
source, and infers member-type against member-type;
`inferFromSignatures` (`:838+`) matches signatures bottom-up.
`Promise<never>` and `PromiseLike<T>` both carry `then`, so the
member walk reaches T through the signature and records T:=never —
the source is MATCHED and drops out of the unmatched union.

So the fourth piece is: **inference through object MEMBERS**, which
this port has not built at all. That is a larger and better-defined
piece than the entry above claimed, and it explains why all three
attempts produced the identical promiseType signature — none of
them could ever have matched those sources.

Recorded as a correction rather than an edit per the project's
non-negotiable: the wrong reading was published, and the reason it
was wrong (a guess where a read was needed, inside a document whose
whole point is that transcription beats induction) is worth more
than the corrected fact.
