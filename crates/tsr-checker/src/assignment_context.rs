//! The nil arms of `getContextualTypeForAssignmentExpression`
//! (`checker.go:29843`), as a syntactic-plus-symbol test the
//! no-contextual-type walk (`has_no_contextual_type`, `signatures.rs`) can
//! ask without computing a type.
//!
//! Native answers **no contextual type** for the right operand of
//! `module.exports = expr`, and of an assignment declaration
//! (`binary.Symbol != nil`, the binder's expando / `exports.x` / `F.x`
//! forms) unless its receiver is a variable whose declaration carries a type
//! annotation. Those are exactly the cases where typing the right operand
//! from the left would ask for the type the right operand is declaring.
//! `contextual_type_for_binary_operand`'s assignment arm (`contextual.rs`)
//! answers the same nil for the same shapes; this is its proof-of-absence
//! twin. `docs/parity/notes/r5-js.md` §3.4.
//!
//! Convention record (`docs/conventions.md`): no cache, no side table; the
//! work is one name resolution of the receiver identifier.

use tsr_ast::{Expression, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;

impl crate::Checker<'_, '_> {
    /// Whether native's `getContextualTypeForAssignmentExpression` answers
    /// nil for `binary`'s right operand. `false` means "context may exist":
    /// the caller keeps its own refusal.
    pub(crate) fn assignment_has_no_contextual_type(
        &self,
        binary_id: NodeId,
        binary: &tsr_ast::BinaryExpression<'_>,
    ) -> bool {
        let receiver = match binary.left {
            Some(Expression::PropertyAccessExpression(access)) => access.expression,
            Some(Expression::ElementAccessExpression(access)) => access.expression,
            _ => return false,
        };
        let declares = self.binder.symbol_of(binary_id).is_some();
        match receiver {
            Some(Expression::Identifier(identifier)) => {
                let Some(reference) = identifier.node_id else { return false };
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    reference,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                // `getExportSymbolOfValueSymbolIfExported`.
                let symbol = self.binder.symbols().get(symbol).export_symbol.unwrap_or(symbol);
                // "No contextual type for an expression of the form
                // 'module.exports = expr'."
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::MODULE_EXPORTS) {
                    return true;
                }
                if !declares {
                    return false;
                }
                // The contextual arm keeps a TypeScript class receiver on the
                // left operand's type (its synthetic prototype); so does this.
                let merged = self.binder.merged_symbol(symbol);
                if !self.in_js_file(binary_id)
                    && self.binder.symbols().get(merged).flags.contains(SymbolFlags::CLASS)
                {
                    return false;
                }
                // Only an annotated variable supplies context.
                match self.binder.symbols().get(symbol).value_declaration {
                    Some(declaration)
                        if self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration =>
                    {
                        self.type_annotation_of(declaration).is_none()
                            && self.jsdoc_type_annotation(declaration).is_none()
                    }
                    _ => true,
                }
            }
            // `F.a.b = expr` / `F[k].b = expr` declaring a member.
            Some(
                Expression::PropertyAccessExpression(_) | Expression::ElementAccessExpression(_),
            ) => declares,
            _ => false,
        }
    }
}
