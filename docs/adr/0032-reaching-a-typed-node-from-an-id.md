# ADR-0032: Reaching a typed node from an id — a dense table, built by the binder

**Status:** accepted, 2026-08-05
**Context:** `bd tsr-4sc.4`, blocking `bd tsr-4sc.2` (`getTypeOfSymbol`)
**Related:** [ADR-0003](0003-tree-plus-side-tables.md) (tree plus side tables),
[ADR-0013](0013-checker-memoisation.md) (types are handles),
[ADR-0009](0009-performance-gate.md) (why peak RSS)

## The forcing constraint

The checker cannot compute a declaration's type. A binder `Symbol` holds
`value_declaration: Option<NodeId>`; the annotation and initialiser it needs live
in the **typed node** — `VariableDeclaration<'a>` — while `NodeTable` stores only
kind, span, flags and parent (`crates/tsr-ast/src/lib.rs:159`–`186`). Nothing in
the workspace maps a `NodeId` back to a `Node<'a>`. So the checker can reach a
declaration's *position* and not its *contents*, and `getTypeOfSymbol` cannot be
written at all.

Upstream never meets this: a Go `*ast.Symbol` holds a real `*ast.Node`, so the
question does not arise. This is [ADR-0003](0003-tree-plus-side-tables.md) — a
tree with no back-edges, everything cyclic in id-keyed side tables — meeting a
design that assumed pointers. It is a consequence of a decision this project
made deliberately, not an oversight in the port.

## What was measured

`crates/tsr-binder/examples/node_lookup.rs`, over the same four fixtures as
`examples/rss.rs` so the numbers sit beside the binder's recorded figures.
419,572 nodes, 41,831 symbols, 5,557,080 source bytes. Each option runs in **its
own process**, so a peak-RSS reading is never the sum of what was built before
it. `exact` is computed from allocation capacity, not estimated.

Three readings per option, reported as median with range. Taken at a 1-minute
load average of 1.89 — the first set was taken at 8.10, with this session's own
`cargo` builds still running, and was discarded. The timings turned out to be
insensitive to that (they agree within 5%), but they were not trusted until
re-taken.

| option | entries | exact KiB | RSS Δ KiB | populate | answers `parent(id)`? |
|---|---:|---:|---:|---:|:--:|
| `none` (baseline) | 0 | 0 | 0 | — | — |
| **dense** `Vec<Option<Node>>` by `NodeId` | 419,565 | 6,555 | **6,656** (6,540–6,760) | **4.02 ms** (4.00–4.05) | **yes** |
| **per-symbol** one `Node` per symbol | 36,590 | 653 | 1,488 (1,480–1,488) | 5.55 ms (5.45–5.68) | no |
| **sparse** `FxHashMap<NodeId, Node>` of declarations | 43,297 | 1,432 | 3,156 (2,924–3,324) | 9.05 ms (8.84–9.08) | no |

For scale: the binder is 15,052 KiB on these fixtures, and parse+bind is 39.8 ms
(`cargo bench -p tsr-binder --bench bind`, summed).

Three things in that table were not what the issue predicted.

**The dense table is the *fastest* to populate**, despite being ten times the
size. It stores by index; the other two probe a hash map once per node to decide
whether to keep it, and `sparse` also pays the insertions. The cheap options are
cheap in bytes only, and `sparse` costs **2.25×** the dense option's populate
time.

**The byte counts overstate the gap.** Both cheap options need a transient
`wanted` set built from the symbol table, which is live while the walk runs, so
their RSS deltas are 2.28× and 2.20× their exact sizes. The dense option's delta
is 1.02× its exact size — no transient at all. The real spread is 6,656 against
1,488 and 3,156: **4.5× and 2.1×**, not the 10× the naive byte comparison
suggested.

**4.02 ms is an upper bound, and a loose one.** It is the cost of a *separate*
walk, and it is 10% of the 39.8 ms parse+bind for the same fixtures. The binder
already walks every node with the typed node in hand and already calls
`set_parent`, so building the table there is one indexed store per node on a
traversal that happens regardless.

## The decision

**A dense `Vec<Option<Node<'a>>>` indexed by `NodeId`, one per source file,
populated by the binder.**

## Why, and it is capability rather than cost

The measurements above rank the options on price. They do not decide it, because
two of the three cannot answer the question the checker asks most often.

