//! What type a *symbol* has.
//!
//! Ported from `Checker.getTypeOfSymbol` (`checker.go:16493`) and the
//! variable/parameter/property worker beneath it. Split out of
//! [`crate::checker`] because the largest single item on the histogram is the
//! arm this module does **not** have — `getTypeOfFuncClassEnumModule`
//! (`bd tsr-4sc.8`).
//!
//! Not to be confused with [`crate::declared`], which answers what a *type*
//! symbol declares. A class `C` **declares** the instance type `C` and **has**
//! the type `typeof C`.

use tsr_ast::{Expression, Node, NodeFlags, NodeId, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{checker::Checker, resolution::PropertyName, types::TypeId};

impl<'a> Checker<'a, '_> {
    /// The type of a symbol.
    ///
    /// Ported from `Checker.getTypeOfSymbol` (`checker.go:16493`). Upstream
    /// dispatches on nine symbol shapes; this slice ports the
    /// variable/parameter/property one and returns `errorType` for the rest.
    ///
    /// **`errorType`, not `anyType`.** Both print `any`, and only one of them is
    /// a claim that the answer *is* `any`. Every unported shape must be
    /// distinguishable from a computed answer or the conformance suite cannot
    /// tell a gap from a result.
    pub fn get_type_of_symbol(&mut self, symbol: SymbolId) -> TypeId {
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::VARIABLE | SymbolFlags::PROPERTY) {
            return self.get_type_of_variable_or_parameter_or_property(symbol);
        }
        // Unported: accessors, functions, classes, enums, enum members, modules,
        // aliases, and the four `CheckFlags` shapes upstream tests first
        // (deferred, instantiated, mapped, reverse-mapped).
        //
        // **The flags test above is currently unobservable**, and that is stated
        // rather than hidden behind a test that would not bite: for every symbol
        // shape this slice reaches, removing the test changes nothing, because a
        // non-variable symbol's declaration kind is rejected by the worker's
        // match anyway and lands on the same `errorType`. It is kept because it
        // is upstream's dispatch (`checker.go:16509`) and it starts to matter the
        // moment `getTypeOfFuncClassEnumModule` exists. Mutating it to `if true`
        // turns no test red today; mutating either `errorType` below does.
        self.intrinsics.error
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrProperty`
    /// (`checker.go:16544`).
    ///
    /// The memo is ADR-0013's read-drop-recurse-write: the lookup's borrow ends
    /// before the recursion, because `TypeId` is `Copy` and nothing borrowed from
    /// `self` survives into it.
    fn get_type_of_variable_or_parameter_or_property(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        let computed = self.get_type_of_variable_or_parameter_or_property_worker(symbol);
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrPropertyWorker`
    /// (`checker.go:16578`).
    fn get_type_of_variable_or_parameter_or_property_worker(&mut self, symbol: SymbolId) -> TypeId {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return self.intrinsics.error;
        };

        // The circularity guard wraps the *whole* computation, so a type that
        // reaches itself through any depth of indirection is caught. Nothing
        // between here and `pop` may return early, or the stack unbalances.
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.report_circularity_error(declaration);
        }

        let kind = self.nodes.kind(declaration);
        let result = match kind {
            SyntaxKind::VariableDeclaration
            | SyntaxKind::Parameter
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature => {
                self.get_widened_type_for_variable_like_declaration(declaration)
            }
            // Unported: property assignments, shorthand, methods, export
            // assignments, binary/call assignment declarations, JSX attributes
            // and enum members.
            _ => self.intrinsics.error,
        };

        if !self.resolutions.pop() {
            // A cycle closed *below* this frame, so the answer computed above was
            // built on a partial one and must not be kept.
            return self.report_circularity_error(declaration);
        }
        result
    }

    /// Ported from `Checker.reportCircularityError` (`checker.go:18822`),
    /// without the diagnostics — the checker has none yet (`bd tsr-5e7.6`).
    ///
    /// The **return type differs by cause**, which is easy to get wrong because
    /// the two print identically:
    ///
    /// - a self-referencing *type annotation* yields `errorType`;
    /// - a self-referencing *initialiser* yields `anyType`.
    ///
    /// `bd tsr-4sc.2`'s issue text said "errorType" flatly. It is not.
    fn report_circularity_error(&mut self, declaration: NodeId) -> TypeId {
        if self.type_annotation_of(declaration).is_some() {
            return self.intrinsics.error;
        }
        self.intrinsics.any
    }

    /// Ported from `Checker.getWidenedTypeForVariableLikeDeclaration`, which is
    /// `widenTypeForVariableLikeDeclaration(getTypeForVariableLikeDeclaration(..))`
    /// (`checker.go:16610`, `:18242`).
    fn get_widened_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> TypeId {
        match self.get_type_for_variable_like_declaration(declaration) {
            Some(id) => id,
            // Upstream returns `anyType` for a declaration with neither an
            // annotation nor an initialiser (`checker.go:18264`) — a genuine
            // answer, the implicit any, not a gap. So `anyType` is right here
            // where `errorType` is right for an unported form.
            None => self.intrinsics.any,
        }
    }

    /// Ported from `Checker.getTypeForVariableLikeDeclaration`
    /// (`checker.go:16652`), restricted to the two paths this slice covers.
    ///
    /// `None` means "nothing could be inferred", which upstream signals with a
    /// nil `*Type` and turns into the implicit `any` one level up.
    fn get_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> Option<TypeId> {
        // An annotation wins over an initialiser, always.
        if let Some(annotation) = self.type_annotation_of(declaration) {
            return Some(self.get_type_from_type_node(annotation));
        }
        let initializer = self.initializer_of(declaration)?;
        let initializer_type = self.check_expression(initializer);
        Some(self.get_widened_literal_type_for_initializer(declaration, initializer_type))
    }

    /// Ported from `Checker.getWidenedLiteralTypeForInitializer`
    /// (`checker.go:16897`).
    ///
    /// This is the rule behind the most frequently surprising line in a `.types`
    /// baseline: `const x = "a"` is `"a"` and `let x = "a"` is `string`, from the
    /// same initialiser expression. A `const` keeps the literal; anything else
    /// widens it.
    fn get_widened_literal_type_for_initializer(
        &mut self,
        declaration: NodeId,
        id: TypeId,
    ) -> TypeId {
        if self.combined_node_flags(declaration).intersects(NodeFlags::CONSTANT) {
            return id;
        }
        self.get_widened_literal_type(id)
    }

    /// The type annotation of a declaration, if it has one.
    pub(crate) fn type_annotation_of(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.r#type,
            Node::ParameterDeclaration(node) => node.r#type,
            Node::PropertyDeclaration(node) => node.r#type,
            Node::PropertySignatureDeclaration(node) => node.r#type,
            _ => None,
        }
    }

    /// The initialiser of a declaration, if it has one.
    fn initializer_of(&self, declaration: NodeId) -> Option<Expression<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.initializer,
            Node::ParameterDeclaration(node) => node.initializer,
            Node::PropertyDeclaration(node) => node.initializer,
            _ => None,
        }
    }
}
