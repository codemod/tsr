//! The TypeScript AST.
//!
//! Node shapes, syntax kinds, and unions are **generated** from
//! `vendor/typescript-go/_scripts/ast.json` — the same schema-validated
//! definition upstream feeds to its own Go generator. Conformance is therefore a
//! property of the build: regenerate, and any upstream change appears as a diff.
//! See `tests/kind_conformance.rs` for the assertions that keep it honest.
//!
//! # Shape of the AST
//!
//! Per PLAN.md §3.2, *the tree is a tree*. Node structs hold only syntax children,
//! as direct arena references. Everything cyclic or phase-specific — parent,
//! symbol, scope, type, flow node — lives in id-keyed side tables ([`tsr_core`]),
//! which is what upstream also does via its 23 `LinkStore`s and what oxc does via
//! `AstNodes`.
//!
//! Concretely, fields upstream marks `goOnly` (`Symbol`, `Locals`, `FlowNode`,
//! `NextContainer`, `facts`) are absent from these structs by design.

mod flags;
mod generated;
pub mod parent;
pub use generated::visit::push_children;
pub use parent::assign_parents;

pub use flags::{ModifierFlags, NodeFlags, TokenFlags};
pub use generated::visit::Visit;
pub use generated::{alias::*, kind::SyntaxKind, nodes::*, visit};
use tsr_core::{Span, define_index};

define_index! {
    /// Identifies a node within a single source file.
    ///
    /// Assigned by the parser and used to key every side table. Scoped per file,
    /// not per program: a `NodeId` is only meaningful alongside the file it came
    /// from.
    pub struct NodeId;
}

/// Nodes that carry a [`NodeId`] slot.
///
/// Implemented for every generated node, so the parser can register any of them
/// through one generic path instead of repeating the assignment per node type.
pub trait HasNodeId {
    /// Record the id assigned by the parser.
    fn set_node_id(&self, id: NodeId);

    /// The id, or `None` if the node has not been registered.
    fn node_id(&self) -> Option<NodeId>;
}

/// A token node.
///
/// Upstream models tokens as a generic `Token[TKind]` with 33 named
/// instantiations (`AsteriskToken`, `QuestionToken`, …). Since the instantiations
/// differ only in which kinds they admit, they collapse here into one type
/// carrying its kind — the constraint is documented on each field that uses it.
/// Deliberately not `Copy`, unlike every other single-field node: the
/// `node_id` cell is what makes a token findable in the side tables, and a
/// `Copy` token would silently duplicate an id that identifies one position.
#[derive(Debug, Clone)]
pub struct Token<'a> {
    /// Which token this is.
    pub kind: SyntaxKind,
    /// Key into the side tables holding this node's kind, span, and parent.
    pub node_id: std::cell::Cell<Option<NodeId>>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl Token<'_> {
    /// Create a token of the given kind.
    #[must_use]
    pub const fn new(kind: SyntaxKind) -> Self {
        Self { kind, node_id: std::cell::Cell::new(None), _marker: std::marker::PhantomData }
    }
}

impl HasNodeId for Token<'_> {
    fn set_node_id(&self, id: NodeId) {
        self.node_id.set(Some(id));
    }

    fn node_id(&self) -> Option<NodeId> {
        self.node_id.get()
    }
}

/// Per-node data assigned during parsing and binding.
///
/// A struct-of-arrays side table rather than fields on each node: the checker
/// reads parents constantly without touching flags, and keeping the columns apart
/// means those reads do not drag flag bytes through cache. Upstream reads
/// `.Parent` 2,092 times across `checker`, `ls`, and `binder`.
#[derive(Debug, Default)]
pub struct NodeTable {
    parent: Vec<Option<NodeId>>,
    kind: Vec<SyntaxKind>,
    span: Vec<Span>,
    flags: Vec<NodeFlags>,
}

impl NodeTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty table sized for `nodes` entries.
    ///
    /// Four parallel vectors means four capacity checks and four growth
    /// reallocations per doubling; reserving once removes all of them. The caller
    /// estimates from source length — being wrong costs a little memory, while
    /// being right removes ~4% of parse time on a large file.
    #[must_use]
    pub fn with_capacity(nodes: usize) -> Self {
        Self {
            parent: Vec::with_capacity(nodes),
            kind: Vec::with_capacity(nodes),
            span: Vec::with_capacity(nodes),
            flags: Vec::with_capacity(nodes),
        }
    }

    /// Number of nodes recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    /// Whether any nodes are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    /// Record a node, returning its id.
    ///
    /// # Panics
    /// Panics if the file contains more than `u32::MAX - 1` nodes.
    pub fn push(&mut self, kind: SyntaxKind, span: Span, flags: NodeFlags) -> NodeId {
        let id = NodeId::new(u32::try_from(self.len()).expect("node count exceeds u32"));
        self.parent.push(None);
        self.kind.push(kind);
        self.span.push(span);
        self.flags.push(flags);
        id
    }

    /// The node's kind.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn kind(&self, id: NodeId) -> SyntaxKind {
        self.kind[id.as_u32() as usize]
    }

    /// The node's span.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn span(&self, id: NodeId) -> Span {
        self.span[id.as_u32() as usize]
    }

    /// The node's flags.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn flags(&self, id: NodeId) -> NodeFlags {
        self.flags[id.as_u32() as usize]
    }

    /// The node's parent, or `None` for the source file root.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parent[id.as_u32() as usize]
    }

    /// Set the node's parent. Called by the binder.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    pub fn set_parent(&mut self, id: NodeId, parent: NodeId) {
        self.parent[id.as_u32() as usize] = Some(parent);
    }

    /// Discard rows from `len` onward.
    ///
    /// Used by speculative parsing: a rejected attempt registers nodes that never
    /// enter the tree. Sound only because ids are handed out sequentially and a
    /// discarded node's id is not yet referenced anywhere.
    pub fn truncate(&mut self, len: usize) {
        self.parent.truncate(len);
        self.kind.truncate(len);
        self.span.truncate(len);
        self.flags.truncate(len);
    }

    /// Walk from `id` up to the root.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self.parent(id), move |&n| self.parent(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_discriminants_are_contiguous_from_zero() {
        assert_eq!(SyntaxKind::Unknown as u16, 0);
        assert_eq!(SyntaxKind::ALL.len(), SyntaxKind::COUNT as usize);
        for (i, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(*kind as u16 as usize, i, "kind {kind} has the wrong discriminant");
        }
    }

    #[test]
    fn from_u16_round_trips_and_rejects_out_of_range() {
        for kind in SyntaxKind::ALL {
            assert_eq!(SyntaxKind::from_u16(kind as u16), Some(kind));
        }
        assert_eq!(SyntaxKind::from_u16(SyntaxKind::COUNT), None);
        assert_eq!(SyntaxKind::from_u16(u16::MAX), None);
    }

    #[test]
    fn node_table_tracks_parents_and_ancestors() {
        let mut t = NodeTable::new();
        let root = t.push(SyntaxKind::SourceFile, Span::new(0, 10), NodeFlags::empty());
        let child = t.push(SyntaxKind::Block, Span::new(1, 9), NodeFlags::empty());
        let grandchild = t.push(SyntaxKind::EmptyStatement, Span::new(2, 3), NodeFlags::empty());
        t.set_parent(child, root);
        t.set_parent(grandchild, child);

        assert_eq!(t.kind(child), SyntaxKind::Block);
        assert_eq!(t.parent(root), None);
        assert_eq!(t.ancestors(grandchild).collect::<Vec<_>>(), vec![child, root]);
    }
}
