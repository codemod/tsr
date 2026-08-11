//! TS2341 — `Property '{0}' is private and only accessible within class '{1}'.`
//!
//! The syntactic slice of `checkPropertyAccessibility`: a **private static**
//! member reached through an identifier that names a class, from outside that
//! class's body. A private *instance* member needs the receiver's type and is
//! declined.
//!
//! `docs/architecture/checker-notes-diag2.md` §999.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// One property access.
    pub(crate) fn check_private_static_access(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let Some(receiver) = access.expression.and_then(|e| e.node_id()) else { return };
        if self.nodes.kind(receiver) != SyntaxKind::Identifier {
            return;
        }
        let Some(tsr_ast::MemberName::Identifier(member)) = access.name else { return };
        let Some(class) = self.identifier_names_a_class(receiver) else { return };
        let Some((declaring, owner_name)) = self.private_static_member(class, member.text) else {
            return;
        };
        // **Containment is measured against the class declaration**, not the
        // symbol: `namespace D { … D.bar … }` merges with `class D` and is
        // still outside its body. §999.
        if self.nodes.ancestors(node).any(|ancestor| ancestor == declaring) {
            return;
        }
        let Some(at) = member.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_1,
                span,
                [member.text.to_string(), owner_name],
            ),
        );
    }

    /// The class declaration this identifier names, if it names one.
    fn identifier_names_a_class(&mut self, receiver: NodeId) -> Option<NodeId> {
        let text = self.identifier_text(receiver).map(str::to_string)?;
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            receiver,
            &text,
            SymbolFlags::VALUE | SymbolFlags::TYPE,
        )?;
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        declarations
            .into_iter()
            .find(|&declaration| self.nodes.kind(declaration) == SyntaxKind::ClassDeclaration)
    }

    /// Walk `class` and its `extends` chain for a `private static` member of
    /// this name, answering the **declaring** class and its name. §999.
    fn private_static_member(&mut self, class: NodeId, name: &str) -> Option<(NodeId, String)> {
        let mut current = Some(class);
        for _ in 0..16 {
            let at = current?;
            let Some(Node::ClassDeclaration(declaration)) = self.node_map.get(at) else {
                return None;
            };
            for element in declaration.members {
                let Some(id) = element.node_id() else { continue };
                if !self.member_is_static(id) {
                    continue;
                }
                let modifiers = match self.node_map.get(id) {
                    Some(Node::PropertyDeclaration(n)) => n.modifiers,
                    Some(Node::MethodDeclaration(n)) => n.modifiers,
                    _ => continue,
                };
                if !tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::PrivateKeyword) {
                    continue;
                }
                let matches = self
                    .declaration_name_of(id)
                    .and_then(|at| self.identifier_text(at))
                    .is_some_and(|text| text == name);
                if matches {
                    let owner = declaration.name.and_then(|n| n.node_id)?;
                    return Some((at, self.identifier_text(owner)?.to_string()));
                }
            }
            current = self.base_class_declaration(at);
        }
        None
    }

    /// The class this one `extends`, resolved syntactically. §999.
    fn base_class_declaration(&mut self, class: NodeId) -> Option<NodeId> {
        let Some(Node::ClassDeclaration(declaration)) = self.node_map.get(class) else {
            return None;
        };
        for clause in declaration.heritage_clauses {
            if clause.token.kind != SyntaxKind::ExtendsKeyword {
                continue;
            }
            let base = clause.types.first()?;
            let expression = base.expression?.node_id()?;
            if self.nodes.kind(expression) != SyntaxKind::Identifier {
                return None;
            }
            return self.identifier_names_a_class(expression);
        }
        None
    }
}
