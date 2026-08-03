# ADR-0003: The tree is a tree; everything cyclic lives in id-keyed side tables

**Status:** Accepted
**Date:** 2026-08-02

## Context

Go's object graph is cyclic and mutably shared:

```go
type Node struct {
    Kind Kind; Flags NodeFlags; Loc core.TextRange
    id atomic.Uint64
    Parent *Node          // back-pointer
    data nodeData         // interface, ~192 implementations
}

type Type struct {
    flags TypeFlags; objectFlags ObjectFlags; id TypeId
    symbol *ast.Symbol
    checker *Checker      // back-pointer to the owning checker
    data TypeData
}
```

`Node.Parent`, `Type.checker`, and the `Symbol ↔ Type ↔ Node` cycles make a direct
`Rc<RefCell<…>>` translation both unwritable and slow, and it would forfeit the
single largest reason to be in Rust: cheap, sound parallelism.

**The first draft of the plan got this wrong.** It proposed making the AST *itself*
index-based — a 192-variant enum in `IndexVec<NodeId, Node>` with children stored
as `NodeId`. That is a defensible design, and it is not what either mature
implementation does.

## Decision

**The tree is a tree.** Node structs hold only syntax children, as direct arena
references. Everything cyclic or phase-specific — parent, symbol, scope, type,
flow node — lives in a side table keyed by a typed id.

## Reasoning

Two independent, mature implementations converged on this split, which is worth
more than a derivation from first principles:

**oxc.** The AST is a plain bump-allocated tree with no back-edges.
`oxc_semantic::AstNodes<'a>` then holds the graph structure separately:

```rust
pub struct AstNodes<'a> {
    nodes: IndexVec<NodeId, AstNode<'a>>,
    parent_ids: IndexVec<NodeId, NodeId>,   // parent is a side table, not a field
    flags: IndexVec<NodeId, NodeFlags>,
    cfg_ids: IndexVec<NodeId, BlockNodeId>,
}
```

**typescript-go.** `internal/core/linkstore.go` defines `LinkStore[K, V]` and a
paged variant; `internal/checker/checker.go` holds **23+ separate link stores**
(`nodeLinks`, `valueSymbolLinks`, `typeAliasLinks`, `membersAndExportsLinks`, …)
keyed by `*ast.Node` / `*ast.Symbol`. All checker state is *already* off-node.
`PagedLinkStore`, keyed by dense integer ids in 256-entry pages, is an `IndexVec`
with paging in all but name.

So the port is *more* natural than the first draft assumed. Keeping children as
direct references avoids id indirection and a bounds check on the hot traversal
path, while everything that actually needs to be a graph becomes a table.

Two further forcing facts:

- `.Parent` is read **2,092 times** across `checker`, `ls`, and `binder`. Parent
  access must be O(1) and allocation-free — but it does not need to be a field, and
  a struct-of-arrays column keeps those reads from dragging flag bytes through
  cache.
- Making the AST plain data makes a parsed file `Send + Sync` and serializable,
  which is what parallel checking and incremental caching both need.

## Consequences

- Node structs omit every field upstream marks `goOnly` — `Symbol`, `Locals`,
  `FlowNode`, `NextContainer`, `facts`. This is enforced by the generator.
- `Flags` lives on the `NodeTable` envelope rather than on each node struct.
- Two table shapes, mirroring upstream's two: dense struct-of-arrays
  (`side_tables!`) for data every node has, and `PagedTable` for sparse data —
  the shape the checker's 23 link stores need.
- Nodes carry a `NodeId` so the side tables can be keyed from a node reference.

## How we would know this was wrong

If profiling shows side-table lookups dominating checker time versus a
field-access design, or if the borrow checker forces `RefCell` around the tables
so pervasively that the parallelism benefit evaporates. The latter is the open
risk tracked by spike `tsr-6n3` (checker memoisation vs. borrowck), which has no
prior art in oxc — their checker is a 144-line no-op scaffold.

## Alternatives

- **`Rc<RefCell<Node>>`.** Rejected: unwritable at this scale, and forfeits
  parallelism.
- **Fully index-based AST** (children as `NodeId`). The first draft's proposal.
  Still viable, and would win if long-lived AST storage across LSP edits proves
  harder than `self_cell` makes it look. Rejected on the evidence above.
