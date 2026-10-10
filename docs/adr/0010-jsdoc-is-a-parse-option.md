# ADR-0010: JSDoc is a parse option

- **Status:** accepted; the compiler driver's use of it is partly superseded by
  [ADR-0053](0053-jsdoc-deferred-in-checked-ts-files.md), which defers JSDoc
  in non-JavaScript program files as typescript-go does.
- **Date:** 2026-08-03
- **Supersedes:** [ADR-0008](0008-jsdoc-parsed-eagerly.md) in part — eager parsing
  stands, but *whether* to parse is now the caller's, not always yes.
- **Related:** [ADR-0009](0009-performance-gate.md) (the ratio this changes)

## The forcing constraint

[ADR-0008](0008-jsdoc-parsed-eagerly.md) named its own falsifier:

> A real editor workload shows parse time dominated by JSDoc — the 7.5% figure
> above becomes, say, 25% on a large project's `.ts` files.

**It fired.** A `perf` profile of `dom.generated.d.ts` — a lib file every
TypeScript project parses — attributes 25% of samples to JSDoc functions, and
turning JSDoc off measures the true figure higher still:

| | with JSDoc | without | JSDoc's share |
|---|---:|---:|---:|
| `dom.generated.d.ts` wall clock | 19.83 ms | 10.91 ms | **45%** |
| `dom.generated.d.ts` allocations | 60,182 | 26,044 | **57%** |
| `jsxComplexSignature….tsx` wall clock | 154.4 µs | 95.1 µs | **38%** |
| `checker.ts` wall clock | 29.61 ms | 28.84 ms | 3% |

ADR-0008's estimate came from `tests/cases/compiler`, which has 755 documented
nodes across 4.5 MB. `dom.generated.d.ts` has 6,151 in 2.3 MB. The 7.5% figure
was not wrong for the corpus it was measured on; it was measured on the wrong
corpus, because a `.d.ts` is exactly the documentation-dense shape a real project
parses most of.

The second constraint is comparability. typescript-go **does not build JSDoc nodes
for `.ts`/`.tsx` at all** — `withJSDoc` sets a flag and returns
(`internal/parser/jsdoc.go`). Benchmarking our JSDoc-building parse against that
under [ADR-0009](0009-performance-gate.md) is not a like-for-like ratio: on
`dom.generated.d.ts` we were doing 45% more work and reporting the result as
though it were the same work.

## The decision

JSDoc parsing becomes a field on `ParseOptions`, defaulting to **on**.

```rust
pub struct ParseOptions {
    pub script_kind: ScriptKind,
    pub jsdoc: bool,
}
```

The performance gate's ratio is measured with `jsdoc: false`, because that is what
typescript-go's `ParseSourceFile` does for these fixtures. The benchmark prints
**both** arms side by side so neither can be quoted alone.

### Why the default is on

Turning JSDoc off and then reading `JSDocTable` yields an empty table, not an
error. The two failure modes are not symmetric: paying for JSDoc you did not need
is a performance bug that a benchmark catches, while silently losing every doc
comment is a correctness bug that looks like "the language service has no hover
text" three months later. The default takes the side that fails loudly.

Callers that care — the compiler driver, the benchmark — opt out explicitly.

### What survives from ADR-0008

Everything about *how*. When JSDoc is parsed it is still parsed **eagerly, during
the main parse**, for the lifetime reason ADR-0008 gives: `ParsedFile` bundles
arena, source, and tree in a `self_cell`, and there is no way to allocate into that
arena after construction. This ADR changes only *whether*, not *when*.

## Alternatives

**Keep it unconditional.** Rejected: it makes the ADR-0009 ratio measure different
work on the two sides, which is the one thing that gate exists not to do.

**Default off.** Rejected for the asymmetry above. It would also make the headline
benchmark number the default path, which is a self-favouring choice of exactly the
kind ADR-0009 warns about — the honest arrangement is that the default is the
*expensive* one and the fast number carries a caveat, not the reverse.

**Match upstream exactly — parse for `.js`, defer for `.ts`, except `@see`/`@link`.**
This is where we should end up, and it is not available yet: we have no JS script
kind, no reparser, and no lazy path. A boolean is the honest approximation until
those exist. Revisit when the `.js` reparser lands (`bd` issue under the parser
epic).

## Consequences accepted

- **Two benchmark arms forever.** Any performance claim now needs to say which.
  That is the point, but it makes the numbers harder to quote.
- **A caller can silently get no JSDoc.** Mitigated by the default, not eliminated.
- **The gate's ratio improved for a reason that is not a code improvement.**
  Turning off work is not making work faster. The `+jsdoc` column stays in the
  table so this is visible rather than laundered.

## How we would know this was wrong

- **Consumers routinely want JSDoc and the opt-out is never used.** Then the option
  is complexity without benefit and JSDoc should go back to unconditional.
- **Someone ships a driver with `jsdoc: false` and then wonders where the doc
  comments went.** That is the failure the default exists to prevent; if it happens
  anyway, the flag needs to be harder to set by accident than a struct field.
- **A lazy path becomes possible** (post-construction arena allocation, or
  incremental reparse needing it anyway). Then ADR-0008's rejected first
  alternative wins outright and both this ADR and that one are superseded.
