# ADR-0012: The AST is `Sync`, and stays that way

- **Status:** accepted
- **Date:** 2026-08-03
- **Related:** [ADR-0003](0003-tree-plus-side-tables.md) (tree plus side tables),
  [ADR-0011](0011-unsafe-is-opt-in.md) (which ruled out the easy way to get here)

## The forcing constraint

`docs/architecture/threading.md` records that "parallel checking is the headline
reason typescript-go is fast." Parallel checking means many threads reading one
file's tree at once, which means `&SourceFile` has to be `Send` — which means every
node has to be `Sync`.

It was not. Every generated node carried:

```rust
pub node_id: Cell<Option<NodeId>>,
```

with the comment "a `Cell` so registration does not need `&mut` on a tree the
parser is still building." One `Cell` anywhere in the tree makes the entire tree
non-`Sync`, and the compiler will not let you share it — correctly, because a
`Cell` is exactly the thing that is unsound to read from two threads.

The constraint is timing. This is cheap to fix now and expensive later: every pass
written against a non-`Sync` tree is a pass that has to be revisited, and the
binder is next.

## The decision

`node_id` is a plain `Option<NodeId>`, written through `&mut`.

The `Cell` was never necessary. `Arena::alloc` returns `&mut T` — the block is
freshly allocated, so no other reference to it exists — and the parser assigns the
id immediately:

```rust
let allocated: &'a mut T = self.arena.alloc(node);
allocated.set_node_id(id);
let allocated: &'a T = allocated;   // shared from here on
```

The mutation happens in the window before any shared reference exists, which is
precisely the case `&mut` is for. `HasNodeId::set_node_id` takes `&mut self`, so
the type system enforces it rather than a comment asking politely.

## What it cost

Nothing, on either axis that was in question:

| | before | after |
|---|---:|---:|
| `checker.ts` parse | 22.50 ms | 22.28 ms |
| `dom.generated.d.ts` parse | 8.97 ms | 8.75 ms |
| AST peak RSS (4 fixtures) | 25.0 MB | 25.0 MB |

`Cell<T>` is a `repr(transparent)` wrapper, so the field was always 4 bytes and
still is — `NodeId` is `NonMaxU32`, which makes `Option<NodeId>` the same 4 bytes
as `NodeId`. There was no memory to save and none was lost.

## Why not the other routes

**`unsafe impl Sync` on a "frozen" tree.** Parse with the `Cell`, then hand out a
wrapper asserting no further mutation. This is the standard trick and it works. It
was rejected under [ADR-0011](0011-unsafe-is-opt-in.md): it buys nothing the safe
version does not already give, and it moves a real invariant — "nobody mutates
after parsing" — from the type system into a comment. Reaching for `unsafe` when a
safe design is available and free is exactly the habit that ADR exists to prevent.

**Assign ids at construction.** Pass the `NodeId` into each generated `new()`. Also
safe, also `Sync`, but it needs the id before the node value exists, which means
reserving from the table first and restructuring roughly 240 call sites. Same
outcome, far more churn.

**Leave it and revisit when the checker needs it.** Rejected on timing, above.

## What is `Sync` now, and what is not

Asserted at compile time in `crates/tsr-parser/tests/parse.rs`, and exercised by a
test that walks the tree from four threads at once:

- **`Sync`:** every node type, `Node`, every alias union, `Token`, `NodeTable`,
  `SourceFile`. A parsed tree can be shared across any number of threads.
- **Not `Sync`, deliberately:** `Arena` — `alloc` mutates a bump pointer through
  `&self`, which is what lets the parser hold it immutably. Each parsing thread
  gets its own, which is how the conformance harness already runs.
- **Not `Sync`, consequently:** `ParsedFile`, because it owns an `Arena`. It is
  `Send`, so a worker can parse a file and hand the result back — the current
  model. Sharing one `ParsedFile` across threads needs either a `Sync` arena or a
  frozen view, and neither is built. Filed rather than claimed.

The distinction that matters: **the tree is shareable, the allocator is not.** That
is enough for parallel binding and checking over an already-parsed file, which is
the case that motivated this.

## Consequences accepted

- Every future node field must avoid interior mutability, and the temptation will
  recur — the binder will want to memoise, the checker will want to cache. The
  answer is a side table keyed by `NodeId`, per
  [ADR-0003](0003-tree-plus-side-tables.md), not a `Cell` in the node. Side tables
  can be `Mutex`-wrapped, sharded, or thread-local independently of the tree.
- The compile-time assertion is the only thing keeping this true. A `Cell` added in
  a generated struct would break that test and nothing else, which is why the test
  exists.

## How we would know this was wrong

- **A node genuinely needs per-node mutable state that a side table cannot hold.**
  None is known; the checker's `LinkStore`s upstream are side tables already.
- **`Sync` turns out not to be the binding constraint on parallelism** — for
  instance if the real bottleneck is a shared symbol table needing coarse locking,
  in which case a shareable tree was necessary but nowhere near sufficient. That
  would not make this wrong, but it would make it much less valuable than it looks
  today, and the honest response is to say so when the binder lands.
