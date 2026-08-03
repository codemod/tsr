# Threading

**Status:** groundwork laid; the conformance harness is parallel. Nothing else is.

Parallel checking is the headline reason typescript-go is fast, and PLAN.md §3.5
makes thread-safety a design requirement rather than a later optimisation —
retrofitting it into an arena design is a rewrite, not a tuning pass. This
document records what is true today and what each next step needs.

## The unit of parallelism is a file

Files are independent through parsing: each gets its own arena, its own source
text, and its own node table. There is nothing to contend on, so the concurrency
model is the simplest one that exists — `par_iter` over files.

This is also what oxc does, and why: a shared arena would need locking on the
allocation hot path, which is a pointer bump.

## What is shareable today

Updated 2026-08-03, when the AST became `Sync`
([ADR-0012](../adr/0012-ast-is-sync.md)).

| Type | `Send` | `Sync` | Note |
|---|:--:|:--:|---|
| every node, `Node`, alias unions, `Token` | ✅ | ✅ | no interior mutability |
| `SourceFile` | ✅ | ✅ | a parsed tree can be read by any number of threads |
| `NodeTable` | ✅ | ✅ | plain vectors |
| `Arena` | ✅ | ❌ | `alloc` bumps a pointer through `&self`; one per thread |
| `ParsedFile` | ✅ | ❌ | owns an `Arena` |

The line to remember: **the tree is shareable, the allocator is not.** That is
enough for parallel binding and checking over an already-parsed file — the case
that motivates parallelism at all — and it is not enough to share one `ParsedFile`
between threads, which would need a `Sync` arena or a frozen view. Neither exists.

Both properties are asserted at compile time in `crates/tsr-parser/tests/parse.rs`
and exercised by a test that walks one tree from four threads. The assertions are
the only thing keeping them true: a `Cell` added to a generated node would break
that test and nothing else.

### The rule this implies for later passes

The binder will want to memoise and the checker will want to cache, and the
shortest path to both is a `Cell` or a `RefCell` on the node. That would silently
undo this. Per-node mutable state goes in a side table keyed by `NodeId`
([ADR-0003](../adr/0003-tree-plus-side-tables.md)), which can be locked, sharded,
or made thread-local independently of the tree.


## `Arena` is `Send`, deliberately not `Sync`

```rust
unsafe impl Send for Arena {}
```

An arena exclusively owns its chunks, and the raw pointers it stores point only
into them. Moving it moves the memory, so values allocated on one thread stay
valid after the arena is sent to another.

It is **not** `Sync`, and must not become so: `alloc` mutates the bump pointer
through `&self`, so two threads allocating at once would race. A test asserts the
absence of `Sync` — if that ever starts failing, someone made concurrent
allocation possible without auditing it.

## `ParsedFile` is the sendable unit

A parsed AST borrows from two things: the arena its nodes live in and the source
text its identifiers point into. `(Arena, String, SourceFile<'a>)` is therefore
self-referential, which Rust will not let you name, let alone move — and moving it
is exactly what handing a parse result back from a worker requires.

`ParsedFile` bundles them with [`self_cell`], which stores the owner alongside the
dependent and hands out the borrow only through a closure. One audited
`unsafe impl Send` covers the whole thing. oxc solves the same problem the same
way in `oxc_type_checker::compiler::source_file`.

The tree is reachable only via `with_ast(|source_file| …)`. Handing out
`&SourceFile` directly would let a caller name a lifetime tied to storage the cell
owns, which is the thing `self_cell` exists to prevent.

## Determinism is a requirement, not a nicety

The conformance harness collects outcomes in **case order** and tallies afterwards,
so the committed snapshot is byte-identical regardless of how work was scheduled.
Verified at 1, 3, and default thread counts.

A ratchet whose output depends on thread timing would be worse than no ratchet:
every run would produce a diff, and real regressions would hide in the noise.

Measured on the corpus (12,444 cases, release build): **~25 s of CPU in ~4.6 s
wall**, about 5.7× on this machine.

## What each next step needs

**The binder** stays per-file, so it inherits this model unchanged.

**The checker does not.** It is program-wide: types and symbols are interned in
tables shared across files, and laziness means one file's checking can force
another's. Upstream runs multiple checker instances rather than sharing one, and
that is the model to copy — but it interacts directly with the memoisation
question in `bd tsr-6n3`, which is still unresolved and has no prior art in oxc.
Decide that first; the concurrency shape follows from it.

**Arena reuse.** Every file currently allocates a fresh arena and drops it. oxc
pools them (`AllocatorPool`) so a long-running process reuses chunks instead of
returning them to the allocator. Worth doing when the LSP holds many files, not
before — measure first.

**`NodeTable` and side tables** are per-file and plain data, so they move with the
file. Program-wide side tables (the checker's link stores) will need either
per-checker ownership or a concurrent map; `papaya` is in the bill of materials
for exactly that.

## Not yet built

- Parallel parsing in any real driver — only the harness uses it.
- Arena pooling.
- Any binder or checker, so the hard part of the question is untouched.
