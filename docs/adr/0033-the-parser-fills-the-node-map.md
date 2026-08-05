# ADR-0033: The parser fills the node map

**Status:** accepted, 2026-08-05
**Supersedes:** [ADR-0032](0032-reaching-a-typed-node-from-an-id.md)
**Context:** `bd tsr-4sc.5`, raised by ADR-0032's own falsifier firing
**Related:** [ADR-0003](0003-tree-plus-side-tables.md) (tree plus side tables),
[ADR-0009](0009-performance-gate.md) (why peak RSS)

## What this changes, and what it does not

ADR-0032 settled *what* the structure is and *why* it must exist: a dense
`NodeId → Node` map, because the checker holds ids and needs fields, and because
`NodeTable::parent` is otherwise unusable — upstream reads `.Parent` 1,134 times
in `internal/checker` alone. **None of that is revisited here.** The alternatives
it rejected — one node per symbol, a sparse map of declarations — lost on
capability, and they still do.

What changes is **where the map is filled**: the parser, not the binder. No
consumer of the lookup changes shape; this is a question of who pays and how
much.

## Why ADR-0032 is superseded rather than amended

Because its cost model was wrong in a way that inverted the decision, and the
project's rule is that a decision record is superseded rather than edited.

ADR-0032 chose the binder on the reasoning that the binder "already walks every
node with the typed node in hand, so building the table there is one indexed
store per node on a traversal that happens regardless" — i.e. free in the binder
against a real cost in the parser. Implementation found **both halves of that
false**:

1. **The bind walk does not reach every node.** It reaches 417,837 of the
   419,565 a `push_children` walk reaches — 1,728 short, 0.41%, being `case` and
   `default` keyword tokens and some zero-width `ForOfStatement` nodes.
   `push_children` is generated from `ast.json` and includes token-valued fields;
   `bind_children` ports upstream's `bindChildren`, whose `ForEachChild` does
   not. So the binder needed a *separate* pre-pass, not a free line.
2. **The separate pre-pass was not cheap.** +16.1% on `checker.ts` parse+bind,
   measured against a same-session stashed baseline — larger than the standalone
   walk accounted for, because `vec![None; nodes.len()]` zeroes 4.8 MB on
   **every** bind.

ADR-0032 named this exact outcome as its falsifier and named this exact next
step. This is that step.

## The insight the first ADR missed

`NodeTable::push` hands out ids **sequentially**, and the parser allocates the
typed node immediately afterwards — in `finish_node_with_flags`,
`missing_identifier` and `alloc_token`, the only three registration sites. So in
the parser, recording is a `Vec::push` in the same order.

That removes **both** cost terms at once, which is why this is not a marginal
trade:

- **No zeroed allocation.** The binder had to size a vector by `nodes.len()` and
  fill it sparsely; the parser appends as it goes.
- **No second traversal.** The binder needed a walk to find the nodes; the parser
  is already holding each one at the moment it is created.

ADR-0032 reasoned about the parser as "a second store at node-finish time",
which is accurate, and then compared it against a binder option believed to be
free. Against a binder option that costs a zeroing walk, the comparison is not
close.

## Coverage stops being a question

The binder version had to be *argued* complete, and the argument was wrong by
0.41%. The parser version is complete **by construction**: every node is
recorded at the point it is created, so no walk's notion of "every node" has to
be trusted, and the discrepancy between `push_children` and `bindChildren` stops
mattering to this table entirely.

Verified over the four benchmark fixtures: 419,565 of 419,565, zero missing.

## What it costs instead, and this is the real trade

Every parse now pays 16 bytes per node and a push, **including the callers that
never bind**. That is not hypothetical: `printer_round_trip` parses 11,735 cases
and binds none of them; the scanner and parser suites are the same shape. This
was ADR-0032's stated reason for choosing the binder, and it is the one part of
its reasoning that survives — so it is measured rather than waved through.

## What was measured

Working tree (parser-side) against `HEAD` (binder-side), same session, same
machine, three readings each, at a load average of 0.30. `HEAD` carries no parser
changes, so its parse-only figures double as the parse-only baseline — which is
the number this whole trade turns on.

**Read the two benchmarks separately, never across.** `bench parse` runs with
JSDoc on and `bench bind` with it off (matching
[ADR-0010](0010-jsdoc-is-off-for-ts.md)), so they build different amounts of
tree — `dom.generated.d.ts` reads 17.3 ms in one and 9.6 ms in the other for that
reason alone. Only the parser-vs-binder comparison *within* each row means
anything.

**Parse only.** The binder-side arm carries no parser changes, so its column is a
genuine no-map baseline taken in this same run — this is the price parse-only
consumers pay:

