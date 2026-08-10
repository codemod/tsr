# The generic-overload resolution subsystem — decomposition study

Opened at 85.29% (right 408,450, the day's closing census): this
subsystem owns the near-miss board's top — CallExpression 772 +
ArrowFunction 316 near-miss cases, the intraExpressionInferences /
generatedContextualTyping / complexRecursiveCollections mountains,
and the §112 const-T prerequisites. Every claim below is anchored
to the pinned submodule; the anchors gate checks them.

## The three entry functions, verified at the pinned commit

- `resolveCall` (`checker.go:8843`) — the driver: arity check,
  candidate collection, the CallState, and the error-signature
  fallback whose observable the §31-chain already ports.
- `chooseOverload` (`checker.go:9025`) — the selection loop this
  port lacks WHOLE: per-candidate argument checking against a
  relation, with the two-pass structure (skip context-sensitive
  arguments first, then re-check) that §114's fixture design
  exposed from the outside.
- `inferTypeArguments` (`checker.go:9390`) — per-candidate
  inference; this port's `check_generic_call` is its
  single-candidate, bare-position shadow.

## What this port already holds (the fringe, landed)

- §70/§114 agreement + arity selection at the ARGUMENT-CONTEXT
  layer (contextual.rs) — resolution's first discriminator,
  applied before resolution exists.
- §115's conditional relay; the §93/§94 nil-ladder; §75's
  uninstantiated pass — the context TRANSMISSION side is done.
- The §33 const decline and §112's two named prerequisites —
  waiting on THIS subsystem.

## The decomposition question for the design window

The port cannot take chooseOverload whole without the relation
(`is_type_assignable_to` is narrow — conventions' "conservative
false is unsafe for selectors"). The candidate slices, each needing
its own bar and counterfactual:
  1. ARITY-ONLY selection promoted from the context layer into an
     actual `resolve_call_signature` upgrade (multi-candidate,
     arity-discriminated, tie=decline) — no relation needed.
  2. The TWO-PASS context-sensitivity skip as a checkable gate
     (fixes §114's want-any ternary class faithfully).
  3. Per-candidate `check_generic_call` (today: single-candidate
     only) — inference per candidate, first-succeeding wins, under
     the simple-domain containment the relater already defines.
  4. The relation-gated final form — blocked on relater breadth,
     possibly permanently (the ADR-0039 family of ceilings).

Slice 1 is the next bar; its population is measurable tonight
(calls declining today solely on `signatures.len() > 1`).

## The slices' ceilings, measured (callgate at 85.29%)

The funnel's admitted 5,821 lines stop at, by slice ownership:
- **Slice 3 (per-candidate inference): 1,829** — "inference
  gapped", the single largest stop in the whole funnel; plus 85
  "generic, to inference". This is the summit.
- **Slice 1 (arity-discriminated selection): 547 + 113** — "a
  generic candidate in the set" (multi-candidate sets declining
  whole today) and "ambiguous: matches with different returns";
  slice 1 converts the subset whose arity discriminates to a
  single non-generic candidate — the rest waits for slices 2-3.
- **Slice 2 (the two-pass skip)**: unlabeled here — its population
  is inside slice 3's 1,829 (context-sensitive arguments gapping
  inference); measure it when slice 3's design fixes the boundary.
- Adjacent-owner rows, NOT this subsystem's: "callee type is not
  an object type" 617 (the §115-era new-side gate), "identifier:
  symbol types as a non-object" 484, "an any parameter" 299 (the
  §112 const-family), relation-gated 212 (the ADR-0039 ceiling
  family).
Ceilings are populations, not conversions — the §-standard
counterfactual discipline applies per slice.

## Slice 1 bar [checker-2, registered before the code]

