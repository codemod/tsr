//! Filling in the parent column.
//!
//! The tree has no back-edges — [ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)
//! — so "who is my parent" is answered by [`NodeTable`], and something has to put
//! the answers there. typescript-go does it inside `finishNode`, which calls
//! `overrideParentInImmediateChildren` and re-walks each node's children as the
//! node is created; profiling its parser puts that at **9.6%** of a
//! `dom.generated.d.ts` parse.
//!
//! We do it as one pass afterwards instead. Comparable cost — measured at ~10.5%
//! — but separable: a consumer that never walks upward (a formatter, a syntax
//! highlighter, the conformance harness) can skip it, which is a choice upstream's
//! design does not offer because the pointer lives in the node.

use crate::{Node, NodeId, NodeTable, push_children};

/// Record every node's parent in `nodes`.
///
/// Iterative rather than recursive. The obvious implementation — a [`Visit`] impl
/// that recurses through `walk_node` — overflows the stack on deeply nested input,
/// because tree depth is a function of the *source*, and the parser's own depth
/// guard bounds how deep it will *descend*, not how deep the tree it produces can
/// be. A parser that survives hostile input only to have the next pass abort on it
/// has not survived it.
pub fn assign_parents<'a>(root: Node<'a>, nodes: &mut NodeTable) {
    // (node, the nearest registered ancestor).
    let mut stack: Vec<(Node<'a>, Option<NodeId>)> = vec![(root, None)];
    let mut children: Vec<Node<'a>> = Vec::with_capacity(16);

    while let Some((node, parent)) = stack.pop() {
        let id = node.node_id();
        if let (Some(id), Some(parent)) = (id, parent) {
            nodes.set_parent(id, parent);
        }
        // A node without an id is transparent: its children inherit the nearest
        // registered ancestor rather than getting a hole in the chain.
        let next_parent = id.or(parent);

        children.clear();
        push_children(node, &mut children);
        stack.extend(children.iter().map(|child| (*child, next_parent)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unvisited_node_has_no_parent() {
        let mut nodes = NodeTable::new();
        let root = nodes.push(
            crate::SyntaxKind::SourceFile,
            tsr_core::Span::new(0, 1),
            crate::NodeFlags::empty(),
        );
        assert_eq!(nodes.parent(root), None);
    }
}