| `bench parse` | no map | with map | |
|---|---:|---:|---:|
| `checker.ts` | 22.09 ms | 23.71 ms | **+7.4%** |
| `dom.generated.d.ts` | 17.34 ms | 17.70 ms | **+2.0%** |
| AST memory | 25,856 KiB | 32,256 KiB | **+24.8%** |
| AST bytes per source byte | 4.76 | 5.94 | |

**Parse and bind**, parser-side against binder-side:

| `bench bind` | binder-side | parser-side | |
|---|---:|---:|---:|
| `checker.ts` | 28.37 ms | 23.56 ms | **−17.0%** |
| `dom.generated.d.ts` | 9.57 ms | 9.26 ms | **−3.2%** |
| binder memory | 21,760 KiB | 14,848 KiB | **−31.8%** |
| total bytes per source byte | 8.73 | 8.68 | |

**Whole-corpus wall clock**, `cargo run --release -p tsr-conformance --bin
coverage` over 12,444 cases — the parse-heavy, bind-light consumer this trade is
actually about:

| | binder-side | parser-side | |
|---|---:|---:|---:|
| conformance | 5.84 s | 5.89 s | **+0.9%** |

Three things to draw from that.

**The +6.4 MiB does not appear twice; it moves.** AST memory rises 25,856 →
32,256 KiB (+6,400 KiB, against 6,556 KiB predicted for 419,572 nodes at 16
bytes) and binder memory falls 21,760 → 14,848 KiB, back to roughly its
pre-ADR-0032 figure of 15,056. Total bytes-per-source-byte goes 8.73 → **8.68**:
marginally *better*, because the parser's `push` needs no zeroed vector.

**Parse+bind gets 17% faster than the binder-side implementation**, which is the
whole regression ADR-0032 accepted, returned. The saving is larger than the
parse-side cost because the binder version paid for a zeroed 4.8 MB allocation
and a second traversal, and the parser version pays for neither.

**The predicted downside is real but small.** Conformance is +0.9% — the
parse-only consumers do pay, exactly as ADR-0032 argued they would, and the price
is under a percent of a whole-corpus run rather than the deciding factor it was
assumed to be.

## The consequences accepted

- **Every parse pays**, whether or not it ever binds: +7.4% on `checker.ts`,
  +2.0% on `dom.generated.d.ts`, and +6,400 KiB of AST. `printer_round_trip`
  parses 11,735 cases and binds none; it pays in full and gets nothing. That is
  the honest cost of this decision and it is not zero.
- **`ParsedSourceFile` and `ParsedFile` both grow a field.** In `ParsedFile` the
  map has to live *inside* the self-referential cell, alongside `jsdoc`, because
  it holds `Node<'a>` borrowed from the arena — it cannot be lifted out beside
  the node table the way diagnostics are. New accessor:
  `ParsedFile::with_ast_and_nodes`.
- **The parser now carries a structure it never reads.** The same layering
  objection ADR-0032 raised against the binder, moved one stage earlier. It is
  the price of the id-allocation order being the thing that makes this cheap.
- **Two tables must stay in lockstep forever.** `NodeTable` and `NodeMap` share
  a push order and a `truncate`; a future registration site that updates one and
  not the other breaks every id after the next abandoned parse, silently. The
  invariant is tested rather than commented, and there are only three
  registration sites.
- **A `Vec<Node>`, not `Vec<Option<Node>>`.** Every entry is written the moment
  it is created, so there is nothing to represent absence *within* the file, and
  `get` returns `Option` only for ids that are out of range. The binder version
  needed `Option` for its sparse fill, and got it free from the enum's niche;
  here it is not needed at all.
- **Orphan rows are now harmless rather than merely rare.** A row abandoned by a
  speculative parse (`bd tsr-pum.12`) has a map entry holding the abandoned node,
  because both tables truncate together. It is unreachable from the tree either
  way, and no longer an empty slot that a consumer could mistake for a gap.

## How we would know this was wrong

- **If parse-only memory matters more than parse+bind time**, for a consumer
  this project actually has. The printer round-trip suite is the stress case at
  11,735 parse-only cases; if a future consumer parses far more than it binds
  *and* is memory-bound, the map should become opt-in via `ParseOptions` — which
  is a smaller change than moving it again, since `assign_parents` already
  establishes that pattern.
- **If the map and the node table ever disagree about an id.** They are kept in
  lockstep by construction — the same push order, the same `truncate` on
  speculative rollback — and
  `the_node_map_survives_speculative_backtracking` pins it. A divergence would
  make every id after an abandoned parse attempt name a different node in each
  table, which is silent and severe. That test was verified to fail with the
  lockstep `truncate` removed, and it goes red on a source as ordinary as
  `const a: string = 'x';` — speculative parsing is not an edge case.
- **If lazy or opt-in construction becomes necessary for the LSP** (`bd
  tsr-of6`), where one file of hundreds is open. Still deferred, still
  cheap to add later, and now cheaper: an opt-in flag on the parser is less
  invasive than the per-file laziness ADR-0032 contemplated in the binder.
