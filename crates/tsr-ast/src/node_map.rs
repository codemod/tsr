//! `NodeId` → the typed node.
//!
//! The way back into the tree. [`NodeTable`](crate::NodeTable) records what a
//! node *is* — kind, span, flags, parent — and this records the node itself, so
//! that a consumer holding only an id can read its fields.
//!
//! # Why this exists at all
//!
//! The checker cannot compute a declaration's type without it. A binder `Symbol`
//! holds `value_declaration: NodeId`, while the annotation and initialiser live
//! in the typed node. Upstream never meets the problem: a Go `*ast.Symbol` holds
//! a real `*ast.Node`. This is
//! [ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md) — a tree with no
//! back-edges — meeting a design that assumed pointers.
//!
//! It also makes `NodeTable::parent` useful, which is the larger half: a parent
//! is an id, and upstream reads `.Parent` 1,134 times in `internal/checker`
//! alone.
//!
//! # Why the parser fills it
//!
//! Because the parser is the only stage where it is nearly free. Ids are handed
//! out sequentially by `NodeTable::push`, and the parser allocates the typed node
//! immediately afterwards, so recording is a `Vec::push` in the same order — no
//! zeroed allocation, and no second traversal.
//!
//! The binder was tried first and measured worse on both counts; see
//! [ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md), which
//! supersedes ADR-0032.

use crate::{Node, NodeId};

/// Every node the parser registered, indexed by [`NodeId`].
///
/// Kept in lockstep with [`NodeTable`](crate::NodeTable): the same `push` order
/// and the same `truncate` on speculative rollback, so an id valid in one is
/// valid in the other.
#[derive(Debug, Default)]
pub struct NodeMap<'a> {
    nodes: Vec<Node<'a>>,
}

impl<'a> NodeMap<'a> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty map sized for `nodes` entries.
    #[must_use]
    pub fn with_capacity(nodes: usize) -> Self {
        Self { nodes: Vec::with_capacity(nodes) }
    }

    /// How many nodes are recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no node is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Record a node. Called by the parser, immediately after `NodeTable::push`.
    ///
    /// The caller is responsible for keeping the order: this pushes, so entry
    /// *n* must be the node whose id is *n*. [`NodeMap::get`] is only meaningful
    /// because that holds, and the parser has a test that says so.
    pub fn push(&mut self, node: Node<'a>) {
        self.nodes.push(node);
    }

    /// The typed node behind an id, or `None` if the id is not from this file.
    ///
    /// `Option` rather than a panic: an id can outlive the tree it named — a row
    /// abandoned by a speculative parse, or simply an id from another file — and
    /// a checker that meets one should fall back to an error type, not crash.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<Node<'a>> {
        self.nodes.get(id.as_u32() as usize).copied()
    }

    /// Discard entries from `len` onward, mirroring `NodeTable::truncate`.
    ///
    /// Speculative parsing registers nodes that never enter the tree. Both tables
    /// must roll back together or every id after the abandoned attempt refers to
    /// a different node in each.
    pub fn truncate(&mut self, len: usize) {
        self.nodes.truncate(len);
    }

    /// Bytes held, for `docs/architecture/performance.md`.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.nodes.capacity() * size_of::<Node<'a>>()
    }
}
