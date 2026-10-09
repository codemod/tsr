//! `checkVariableLikeDeclaration`'s disposable-initializer arm
//! (`checker.go:5899`-`:5913`): TS2851 for `await using`, TS2850 for `using`.
//!
//! No cache, side table or traversal: the queries are the initializer's
//! memoised expression type, the declared types of the global
//! `AsyncDisposable`/`Disposable` interfaces, the object-literal widening
//! `widenTypeForVariableLikeDeclaration` performs, and one assignability
//! relation per declaration, reported through `assignreport.rs`'s reporter.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Message, messages};

use crate::checker::Checker;
use crate::types::TypeId;

impl Checker<'_, '_> {
    /// The `using` / `await using` initializer check of
    /// `checkVariableLikeDeclaration` (`checker.go:5790`): on the symbol's
    /// value declaration, outside a `for...in`, the widened initializer type
    /// must be assignable to `AsyncDisposable | Disposable | null | undefined`
    /// (`await using`) or `Disposable | null | undefined` (`using`), reported
    /// at the initializer with the declaration form's head message. A missing
    /// global interface is upstream's `emptyObjectType` and skips the check.
    ///
    /// `blockScopeKind == NodeFlagsAwaitUsing` is the list's `CONST | USING`
    /// flags. Only a definite `NotRelated` on a
    /// reportable pair reports, `assignreport.rs`'s rule for the same
    /// `checkTypeAssignableTo`.
    pub(crate) fn check_using_declaration_initializer(&mut self, node: NodeId) {
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) else { return };
        let Some(initializer) = declaration.initializer else { return };
        let Some(initializer_id) = initializer.node_id() else { return };
        let Some(list) = self.nodes.parent(node) else { return };
        if self.nodes.kind(list) != SyntaxKind::VariableDeclarationList
            || !self.nodes.flags(list).contains(tsr_ast::NodeFlags::USING)
        {
            return;
        }
        let Some(statement) = self.nodes.parent(list) else { return };
        if self.nodes.kind(statement) == SyntaxKind::ForInStatement {
            return;
        }
        let awaited = self.nodes.flags(list) & tsr_ast::NodeFlags::BLOCK_SCOPED
            == tsr_ast::NodeFlags::CONSTANT;
        // `node == symbol.ValueDeclaration` (`checker.go:5894`).
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        if self.binder.symbols().get(symbol).value_declaration != Some(node) {
            return;
        }
        let (members, message): (Vec<TypeId>, &'static Message) = if awaited {
            let (Some(async_disposable), Some(disposable)) = (
                self.global_disposable_type("AsyncDisposable"),
                self.global_disposable_type("Disposable"),
            ) else {
                return;
            };
            (
                vec![async_disposable, disposable, self.intrinsics.null, self.intrinsics.undefined],
                &messages::THE_INITIALIZER_OF_AN_AWAIT_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_ASYNCDISPOSE_OR_SYMBOL_DISPOSE_METHOD_OR_BE_NULL_OR_UNDEFINED,
            )
        } else {
            let Some(disposable) = self.global_disposable_type("Disposable") else { return };
            (
                vec![disposable, self.intrinsics.null, self.intrinsics.undefined],
                &messages::THE_INITIALIZER_OF_A_USING_DECLARATION_MUST_BE_EITHER_AN_OBJECT_WITH_A_SYMBOL_DISPOSE_METHOD_OR_BE_NULL_OR_UNDEFINED,
            )
        };
        let initializer_type = self.check_expression(initializer);
        // `widenTypeForVariableLikeDeclaration(initializerType, node, false)`:
        // `getWidenedType`, whose object-literal normalisation is
        // `getWidenedTypeWithContext`; the implicit-any report is off.
        let source = self.widen_object_literal_freshness(initializer_type);
        let target = self.get_union_type(&members);
        // `checkTypeAssignableTo` with the head message and no expression to
        // elaborate: the shared reporter, so `reportRelationError`
        // (`relater.go:4816`) replaces the head with the missing-property
        // report (TS2741) when the chain names the same pair — without
        // `strictNullChecks` the union is `Disposable` itself, and with it
        // `isRelatedToEx` narrows a non-nullable source to `Disposable`.
        let span = self.error_span(initializer_id);
        self.report_relation_failure(initializer_id, span, None, source, target, Some(message));
    }

    /// `getGlobalAsyncDisposableType` / `getGlobalDisposableType`
    /// (`checker.go:1069`-`:1070`, `getGlobalTypeResolver(name, 0, true)`): the
    /// interface's declared type, or `None` where upstream answers
    /// `emptyObjectType`.
    fn global_disposable_type(&mut self, name: &str) -> Option<TypeId> {
        let symbol = self.global_type_symbol_with_arity(name, 0)?;
        let declared = self.get_declared_type_of_symbol(symbol);
        (!self.is_gap(declared)).then_some(declared)
    }
}
