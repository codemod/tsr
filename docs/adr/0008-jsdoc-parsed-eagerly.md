# ADR-0008: JSDoc is parsed eagerly, not on demand

- **Status:** partly superseded by [ADR-0010](0010-jsdoc-is-a-parse-option.md)
  — *when* JSDoc is parsed is still eager and still for the reason below; *whether*
  it is parsed became a caller's choice once the falsifier at the bottom of this
  document fired on `dom.generated.d.ts` (45% of parse time, not the 7.5%
  measured here).
- **Date:** 2026-08-03
- **Supersedes:** nothing
- **Related:** [ADR-0003](0003-tree-plus-side-tables.md) (side tables),
  [ADR-0006](0006-conformance-oracle.md) (assert against generated Go)

## The forcing constraint

typescript-go parses JSDoc **lazily** for `.ts` and `.tsx` files. During the main
parse it only sets a flag:

```go
// internal/parser/jsdoc.go, withJSDoc
if !p.isJavaScript() {
    node.Flags |= ast.NodeFlagsHasJSDoc
    if info&jsdocScannerInfoHasSeeOrLink == 0 {
        return nil          // parse later, on first Node.JSDoc() access
    }
}
```

The actual parse happens in `parseJSDocForNode`, called from
`ast.SetParseJSDocForNode` when something first asks a node for its
documentation. That function borrows a parser from a pool, re-initialises it over
the *whole* source file, and parses the one comment.

We cannot do that. A `ParsedFile` bundles the arena, the source, and the tree in a
`self_cell` (`crates/tsr-parser/src/parsed_file.rs`), and JSDoc nodes have to be
arena-allocated with the same `'a` as the rest of the tree. `self_cell` hands out
`&Ast<'a>` through `with_dependent`; there is no way to get an `&'a Arena` back out
afterwards with a lifetime long enough to allocate nodes that outlive the closure.
Lazy parsing would mean either a second arena per file with its own lifetime, or
abandoning the self-referential bundle that makes `ParsedFile` a `Send` unit of
work — see [ADR-0003](0003-tree-plus-side-tables.md) and
`docs/architecture/threading.md`.

## The decision

Parse JSDoc during the main parse, gated on a token flag.

The scanner already walks the trivia before every token. While it is there it sets
`TokenFlags::PRECEDING_JSDOC_COMMENT` if the trivia contained a `/** … */`, so the
parser's test is a bit test on a value already in a register:

```rust
if !self.token.flags.contains(TokenFlags::PRECEDING_JSDOC_COMMENT) {
    return &[];
}
```

Only when that passes does anything else happen: the comment ranges are located by
scanning *forward* over the construct's own leading trivia
(`jsdoc_ranges_in(source, full_start, token_start)`), and each is parsed by
retargeting the main scanner at the comment body.

## What it costs

Measured with `cargo run --release -p tsr-parser --example jsdoc_cost <dir>`, which
parses a corpus twice — once normally, once with `TSR_NO_JSDOC=1`:

| Corpus | Files | With JSDoc | Without | Overhead |
|---|---|---|---|---|
| `tests/cases/compiler` | 6,412 (4.5 MB) | 92.8 ms | 86.4 ms | **+7.5%** |
| `tests/cases/conformance/jsdoc` | 341 (178 KB) | 3.69 ms | 2.24 ms | **+64%** |

*This table is what led the decision astray — see
[ADR-0010](0010-jsdoc-is-a-parse-option.md). Neither corpus resembles the `.d.ts`
files a real project spends its parse time on, where the true figure is 45%.*

The first row is what ordinary TypeScript costs: 755 documented nodes across 4.5 MB,
so almost every token's flag test fails and nothing is allocated. The second row is
the worst case — files that are mostly JSDoc — and is the honest upper bound.

## Alternatives

**Lazy, as upstream does it.** Rejected for the lifetime reason above, not because
it is worse. It is strictly better on cost: a `.ts` file whose JSDoc nobody reads
pays nothing. It would win the moment we can allocate into a parsed file's arena
after construction.

**Lazy with a second arena for JSDoc.** A per-file `Arena` behind a `OnceCell`,
with JSDoc nodes allocated there. Rejected as premature: it doubles the allocator
state per file and splits the tree across two lifetimes, complicating every
consumer, to buy back at most 7.5% of parse time on realistic input. Worth
revisiting if the language service turns out to reparse files often enough for
that to matter.

**Upstream's middle path — flag always, parse only for `@see`/`@link`.** Upstream
takes it because those two tags are needed for unused-identifier checks even in
`.ts`. Rejected here because it leaves the language service with no JSDoc for hover
in the overwhelmingly common case, which is the main thing JSDoc is *for* in a
TypeScript file.

## Consequences accepted

- Parse time on JSDoc-dense files is up to 64% higher than it needs to be. On
  ordinary TypeScript it is 7.5%.
- Memory holds every parsed comment for the file's lifetime, whether or not anyone
  reads it.
- The `TSR_NO_JSDOC` environment variable exists so the cost stays measurable. It
  is read once into a `OnceLock`, because reading it per node would distort the
  very measurement it supports. *(Replaced by `ParseOptions::jsdoc` in
  [ADR-0010](0010-jsdoc-is-a-parse-option.md); the env var no longer exists.)*

## How we would know this was wrong

Any of these flips the decision:

1. A real editor workload shows parse time dominated by JSDoc — the 7.5% figure
   above becomes, say, 25% on a large project's `.ts` files.
2. `ParsedFile` gains a way to allocate into its arena post-construction (for
   instance, if incremental reparse needs it anyway). The lifetime obstacle is the
   *only* reason we are not lazy, so removing it removes the reason.
3. Memory profiling shows retained JSDoc nodes are a meaningful share of a large
   project's footprint.

The falsifier is deliberately concrete: re-run `examples/jsdoc_cost` against the
project in question. If the overhead there is materially worse than the 7.5%
measured here, this ADR should be superseded.
