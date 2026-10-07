//! TS2806 — `Private accessor was defined without a getter.`
//!
//! `checkPropertyAccessExpressionOrQualifiedName`'s private-name arm
//! (`checker.go:11307`): once `getPrivateIdentifierPropertyOfType(leftType,
//! lexicallyScopedSymbol)` finds the property, a set-only accessor read in
//! any position but a definite assignment target is reported at the access.
//!
//! The lexically scoped symbol (`lookupSymbolForPrivateIdentifierDeclaration`)
//! is the private name declared by the nearest enclosing class that declares
//! it; the receiver's property must be that class's member (a same-spelled
//! name of another class is a different symbol upstream, and TS18014's
//! shadowing report instead). No cache, side table or traversal beyond the
//! access's class ancestors. `docs/parity/notes/misc-checks.md` §13.

use tsr_ast::{ClassElement, MemberName, Node, NodeId, PropertyName};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::expressions::AssignmentTargetKind;

impl Checker<'_, '_> {
    pub(crate) fn check_private_setter_read(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let Some(MemberName::PrivateIdentifier(name)) = access.name else { return };
        if self.assignment_target_kind(node) == AssignmentTargetKind::Definite {
            return;
        }
        let Some(lexical) = self.lexical_private_name_class(node, name.text) else { return };
        let Some(receiver) = access.expression else { return };
        let receiver_type = self.check_expression(receiver);
        if self.is_error(receiver_type) {
            return;
        }
        let receiver_type = self.get_non_nullable_type(receiver_type);
        let apparent = self.apparent_type(receiver_type);
        let Some(property) = self.get_property_of_type(apparent, name.text) else { return };
        let record = self.binder.symbols().get(property);
        let flags = record.flags;
        if !flags.intersects(SymbolFlags::SET_ACCESSOR)
            || flags.intersects(SymbolFlags::GET_ACCESSOR)
        {
            return;
        }
        if record.value_declaration.and_then(|declaration| self.containing_class_of(declaration))
            != Some(lexical)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(&messages::PRIVATE_ACCESSOR_WAS_DEFINED_WITHOUT_A_GETTER, span),
        );
    }

    /// The class whose `#name` declaration `lookupSymbolForPrivateIdentifierDeclaration`
    /// finds: the nearest enclosing class declaring it.
    fn lexical_private_name_class(&self, node: NodeId, text: &str) -> Option<NodeId> {
        self.nodes.ancestors(node).find(|&ancestor| {
            let members: &[ClassElement<'_>] = match self.node_map.get(ancestor) {
                Some(Node::ClassDeclaration(class)) => class.members,
                Some(Node::ClassExpression(class)) => class.members,
                _ => return false,
            };
            members.iter().any(|member| {
                let name = match member {
                    ClassElement::PropertyDeclaration(n) => n.name,
                    ClassElement::MethodDeclaration(n) => n.name,
                    ClassElement::GetAccessorDeclaration(n) => n.name,
                    ClassElement::SetAccessorDeclaration(n) => n.name,
                    _ => return false,
                };
                matches!(name, PropertyName::PrivateIdentifier(p) if p.text == text)
            })
        })
    }
}
