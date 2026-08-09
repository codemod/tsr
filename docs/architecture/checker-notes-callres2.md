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