`nodes.parent(id)` returns a `NodeId`. Inspecting that parent — which is what
every `.Parent` read in upstream does — requires the typed node at that id.
Upstream reads `.Parent` **1,134 times in `internal/checker` alone**, and 2,092
times across `checker`, `ls` and `binder`. `getTypeForVariableLikeDeclaration`
reaches `declaration.Parent.Parent` in its first ten lines
(`checker.go:16656`) to decide whether a variable is a `for..in` binding.

`per-symbol` and `sparse` answer exactly one question — "the declaration node of
this symbol" — and nothing else. Adopting either means the first parent-dependent
check forces the dense table anyway, *after* a cheaper structure has been
threaded through the binder's public API and the checker written against it. The
saving is 5 MiB; the cost of being wrong is a migration of everything built on
top.

Given that, the price ordering is a tiebreak rather than the argument, and it
happens to point the same way on time.

## The alternatives, taken seriously

**`per-symbol` (653 KiB exact, 1,488 KiB resident).** The cheapest, and it is
genuinely sufficient for `getTypeOfSymbol` in isolation: from the declaration
node, the annotation and initialiser are plain field accesses, and expression
types arrive top-down from the walk. It loses on parent access, and on
`symbol.declarations` — a merged symbol has several declarations and this keeps
only `value_declaration`. **It would win if the checker never needed to look
upward.** That is the falsifier below.

**`sparse` (1,432 KiB exact, 3,156 KiB resident).** Same answers as `per-symbol`,
keyed by node rather than by symbol, and strictly worse on every measured axis —
2.1× the memory and 1.6× the populate time, for no additional capability. Its
only advantage would be if declarations were a tiny fraction of nodes; they are
43,297 of 419,572, about 10%, which is not tiny enough to pay for hashing.

**Lazy construction, per file, on first query.** Not a fourth option so much as a
modifier on the dense one, and deliberately deferred. It buys nothing for the
conformance harness, which checks every file it loads, and it is exactly what an
LSP wants when one file of hundreds is open. The dense table is already per
source file, so laziness can be added later without changing its shape or its
consumers. Revisit at `bd tsr-of6` (the LSP epic), not before.

**Populating from the parser instead of the binder.** The parser calls
`NodeTable::push` to allocate an id *before* the typed node is constructed, so it
would need a second store at node-finish time. That is achievable but puts the
cost on every parse, including the many callers that never bind — the printer
round-trip suite parses 11,735 cases and binds none of them. The binder is the
right owner because it is the first stage that needs the tree *and* the ids
together.

## The consequences accepted

- **+6,656 KiB on these fixtures: +44% on the binder's memory**, and total
  bytes-per-source-byte moves 7.54 → 8.75. This is real and it will show on the
  rss gate. It is the price of not having pointers in symbols.
- **Cost is linear in nodes**, so a large program pays proportionally. Nothing
  here is amortised.
- **The table is sized by `NodeTable::len()`**, which counts rows registered
  rather than nodes in the tree: 7 of 419,572 rows are orphans from abandoned
  speculative parses (`bd tsr-pum.12`, found by this same measurement). The waste
  is 112 bytes and the empty slots cost nothing extra, because `Option<Node>` is
  16 bytes — the same as `Node`, the enum's niche absorbing the discriminant.
  That niche is also why a sparsely-filled dense table costs exactly what a full
  one does, which is what makes this shape viable at all.
- **The binder's public API grows a lookup**, and the binder now holds a
  structure the binder itself does not use. That is a genuine layering cost and
  the reason this needed a decision rather than a commit.

## How we would know this was wrong

- **If the checker, once substantially ported, never resolves a `parent(id)` to a
  typed node.** Then capability was not the deciding factor, `per-symbol` was
  sufficient, and 5 MiB was spent for nothing. The check is mechanical: count the
  call sites as the port proceeds. Upstream's 1,134 makes this unlikely, but
  "upstream does it" is not the same as "our port must", and if the count is
  still zero after the first ~10,000 lines of checker, revisit this.
- **If memory turns out superlinear on larger inputs.** The claim here is
  linear-in-nodes, measured at one size. If the rss gate on a bigger corpus shows
  the ratio worsening rather than holding at ~1.21 bytes per source byte, the
  model is wrong and lazy construction stops being deferrable.
- **If populating in the binder measurably slows binding** by more than the
  ~4.0 ms a separate walk costs. That would mean the store per node is not the
  cheap operation this assumes, and the parser-side or lazy variants deserve
  re-measuring. `cargo bench -p tsr-binder --bench bind` against a same-session
  stashed baseline is the test, three readings, as ADR-0009 requires.
