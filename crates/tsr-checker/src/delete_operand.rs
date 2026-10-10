//! `checkDeleteExpression`'s private-name arm (`checker.go:10811`, TS18011)
//! and its symbol arms (`checker.go:10814`): TS2704 for a read-only operand
//! and TS2790 (`checkDeleteExpressionMustBeOptional`, `checker.go:10825`) for
//! a non-optional one.
//!
//! Both read `getResolvedSymbolOrNil(expr)` — the property symbol the access
//! resolved to — and TS2790 tests **that symbol's** type
//! (`getTypeOfSymbol`), not the operand expression's: a flow-narrowed or
//! optional-chain `undefined` in the expression type does not make the
//! property optional. The symbol is found the way
//! `checkPropertyAccessExpressionOrQualifiedName` finds it: the receiver's
//! non-nullable apparent type (an optional chain's
//! `getOptionalExpressionType` plus `checkNonNullType`), then
//! `getPropertyOfType`. No cache, side table or traversal.
//! `docs/parity/notes/misc-checks.md` §10.

use tsr_ast::{Expression, MemberName, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;

impl Checker<'_, '_> {
    /// `checkDeleteExpression` (`checker.go:10804`) after its
    /// not-an-access arm (TS2703, `check_reference_expression`).
    pub(crate) fn check_delete_operand_symbol(&mut self, node: NodeId) {
        let Some(Node::DeleteExpression(delete)) = self.node_map.get(node) else { return };
        let Some(operand) = delete.expression.and_then(|e| e.node_id()) else { return };
        // `expr := ast.SkipParentheses(node.Expression())`.
        let expr = self.skip_reference_spine_parentheses(operand);
        // `ast.IsPropertyAccessExpression(expr) && ast.IsPrivateIdentifier(expr.Name())`
        // (`checker.go:10811`): TS18011, and the symbol arms still run.
        if let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(expr)
            && matches!(access.name, Some(MemberName::PrivateIdentifier(_)))
        {
            self.report_delete_operand(
                expr,
                &messages::THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_PRIVATE_IDENTIFIER,
            );
        }
        let Some(symbol) = self.delete_operand_resolved_symbol(expr) else { return };
        if self.delete_operand_is_readonly(symbol) {
            self.report_delete_operand(
                expr,
                &messages::THE_OPERAND_OF_A_DELETE_OPERATOR_CANNOT_BE_A_READ_ONLY_PROPERTY,
            );
            return;
        }
        if !self.strict_null_checks {
            return;
        }
        // `CheckFlagsReadonly` lives on synthetic and instantiated property
        // symbols, which this port does not flag; a symbol that is not its
        // own declaration's symbol may be one, so the optional arm declines.
        let record = self.binder.symbols().get(symbol);
        let Some(declaration) = record.value_declaration else { return };
        if self.binder.symbol_of(declaration).map(|own| self.binder.merged_symbol(own))
            != Some(self.binder.merged_symbol(symbol))
        {
            return;
        }
        let ty = self.get_type_of_symbol(symbol);
        if self.is_type_any(ty)
            || self.store.get(ty).flags.intersects(TypeFlags::ANY_OR_UNKNOWN | TypeFlags::NEVER)
        {
            return;
        }
        // `hasTypeFacts(t, TypeFactsIsUndefined)`; the
        // `exactOptionalPropertyTypes` arm is not ported (§587 of
        // `checker-notes-diag2.md`).
        if self.nullish_facts(ty).1 {
            return;
        }
        self.report_delete_operand(
            expr,
            &messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_OPTIONAL,
        );
    }

    fn report_delete_operand(&mut self, expr: NodeId, message: &'static tsr_diagnostics::Message) {
        let Some(file) = self.source_file_of_for_diagnostics(expr) else { return };
        let span = self.error_span(expr);
        self.report(file, Diagnostic::new(message, span));
    }

    fn skip_reference_spine_parentheses(&self, mut node: NodeId) -> NodeId {
        for _ in 0..64 {
            let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node) else {
                return node;
            };
            let Some(next) = inner.expression.and_then(|e| e.node_id()) else { return node };
            node = next;
        }
        node
    }

    /// `getResolvedSymbolOrNil(expr)` for a property access with an
    /// identifier name, or an element access whose argument is a string or
    /// canonical numeric literal. `None` when the access resolves no
    /// property here (upstream then skips both arms too, or this port cannot
    /// find the member).
    fn delete_operand_resolved_symbol(&mut self, expr: NodeId) -> Option<SymbolId> {
        let (receiver, name): (Expression<'_>, String) = match self.node_map.get(expr)? {
            Node::PropertyAccessExpression(access) => match access.name? {
                MemberName::Identifier(name) => (access.expression?, name.text.to_string()),
                // A private name resolves only lexically inside its declaring
                // class; the access's own check answers the error type when
                // that lookup fails, and then there is no symbol.
                MemberName::PrivateIdentifier(name) => {
                    let receiver = access.expression?;
                    let text = name.text.to_string();
                    let access_type =
                        self.check_expression(Expression::PropertyAccessExpression(access));
                    if self.is_gap(access_type) {
                        return None;
                    }
                    (receiver, text)
                }
            },
            Node::ElementAccessExpression(access) => {
                let name = match access.argument_expression? {
                    Expression::StringLiteral(literal) => literal.text.to_string(),
                    Expression::NoSubstitutionTemplateLiteral(literal) => literal.text.to_string(),
                    Expression::NumericLiteral(literal)
                        if literal.text == "0"
                            || (!literal.text.starts_with('0')
                                && literal.text.bytes().all(|b| b.is_ascii_digit())) =>
                    {
                        literal.text.to_string()
                    }
                    _ => return None,
                };
                (access.expression?, name)
            }
            _ => return None,
        };
        let receiver_type = self.check_expression(receiver);
        if self.is_type_any(receiver_type) {
            return None;
        }
        let receiver_type = self.get_non_nullable_type(receiver_type);
        if self
            .store
            .get(receiver_type)
            .flags
            .intersects(TypeFlags::ANY_OR_UNKNOWN | TypeFlags::NEVER)
        {
            return None;
        }
        let apparent = self.apparent_type(receiver_type);
        self.get_property_of_type(apparent, &name)
    }

    /// `isReadonlySymbol` (`checker.go:13849`). The port's
    /// `is_readonly_symbol` covers the declaration arms except a `readonly`
    /// modifier on a signature; `getDeclarationModifierFlagsFromSymbol`
    /// reads the value declaration's modifiers whatever its kind.
    fn delete_operand_is_readonly(&mut self, symbol: SymbolId) -> bool {
        if self.is_readonly_symbol(symbol) {
            return true;
        }
        let record = self.binder.symbols().get(symbol);
        if !record.flags.intersects(SymbolFlags::PROPERTY) {
            return false;
        }
        let Some(declaration) = record.value_declaration else { return false };
        let modifiers = match self.node_map.get(declaration) {
            Some(Node::PropertySignatureDeclaration(signature)) => signature.modifiers,
            Some(Node::ParameterDeclaration(parameter)) => parameter.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            Node::from(*modifier)
                .node_id()
                .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ReadonlyKeyword)
        })
    }
}
