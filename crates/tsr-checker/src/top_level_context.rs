//! `ast.IsInTopLevelContext` (`ast/utilities.go:1778`) for the binder's
//! TS1262 arm (`checkContextualIdentifier`, `binder.go:1311`).
//!
//! Two steps:
//!
//! 1. The name of a class or function **declaration** is a binding identifier
//!    in its surrounding scope, so the walk starts from the declaration.
//!    `export function await() {}` is at the top level.
//! 2. `GetThisContainer(node, includeArrowFunctions=true, false)` is the
//!    source file. An arrow, every function-like, a namespace, an enum, a
//!    class property and a static block each stop the walk.
//!
//! `docs/parity/notes/r6-smallcodes4.md` §2.4 records the hook and its
//! measurement.

use tsr_ast::{Node, NodeId, SyntaxKind};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `ast.IsInTopLevelContext(node)` (`ast/utilities.go:1778`).
    pub(crate) fn is_identifier_in_top_level_context(&self, node: NodeId) -> bool {
        let mut node = node;
        if let Some(parent) = self.nodes.parent(node) {
            let declared_name = match self.node_map.get(parent) {
                Some(Node::ClassDeclaration(declaration)) => {
                    declaration.name.and_then(|name| name.node_id)
                }
                Some(Node::FunctionDeclaration(declaration)) => {
                    declaration.name.and_then(|name| name.node_id)
                }
                _ => None,
            };
            if declared_name == Some(node) {
                node = parent;
            }
        }
        self.get_this_container(node, true)
            .is_some_and(|container| self.nodes.kind(container) == SyntaxKind::SourceFile)
    }
}
