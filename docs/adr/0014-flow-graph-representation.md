# ADR-0014: Flow nodes are 16-byte records in a per-file store, with pooled antecedent cells

- **Status:** accepted
- **Date:** 2026-08-04
- **Settles:** the representation half of `bd tsr-y4u.2` (Binder: control-flow graph)
- **Related:** [ADR-0003](0003-tree-plus-side-tables.md) (side tables),
  [ADR-0012](0012-ast-is-sync.md) (the AST is immutable and `Sync`),
  [ADR-0013](0013-checker-memoisation.md) (handles, not references)

## The forcing constraint

The control-flow graph is the largest single data structure the binder produces,
and the checker's hottest read. Two properties of it are awkward:

1. **It is cyclic and mutated after publication.** `addAntecedent`
   (`internal/binder/binder.go:542`) appends to a label that other flow nodes
   already point at, and a loop label's back edge points at a node created after
   the label. There is no order in which the graph can be built bottom-up.
2. **A label's antecedent list grows while other labels' lists are also
   growing.** Inside `bindTryStatement` alone, three labels accumulate entries
   interleaved with each other and with arbitrary recursion into the try block.

Upstream's answer is `*FlowNode` pointers plus a heap-allocated singly linked
list of `FlowList` cells (`internal/ast/flow.go:27–37`), backed by two arenas.
That is 32 bytes per node (a `uint32` and three pointers, padded) and 16 bytes
per list cell.

Neither the pointer graph nor the per-node allocation is available to us.
ADR-0012 makes the AST immutable and `Sync` so the checker can read it from
several threads; a `&`-linked graph whose labels are mutated during construction
would need `Arc<Mutex<…>>` or interior mutability per node, which is exactly what
that ADR exists to avoid.

Scale, measured on the four benchmark fixtures (`checker.ts`,
`dom.generated.d.ts`, `Herebyfile.mjs`, and a `.tsx` case — 5.56 MB of source,
419,464 AST nodes): **81,713 flow nodes**, one per 5.1 AST nodes. Whatever a
record costs is multiplied by that.

## The decision

**A flow node is a `FlowId` — a `NonMaxU32` index — into one `FlowStore` per
file. A record is 16 bytes. Label antecedents are a singly linked list of 8-byte
cells in a flat arena shared by every label.**

```rust
struct Record {
    flags: FlowFlags,          // u16; thirteen bits defined
    node: Option<NodeId>,      // 4 bytes via the NonMax niche
    antecedent: Option<FlowId>,// 4 bytes
    aux: u32,                  // overloaded by `flags`
}                              // = 16 bytes

struct Cell { flow: FlowId, next: Option<CellId> }  // = 8 bytes
```

`aux` carries the head of the antecedent list for a label, an index into a
side vector of clause ranges for a switch-clause node, and an index into a side
vector of reductions for a reduce label. It is private, and each reader goes
through an accessor that checks `flags` first
(`FlowStore::antecedents`, `switch_clause`, `reduce_label`), so the overloading
cannot escape the module.

`FlowId::ZERO` is always the file's single unreachable node. The binder compares
`current_flow == flow.unreachable()` in four places, exactly where upstream
compares `b.currentFlow == b.unreachableFlow` by pointer; one canonical node is
what makes that test mean the same thing.

## The alternatives, taken seriously

### `Vec<FlowId>` per label

The obvious Rust shape, and the one to beat. Rejected on arithmetic: a `Vec`
header is 24 bytes plus a heap allocation, for a list whose **median length is
one**. There are 81,713 flow nodes across the fixtures, of which labels are a
large minority; paying 24 bytes and an allocator round trip each to store, most
often, a single 4-byte id is worse than the 8-byte cell in every dimension —
size, allocation count, and locality.

