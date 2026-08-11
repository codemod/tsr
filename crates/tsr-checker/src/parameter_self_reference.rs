//! TS2372 — `Parameter '{0}' cannot reference itself.`
//!
//! `resolveNameHelper` (`checker.go:1850`-`:1858`). Upstream reports while
//! resolving, from two pieces of walk state this port does not thread —
//! `associatedDeclarationForContainingInitializerOrBindingName` and
//! `withinDeferredContext`. Both are recoverable from the tree: the associated
//! declaration is the nearest enclosing parameter reached **through its
//! initializer**, and the deferred context is any function-like crossed on the
//! way there.
//!
//! `docs/architecture/checker-notes-diag2.md` §980.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// One identifier in a value position.
    pub(crate) fn check_parameter_self_reference(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors || !self.is_value_reference(node) {
            return;
        }
        let Some(parameter) = self.enclosing_parameter_initializer(node) else { return };
        let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
            return;
        };
        let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { return };
        if name.text != text {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PARAMETER_0_CANNOT_REFERENCE_ITSELF,
                span,
                [text.to_string()],
            ),
        );
    }

    /// The parameter whose **initializer** contains `node`, if no function-like
    /// lies between — `a = () => a` defers the read and is legal. §980.
    fn enclosing_parameter_initializer(&self, node: NodeId) -> Option<NodeId> {
        let mut child = node;
        for ancestor in self.nodes.ancestors(node) {
            if self.is_function_like_or_static_block(ancestor)
                && self.nodes.kind(ancestor) != SyntaxKind::Parameter
            {
                return None;
            }
            if let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(ancestor) {
                // Reached **through the initializer**, not through the type
                // annotation — `(x: typeof x)` is falsifier 3.
                return (parameter.initializer.and_then(|e| e.node_id()) == Some(child))
                    .then_some(ancestor);
            }
            child = ancestor;
        }
        None
    }
}
