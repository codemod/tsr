# ADR-0004: Take oxc's designs and dependency choices; link no `oxc_*` crate

**Status:** Accepted
**Date:** 2026-08-02
**Relates to:** [ADR-0002](0002-own-ast.md)

## Context

oxc (~960k LOC) has solved, in production, most of the infrastructure problems
this port would otherwise hit: arena allocation with a thread-safe pool, typed
indices with niche optimisation, self-referential AST storage, diagnostics
rendering, module resolution.

An earlier draft of the plan proposed depending on the published crates directly —
`oxc_allocator`, `oxc_index`, `oxc_span`, `oxc_diagnostics`, `oxc_resolver`.

## Decision

Inspiration, not dependency. We take oxc's **designs** and its **third-party
dependency choices**, but link no `oxc_*` crate.

## Reasoning

The `oxc_*` workspace crates are pre-1.0 (currently `0.138.0`) and release weekly
with breaking changes. That is the wrong thing to pin a multi-year, 300k-LOC
port's *foundational types* to — `Span` and `NodeId` appear in every signature in
the codebase, and a breaking change to them is a whole-repo churn event on
someone else's schedule.

Their *dependencies*, by contrast, are mature and independently versioned. oxc's
selections amount to a well-tested bill of materials for exactly this workload,
and adopting it directly gets the engineering without the coupling:

| Need | Crate | Note |
|---|---|---|
| Arena | `allocator-api2` + `hashbrown` | **`oxc_allocator` is not bumpalo** — it is a custom allocator over the `Allocator` trait. Its `AllocatorPool` (thread-safe reuse across parallel workloads) is the design to copy |
| Typed indices | `index_vec` + `nonmax` | `NonMaxU32` makes `Option<NodeId>` 4 bytes via niche optimisation |
| Self-referential AST storage | `self_cell` | See [ast.md](../architecture/ast.md) |
| Hashing / maps | `rustc-hash`, `hashbrown`, `papaya` | `papaya` is a concurrent map — relevant to parallel checker link stores |
| Diagnostics | `miette` | labeled spans, LSP-compatible |
| Parallelism | `rayon` | |
| Strings / vectors | `compact_str`, `smallvec` | |
| Number formatting | `dragonbox_ecma` | covers tsgo's `jsnum` (584 LOC) outright |
| Scanner | `memchr`, `simdutf8`, `unicode-id-start`, `phf` | `phf` for keyword lookup |
| Globs / JSONC | `fast-glob`, `json-strip-comments` | tsconfig `include`/`exclude` |
| LSP | `tower-lsp-server`, `ropey` | replaces much of tsgo's 21k-LOC `lsp` |
| Position lookup | `rust-lapper` | interval tree — tsgo's `astnav` / `positionmap` |
| Graphs | `petgraph` | flow graph, module graph |
| Testing | `insta`, `similar`, `criterion2` | `similar` replaces upstream's `patience` diff |

So `tsr-core`, `tsr-path`, `tsr-vfs`, and `tsr-diagnostics` do get written, but as
thin TypeScript-specific layers over the above rather than from-scratch
infrastructure. We are writing a `Span` newtype, not an allocator.

## Consequences

- More code than depending on oxc directly, but bounded and shallow.
- No coupling to a pre-1.0 release cadence for types that appear everywhere.
- We copy specific designs deliberately and cite them: `AllocatorPool`,
  `multi_index_vec!` (struct-of-arrays keyed by index, itself modeled on Zig's
  `MultiArrayList`), `self_cell` for arena+AST storage, `tasks/ast_tools`-style
  codegen, and the committed conformance snapshot format.

## Open question

`oxc_resolver` is the one genuine exception worth revisiting: it is
independently versioned at **11.23.0** — post-1.0, mature, used by rspack — and
module resolution is a large, fiddly, well-specified problem. If `tsr-module`
proves expensive, depending on it is a reasonable trade that does not touch
foundational types.

## How we would know this was wrong

If we spend more than a few weeks total on infrastructure that a stable oxc crate
would have provided, the ratio was misjudged.