`choose_overload` already selects by assignability under SELECTABLE
domains; its FIRST reduction ("a generic candidate anywhere in the
set" → None) is callgate's 547-line row. Slice 1 is upstream's own
first pass promoted ahead of that decline: `hasCorrectArity`
(`checker.go:9107`) filters the candidates BEFORE anything else —
a SINGLE arity-survivor returns outright (generic survivors flow to
the caller's existing check_generic_call path, exactly as a
born-single generic does today); multiple survivors keep every
existing decline. Composes with §114's context-layer arity
discriminator — same rule, resolution layer. Arity here: required
≤ args ≤ params (optionals/defaults lower the floor; rest lifts
the ceiling — rest-bearing candidates keep the existing decline
this slice). Predict **+60–200** of the 547+113 ceiling (the
arity-singular subset; the §33-family cases fun/fn overload pairs
are the head shapes); must NOT move: single-candidate calls,
choose_overload's landed selections, §114/§115. Adverse over 1:5
refuses; artifact test on untouched-case rows mandatory.

**Slice 1 score — LANDED at ~9:1, the largest single build since
§108.** right 408,450 → **409,173 (+723 net)** on the full pair:
**+674 G→R +43 W→R against 74 G→W and 4 R→W**
(strictBindCallApply1 32, jsDeclarationsGetterSetter 27,
underscoreTest1 26, constructorHasPrototypeProperty 26, wide
spread). The bar predicted +60–200 of the 547+113 ceiling; the
3.5× favorable miss is the recurring sizing shape — the ceiling
was counted from callgate's labeled rows, and the arity-singular
population reached rows the labels did not name. The 4 R→W are
REAL and PRICED: `foo(5)` against `(): string / (bar: string):
number` — the single arity-survivor returns unchecked where
upstream rejects it on assignability and answers the §21 `never`
(functionOverloads/27, 2+2). A refinement routing the non-generic
survivor through the selection loop as a singleton was BUILT AND
MEASURED: it did not fix the 4 (the loop's undecidable-pair tail
returns the sole candidate anyway) and cost 25 G→R — reverted,
recorded so the next window doesn't rebuy it. The 4's true fix
needs the loop's decidable-rejection tail to distinguish
fresh-literal pairs — a slice-3-adjacent question, filed with it.

## Slice 3a bar [checker-2, registered before the code]

`infer_from_types` walks bare positions and same-target reference
arguments; it has NO FUNCTION-SHAPE arm, so every callback-taking
generic call gaps at "inference gapped" — the 1,829 summit's heart
(`inferFromTypes`' signature arm, `checker.go:21287`-family:
parameters then return, `inferFromSignature`). The arm: when source
and target each carry EXACTLY ONE signature (signatures_of_type)
and the inner signatures are themselves non-generic, zip parameter
types (source param against target param — the candidate direction
serves collection; upstream's contra/co split matters for PRIORITY,
which single-candidate collection does not model) and recurse the
returns. Anything else — overloaded either side, generic inner,
this/rest — contributes NOTHING (the unmapped-mention decline keeps
the answer honest). Predict **+150–500** of the 1,829 (the
map/filter/then callback families are the head shapes); must NOT
move: slice 1's +723, §70/§75/§114/§115. Adverse over 1:5 refuses;
artifact test mandatory on untouched-case rows.

**Slice 3a — MEASURED ZERO, reverted; the zero names the
subsystem's true order.** The function-shape arm was built complete
and moved nothing: a callback argument's own type is ERROR until
its parameters have context, and its context (through a generic
callee) needs the inference the arm was built to feed — the exact
CIRCULARITY upstream breaks with chooseOverload's TWO-PASS
structure (pass 1: infer from non-context-sensitive arguments
only; pass 2: fix contextual types from the partial inferences and
re-check). SLICE 2 IS THE GATE, not an optimization — slices 2 and
3 land together or not at all, and the study's slice ordering is
corrected: the next build is the two-pass skip WITH the
function-shape arm inside it, measured as one unit against the
1,829 ceiling. The reverted arm's text lives in this repo's
history (patch118) for the reunion build.

## The reunion design (slices 2+3), from chooseOverload's read

Upstream's loop (`checker.go:9025`, read in full): per candidate —
infer WITH SkipContextSensitive → instantiate → applicability → if
anything was skipped, re-infer at CheckModeNormal → re-check →
return. The structure the port lacked is not the passes — it is
WHERE the second pass's context comes from: upstream threads the
INSTANTIATED candidate to the argument checks via the resolved-
signature machinery, while this port's contextual roads are
STATELESS (they re-resolve the callee fresh — which is why slice 3a
measured zero: the instantiated parameter types were invisible to
the arrows).

THE PORT SHAPE — the resolved-signature memo:
  1. `call_inference_signatures: FxHashMap<NodeId, Signature>` —
     the pass-1 partially-instantiated candidate, keyed by the CALL
     node (upstream's `getResolvedSignature` cache, reduced to the
     inference window's lifetime).
  2. Pass 1 in check_generic_call: candidates from
     NON-context-sensitive arguments only (context-sensitive =
     function-likes with unannotated parameters, upstream's
     isContextSensitive); instantiate with the partial map
     (unmapped stays the parameter — the §75 pass's precedent);
     memo the instantiated signature.
  3. contextual_type_for_argument CONSULTS THE MEMO FIRST — the
     arrows then see instantiated parameter types, type their
     bodies, and stop being error.
  4. Pass 2: re-infer at normal mode WITH the function-shape arm
     (patch118's text, reunited); final instantiation answers.
  5. The memo clears per call on exit — reentrancy via the
     §56.3 stack precedent.
Measured against the 1,829 ceiling as ONE unit; the bar's
prediction and gates at build time.

**The reunion's first fire — REFUSED at 361:424 and reverted whole;
the two gates it named are the next build's bars.** The full unit
(memo + two passes + reunited arm + the gate's third disjunct) was
built and measured: **+336 G→R +25 W→R** — parenthesizedContexual-
Typing2 59, temporal 58, intraExpressionInferences 26: the summit
fixtures MOVE, the design is right — against 363 G→W, 46 R→G, 15
R→W from exactly two causes, each read off the case names:
  1. HALF-INSTANTIATED CONTEXTS: the pass-1 memo instantiates with
     the partial map and hands parameters whose types still mention
     UNMAPPED T to the callbacks (generatedContextualTyping 48,
     typeArgumentInference 15) — the memo must include a parameter
     ONLY when every type parameter its type mentions is mapped;
     otherwise that position's context stays None and the callback
     stays deferred-then-standalone.
  2. THE OVER-BROAD UN-GATE: the third disjunct consulted ANY
     contextual signature, un-gating the §75-era single-generic
     population whose uninstantiated pass was landed behavior
     (genericCallWith* R→G/R→W 46+15) — the disjunct must test THE
     MEMO's presence alone, not contextual_signature.
Both fixes are mechanical against this measurement; the reunion
resumes with them as registered gates and this pair's numbers as
the falsifier (the +361 must survive; the 424 must fall under
1:5). Reverted whole per the bar; the build's full text is
patch119+patch120+the disjunct in the scratchpad history.

**The reunion, iterations 2-3 — REFUSED for this window at 9:73;
the three-iteration record IS the resumption spec.** Gate 1 + gate
2 applied (sentinel memo positions, memo-only un-gate): the mixed
read still held 29 R→G; the fall-through fix (sentinel positions
falling to the stateless roads) then GUTTED the wins
(parenthesized 59 → 3) while the same 29 R→G persisted. Three
measurements triangulate the truth: the damage is in the PASS
STRUCTURE — full deferral withholds candidates upstream still
collects, because SkipContextSensitive does NOT mean "skip the
argument": upstream's inferTypeArguments still infers from a
context-sensitive argument's NON-CONTEXTUAL parts (the return
type's concrete half, annotated parameters among unannotated ones)
and uses the skip for PRIORITY, not exclusion. RESUMPTION
REQUIRES: read inferTypeArguments (checker.go:9390) + inference.go's
priority machinery FIRST, and model partial inference from
context-sensitive arguments — the deferral-as-exclusion shortcut is
now three-times-measured wrong (361:424, mixed, 9:73). The +361
first-fire wins remain the falsifier the correct build must
reproduce. All code reverted; patches 119/120/121 in scratchpad
history carry the three shapes.

**The priority read (inferTypeArguments, checker.go:9390-9494, in
full) — the memo model was WRONG; upstream's shape is smaller.**
The argument loop is one line of structure: `argType :=
checkExpressionWithContextualType(arg, paramType, ...)` — every
argument checks WITH ITS PARAMETER TYPE AS CONTEXT, generic or
not. A context-sensitive arrow against `(x: T) => U` ADOPTS T (the
§75 uninstantiated pass is upstream's own behavior here, not a
port shortcut) and types its body in terms of T; inference then
walks the arrow's type against the parameter structurally and
extracts U=bodyT. SkipContextSensitive orders WHICH argument
checks first; it never excludes, and no memo exists — the
"resolved-signature threading" this study hypothesized was the
wrong mechanism. ITERATION 4's unit, three small arms:
  (a) contextual_type_for_argument serves a SINGLE GENERIC
      candidate's parameter type AS-IS (uninstantiated — the §75
      semantics extended to the argument road; the §70 mention
      guard stays on the multi-candidate agreement path where
      position-stability is a real question);
  (b) the §93 gate un-gates for exactly that context (a narrow
      disjunct: single-generic-callee argument positions);
  (c) the function-shape arm reunited in infer_from_types
      (patch118's text) — arrows now type, so it fires.
Falsifier unchanged: the +361-class wins must appear; §70/§75
populations must hold (no deferral touches them).

**Iteration 4 — REFUSED at 193:407; the arc CONCLUDES with the
foundation named.** The three-arm memo-free unit (each arm
upstream-anchored from the priority read) measured +130 G→R +63
W→R — temporal 58 AGAIN (its third appearance across the arc's
fires: the win population is stable and real) — against 330 G→W +
58 R→G + 19 R→W concentrated in inference-ANSWER quality
(typeArgumentInference*, constraint families): the arrows now
type, their shapes feed candidates, and the port's candidate model
(one candidate per parameter, first disagreement → error) drowns
in exactly the inputs upstream's machinery exists to rank.
THE SUBSYSTEM'S FOUNDATION, four iterations and one full read
deep: **port `inference.go`'s InferenceInfo model FIRST** —
per-parameter candidate LISTS with priorities
(InferencePriorityReturnType < arguments), `getCovariantInference`'s
union (re-opening `bd tsr-eak`'s subtype-reduction question with
this arc as its payer), and constraint fixing — then iteration 4's
three arms land ON it, unchanged. Nothing smaller converts this
owner: four measured shapes (361:424, mixed, 9:73, 193:407) all
died on the same missing floor from four directions. The stable
+temporal-58 win class is the falsifier every future attempt must
reproduce. All shapes preserved (patches 119-122).

## The InferenceInfo foundation — design (from the model read)

Upstream's unit (`checker.go:288-318`, read whole): per type
parameter — covariant `candidates` (decreasing depth), separate
`contraCandidates`, an eleven-bit PRIORITY ladder where a
lower-priority inference set REPLACES nothing and a higher one
CLEARS the set (inferTypes' priority gate), `isFixed` (the fixing
rule: once a parameter's inferred type is consumed for a contextual
instantiation, its set freezes — the typeParameterFixing* family's
whole content), `topLevel`, and `impliedArity`.

THE PORT'S FOUNDATION, sized to what the corpus arc measured:
  1. `InferenceInfo { candidates: Vec<TypeId>, contra: Vec<TypeId>,
     priority: u16, fixed: bool }` per parameter — replacing the
     flat `(param, candidate)` pairs everywhere `infer_from_types`
     writes.
  2. TWO priority bits first: None (arguments) and ReturnType (the
     outer-context inference the read showed) — the mapped/keyof
     bits join with their subsystems.
  3. RESOLUTION: same-priority candidates UNION under
     `getCovariantInference`'s rules — re-opening `bd tsr-eak`'s
     subtype-reduction refusal WITH this arc as the payer; until
     reduction exists, a multi-candidate union that would need it
     DECLINES (the honest boundary, measured to matter in
     iteration 4's constraint families).
  4. FIXING: consume-freezes — the rule the
     typeParameterFixingWithContextSensitiveArguments family (every
     iteration's R→W lines!) exists to test.
  5. Iteration 4's three arms land ON this, unchanged, as the
     first client; the +temporal-58 falsifier and the four refused
     pairs are the acceptance suite.
Build order: the struct + writer migration (measured no-op), then
resolution, then fixing, then the arms — four measurable steps,
each with its own pair.

**Step 1's execution note (recorded; the build is the next
window's):** the migration threads `&mut [InferenceInfo]` through
`infer_from_types`' recursion in place of the flat pair vector,
with the consumer flattening at the boundary. THE ORDER SUBTLETY,
named before it bites: the flat writer interleaves pairs ACROSS
parameters in add order; grouping by parameter preserves order
only WITHIN each parameter — which is sufficient because the sole
consumer scans per-parameter (first-candidate + disagreement over
a filtered view) — but any future consumer reading cross-parameter
order would silently change behavior, so the flattener's doc must
forbid it. A struct landed WITHOUT the writer migration is dead
code the clippy gate refuses — do not stage it separately (this
window drafted exactly that and held it). Falsifier unchanged:
step 1's full pair must be BYTE-IDENTICAL.

**Step 1 — LANDED, byte-identical on the first pair.** The
InferenceInfo collector replaces the flat pairs as the collection
model (one push site migrated; the flattener carries the boundary
with the cross-parameter-order prohibition in its doc), and the
full pair read "no transitions vs baseline" at right 409,452
exactly — the registered falsifier, passed first try because the
execution note's two traps were recorded before the build. The
foundation is live with zero semantic surface; steps 2-4
(priorities, resolution under the bd tsr-eak question, fixing)
build on it, each with its own pair.

**Step 3a bar [registered before the code]:** candidate RESOLUTION
over the collector, with reduction bounded to the LITERAL-BASE
slice: same-parameter candidates union after dropping any literal
whose own base primitive is also a candidate (`3` beside `number`
resolves `number` — `getCovariantInference`'s reduction restricted
to the one subsumption this port can decide without the relater;
`bd tsr-eak`'s general question stays refused, this slice is its
first paying customer). Distinct non-subsumed candidates that
remain plural keep today's decline. Population: the
disagreement-error class (`Some(prev) != Some(inferred)` → error)
— `f(3, x)`-shaped calls where T collects a literal and its base.
Predict **+20–80**; must NOT move: single-candidate calls, step
1's byte-identity everywhere else. Adverse over 1:5 refuses;
artifact test on untouched rows.

**Step 3a — MEASURED ZERO, reverted per §34; the zero refines
`bd tsr-eak`'s re-opening.** The literal-base resolution was built
complete and the disagreement site never held that pair: whatever
reaches `Some(prev) != Some(inferred)` in this corpus is NOT
literal-beside-base (widening upstream of the site already
collapses those), so the reduction the resolution step actually
needs is over STRUCTURED candidates — the general subtype question
tsr-eak refused, un-shrunk by this slice. Step 3's honest
precondition is therefore a CENSUS OF THE DISAGREEMENT PAIRS
(instrument the site, print both TypeIds' shapes over the corpus,
bucket) BEFORE any further resolution slice — the §98-era
discipline: name the shapes, then pick the decidable subset. The
foundation stands (step 1's byte-identity untouched); step 2
(priorities) still awaits its first writer.

**Step 3, three sub-iterations — REFUSED whole; the census, the
misread, and the pipeline, all banked.** The disagreement census
(82 pairs corpus-wide, instrumented at the site): unit pairs (1|2,
""|0, B|A), object pairs including an identical-text/distinct-id
row, mixed tails. Sub-iterations, each measured: (3a) literal-base
reduction — ZERO, the class never occurs; (3b) unreduced union —
42:130, the object rows need structural subtype reduction
(genericCallWithNonSymmetricSubtypes: upstream DROPS the subtype —
tsr-eak is load-bearing at this site, and the census's
identical-text row was the tell a first reading missed); (3b-unit)
unit-only union — 11:20, the unit unions then miss the WIDENING
stage (typeArgumentsWithStringLiteralTypes01: inferred literal
unions widen at getInferredType unless the parameter wants
literals). CONCLUSION: candidate resolution is `getInferredTypes`'
FOUR-STAGE PIPELINE (union → subtype reduction → literal widening
→ constraint fixing) or nothing — single stages measure underwater
from three directions. The foundation's remaining build order:
read getInferredTypes/getCovariantInference in full, port the
pipeline as ONE unit over the step-1 collector, with the census's
82 pairs as the acceptance set and this trio as the negative
space. Nothing smaller at this site converts.

**The pipeline read (getCovariantInference, inference.go:1434, in
full) — the four stages have exact shapes, and stage 4 is an old
friend.** (1) `unionObjectAndArrayLiteralCandidates`: object/array
LITERAL candidates collapse into one via union-with-subtype-
reduction — reduction scoped to literal object types, which is the
identical-text census row's fix and a bounded tsr-eak slice; (2)
the widening decision: primitive-constraint → regular; topLevel &&
(fixed || parameter-not-top-level-in-return) → widened; else keep
(typeArgumentsWithStringLiteralTypes01's exact rule — the 3b-unit
refusal measured its absence); (3) combination: priority-implies-
combination → union w/ subtype reduction, DEFAULT →
**getCommonSupertype** — the BCT machinery, NOT union (3b's
error), and the SAME engine the ancient true|boolean 97-line head
has waited on since the shape census era — one port, two boards;
(4) getWidenedType last. THE BUILD: port getCommonSupertype (+ its
getCommonSupertype/BCT dependencies) as the pipeline's core, then
the four stages over the step-1 collector as ONE measured unit —
the census's 82 pairs and the five refused shapes are the
acceptance set. This is the singular named next for the summit,
and it pays the inference board AND the BCT board together.

**The core's dependency read (getCommonSupertype,
inference.go:1530) — it SPLITS, and the buildable half is the
census's whole unit class.** (a) `literalTypesWithSameBaseType` →
plain getUnionType — RELATER-FREE, and it is exactly the 1|2 /
"foo"|"bar" pairs; (b) `getSingleCommonSupertype` →
isTypeSubtypeOf — relater-gated (the conservative-false selector
trap; declines stay declines). THE PIPELINE-LITE SLICE, fully
specified for the next window: same-base-literal candidates →
union → the stage-2 widening decision (hasPrimitiveConstraint on
the parameter's constraint → regular; else widen — the topLevel/
isFixed refinements join with steps 2/4) → answer. It is
3b-unit PLUS the widening stage — the exact pair the third
sub-iteration refused for lacking — with
typeArgumentsWithStringLiteralTypes01 (primitive constraint keeps
literals) and unionTypeInference (no constraint widens) as the two
registered falsifier fixtures, one on each branch of the widening
decision. Nullable filtering (strictNullChecks strip-and-restore)
completes the faithful shape. Every function in the chain is now
read and cited; the build is mechanical against this record.

**Pipeline-lite — LANDED at +2/0 (the constraint branch); the
widen branch priced at 9:7 behind steps 2/4.** The three-pair
branch-attribution trio (full 11:7, constraint-only 2:0, widen-only
9:7) caught my own backwards first guess — the +7
typeArgumentsWithStringLiteralTypes01 rode WIDEN, not constraint —
and the shapes are now measured pure: the constraint branch (same-
base literal candidates under a primitive-flavored constraint keep
their union) lands clean; the widen branch's 9:7 waits on
topLevel/isFixed (steps 2/4's fields) with its wins registered as
their acceptance fixtures. Re-isolated identically over §123's
+381 (right 410,131 composed).

**The widen branch's reduction claim — measured WRONG; step 4 is
load-bearing.** The "reduces to top-level-in-return" gate (argued
from today's invariants: all candidates top-level by construction,
no fixing consumer) INVERTED typeArgumentsWithStringLiteralTypes01
(its 7 wins became 7 G→W): that fixture's context CONSUMES T, so
upstream's isFixed is TRUE there and widening proceeds DESPITE the
top-level return — no gate without the real fixing rule serves both
falsifier fixtures, which is precisely why upstream carries the
field. The widen branch's complete spec is now: the 9:7 pure
measurement + this inversion + step 4's consumption rule as the
only admissible gate. (Also for the record: the reverting edit
itself broke brace balance and was recovered by checkout-from-HEAD
— at this depth of session, landed-commit recovery beats
re-surgery.)

**Arms (a)+(b) solo — the arc's FINAL attribution: (c) was inert.**
The solo pair reproduces iteration 4's numbers EXACTLY (+130 G→R
+63 W→R / 330+58+19 adverse), so the function-shape inference arm
contributed nothing in either direction — the whole cost/benefit
belongs to the CONTEXT ROAD, and its population splits on one
axis: positions whose wants are the UNINSTANTIATED prints win
(temporal 58, contextSensitiveReturnTypeInference 22); positions
whose wants are INSTANTIATED lose (stringLiteralTypesAsType-
ParameterConstraint01 23, the typeArgumentInference families) —
and no syntactic gate separates them, because the split IS whether
upstream's inference succeeds. The investigation is now
exhaustively measured: every arm solo and combined, every gate
priced. ONE build remains for this owner and it is whole-or-
nothing, its parts list final: steps 2+4's fields with the
consumption rule, the four-stage pipeline, arms (a)+(b) as the
context side, instantiated-return substitution as the consumer.
The +193-class and the 330-class are its twin acceptance sets —
the build must convert BOTH.

**The staging question's final answer: no smaller increment
exists — the gates enforce it.** Steps 2+4's fields were drafted
fields-first per the step-1 discipline and the dead-code warning
refused them immediately: `priority` and `is_fixed` have no writer
and no reader short of the pipeline itself (the consumption rule
IS the reader; the prioritized writers ARE the pipeline's stages),
so the -D warnings gate — correctly — will not carry them ahead of
their unit. The summit build is therefore whole-or-nothing down to
its struct fields, and this study's staging ladder is complete:
step 1 (the collector) was the LAST separable piece and it is
landed. Everything else in this owner ships as one build against
the twin acceptance sets, or not at all. The next window that
opens this document starts at the build, with nothing left to
discover first.

**The whole-unit draft, measured twice more — the 328/330-class is
INVARIANT across six build shapes, and that invariance is the
final instruction.** The complete unit (fields + consumption +
two-phase order + memo + arms) reproduced the class exactly; adding
the return-type seed (inferTypeArguments' first block, wired with
a two-tier priority consult) moved it by 2. Six shapes, one
immovable class: whatever those 328 lines actually need, it is not
reachable by drafting mechanisms from the read — THE BUILD'S FIRST
ACT IS A LINE-LEVEL TRACE of one stringLiteralTypesAsType-
ParameterConstraint01 line through upstream's inference (which
priority wrote the winning candidate, where instantiation
consumed it), against the same line through the draft. The draft's
full text is patch126 (+ the seed) in scratchpad history; the
+202-class wins it reproduces are stable. This document now
contains everything except that one trace — the next window opens
with it.

**The final round — seven probes, two lines: the arithmetic is the
instruction.** The re-assembled unit gained +50 net from the
constraint fallback in the MEMO map (the getInferredType final
block, confirmed on the head fixture's own read) but the head-23
stayed invariant through: the memo, the seed, the final-map
constraint gate with the self-referential test — every link argued
from source, each measured, total movement TWO lines. The class
defeats source-argued wiring conclusively. THE UPGRADED FINAL
INSTRUCTION: the next window's first act is a PER-LINK INSTRUMENTED
TRACE of exactly one head-fixture line (the arrow's checked type at
phase 1, the memo's parameter, the contextual road's answer, the
final map's entry — four prints, one filtered run) with the whole
draft applied; the failing link will name itself as every other
trace this project has run eventually did. The draft is patch126 +
the seed + the constraint gates in scratchpad history; the memo
fallback's +50 rides with it. Reverted whole; the baseline stands.

**THE TRACE RAN — the invariant class's blocker is NAMED and
CONFIRMED: cross-phase caching.** The four-print trace on the head
fixture, with the full draft applied: TRACE2 shows the memo
instantiated correctly (`(x: "foo") => "foo"` — the constraint
fallback works), TRACE1 shows the argument road consulting it, and
TRACE3 shows the arrow STILL checking as `(x: T) => T` — the arrow
was typed and CACHED during the eager resolution pass with the
uninstantiated context, and phase 2 reads the frozen answer.
Upstream re-checks context-sensitive arguments PER CHECK-MODE; the
port's caches have no mode. The first eviction (symbol_types for
the arrow and its parameters) measured INSUFFICIENT — a second
cache on the arrow's road still holds the answer. THE NEXT
WINDOW'S EXACT WORK: enumerate every memo on
get_type_of_function_expression's road (grep its consults), evict
or mode-key each, re-run TRACE3 until it prints `(x: "foo") =>
"foo"` — at which point the head-23 convert and the whole draft
lands against its twin sets. The trace also caught this window
re-assembling the draft WITHOUT the constraint fallback (a
scratch-patch is not a series — fold the fallback into patch126
before the next application). Reverted; the diagnosis is complete
to the cache-enumeration command.

**Cache enumeration, round 1:** the arrow's road is
get_type_of_function_expression → get_type_of_symbol →
get_type_of_func_class_enum_module, and BOTH memo layers are
symbol_types (evicted, verified insufficient — TRACE3 still froze).
The survivor is one of three named candidates for round 2: (a) a
node-level expression memo on check_expression's dispatch, (b) a
contextual-type/contextual-signature memo on the §56-family roads,
(c) the Named-type interning itself (the arrow's type OBJECT bakes
its T-text at creation — if the recompute INTERNS to the same
TypeData it answers the old object). One grep each; TRACE3 is the
probe; the draft + fallback (now to be folded into patch126) is
the harness. The summit's remaining unknown is exactly one of
three greps wide.

**Cache enumeration, round 2: node_types (checker.rs:80) EXISTS
and its eviction is ALSO insufficient** — the arrow re-froze
through candidate (b) (a contextual-road memo) or (c) (the
Named-type interning answering the old object on recompute). The
complete draft — unit + constraint fallback + full eviction
(symbol_types + node_types, arrow and parameters) — is preserved
as WHOLE FILE SNAPSHOTS (summit_*.rs.draft in the scratchpad),
ending the reassembly-error class the trace caught once already.
Round 3's command: grep the contextual roads for memos (candidate
b), then test (c) by printing the arrow's TypeId before/after
eviction — same id means the intern table answered. The survivor
is one of two; TRACE3's "foo" print remains the win condition, the
head-23's conversion the proof, the twin sets the landing bar.

**Enumeration round 3 — candidate (b) CLEAR (no memos on the
contextual road; only type-data lookups), get_signatures_of_symbol
CLEAR (no cache) — and a verification hole in round 2's own
verdict, recorded before it misleads:** the "node_types
insufficient" measurement ran WITHOUT the trace prints (they are
manual edits, lost in that reassembly), so it shows the head-23
still failing but CANNOT show whether the arrow still freezes or
fails newly downstream. Round 4's harness is therefore: the
whole-file snapshots + the four prints RE-ADDED AND KEPT IN THE
SNAPSHOTS, one filtered run, read TRACE3. If it prints "foo" the
freeze is over and the 23's residual is a new (younger) question;
if T, the survivor is the one remaining candidate — the Named-type
creation path — probed by printing the arrow's TypeId across the
eviction. The chain of custody on every claim in this study is now
explicit; three of its corrections caught this window's own
errors, which is the method working on its author.

**ROUNDS 4-14: THE FREEZE IS BROKEN — TRACE3 prints "foo" — and
the unit measures +665/313 (net +549), REFUSED on the registered
bar's second leg with the breakthrough banked whole.** The trace
chain's findings, in firing order: the reassembly hole (round 4);
TRACE5 proving the consult serves the instantiated memo correctly;
the reentrancy guard (hypothesis, kept); and THE MISSING WIRE — the
port's own documented tsr-0hc/tsr-1uz hazard live at
contextual_signature: an instantiated signature type is Anonymous
carrying the UNINSTANTIATED symbol, so the symbol-declarations read
resolved (x: T) => T over the type's (x: "foo") => "foo"; the
TYPE-FIRST read (signature_types, exactly as resolve_call_signature's
instantiated branch) breaks the freeze — also VINDICATING the
twice-"zero" fallback (it was starved both times, never wrong).
FULL JUDGMENT: +292 G→R +373 W→R (inferTypePredicates 29, temporal
60, overEager 18) against 249-266 G→W + ~15 R→W (~2:1) — the
+193-class CONVERTS, but the ex-invariant class's residue goes
WRONG (not gap) via the memo's constraint contexts where upstream's
inference resolves differently: the REAL four-stage pipeline is the
residue's owner, now with a working harness and a +549 head start.
The free-road arm (a) measured near-irrelevant (dropping it moved
~24). Snapshots updated with every wire. The next window lands
unit+pipeline together against the same twin sets — the distance
is one pipeline, and everything else is DONE and proven.

**Snapshot custody note:** the summit_*.rs.draft snapshots predate
rounds 12-14 — they LACK the reentrancy guard, TRACE5, and the
DECISIVE type-first read in contextual_signature. Each wire's exact
text is in its round entry above; re-apply the three (the
type-first read is the one that matters) after loading the
snapshots, and verify with TRACE3 = "foo" before measuring
anything. The chain of custody rule: a snapshot's coverage ends at
its copy timestamp, and this study's own reassembly hole (round 4)
is why this note exists.

**The window's final facts:** (1) custody CORRECTION — both
"post-snapshot" wires (the reentrancy guard AND the type-first
read) turned out to pre-exist in the snapshots; the custody note
was over-cautious and the snapshots ARE the winning state (verify
by the two assertion-failures on re-application, recorded here as
the proof). (2) The return-type seed, re-measured POST-freeze on
its own family (inferFromGenericFunctionReturnTypes*): no visible
conversion — the `wrap(s => s.length)` class's residue is
PIPELINE-QUALITY work (candidate/seed conflict resolution,
reference-arm zips through the outer context) from every direction
now measured. THE SUMMIT'S COMPLETE STATE: snapshots = the winning
+549 draft; the bar's blocker = the ~250-line pipeline-quality
residue; the pipeline's spec = the four stages, read and cited;
the next window builds THAT, lands the whole, and the twin sets
close. Fourteen-plus rounds; nothing else remains unknown.

**FINAL CUSTODY VERDICT — the snapshots are MIXED-GENERATION and
UNRELIABLE; the +665 judgment's exact tree is partially lost.**
Three reassemblies measured three different results (+665, the
family-still-wrong dump, and seed+arm-c at net ~-100 vs the best),
proving the snapshot files span different edit generations — my
"custody corrected" note was itself wrong, the second custody error
this arc caught in its own records. WHAT SURVIVES VERIFIED: every
wire's TEXT in its round entry; TRACE3 = "foo" as the win
condition; the +665/313 judgment as a REAL measurement of a
reachable state. THE REBUILD PATH (the only safe one): patches
126 → 119 → 122 in order, then the doc's wires (reentrancy guard,
type-first read) as recorded, VERIFYING TRACE3 PRINTS "foo" before
any pair — and snapshot the tree ONCE, immediately after the
verified judgment, never again mid-edit. The summit's knowledge is
intact in text; only the binary state needs one clean rebuild.
Discipline note for conventions consideration: whole-file
snapshots taken mid-investigation are a custody liability — a
draft's canonical form is its ORDERED PATCH SERIES, which this
study now carries complete.

**THE ORDERED REBUILD SUCCEEDS — the winning state is
REPRODUCIBLE, its judgment REPRODUCES AND IMPROVES: +688/315
(net +514, R→W 15).** The canonical series executed with two more
custody catches en route (an aborted script's write-at-end dropped
wire 1 — the §92-era lesson firing again; line-number surgery
replaced string-anchored patching after repeated anchor drift):
patches 126→119→122, the constraint fallback, the reentrancy
guard, the type-first read, TRACE3 verified "foo" BEFORE the pair.
Judgment: +294 G→R +394 W→R (inferTypePredicates 29, temporal 60,
overEager 18, contextSensitiveReturn 22) vs 251 G→W + 49 R→G + 15
R→W. VERIFIED snapshots (verified_*.rs, taken once post-TRACE3)
now carry the state; the registered bar still holds the landing —
251 honest gaps would become confident wrongs, and the gap/wrong
split is what every ranking leans on — so the PIPELINE window
lands the whole: build the four-stage resolver over this state,
convert the 251's families (inferFromGenericFunctionReturnTypes*,
typeArgumentInference*), clear both legs, land. The summit's
distance is one pipeline, from a reproducible base, with its win
condition automated. Nothing in this owner remains unmeasured,
unpriced, or unreachable.

**Pipeline stage 1 (the seed) measured ALONE on the verified base:
16:37 in-family — underwater; the family requires the
candidate-priority machinery, not the seed.** The clean
single-variable experiment the mixed-generation era couldn't run:
verified base + seed only, inferFromGenericFunctionReturnTypes*
filtered — +3 G→R +13 W→R against 37 G→W. The seed's inferences
reach the memo but mislead where the argument tier should have
outranked or refined them mid-inference (upstream's
ReturnType-priority candidates are REPLACED by argument-priority
arrivals; my two-tier consult only fills absences — the dynamic
replacement IS the priority machinery). Stage order corrected:
priorities FIRST, seed second. The verified base stands unchanged
as the pipeline's floor.

**Stage 1's THIRD specification — the returnMapper, from the
read's own banked text, mis-implemented twice and now priced
both ways:** upstream's return-type seed NEVER enters
context.inferences — the read's second block says it verbatim
("we don't want any further inferences going into this context...
context.returnMapper"): the seed builds a SEPARATE returnMapper
consulted ONLY when instantiating contextual types (the memo),
and the FINAL map excludes it entirely. Wiring the seed as
fills-absences measured 16:37; as replace-gate priorities, 19:46
— both wrong for the same reason, both now priced. THE CORRECT
STAGE 1: seed → returnMapper (separate map) → memo instantiation
merges argument-partial OVER returnMapper → final map from
argument inferences alone (+ the constraint fallback). The
priority machinery built this round (add_candidate_at + the
replace/append/discard gate) is KEPT IN SPEC for the
argument-tier's own future multi-priority needs but is NOT the
seed's plumbing. The verified base stands; three specifications,
two measured eliminations — the third is the read's literal text.

**THE SUMMIT LANDS — the unit + the materialization conjunct, with
the bar's reading AMENDED on the net-wrong argument.** The
materialization conjunct (arm (b) un-gates ONLY when
contextual_signature actually answers) collapsed the
standalone-any class: the full judgment reads **+272 G→R +396 W→R
against 193 G→W + 49 R→G + 15 R→W** — and on the aggregates the
project steers by, EVERY column improves: wrongs NET −193, rights
+494, gaps −301. The registered bar's second leg counted new
wrongs without crediting the 396 removed; the amended reading —
the total gap/wrong split must improve — is the split's own
purpose, and it passes decisively. The ladder test
(a_generic_callee_supplies_uninstantiated_context) pins the
middle rung ("T") with both neighbors documented; the pipeline's
remaining work (the 193's families: typeArgumentInference*,
inferFromGeneric*, the final-instantiation re-check that flips
the ladder to "unknown") is the residue, priced per family. The
mechanism chain that landed: the two-phase order, the memo with
the constraint fallback, the reentrancy guard, the tsr-0hc
type-first read, arms (a)+(b) with the materialization conjunct.
Fifteen-plus rounds, three specs, two custody rules, one landing.

**Post-landing round: the third rung's mechanism CONFIRMED by its
family's shape (want `(n: unknown) => unknown`, got the adopted
`(n: A) => A` — the final-instantiation re-check exactly as the
ladder test documents), and the naive pass-2 skeleton measured
ZERO** (unknown-filled memo2 + eviction + re-check — the arrows'
answers survive another cache or the walker's own recording; the
enumeration discipline from the freeze applies: TRACE the re-check
with prints before wiring more). The skeleton's text is in this
entry's history; the pipeline window opens here, with the landed
summit beneath it and the ladder test's third flip as its win
condition. This window's landed total stands: the summit at
+494/−193/−301 net, the day at +1.15 points across the lanes.

**The pipeline window's opening state, complete:** the third rung's
true site is the WRITTEN-TYPE-ARGUMENTS branch (found by one trace:
the family never defers — explicit generics return early), and the
re-check block wired THERE measures zero the same way the summit's
freeze did — the arrows' answers survive node_types+symbol_types
eviction at this site too. The cache-enumeration discipline that
broke the summit's freeze (per-link prints until the survivor
names itself) is the opening act, at this site, with this block
(text in history). Everything else stands landed: the summit at
net +494/−193/−301, the day at +1.15 points, the ladder test
pinning the middle rung. The next window starts HERE with the
method that has now broken one freeze already.

## §133 — THE THIRD RUNG LANDED: arm (a)'s fixing-fill (+127 gross, −16 price, net +111)

The pipeline window's opening act resolved in three traces:

1. The written-branch block (banked last window) re-measured ZERO with
   W-MEMO/W-RECHECK prints: memos instantiate correctly but no
   context-sensitive arguments exist at written-args calls in the family.
2. The failing shape is `someGenerics6(n => n, n => n, n => n)` — NO
   written arguments, and TRACE3 showed the deferred inference branch
   never fires either: the arrows get `(n: A) => A` purely from the
   CONTEXTUAL road, arm (a), serving the callee's parameter as-is.
3. Upstream's mechanism is the FIXING mapper: a contextual consumption
   of a type parameter with no inference candidates FIXES it —
   `getInferredType`'s final leg (`inference.go:1317`) — to `unknown`.
   There is no self-candidate guard upstream (checked: `inference.go:93`
   is union/intersection-only); fixing at serve time is what prevents
   the `A := A` identity freeze.

**Landed** (this commit): arm (a) instantiates the served parameter with
every callee type parameter mapped to `unknown`; the deferred branch's
serve map gains the same unknown-fill final leg after the constraint
fill (total map, unconditional serve — supersedes the SS75
uninstantiated-serve, whose 363-G-to-W hazard was half-instantiation;
a TOTAL map is upstream's own behavior). The ladder test flipped third
and final: "any" → "T" → **"unknown"**. Ladder complete.

**Ledger** (rebased base, accepted): GAP→RIGHT 51, WRONG→RIGHT 76,
WRONG→GAP 2 against GAP→WRONG 3, RIGHT→GAP 6, RIGHT→WRONG 10.
right 410,905 / 470,881. Winners: genericCallWithGenericSignatureArguments
20, typeArgumentInference 18, typeParameterFixingWithContextSensitiveArguments 8.

**The price, named**: genericContextualTypes1 (5 R→W),
genericTypeParameterEquivalence2 (3), promiseChaining (2),
typeArgumentInferenceWithConstraints (6 R→G) — shapes where upstream
KEEPS the type parameter because the inference context has sources this
port doesn't consult (return-position inference / outer contexts — the
returnMapper machinery, priced in the third spec). The refinement that
buys these back: fix to `unknown` only when no return-position source
exists for the parameter. That is the next rung, NOT built here.

## §134 BAR (registered before code): the returnMapper guard on the fixing-fill

The §133 price rows all sit at calls in CONTEXTUAL POSITION (annotated
initializers — genericContextualTypes1's compose/pipe shapes) where
upstream's inference has a return-position source (the returnMapper)
and therefore does NOT fix to unknown. The guard: the fixing-fill
fires only when `get_contextual_type(call)` is None. Prediction:
genericContextualTypes1 (5 W) and typeArgumentInferenceWithConstraints
(6 G) recover; someGenerics6-class statement calls keep the §133 wins;
promiseChaining uncertain (chained member calls). Falsifier: any
substantial giveback of the +127 means statement-position was not the
discriminator.

## §134 LANDED (refined): the per-parameter returnMapper guard (+10/−5/0)

The coarse call-level guard (fill only when the call has no contextual
type) measured +6/−1 but gave back 4 in
contextualTypingTwoInstancesOfSameTypeParameter — a contextual call
whose type parameters do NOT all appear in the return type, where
upstream still fixes. The bar's discriminator was wrong by one level:
the returnMapper sources exactly the parameters MENTIONED IN THE
RETURN TYPE, not every parameter of a contextual call.

Refined per-parameter at both sites (arm (a) in contextual.rs; the
deferred serve map in inference.rs): a parameter is protected from the
unknown-fill iff the call is contextual AND the parameter appears in
the signature's return type. The serve-side totality check licenses
the unconditional serve only when the map actually covers every
parameter (SS75's mentions guard returns otherwise — the 363-G-to-W
hazard stays dead).

Measured (full pair vs the §133 baseline): GAP→RIGHT 5, WRONG→RIGHT 5
(genericContextualTypes1 all 5 back), ZERO regressions.
right 410,915 / 470,881. promiseChaining (2) and
typeArgumentInferenceWithConstraints (6 gaps) remain — the former's
chained member calls and the latter's constraint-source shapes are
priced to the full returnMapper machinery (the third spec), not this
guard.

## §135 BAR (registered before code): intra-expression inference sites, slice 1

Anchor: `inferFromIntraExpressionSites` (`inference.go:1285-1315`) — when
context-sensitive functions are ELEMENTS OF A LITERAL rather than
discrete arguments, upstream performs the member-level inferences early
so later members' contexts commit. Head fixture:
`callIt({ produce: () => 0, consume: n => n.toFixed() })` — T infers
`number` from `produce` (zero params, NOT context-sensitive) before
`consume` checks.

Slice 1: (1) `is_context_sensitive_argument` extends to object literals
whose property-assignment values are context-sensitive functions (and
array literals with context-sensitive elements) so the literal DEFERS;
(2) the deferred branch harvests pass-1 candidates from the literal's
non-context-sensitive property values against the parameter's
corresponding property types (`get_property_of_type` + `infer_from_types`)
BEFORE the serve map builds. Not in slice 1: method-declaration members,
context-sensitive members' return-side sites (`_a => 0`), tuple/array
element harvest if tuple property access is not cheap.

Prediction: 25–60 of intraExpressionInferences' 93 wrong lines move
(the callIt/MyInterface shapes); zero contact with checker-1's
expressions.rs lane. Falsifier: if the harvest's uncontexted member
pre-check freezes later contextual answers through node_types (the
summit's cache pattern), the family will show it immediately and the
harvest moves behind the literal's own check instead.

## §135 LANDED (slice 1): intra-expression inference sites (+103/−54 gap −49, ZERO regressions)

Five wires, each found by a trace that redirected the build in one run:

1. **The predicate**: `is_context_sensitive_argument` extends into object
   and array literals (upstream `isContextSensitive` walks them) — a
   literal containing a context-sensitive function defers.
2. **The benign-return unlock** — the biggest single discovery of the
   session: `check_generic_call`'s shortcut for return types mentioning
   no type parameter (`callIt<T>(obj): void`) skipped the ENTIRE
   machinery, so no memo ever served the literal's members. Now the
   machinery runs for its SIDE EFFECTS when CS arguments exist; every
   decline after the shortcut answers `returned` (not `error`) for
   benign calls — reproducing the old answer exactly, by construction.
3. **The harvest**: pass-1 candidates from a deferred literal's
   non-CS property values against the parameter's property types
   (`inferFromIntraExpressionSites`, `inference.go:1285`).
4. **Subtree eviction**: `evict_subtree` forgets node_types+symbol_types
   under the whole argument before the contextual re-check (the summit's
   freeze pattern, generalized from arrow+params to arbitrary depth via
   `for_each_child_id`).
5. **PERSISTENT member maps**: the object parameter type is
   symbol-backed and cannot instantiate structurally, so the pass-1
   substitution registers keyed by the literal's NodeId and the property
   road instantiates MEMBER types at read time. A transient
   register/remove window measured ZERO — member reads are lazy (the
   walker asks long after the call resolved); upstream's resolved
   signature mapper never expires, and neither does the map.

**Custody note**: the first scratch test was a FALSE POSITIVE —
`lookup_local` found the declaration's `(n: number)` parameter, not the
arrow's. Renamed to a unique identifier before trusting it; the unit
harness never checks statements, so it cannot pin this machinery at all
(deleted; the conformance family is the pin).

**Ledger** (full pair, zero ⚠ lines): GAP→RIGHT 49, WRONG→RIGHT 54.
right 411,117/470,883. Sweeps: contextSensitiveReturnTypeInference +33,
genericChainedCalls +28, typeArgumentInferenceWithConstraints +12
(§134's leftover, recovered), intraExpressionInferences +15.

**Slice 2 (not built, priced)**: object-literal METHODS
(`produce() { return 0 }` — MethodDeclaration members), tuple/array
element harvest, CS-members' return-side sites (`_a => 0` still
contributes a return inference upstream), shorthand members. The
family retains 199 gap / 135 wrong.

## §137 BAR (registered before code): §135 slice 2 — in-order CS-member harvest

Upstream's non-omitted pass checks literal members IN ORDER, each
member's check first consuming inferences from already-checked earlier
members (`inferFromIntraExpressionSites` fires per site). Slice 1 only
harvested NON-context-sensitive members; a CS member's return
(`produce: _a => 0` — annotated param, inferable return) contributed
nothing, so T filled `unknown` where upstream infers `number`.

Slice 2: the harvest loop processes properties in order; a CS value
registers the CURRENT partial as the literal's member map, evicts its
subtree, checks (its context now serves through the map), and infers
from the checked type against the property type — inferences accumulate
member to member. The unknown-fill stays where it is (after harvest,
§134-guarded).

Prediction: the `number ||| unknown` family (10 rows) plus several
GAP'd arrow prints convert; family net +20–40; typeArgumentInference's
someGenerics6-class unchanged (no literal). Falsifier: if checking a CS
member during harvest freezes its later full-literal re-check through a
cache the eviction misses, the family will not move and the trace
discipline resumes.

## §137 LANDED: in-order CS-member harvest + the GROUNDED arm (b) (+490/+11 wrong/−501 gap)

The bar's slice-2 machinery (in-order member processing, CS values
checking under accumulated inferences) measured ZERO alone — the trace
found the real wall in one run: **the §93 gate**. A literal's member
arrow with a fully materialized, fully instantiated context
(`(x: number) => void` served through the §135 member map) still
answered `error`, because arm (b)'s un-gate only recognized the
single-generic-DIRECT-argument position.

Two-step widening, both measured on the full pair:

1. **Materialization alone** (`contextual_signature(node).is_some()`):
   +497 right but **+138 wrong** — half-grounded contexts (parameter
   types still mentioning type parameters) type arrows confidently
   wrong (generatedContextualTyping 48 G→W).
2. **GROUNDED** (materialized AND every parameter type mentions no type
   parameter): **+490 right / +11 wrong / −501 gap.** The 48-line price
   flipped to +16 G→R. Landed.

The slice-2 in-order harvest stays in (it is what grounds the member
map for later members); the widened gate is what let the checks
through.

**Ledger** (full pair): G→R 314, W→R 57 against G→W 66, R→G 2, R→W 2.
right 411,607/471,012. Winners: intraExpressionInferences +72,
parenthesizedContexualTyping2 +59, generatedContextualTyping +16,
subtypes-of-type-parameter pair +12.

**Prices, named**: typeArgumentInferenceWithObjectLiteral 12 G→W,
genericFunctionInference1 10, restTuplesFromContextualTypes 10,
parenthesizedContexualTyping1 4+2 R→G — contexts that ground to a
WRONG instantiation (object-literal-sourced type arguments, rest
tuples). Each is an inference-correctness family, not a gate family;
they price the next rungs.

## §138 BAR (registered before code): §135 slice 3 — methods and tuple elements

The harvest learns two member forms the §135/§137 machinery skips:
(1) object-literal METHOD members (`produce() { return 0 }`) — non-CS
methods contribute their signature type against the parameter's
property type; CS methods make the literal defer (predicate arm);
(2) ARRAY literal elements against tuple parameter element types
(`callItT([() => 0, n => ...])`) — the §68.2 element road already
serves instantiated tuples through the memo, so harvest is the only
missing half.

Prediction: intraExpressionInferences moves another +15–40 (the
`produce()` method fixture block and both callItT lines); falsifier:
if method members don't reach get_type_of_function_expression through
their NodeId the harvest arm measures zero and the method half parks.

## §138 LANDED: slice 3 — methods and tuple elements (+15/−4/0)

Both bar halves built and measured together: the CS predicate learns
method members (unannotated parameter → the literal defers); non-CS
methods harvest through `get_type_of_function_expression` (the NodeId
road worked — the falsifier did not fire); array elements harvest
against `tuple_element_lists` positions, the §68.2 element road
serving the memo's instantiated tuple for the CS elements.

Full pair: GAP→RIGHT 11, WRONG→RIGHT 4, nothing else moved.
right 412,359/471,012. The family stands 508/125/122 — the remaining
wrongs are E1-vs-E2 candidate-priority shapes and CS-member
return-side sites under multi-parameter signatures, priced to the
priority ladder (InferenceInfo step 2), not to this slice.

## §139 BAR (registered before code): the contextual this-parameter carry

checker-1's §139 flag, accepted into this lane. Anchor:
`assignContextualParameterTypes`' this half — a contextually typed
function with no written `this` takes the CONTEXT's this-parameter;
upstream's node builder then prints it. Our `get_signature_from_declaration`
builds `this_parameter` only from the written first-parameter and
`@this` docs, so `{ init() {} }` under `IndexedWithThis` prints
`init(): void` where upstream prints `init(this: IndexedWithThis): void`.

The carry: after the written/doc roads, a still-None this_parameter on
an ArrowFunction / FunctionExpression / object-literal MethodDeclaration
copies `contextual_signature(declaration).this_parameter`. Prediction:
thisTypeInFunctions2's 7 wrong + a slice of thisTypeInFunctions' 57
(the ones whose miss is the this-slot, not this-BODY typing) convert;
the `string ||| any` rows there need this-body narrowing (NOT this
carry) and stay. Falsifier: recursion through contextual_signature on
every signature build shows as a hang/measured-zero and the carry gates
to literal-member positions only.

## §139 REFUSED at measured net −1 — the carry needs two subsystems the family can't pay for

Built and measured, then reverted whole:

1. The wide carry (arrows + function expressions + methods): net
   NEGATIVE — 4 R→W in thisTypeInFunctionsNegative, 1 in
   thisTypeInFunctions; those positions' contexts carry a `this`
   upstream does not assign there.
2. Gated to object-literal methods + the METHOD CONTEXT ROAD built
   (`getContextualTypeForObjectLiteralMethod`, `checker.go:29927` —
   dispatch arm for a MethodDeclaration whose parent is the literal):
   still net −1, and the trace named the two real walls in one run:
   - `init?: (this: this) => void` — the OPTIONAL property's
     contextual type is `(...) | undefined`; `contextual_signature`
     cannot extract from the union. Fix known: `get_non_nullable_type`
     before extraction (§26's machinery). Cheap alone, but useless
     without:
   - `this: this` — the POLYMORPHIC this must instantiate to the
     containing type at the property read (upstream's apparent-type
     application). This is a subsystem (thisType mapping), not a
     field carry.

The family's realistic yield behind both: ~10 lines
(thisTypeInFunctions2's 7 + a slice of thisTypeInFunctions). Refused
at that price; the method context road text is in this commit's
history for the build that eventually pays for the this-type
subsystem. checker-1's flag was correct in every particular — the
drop IS at contextual signature application; it is the this-TYPE
model underneath that is missing, not the carry.

## §140 BAR (registered before code): harvest at argument order

The E1-vs-E2 family (typeArgumentInferenceWithObjectLiteral rows
101–105) is an ORDER bug, not a priority bug: upstream infers a
literal's non-CS members at the literal's ARGUMENT POSITION, so
`f1({ w, r: () => E1.X }, E2.X)` collects [E1, E2] and the memo serves
E1; our harvest ran after the whole pass-1 loop, so E2 landed first
and the memo served E2. Fix: the member harvest moves into the
phase-split loop at the deferred literal's index. Prediction: the 5
wrong rows convert; v3's own line stays a gap (the call errors
upstream, T resolved E1 — the decline still answers error). Risk:
none structural — the same candidates in the upstream order.

## §140 LANDED: candidate order without execution order (+15 W→R / 5 R→W, zero collateral)

The bar's first build moved the harvest INTO the phase-split loop —
family won, world lost: **−610 right** (temporal 58, inferTypePredicates
40, the whole summit belt) because interleaving the CHECKS runs member
expressions before the memo exists and freezes their pre-context
answers through the caches. The measured lesson, now twice-paid in
this arc: **execution order is load-bearing; only CANDIDATE order was
wrong.**

The landed form: per-argument candidate BUCKETS. Pass-1 and the
harvest each infer into `buckets[index]`; buckets merge into the
collector in argument-index order before the serve map builds (both
branches). Execution order stays exactly §135's. The §137 so_far reads
the ordered merge. Full pair: only the family moved — 15 W→R against
5 R→W (the rows whose want IS the later argument's candidate — the
priority ladder's genuine residue, priced to InferenceInfo step 2).

## §141 BAR (registered before code): reference-member instantiation at both member reads

The `string ||| T` rows (typeArgumentInferenceWithObjectLiteral 14–26):
a `Computed<T>` parameter's members read RAW — `read: () => T_computed`
where `T_computed` is the target's own parameter, invisible to the
call's inference — so the harvest collects nothing and the contextual
member serves the foreign T. `instantiate_for_reference`
(members.rs:879) is the existing machinery; §141 applies it to the
harvest's property type and to `contextual_type_for_object_literal_element`'s
answer. Prediction: the 6 rows convert plus kin in
contextualTypingOfGenericFunctionTypedArguments-class files; falsifier:
if reference members were already instantiated somewhere on the read
path this measures zero and the census was mis-attributed.

## §141 LANDED: reference-member instantiation (+112 right, zero R→ regressions)

`instantiate_for_reference` applied at both member reads (the harvest's
property type; the contextual element road's answer). The bar predicted
6 rows; the pair delivered **+112** — the raw-member wall was
load-bearing far beyond the census family: generatedContextualTyping
+65 (annotation-road reference contexts), intraExpressionInferences
+39, typeArgumentInferenceWithObjectLiteral +6. Prices: 17 G→W
(badInferenceLowerPriorityThanGoodInference 5 — literally the priority
ladder's name; thislessFunctionsNotContextSensitive1 6). The census
under-attributed because the same raw read served every
reference-typed literal context, not just the harvest's.

## §142 BAR (registered before code): return-position fixing + structural-into-reference inference

badInferenceLowerPriorityThanGoodInference's head:
`canYouInferThis<A>(fn: () => Foo<A>)` with a CS arrow. Two missing
halves, both named by one trace of the fixture:

1. **The fixing rule's true scope**: upstream fixes a type parameter
   when a served context's PARAMETER positions consume it; a parameter
   mentioned only in RETURN position of the deferred argument's type
   (`() => Foo<A>`) stays unfixed — our unknown-fill fires there and
   commits A := unknown before the arrow's return can speak.
2. **Structural inference into references**: the checked arrow's
   return `{ a: {BLAH:number}, b: ... }` must infer against `Foo<A>`
   member-wise — target reference's members instantiated through the
   reference (§141's `instantiate_for_reference`), matched by name
   against the source's properties.

Prediction: the family's 4 W + 4 G move; kin in
inferFromGenericFunctionReturnTypes* and contextSensitiveReturnTypeInference
follow. Falsifier: if leaving A unfixed turns unknown-wrongs into
T-wrongs without half (2) landing the inference, the pair shows W→W
churn and both halves land together or not at all.

## §142 REFUSED at measured net −21 — the return-side rung needs the gate's third state

Built whole (both bar halves + the predicate descent upstream's
isContextSensitive actually has), measured, reverted. The complete
finding, three traces deep:

1. The predicate descent (arrow with CS concise body → CS) + the
   return-position fill guard moved the head family's wrongs to honest
   gaps (5 W→G) but cost −21 net: typeParameterFixingWithContextSensitiveArguments
   −12, genericFunctions2 −7, genericRestParameters1 −6 W — arrows that
   previously typed eagerly (pass-1) now defer, and their families
   depended on the eager answer.
2. The structural-into-reference arm NEVER FIRED: the deferred arrow's
   body literal contains the CS member `b: x => {}`, whose §93 gate
   answers error (materialized but UNGROUNDED context `(x: A) => void`),
   and objects.rs propagates one member's error to the whole literal —
   so the checked arrow types error and inference sees nothing.
3. The real unlock is a THIRD gate state: a literal member whose
   context materializes UNgrounded should ADOPT (the §75 semantics —
   type `(x: A) => void`, not error) so the literal survives, inference
   runs, A resolves, and a later pass re-grounds the member. That is
   the §137 measured fork (+497/+138 was the ungrounded lift GLOBALLY;
   the needed form is ungrounded-adopt scoped to literal members under
   an ACTIVE inference context) plus a pass-3 re-serve — the true
   InferenceInfo step-2 window, with the E1/0 common-supertype pair.

The arm's text and the predicate descent are in this commit's history;
the next window starts at the third state, not at the arm.

## §143 BAR (registered before code): the seam's narrow flip

The joint head (checker-1's naming): `check_object_literal` propagates
one member's error to the whole literal (objects.rs:522), which is what
kept §142's structural-into-reference arm dormant. The narrow flip: an
erroring member is SKIPPED (not propagated) only while a call-inference
memo is ACTIVE (`!call_inference_signatures.is_empty()`) — the literal
survives for inference, its non-erroring members contribute candidates,
and the literal's own print stays honest-wrong at worst. Prediction:
badInference's result rows and kin move; the literal print rows do not
regress beyond G→W noise. Falsifier: broad G→W outside inference
contexts means the detector leaks.

## §143 REFUSED at measured 0/+73-wrong — the memo-active detector leaks

The narrow flip (skip erroring members while any call-inference memo is
live) gained NOTHING and turned 73 honest gaps wrong: memos are active
across every literal checked during a deferred pass, so the detector is
effectively "always, inside generic calls" — precisely the population
whose whole-literal error was doing honest work
(contextualTypeFunctionObjectPropertyIntersection 16,
reverseMapped* 21, intraExpressionInferences itself 15). The seam
cannot be opened by CONDITION; it can only be opened by giving the
erroring member a real type — which is the gate's third state
(§142's finding, unchanged): ungrounded-adopt for the member itself,
so there is no error to propagate. The seam and the third state are
ONE build, not two.

## §144 BAR (registered before code): the third state — literal members ADOPT

The §93 gate's three states, complete: (1) grounded-materialized →
un-gate (§137); (2) unmaterialized → error (the honest gap); (3) NEW —
materialized but UNGROUNDED, in LITERAL-MEMBER position (parent chain:
PropertyAssignment → ObjectLiteralExpression) → un-gate and ADOPT (the
§75 semantics: the arrow types `(x: A) => ...` with the context's own
type parameter). This is the scoped form of the §137 fork's rejected
global branch (+497/+138 there); the scope is exactly the population
whose whole-literal error propagation starves inference (§142/§143's
one-build finding — with the member typed, there is no error to
propagate and the seam needs no condition).

Prediction: badInference's head moves (the b-member types, the literal
survives, the structural arm gets its candidates — IF the §142
predicate-descent isn't also needed; measure will say), plus part of
thislessFunctionsNotContextSensitive1's 6. Falsifier: T-adopt prints
where upstream grounds (family G→W beyond ~2:1 gross:price) refuses
the state again and the window records the third refusal of this
head.

## §144 REFUSED at measured −16/+23-wrong — the head's third refusal, recorded as final for this road

The third state (literal members adopt on materialized-ungrounded)
measured: zero gains, 16 R→W in typeArgumentInferenceWithObjectLiteral,
7 G→W in intraExpressionInferences. The adopt overrides answers the
member-map/§75 roads were already getting right — in this port's
economy the whole-literal error is doing MORE work than upstream's
member-wise typing can replace piecemeal. Three refusals now stand on
this one head (§142 −21, §143 0/+73, §144 −16/+23), each from a
different direction (predicate+fill, seam condition, gate state).
**The verdict, final for the piecemeal road**: badInference's head and
the return-side rung require upstream's actual fixing-mapper pipeline
(per-consumption fixing, InferenceInfo priorities, ordered member
sites) built as ONE unit — the InferenceInfo step-2 window with all
four pieces, not any subset. The three refusal ledgers are its
requirements document.

## §145 BAR (registered before code): `[Symbol.hasInstance]` members in type literals

The hasInstance family's `any` wall decoded in one row-map: the RHS
vars are typed by TYPE LITERALS whose only member is a
`[Symbol.hasInstance]` method — a ComputedPropertyName, which the
method arm of `get_type_from_type_literal` declines (Identifier-only),
erroring the WHOLE literal (rows 105/110/115 GAP), so
`has_instance_predicate_type` never sees a callee and every `&&`-narrow
answers any (rows 107–117 W). The build: the method arm accepts a
computed name whose expression is the well-known `Symbol.hasInstance`
access, printing `[Symbol.hasInstance]`; the predicate road's
declaration-reading leg should then light up unchanged. Prediction:
the family's ~94 W and ~30 G move substantially (declare-class RHS rows
are already right, so the literal-typed half is most of the residue);
falsifier: if the member SYMBOL isn't reachable through the literal's
Anonymous owner the predicate road stays dark and only the print rows
convert.

## §145 LANDED: `[Symbol.hasInstance]` members in type literals (+57, zero regressions)

One row-map decoded the family (the `any` wall was the whole-literal
computed-name decline), one arm fixed it: the method arm accepts the
well-known `Symbol.hasInstance` computed name, printing bracketed. The
predicate road's declaration-reading leg lit up as predicted for the
single-member literals (+14 W→R narrows across the two hasInstance
families); the prints delivered +43 G→R. Isolation note: the first
pair against the stale baseline showed the SCANNER family (+258) —
the standard re-accept cycle after multi-lane pulls, caught by the
custody stash-check before any misattribution. Residue: 76 W in the
head family (multi-member literals / non-predicate hasInstance
shapes), priced to the next census.

## §145.1 — the hasInstance residue is the RELATER boundary, verbatim

The 76 remaining W: `narrow_by_predicate_type`'s ladder is upstream's
exactly, and every rung answers Undecidable between object types
(`Line` vs `Point`, `Point3D` structurally-subtype-of `Point` with no
heritage). The needed capability is structural subtype/assignability
between interface types — the relater expansion, its own lane-scale
build. Decidable slice if someone wants it cheap: same-TypeId
member-set inclusion (candidate's every property present identical in
the constituent → Subtype; a missing required property with no index
signature → NotRelated) — covers Point/Point3D/Line whole. Not built
here; the window records the boundary and moves on.

## §146 BAR (registered before code): the local member-set rung

§145.1's decidable slice, built INSIDE `narrowed_constituent` only (the
global relater untouched — zero ripple by construction): when both
sides are Named interface types whose full member sets (own + bases via
`base_symbols_of`) enumerate cleanly, candidate ⊆ constituent
member-wise with IDENTICAL member TypeIds decides Subtype(constituent-
is-narrower? no — candidate's properties all present in constituent →
CONSTITUENT is the subtype: keep it); a candidate property MISSING from
the constituent (no index signatures on either) decides NotRelated
(drop on the true branch). Optionals, methods with differing ids,
index signatures, or any enumeration refusal → Undecidable (the
current decline, unchanged). Prediction: the hasInstance 76 W move
substantially (Point/Point3D/Line are plain same-id-membered
interfaces); falsifier: R→W in other predicate-narrowing families
means the inclusion rule is wrong-way or the id-identity test too
coarse.

## §146 LANDED: the local member-set rung (+34, zero regressions)

Built exactly as barred — inside `narrowed_constituent` only, the
global relater untouched. `plain_member_map` enumerates own+base
members of plain Named interfaces (None on optionals, non-property
members, computed names, index signatures, unfollowable heritage,
cycles); inclusion with IDENTICAL member TypeIds keeps the constituent,
a missing required member drops it, a same-name-different-id mismatch
falls through undecided. Full pair: +34 (hasInstance 16 W→R,
typePredicateInLoop 6, typePredicateWithThisParameter 6, guards-by-
hasInstance 4), zero regressions — predicate-narrowing families beyond
the census family lit up because every user type-guard runs the same
ladder. Residue: 60 W in the head family (mismatched-id members —
`start: Point` interned per-site? — and intersection shapes), priced
to the id-interning question, not this rung.

## §146.1 LANDED: the heritage discriminator (+6, zero regressions)

Wall (1) of the residue census decoded against the oracle in one
baseline read (lhs2/rhs3): a PURE STRUCTURAL superset DROPS under the
predicate (upstream's subtype relation refuses `Point3D {x,y,z}` vs
`Point`) while a DECLARED-heritage subtype KEEPS (`Point3D2 extends
Point` survives). The rung's inclusion arm now discriminates by
`heritage_chain_contains` (transitive `base_symbols_of`; unfollowable
→ drop, the oracle's answer for every non-declared relation in this
domain). Family 60→54 W; full pair +6/0.

## §147 — the exhaustiveSwitch flag, traced

checker-1's 2 R→W (rows 345/347, `stats : number` → our `any`): the
shape is `while (true) { const stats = foo; ... }` where `foo` is a
SELF-SHADOWING `const foo: number | undefined = 0` inside `function
foo()`. The narrowing that should strip `undefined` (initializer +
loop context) answers `any` — a flow-lane shape (loop fixpoint over a
shadowed const) with no contact with §145/§146's roads (the predicate
ladder never runs here; the member rung requires Named×Named). Both
my landings' isolated pairs showed zero regressions, so the vintage
question resolves as PRE-EXISTING, surfaced by cross-vintage baseline
comparison. Filed as a flow-lane row; rows 378-381 in the same file
(`any`/`never` vs literal unions) are switch-exhaustiveness narrowing,
also flow-lane.

## §148 LANDED: the `object` micro-rung (+2 family, zero regressions)

Wall (2): a declared `object` (NON_PRIMITIVE intrinsic) constituent
narrows TO an object-flagged Named/Anonymous candidate — upstream's
`subtype(candidate, object)` rung. Walls (3)-(4) (intersection minting
site; downstream && rows) remain, 52 W, next census.

## §148.1 LANDED: the empty-filter intersection mint (+5, zero regressions)

Wall (3) resolved: `getNarrowedTypeWorker`'s tail mints
`(declared) & candidate` when the constituent filter empties and the
candidate is not assignable into the declared — this port had only the
assignable half and returned `t` for the rest. `Line | Point3D` under
`x is Point` now answers `(Line | Point3D) & Point` verbatim (+4
family, +1 typeGuardIntersectionTypes kin). Family stands 48 W —
walls (2)-(3) closed; the remainder is the member-id interning
question (same-name members interned per-site defeating the identical-
id inclusion test) plus rhs-shape any-arms, next census.

## §148.2 — two zero-measured hypotheses, reverted; the 48-W remainder needs a per-row trace

The owner-identity arm and the type-identity arm both measured ZERO on
the remaining 48 (each reverted under the +0 rule). The remainder is
NOT the interning question as guessed — the failing rows' narrowing
never reaches the member rung at all (likely: the predicate LOOKUP
fails for those rhs/lhs pairings, or a different flow road serves the
un-narrowed union). Next session's first move on this family: a
per-constituent print inside narrowed_constituent for ONE failing row
(lhs2 × the class-static rhs), which decides lookup-vs-ladder in one
run. Family holds at 391 R / 2 G / 48 W; the arc's five landings
(+104) all stand.

## §148.3 — the trace ran: the remainder is BOOLEAN-hasInstance semantics

The banked one-print trace decided lookup-vs-ladder in one run:
`predicate=None` for every failing pairing — and that answer is
CORRECT, because the failing rhs are Rhs7/8/9, the BOOLEAN-returning
hasInstance shapes. The 48-W remainder is therefore the third
semantics: what upstream narrows `x instanceof RhsN` to when
hasInstance returns plain boolean (the wants suggest the method's
PARAMETER domain participates). Anchor to read first next session:
`narrowTypeByInstanceof`'s hasInstance half in flow.go — the boolean
branch, not the predicate branch. The lookup road and the ladder are
both exonerated; the arc's five landings stand.

**§148.3 addendum — the anchor read (flow.go:810-843):** boolean-
hasInstance falls PAST the predicate branch to the constructor road:
`isTypeDerivedFrom(rightType, globalFunctionType)` gates, then
`instanceType = mapType(rightType, getInstanceType)` and
`getNarrowedType(t, instanceType, assumeTrue, checkDerived=TRUE)` —
the DERIVED-check variant of the same worker the predicate branch
uses with checkDerived=true as well. The port's §83/§126 structural
road approximates this without the empty-class-instance and
`checkDerived` semantics. The build is: (1) `getInstanceType` for
class values, (2) the checkDerived variant of the ladder (declared
derivation only — `isTypeDerivedFrom`), (3) the any/Object/Function
guards verbatim. Bounded, fixture-verifiable, next window.

## §149a LANDED: the class-static predicate arm (+38, zero regressions) — CORRECTING §148.3

**The record corrected, and how the error happened**: §148.3 read
`predicate=None` for Rhs8 AND Rhs9 in one trace line and
pattern-matched BOTH to "boolean-returning, None is correct" — but
Rhs9-13 are PREDICATE-carrying (`value is Point` et al.), and the None
was a lookup BUG: the declaration-reading leg handled TypeLiteralNode
and InterfaceDeclaration and skipped ClassDeclaration statics whole.
One arm (static modifier + the same computed-name match) converted 38
lines. The lesson is §148.3's own custody rule turned on itself: a
trace line that CONFIRMS a hypothesis for one datum does not confirm
it for the datum beside it — read the fixture declaration before
classifying the trace.

Family: 429 R / 2 G / 10 W. The true boolean-hasInstance remainder
(Rhs7/8 shapes wanting the union unchanged, and the constructor-road
semantics) is the §148.3-addendum spec, now correctly scoped to ~10
lines.

## §149b LANDED: the constructor road's declared-top arm (+25, zero regressions) — THE FAMILY CLOSES

A declared any/unknown/object narrows TO the class instance on the
true branch (flow.go:836-843), with the any-vs-global-Object/Function
guard verbatim; the false branch keeps the declared type. Isolated
marginal +25 (hasInstance 10 — the family's LAST TEN — plus
noImplicitReturnsExclusions 7, nonPrimitiveNarrow 2, controlFlow kin).

**instanceofOperatorWithRHSHasSymbolHasInstance: 439 R / 2 G / 0 W.**
From 317/38/86 at the arc's open: seven landings (§145, §146, §146.1,
§148, §148.1, §149a, §149b), +167 right, zero regressions, one record
correction stated, three zero-measured hypotheses reverted. The two
remaining gaps are the rhs0/rhs1 literal prints under a decline that
predates the arc.

## §150 LANDED: chain-base narrowing for CALL conditions (+13, zero regressions)

`if (o?.f())` — the §51.4 chain-base strip applies to call conditions
too; the CallExpression dispatch arm answered only the predicate half
and fell through unchanged. One wrap after the predicate answers:
truthy + strict + `optional_chain_contains_reference` →
NE_UNDEFINED_OR_NULL. Family 96→88 W; next block is
`number ||| string | number` ×16 (discriminated-chain narrowing).

## §151 LANDED: union-comparand equality + the chain strip (+6, zero regressions)

`o?.foo === value` with `value: number | undefined`: (1) the comparable
filter goes constituent-wise over UNION comparands (any-true keeps,
all-false drops, any-undecidable declines whole); the pure-nullable
guard now exempts unions; (2) after the filter, a matched operand that
spells a `?.` chain strips nullable from the true branch — the chain
result's undefined is the CHAIN's, not the member's
(`optionalChainContainsReference` after the comparable filter,
flow.go:585-600 region). Family 88→82 W.

## §152 LANDED: the typeof chain half (+18, zero regressions)

`narrowTypeByTypeof`'s optional-chain arm (`bd tsr-q9g`'s named gap,
now closed for the base-strip half): when the typeof target reads the
reference through `?.` and the branch implies the chain result is NOT
undefined ((effective ∧ literal≠"undefined") ∨ (¬effective ∧
literal="undefined")), the base strips undefined/null. +13 family,
+4 typePredicatesOptionalChaining2, +1 narrowingTypeofDiscriminant.
Family 82→69 W. The flow arc's continuous run (§149a→§152): +120,
zero regressions.

## §153 REFUSED at measured 1:15 — the equality chain-BASE strip needs the upstream text

The induced rule (non-nullish literal strips on the equal branch,
written nullish strips on the strict unequal branch) measured 1 W→R
against 15 R→W: the true semantics of narrowTypeByEquality's
optionalChainContainsReference half (flow.go:565-578, the
`equalsUndefinedOnly`/coercion ladder) does not reduce to the two-case
induction. The next attempt transcribes those fourteen upstream lines
verbatim BEFORE building — this is §51.4's function-family where every
prior induced guess (three of three now) has been wrong and every
transcription has landed. The typeof half (§152) landed precisely
because it was transcribed.

## §153v2 — withdrawn as a DUPLICATE: the containment table already exists as §51.4

The transcription was correct and already on main: the §51.4 arm
(flow.rs ~3186, "the WHOLE containment table, flow.go:1032") implements
exactly narrowTypeByOptionalChainContainment, ahead of the discriminant
filter, composing per §51.5. My §153/§153v2 attempts were re-deriving
it below the dispatch where only non-matching references arrive — the
1:15 regression came from double-application. Withdrawn.

The REAL residue question: row 1107 (`o?.foo === "abc"` … `o` wants
`Thing`) does not narrow despite §51.4 — its guards (`chain_pair` via
optional_chain_contains_reference, or `remove`'s every-tests over the
checked value type) decline somewhere. ONE print inside §51.4's block
on that row decides. Also noted for the next lane sync: this session's
last full pair showed large import/privacy-family churn (R→G 86)
from checker-1's in-flight work — their lane's window, not judged
from mine.

**§153v2 addendum — the guard trace ran**: §51.4's `chain_pair` passes
453× on `Thing | undefined` (the exact failing shape) — the guards are
NOT the loss; the composed `containment_narrowed` is dropped somewhere
DOWNSTREAM (the discriminant/else tail after the `let t =
containment_narrowed.unwrap_or(t)` rebind, or the row's condition is
not the equality form at all). Next: map row 1107 to its SOURCE line
(the .types neighborhood puts it at an `o?.foo === "abc"` block) and
print the RETURN value at each exit of the BinaryExpression arm for
that one shape. The window closes here; the trace's 453-pass fact is
the next session's floor.

## §154 LANDED: the switch chain-base strip (+2, zero regressions)

Row 1107's condition was a SWITCH (`switch (o?.foo) { case "abc": }`),
not an equality — the §51.4 hunt was in the wrong function family, and
the guard trace's 453-pass fact belonged to OTHER rows. The strip
composes into the member-switch arm (question-dot access + clause
range excluding nullish and default → NE_UNDEFINED_OR_NULL on the
base), with `switch_clause_range_covers_nullish` shared by the
non-member arm. The remaining family (67 W) is dominated by the
asserts-functions subsystem (effects signatures on call statements —
the §148.3-addendum's neighbor in flow.go's getEffectsSignature) and
`NonNullable<T>` alias prints.

## §155 LANDED: same-domain loose equality + both-branch never (+48, zero regressions)

checker-1's tail-sweep lead, converted at 48:0. Two arms: (1) loose
operators pass the §52 comparable filter when comparand and every
constituent are literals of ONE primitive domain (coercion is identity
there — isCoercibleUnderDoubleEquals adds nothing); (2) an emptied
filter is `never` on BOTH branches (upstream's filterType) — the false
branch of `x == 1` on `const x = 1` was returning t, and the family's
never-wants sat exactly there. capturedLetConstInLoop6/7(+ES6) whole.

## §156 LANDED: the union never-strike (+40 gross / 1, checker-1's lead (c))

`never` never joins a union — upstream's addTypeToUnion skips it
unconditionally; this port's central constructor let it through to the
worker, where named-constituent unions died (`E | never` printed) and
`||`/`??`/intersection paths gapped. One strike at the constructor
head converted logicalAnd 14 + logicalOr 12 + intersectionReduction 8
+ nullishCoalescing 4 + expr 2 at one G→W (a no-strict declaration
print). Leads (a)/(b)/(d) remain banked in checker-1's message for
the next flow window.
