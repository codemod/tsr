# The signature-links table (§469) — the §445 refusal's reopening condition, built

**Lane**: checker-1. **Landed at**: the §469 commit (see STATUS.md §1 for the
measured numbers). Upstream anchors are to the pinned submodule commit.

## The forcing constraint

STATUS.md §5 recorded §445's refusal in these words: generalising the
literal-contextual return read beyond written annotations — the call-argument
road, where the fnexpr 98-line literal-contextual class mostly lives —
**overflowed the stack on the full corpus**, because a call argument's
contextual signature resolves the callee and the callee's own type computation
can re-enter the same return inference. Upstream breaks that cycle with
`resolvingSignature` parked in `signatureLinks` (`checker.go:8427`, read at
`:29785`), and the refusal named its reopening condition: *a signature-links
table (park a sentinel while a call's signature resolves; answer it on
re-entry)*.

## What was built, and why it is two sets rather than a links cache

Upstream's `signatureLinks` is a per-node store serving **two** roads:

1. **the call side** — `getResolvedSignature` parks `resolvingSignature` under
   the CALL node before resolving and caches the result after
   (`checker.go:8410-8440`); `getContextualTypeForArgumentAtIndex` reads the
   sentinel and answers it instead of re-resolving (`:29785`).
   `getTypeAtPosition` on the parameterless, restless sentinel is `anyType` at
   every index (`relater.go:1757`), so a re-entrant argument is contextually
   typed `any` for the duration.
2. **the declaration side** — `getSignatureFromDeclaration` caches the built
   signature under the DECLARATION node, so a function expression's signature
   materializes once and any cyclic re-query hits the cache instead of
   rebuilding.

This port transcribes each side to the one bit its road actually reads:

- `Checker::resolving_signature_calls` (`checker.rs`) — call nodes whose
  contextual-argument resolution is in flight;
  `contextual_type_for_argument` parks on entry to its resolving section and
  answers `Some(any)` on re-entry, exactly the sentinel's `getTypeAtPosition`
  semantics. The park deliberately spans only the resolving tail: the pass-1
  memo consult above it (`call_inference_signatures`) is this port's stand-in
  for a *completed* `resolvedSignature` read and must stay reachable on
  re-entry paths that arrive after pass-1 populated it.
- `Checker::contextual_return_in_flight` (`checker.rs`) — declarations whose
  `inferred_return_type` is currently consulting the contextual road; a
  declaration already in flight declines the consult and keeps the pre-§469
  answer, widening.

**The alternative taken seriously**: a real per-node cache (park sentinel,
store result, serve repeats) rather than in-flight sets. Rejected for now
because this checker rebuilds signatures per query *by design elsewhere too*,
and introducing one memoized road would make its answers order-dependent
relative to every unmemoized one — the exact class of bug §162's collapsed
receiver was. What would make the cache win: a measured performance wall, or a
divergence traced to rebuild-vs-cache answer drift. Neither is on the board.

## Why the declaration side exists at all (the measured second cycle)

The call sentinel alone still overflowed, deterministically, on
`conformance/intraExpressionInferences`. The depth probe (TSR_CTX_DEPTH, since
removed) showed **one declaration re-entering the arm at every depth to the
cap** — a true self-cycle: the pass-1 memo consult answers before the call
sentinel, and the memo's instantiated parameter type carries the argument
literal's OWN member symbols; reading a member back out re-builds that
member's arrow signature, whose return inference asks the contextual road
again. Upstream cannot loop here because of the declaration-side cache (2
above). The in-flight set is that cache reduced to its cycle-breaking bit.

A 32-deep lid was tried first, and the history is worth exact wording because
it reversed once: the lid stopped the overflow but the histogram under it
showed the SAME declaration marching to whatever cap was set — a cycle wants
a breaker, not a lid — so the in-flight set replaced it. Then the
in-flight-only build measured **nondeterministically**: one full scorepair
run clean, the rerun of the same binary overflowing, `traceone` on the case
flipping with the environment. The parks break true cycles, but genuine
DISTINCT-declaration nesting still sat at the 8 MiB edge, because each
consult's subtree re-runs call resolution unmemoized. So the third piece is a
**depth budget of 16** (`contextual_return_depth`), the same policy as the
binder's and printer's `MAX_DEPTH`: the walk carries its own bound rather
than the harness growing a stack nobody else has
(`tsr-conformance/src/main.rs`, the WORKER_STACK comment). Productive depth
measured ≤ 9 corpus-wide in the §469 histogram; past the budget the arm
declines and the answer is the pre-§469 one, widening. All three pieces are
needed: parks without budget → jitter-dependent overflow (measured);
budget without parks → the cycle marches to the cap and burns it (measured).

## The generalized §445 arm, and its deliberate asymmetry

`inferred_return_type` now consults `contextual_signature` at EVERY position
where one materializes, not only under a written annotation. The tri-state
asymmetry with the written arm is deliberate and load-bearing:

- **written annotation, nil signature** → GAP (unchanged): under a visible
  annotation this port cannot tell nil-because-none from
  nil-because-unported, and widening there manufactures wrong answers.
- **any other position, nil signature or undecidable literal test** → WIDEN
  (this port's standing answer, and upstream's own answer on a nil contextual
  type). Only a decided `Some(true)` from `is_literal_of_contextual_type`
  (`checker.go:25522`) changes an answer, so the arm can only keep literals
  upstream keeps.

Known, accepted miss: the ASYNC road compares the candidate against the raw
contextual return rather than `GetPromisedTypeOfPromise` of it
(`checker.go:20407`) — shared with the pre-existing written arm, recorded
here rather than silently widened scope.

## The measured result

Scorepair against the clean §467 baseline (474,196 lines): **32 W→R against
7 R→W**, gap column unmoved, net +25 right. All 7 R→W sit in
`conformance/typeArgumentInferenceWithObjectLiteral`, a case already failing
in the baseline (21 WRONG rows): `f1({ w: x => x, r: () => 0 }, 0)` — the
in-flight decline starves a pass-1 inference that previously converged on
`number`, and the members print their fresh literals. Recordable under the
window's rule (R→W in an already-failing case, favorable side a 4.6:1
multiple).

## How you would know this was wrong

- The 7 R→W growing beyond their one case, or appearing in a PASSING case —
  the scorepair protocol catches both.
- `conformance/intraExpressionInferences` overflowing again — the whole
  conformance run aborts, which no one can miss. The park mutation was
  verified both ways on 2026-08-13: guard present → case passes; guard
  ignored → overflow. The distilled unit fixture in
  `tests/return_inference.rs` measured VACUOUS under the same mutation (the
  cycle needs lib machinery the unit harness does not load) and its comment
  says so — it pins the answer, the corpus case is the gate.
