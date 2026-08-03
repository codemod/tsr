# ADR-0001: Idiomatic Rust rewrite, not a mechanical transliteration

**Status:** Accepted
**Date:** 2026-08-02
**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`

## Context

typescript-go is ~301k LOC of non-test Go. Two ways to move it to Rust:

1. **Mechanical transliteration** — keep Go's file layout, function names, and
   control flow near 1:1, mapping `*T` to typed arena indices. Ugly Rust, but
   upstream commits can be diffed and replayed onto the port, and behavioural
   divergence is easy to localise.
2. **Idiomatic rewrite** — redesign around enums, ownership, and Rust idiom.

The forcing constraint is that upstream is *actively developed*, and the 60k-LOC
checker receives a steady stream of correctness fixes.

## Decision

Idiomatic rewrite.

## Consequences

**Accepted cost:** the port permanently forks from upstream. Every upstream fix
must be re-derived by hand against a structurally different codebase rather than
applied as a patch. Over a multi-year effort against an actively maintained
checker, this is the single largest ongoing risk in the project.

**Gained:** exhaustive `match` over union types instead of upstream's untyped
`*ast.Node` plus runtime kind checks; a `Send + Sync` AST that makes parallel
checking sound rather than merely careful; no `Rc<RefCell<_>>` anywhere in the hot
path.

Three mechanisms contain the cost, all Phase 0 and all load-bearing rather than
nice-to-have:

1. **Upstream-anchored doc comments** ([conventions](../conventions.md)) so drift
   tracking is mechanical.
2. **A drift tracker** that walks upstream commits since the pin, classifies each
   by `internal/` package, and files a `bd` issue.
3. **A baseline ratchet** — conformance pass-rate is a CI gate that may only go
   up, so a missed upstream fix surfaces as a baseline diff rather than as a
   silent divergence found by a user.

Mechanisms 2 and 3 are **not yet built** (`bd` issues `tsr-l68`, `tsr-bb4`). Until
they are, the containment is theoretical.

## How we would know this was wrong

If the drift tracker's backlog grows faster than it is drained for two consecutive
quarters, the fork cost is exceeding the idiom benefit, and transliteration of the
checker specifically should be reconsidered. The checker is separable: nothing
about this decision requires the *whole* port to share one fidelity strategy.

## Alternatives

**Mechanical transliteration** wins if upstream's churn rate in the checker stays
high and our re-derivation throughput is the bottleneck. It was rejected because
the resulting Rust would forfeit the exhaustiveness and thread-safety properties
that motivate using Rust at all — leaving a slower, uglier Go.
