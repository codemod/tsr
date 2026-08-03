# ADR-0002: Build our own AST matching TypeScript's shape, not `oxc_ast`

**Status:** Accepted
**Date:** 2026-08-02
**Relates to:** [ADR-0004](0004-oxc-inspiration-not-dependency.md)

## Context

[oxc](https://github.com/oxc-project/oxc) has a production TypeScript parser that
reaches **100%** on the TypeScript conformance corpus (`parser_typescript.snap`,
positive cases), and `oxc_ast` covers full TS syntax. Adopting them would delete
our entire Phase 1 and much of Phase 2.

But `oxc_ast` is *deliberately* not TypeScript's AST. Their `ARCHITECTURE.md`
states the intent plainly: oxc removes what it calls estree's "ambiguous nodes",
splitting `Identifier` into `BindingIdentifier` / `IdentifierReference` /
`IdentifierName`, and follows ESTree conventions throughout.

Meanwhile typescript-go's 60k-LOC checker, its 39k-LOC language service, and all
49,354 reference baselines are written against TypeScript's AST shape.

## Decision

Build our own AST: TypeScript's node kinds, TypeScript's node shapes,
TypeScript's `SyntaxKind` numbering.

## Reasoning

The checker dominates total project cost and its fidelity *is* the product.
Building on `oxc_ast` would impose a per-function translation tax across the two
largest components (checker at 60k LOC, language service at 39k) in order to save
a parser port — and upstream's scanner plus parser is only **13.3k LOC**
(`internal/scanner` 4,256 + `internal/parser` 9,040), which we can validate
exhaustively against 49,354 baselines.

Paying a tax on the 99k-LOC side to save work on the 13k-LOC side is the wrong
trade. Worse, it would put a permanent semantic seam between us and every upstream
fix we need to track, directly undermining the containment mechanisms in
[ADR-0001](0001-idiomatic-rewrite.md).

oxc's AST design is *right for oxc* — a linter and bundler benefit from
disambiguated nodes. It is wrong as a substrate for a port whose oracle is
TypeScript's own baselines.

## Consequences

- We own a scanner and parser: ~13.3k LOC equivalent, plus the conformance work to
  prove them. Neither exists yet (`bd` epic `tsr-pum`).
- We are not coupled to oxc's release cadence for our most fundamental type.
- Exhaustive `match` over TypeScript's own union shapes, which is what the ported
  checker code will be written against.

## How we would know this was wrong

If the parser port stalls past its phase gate while `oxc_parser` remains at 100%,
the trade was misjudged. The cheap check — porting ~5 representative checker
functions onto `oxc_ast` and measuring the friction — was planned as spike
`tsr-5e7.1` and closed as decided without running, on the reasoning above. That
spike remains the right experiment if this decision is ever revisited.

## Alternatives

- **Build on `oxc_ast`/`oxc_parser`/`oxc_semantic`.** Rejected above.
- **Contribute into `oxc_type_checker` upstream.** They have the tsgo driver shell
  (2,274 LOC) and a 144-line no-op checker scaffold; we want the checker. Worth a
  conversation regardless (`bd` issue `tsr-5e7.2`), but it does not resolve the AST
  shape question — their checker would be built on `oxc_ast` too.
