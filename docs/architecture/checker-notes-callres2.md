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
