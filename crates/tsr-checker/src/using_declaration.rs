//! `checkVariableLikeDeclaration`'s disposable-initializer arm
//! (`checker.go:5899`-`:5913`): TS2851 for `await using`, TS2850 for `using`.
//!
//! No cache, side table or traversal: the queries are the initializer's
//! memoised expression type, the declared types of the global
//! `AsyncDisposable`/`Disposable` interfaces, the object-literal widening
//! `widenTypeForVariableLikeDeclaration` performs, and one assignability
//! relation per declaration.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::checker::Checker;
use crate::relater::{Relation, Ternary};
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
    /// `NodeFlagsAwaitUsing` is not recorded by this parser, which consumes
    /// the `await` before `parseVariableDeclarationList` and flags the list
    /// `USING` alone; [`Checker::is_await_using_list`] recovers it from the
    /// token written before the list. Only a definite `NotRelated` on a
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
        let awaited = self.is_await_using_list(list);
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
        // Without `strictNullChecks` the union collapses to one interface, and
        // `reportRelationError` (`relater.go:4816`) then replaces the head
        // message with the relation's missing-property report (TS2741), whose
        // reporter is private to `assignreport.rs`. Declined there.
        if !self.store.get(target).flags.intersects(crate::flags::TypeFlags::UNION) {
            return;
        }
        if !self.assignability_pair_is_reportable(source, target)
            || self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(initializer_id) else { return };
        let span = self.error_span(initializer_id);
        self.report(file, Diagnostic::new(message, span));
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

    /// Is this `USING` list an `await using` one (`NodeFlagsAwaitUsing`,
    /// `Const|Using` upstream)? The parser eats the `await` immediately before
    /// the list (`statement.rs`'s statement and `for` heads), so the token
    /// written before the list's `using` decides it. A missing source text
    /// answers `false`, the `using` reading.
    pub(crate) fn is_await_using_list(&self, list: NodeId) -> bool {
        let Some(file) = self.source_file_of_for_diagnostics(list) else { return false };
        let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes))
        else {
            return false;
        };
        let start = self.nodes.span(list).start as usize;
        let Some(before) = text.get(..start) else { return false };
        let before = before.trim_end();
        before.strip_suffix("await").is_some_and(|rest| {
            !rest.chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        })
    }
}
