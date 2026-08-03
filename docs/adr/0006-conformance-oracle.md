# ADR-0006: Assert conformance against generated Go, not against `ast.json`

**Status:** Accepted
**Date:** 2026-08-02
**Follows from:** [ADR-0005](0005-codegen-from-ast-json.md)

## Context

We generate the AST from `_scripts/ast.json` ([ADR-0005](0005-codegen-from-ast-json.md)).
The obvious way to test that would be to re-read `ast.json` and check the
generator's output against it.

That test is nearly worthless. It asserts the generator against its own input —
so it catches generator bugs, but it cannot catch the case where **the input
itself is wrong**, which is precisely what happened.

At the pinned commit, `ast.json` lists **349** kinds while
`internal/ast/kind_generated.go` — the artifact the Go compiler actually uses —
has **351**. `DeferKeyword` and `JSDocAllType` are absent from the JSON's element
list.

## Decision

Conformance is asserted against `internal/ast/kind_generated.go`: parse the Go
enum and compare names, ordinal positions, and marker resolution.

## Reasoning

The oracle should be the artifact whose behaviour we are trying to match. The Go
compiler's behaviour is defined by the Go enum, not by the JSON that happened to
generate it. Testing against the generator's input tests the wrong thing.

Concretely, `crates/tsr-ast/tests/kind_conformance.rs` asserts:

- Every kind in the Go enum exists here, and vice versa — reporting the **full**
  disagreement rather than the first, so a regeneration gap is visible in one run.
- Each kind's **discriminant value** equals its ordinal position in the Go enum.
  Not merely that `SyntaxKind::ALL` is in the same order: those two properties can
  drift independently, and only the discriminant is what range predicates test.
- Marker constants resolve identically, following alias chains — upstream has
  `LastToken = LastKeyword = DeferKeyword`, so a single-hop resolution is wrong.

`node_conformance.rs` complements this by re-deriving expected node fields from
`ast.json` and comparing against the **generated source text**. Reading back
generated source is deliberate: Rust has no reflection, and this is what catches a
member silently dropped by a deserialisation mismatch. Asserting the generator's
output against its own in-memory model would have caught nothing.

## Verification

A conformance test that cannot fail is worse than none, so each was verified
against injected drift:

| Injected fault | Result |
|---|---|
| Delete `BinaryExpression.right` from generated source | `BinaryExpression: missing fields ["Right"]` |
| Swap two kind discriminants | Caught — *after* strengthening. The original test compared `ALL` ordering only and **passed**, which is the gap this ADR exists to close. |

The second row is the substantive finding: the first version of the kind test was
insufficient, and only injecting the fault revealed it.

## Consequences

- The conformance test contains a small Go-source parser. It is brittle against
  upstream reformatting, and it asserts loudly (`failed to parse any kinds`) if it
  extracts nothing, rather than passing vacuously on an empty list.
- Tests skip when the submodule is absent; CI checks out submodules.
- When `ast.json` lags, we currently inherit the lag. Closing that gap — deriving
  kinds from the Go enum directly, or reporting the divergence as a warning — is
  not yet done.

## How we would know this was wrong

If upstream reformats `kind_generated.go` such that the parser silently extracts a
truncated list. Guarded by the non-empty assertion, but a partial extraction would
still pass. A stronger check would compare against `KindCount`.
