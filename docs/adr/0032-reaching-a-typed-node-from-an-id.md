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

**4.02 ms is the cost of a separate walk** — 10% of the 39.8 ms parse+bind for
the same fixtures.

> **Corrected 2026-08-05, during implementation.** This paragraph originally
> called 4.02 ms "an upper bound, and a loose one", on the grounds that *"the
> binder already walks every node with the typed node in hand, so building the
> table there is one indexed store per node on a traversal that happens
> regardless."* **That is false**, and the correction is in the next section. The
> separate walk turned out to be necessary, so 4.02 ms is the estimate, not a
> ceiling above the real figure. The measured cost is in "The consequences
> accepted".

## The decision

**A dense `Vec<Option<Node<'a>>>` indexed by `NodeId`, one per source file, filled
by a dedicated pre-pass in the binder** (`Binder::record_nodes`, run before the
bind walk).

## The bind walk does not reach every node — corrected during implementation

This ADR was written expecting the table to be a free rider on the bind walk:
record in `bind_inner`, which is the one place holding a node's typed form and its
id together, and pay nothing extra. **That was measured on implementation and is
wrong.**

The bind walk reaches **417,837** of the 419,565 nodes a `push_children` walk
reaches — **1,728 short, 0.41%**. The gap is `case` and `default` keyword tokens,
plus some zero-width `ForOfStatement` nodes from error recovery. The cause is not
a defect on either side: `push_children` is generated from `ast.json` and includes
token-valued fields, while `bind_children` is a port of upstream's `bindChildren`,
whose `ForEachChild` does not visit those tokens. **The two walks answer different
questions**, and the checker's lookup table wants the more inclusive one.

0.41% is small enough to have shipped unnoticed and is exactly the wrong kind of
hole: unprincipled, invisible at the call site, and certain to read as a checker
bug years later. `BindResult::node` returns `Option`, so a miss degrades to
`errorType` rather than to a wrong answer — the safe direction — but "safe when it
fails" is not a reason to let it fail. Completeness is bought explicitly instead.

`crates/tsr-binder/tests/bind.rs` pins this with
`a_case_keyword_is_in_the_table_even_though_the_bind_walk_never_visits_it`, which
was verified to fail against the `bind_inner` version — the shape this ADR
originally specified.

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

This one **gets stronger** now that the free-rider assumption has collapsed. The
comparison was "free in the binder against a cost in the parser"; it is now "one
extra walk in the binder against one extra store in the parser", which is much
closer. It still loses on the 11,735 parse-only cases, and it would put a field
the parser has no use for into the parser's hot path — but if the pre-pass ever
shows up as a real regression, this is the alternative to re-measure first,
not lazy construction.

## The consequences accepted

Measured after implementation, against a same-session stashed baseline, three
readings each, at a load average of 0.32–1.06. Medians:

| | without | with | |
|---|---:|---:|---:|
| `checker.ts` parse+bind | 25.11 ms | 29.16 ms | **+16.1%** |
| `dom.generated.d.ts` parse+bind | 8.91 ms | 9.63 ms | **+8.1%** |
| binder memory | 15,056 KiB | 21,760 KiB | **+44.5%** |
| binder bytes per source byte | 2.77 | 4.01 | |
| total bytes per source byte | 7.54 | 8.68 | |

- **Memory came in as predicted.** +6,704 KiB against the +6,656 KiB this ADR
  estimated, and total bytes-per-source-byte 7.54 → 8.68 against a predicted
  8.75. It is the price of not having pointers in symbols, and it will show on
  the rss gate.
- **Time did not.** +16.1% on `checker.ts` parse+bind is a materially larger
  regression than this ADR implied when it expected the table to ride free on the
  bind walk, and larger than the 4.02 ms standalone walk alone accounts for.
  `checker.ts` is 72% of the benchmark's nodes, so a proportional share of that
  walk would be ~2.9 ms; the measured delta is 4.05 ms. The difference is most
  likely the per-bind allocation: `vec![None; nodes.len()]` zeroes 4.8 MB for
  `checker.ts` on **every** bind, which the standalone measurement paid three
  times and the benchmark pays once per iteration.
- **That is the ADR's own falsifier firing**, and it is recorded rather than
  absorbed: "if populating in the binder measurably slows binding by more than
  the ~4.0 ms a separate walk costs". It did. The named next step is to
  re-measure parser-side population, which avoids *both* costs at once — the
  parser hands out ids sequentially, so the table becomes a `push` with no
  zeroing and no second walk. Filed as `bd tsr-4sc.5`; this ADR is not
  superseded until that is measured, because the parse-only consumers it would
  charge are real (11,735 printer round-trip cases bind nothing).
- **The regression is accepted for now** because the capability is a hard
  prerequisite for any checker at all, and the cheaper shape is a change of
  *where* the table is filled, not of what it is — no consumer of
  `BindResult::node` changes if it moves.
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
- ~~**If populating in the binder measurably slows binding** by more than the
  ~4.0 ms a separate walk costs.~~ **This one fired.** +16.1% on `checker.ts`
  parse+bind, measured the way it specified — `cargo bench -p tsr-binder --bench
  bind` against a same-session stashed baseline, three readings, as ADR-0009
  requires. The consequence it named is now open work (`bd tsr-4sc.5`):
  re-measure parser-side population, which removes the second walk *and* the
  per-bind zeroing. Left struck through rather than deleted, because a falsifier
  that fired is the most useful line in the document.
- **If the memory model turns out superlinear on larger inputs.** The claim is
  linear-in-nodes, measured at one size, and it held to within 1% on
  implementation (+6,704 KiB against +6,656 KiB predicted). If the rss gate on a
  bigger corpus shows the ratio worsening rather than holding near 1.21 bytes per
  source byte, the model is wrong and lazy construction stops being deferrable.
