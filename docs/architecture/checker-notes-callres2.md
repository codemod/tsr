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
