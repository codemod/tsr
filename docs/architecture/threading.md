# Threading

**Status:** production loading, binding and checking remain serial. The
conformance harness is parallel. A fully bound `Program` is verified `Send + Sync`,
and independent checker instances can read it concurrently without sharing their
allocator or mutable type state (2026-10-03, `bd tsr-1yb.3`).

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

## The current program and checker ownership boundary

The historical per-file `ParsedFile` description above predates ADR-0034's
program-wide identity. The production `ProgramFile` now borrows its tree and text
from the caller's arena; it does not own a `self_cell`. The program owns the node
tables, bound symbols, file indexes and resolved-module map. None contains an
allocator reference or interior mutation after construction. A test in
`crates/tsr-execute/tests/checker_ownership.rs` asserts `Program: Send + Sync`
without making `Arena` Sync or adding any unsafe implementation.

The ownership decision is:

1. Finish discovery, parsing, binding and global declaration merges before
   publishing a shared `&Program`. Mutating methods retain their exclusive
   `&mut Program` requirement. The arena's owner stays alive through the worker
   scope; workers never receive the allocator.
2. Build a `Checker` inside each worker. Its `TypeStore`, links, alias caches,
   inference contexts, resolution stacks and mapper caches stay private. Workers
   share program-wide `NodeId` and `SymbolId` identities; a `TypeId` belongs to
   exactly one checker and must never enter another worker's cache.
3. Each checker can lazily force a type from any program file through the
   immutable module host. A worker's assigned file group limits diagnostic work,
   not the declarations it can read. Lazy augmentation or linking changes must
   preserve this boundary rather than introduce shared mutation in `Program`.
4. Return owned diagnostics and file identities. Collect in program order and
   normalize duplicate diagnostics deterministically. For declaration emit or
   type printing, use the checker that created the type, or perform an independent
   query; transferring a bare `TypeId` is invalid.

The compiling test seam runs independently constructed checkers at two and three
workers over cross-file generic imports, imported constructors, true assignment
errors and globally merged interfaces. Their normalized diagnostics match a
single checker. This establishes the ownership boundary on those controls; it
does not establish whole-app determinism or ship a parallel CLI. Those belong to
`bd tsr-1yb.6`, with complete-project measurements and broader augmentation,
recursion and emit controls before enabling workers by default.

Start production measurements at 1, 2 and 4 workers. Default worker selection must
be bounded by available CPUs and a documented memory policy, and
`singleThreaded` must force one. Measure per-worker initialization and redundant
cross-file type forcing before choosing the final default. The initial real-app
measurements show 0.926 GB peak RSS with `noCheck` and 1.118 GB with checking. This
suggests immutable program storage dominates, but subtracting two high-water marks
is not an exact measure of private checker memory. Keep this as a planning signal,
not a proven bound. Verify memory and latency under the actual worker pool.

Loading and parsing need their own design. They currently fill shared node tables
and allocate through one arena; the ready-to-share finished program does not make
those construction phases parallel. Per-file/worker allocation, ID assignment and
deterministic global merges remain work in `bd tsr-1yb.5`.

## Not yet built in production

- Parallel parsing in any real driver — only the harness uses it.
- Arena pooling.
- Parallel binding or checking; both subsystems now exist and run serially.

## Opt-in worker measurement probe

`cargo build --release -p tsr-execute --example checker_workers` builds a
standalone ownership/cost probe. Run the resulting absolute executable from the
project directory with `/absolute/tsconfig.json 1`, `2`, or `4`. The production
CLI remains serial. The probe shares a fully bound Program and gives each scoped
worker a private checker; file affinity follows the complete Program array
index modulo worker count, before filtering eligible files. It caps the requested
count at four, available CPUs and Program files, and honors `singleThreaded`.
These are probe limits, not an approved production default or the full native
`checkers` option contract.

Stdout contains checker diagnostics after comment directives. Stderr records
loaded/checked identities, phase times and each worker's checked count, initial
and final type counts, expression computations and initialization/check time.
Type counts are not allocation bytes, and expression computations are not all
generic instantiations. Measure external process wall/CPU/peak RSS separately.
The probe is not a replacement for CLI config, parse/bind diagnostics or emit.

After fixing diagnostic normalization (`tsr-1yb.1.4`), the exploratory probe's
complete 123 diagnostics match the saved serial CLI, including multiline
messages. The normalizer now strips ANSI decoration and ends message
continuations at summary/phase headings; it does not attach summary table rows
to the last diagnostic. Negative controls retain changes to messages, related
information, locations and codes.

Five fresh-process samples per mode on source `23563207`, after one warmup per
mode and with rotated order, measured the same Next.js workload:

| Workers | Median wall | Wall range | Median CPU | Median peak RSS | Program | Check/join |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 4.733 s | 4.668–4.948 s | 4.728 s | 1.040 GB | 2.327 s | 2.214 s |
| 2 | 3.823 s | 3.698–3.926 s | 5.052 s | 1.073 GB | 2.355 s | 1.275 s |
| 4 | 3.363 s | 3.306–3.426 s | 5.311 s | 1.046 GB | 2.325 s | 0.833 s |

Every run checked the same 1,341 files from 13,097 loaded files, with stable
content fingerprints and the same complete diagnostics as a fresh serial CLI
control. Four workers reduced probe wall time by **28.9%** against one worker,
while CPU rose 12.3%. RSS ranges overlap, so this establishes no memory
reduction or exact private-worker memory bound. Median worker initialization
totals were below 1.1 ms; lazy checking, not construction, creates most private types.

Summed final type counts were 187,759/230,491/280,423 at 1/2/4 workers, and actual
expression computations were 247,852/264,017/278,122. These counts were stable
across samples. Four workers create 49.4% more final type entries and perform
12.2% more expression computations than one. Balanced file counts still yield
skew: in one four-worker sample, checking ranged from 0.492 to 0.761 s and one
worker retained 104,752 types versus 50,693 in another. These are checker-local
counts, not independent memory estimates or native instantiation counters.

The evidence supports evaluating a default of at most four workers, bounded by
available CPUs and files, with `singleThreaded` forcing one. It does not approve
the production default: broader augmentation/recursion/emit/query controls and
constrained-memory behavior remain in `tsr-1yb.3.1` and `tsr-1yb.3.2`. Explicit
native `checkers` overrides must be audited separately from the probe's four-worker
limit. Both tasks precede production scheduling.

Loading remains about 2.3 s, already above half the previously observed native
total of 3.305 s. Checker parallelism alone cannot reach the overall 2x target.
This is a TSR probe comparison; the CLI remains serial and native scope/config
alignment remains incomplete. Local evidence is
`/tmp/tsr-1yb-worker-scaling-repeated.json` and
`/tmp/tsr-1yb-worker-diagnostic-normalization.json`.
