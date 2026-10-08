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
        // The resolver's associated declaration and `getIsDeferredContext`
        // walk (`class_fields.rs`); a binding element's association is not
        // this check's (`docs/parity/notes/r5-classfields.md` §7).
        let Some(parameter) = self
            .parameter_initializer_scope(node)
            .map(|(associated, _)| associated)
            .filter(|&associated| self.nodes.kind(associated) == SyntaxKind::Parameter)
        else {
            return;
        };
        let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
            return;
        };
        let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        // Upstream's `if` / `else if` (`checker.go:1855`-`:1858`): the parameter
        // naming *itself* first, and only then one naming a parameter declared
        // after it. The two cannot both fire. §1007.
        if name.text == text {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PARAMETER_0_CANNOT_REFERENCE_ITSELF,
                    span,
                    [text.to_string()],
                ),
            );
            return;
        }
        if self.names_a_later_parameter(parameter, text) {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PARAMETER_0_CANNOT_REFERENCE_IDENTIFIER_1_DECLARED_AFTER_IT,
                    span,
                    [name.text.to_string(), text.to_string()],
                ),
            );
        }
    }

    /// Does `text` name a parameter of the same list declared **after**
    /// `parameter`? §1007.
    fn names_a_later_parameter(&self, parameter: NodeId, text: &str) -> bool {
        let Some(owner) = self.nodes.parent(parameter) else { return false };
        let parameters = match self.node_map.get(owner) {
            Some(Node::FunctionDeclaration(n)) => n.parameters,
            Some(Node::FunctionExpression(n)) => n.parameters,
            Some(Node::ArrowFunction(n)) => n.parameters,
            Some(Node::MethodDeclaration(n)) => n.parameters,
            Some(Node::ConstructorDeclaration(n)) => n.parameters,
            _ => return false,
        };
        let mut seen_self = false;
        for candidate in parameters {
            let Some(id) = candidate.node_id else { continue };
            if id == parameter {
                seen_self = true;
                continue;
            }
            if !seen_self {
                continue;
            }
            if matches!(candidate.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == text)
            {
                return true;
            }
        }
        false
    }
}