*What would change this:* if labels turned out to be rare and wide (a few
labels with hundreds of antecedents each), the header would amortise and the
contiguity would win. They are not: `finish_label` collapses every label with
zero or one antecedent back into its antecedent, so the labels that survive into
the graph are precisely the genuine merges, and a merge of more than three ways
is unusual outside a `switch`.

### A contiguous range per label

Rejected as impossible rather than as slow. Labels interleave: a range would
require a label's antecedents to be appended consecutively, and constraint (2)
above says they are not.

### An `enum FlowNode` with per-variant payloads

More honest about the six shapes, and it would remove `aux`. Rejected because
the enum's size is its largest variant plus a discriminant: the reduce label
needs a target *and* a list head *and* an antecedent, which pushes the whole
array to 20 or 24 bytes to serve a variant that occurs a handful of times per
file. The `aux`-plus-accessor arrangement pays that cost only where it is
incurred, at the price of one indirection on the two rare kinds.

### Storing the flow node on the AST node, as upstream does

Rejected by ADR-0012, and independently by ADR-0003: the tree has no mutable
fields. The `node -> flow` map is a dense `Vec<Option<FlowId>>` instead — 4
bytes per AST node, 1.6 MiB across the fixtures. A hash map was considered and
rejected: the entries are not sparse. Every statement and every identifier in
the file gets one, which is most of the tree.

## The consequences we accepted

- **The `aux` field is a tagged union without a tag of its own.** It is
  discriminated by `flags`, which is checked in each accessor and asserted in
  `add_antecedent`. A future variant that forgets this gets silent corruption
  rather than a type error. The accessors and the module documentation are the
  mitigation; a stronger one would cost the bytes above.
- **The graph is per-file.** A `FlowId` is meaningless without the `FlowStore`
  it came from, exactly as a `NodeId` is meaningless without its `NodeTable`.
  This is already the project's convention and the checker will hold both.
- **`combine_lists` copies.** Upstream's `combineFlowLists` shares the tail
  and rebuilds the head recursively; ours collects into a reusable scratch
  buffer and walks backwards. The copy is the same size upstream's rebuild is,
  and it removes a recursion whose depth is the number of mutations in a `try`
  block — unbounded in generated code.

## What the numbers came out at

Measured with `cargo run -p tsr-binder --example rss --release`, four fixtures
held live at once:

| | KiB |
|---|---|
| AST | 25,600 |
| Binder, symbols only (before this work) | 12,288 |
| Binder, symbols + flow | 15,616 |
| — of which flow graph | **3,328** |

3.25 MiB for 81,713 flow nodes and 419,464 `node -> flow` entries: 1.28 MiB of
records, 1.60 MiB of the dense map, the balance in cells and slack. The flow
graph is **13% of the AST's footprint** and **21% of the binder's**.

Wall clock did not regress; parse+bind on `checker.ts` went from 27.99 ms to
25.66 ms, because the same change replaced the binder's per-level `Vec` of
children with one shared stack. That saving is not attributable to this ADR — it
would have applied to the symbol-only binder too — but it is why adding the flow
graph cost no measurable time.

## How we would know we were wrong

- **The record grows past 16 bytes.** If a future variant needs a fifth field,
  the arithmetic above has to be redone; at 24 bytes the enum alternative
  becomes competitive and should be reconsidered rather than patched around.
- **`nodes per flow node` drifts far from 5.** The `FlowStore::with_capacity`
  estimate in `Binder::new` is calibrated to 5.1 on the fixtures. If real
  projects come out at 2, the reservation under-shoots and the growth
  reallocations return; if 20, it wastes memory. The RSS example prints the
  ratio for exactly this reason.
- **Profiling shows `add_antecedent`'s linear scan is hot.** It is O(list
  length) per insert, as upstream's is. If a pathological file produced a label
  with thousands of antecedents — a `switch` with thousands of `break`s to it —
  the de-duplication scan would become quadratic. No fixture does; a file that
  did would be the signal to keep a tail pointer or a small set.
