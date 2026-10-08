//! `getESSymbolLikeTypeForNode` (`checker.go:22982`): the `unique symbol`
//! type of a declaration.
//!
//! # The cache and its boundaries
//!
//! - **Native operation:** `c.uniqueESSymbolTypes[symbol]`, minted by
//!   `newUniqueESSymbolType(symbol, "__@name@id")` the first time any of its
//!   three callers asks: a written `unique symbol` (`getTypeFromTypeOperatorNode`,
//!   `checker.go:22969`), a `Symbol()`/`Symbol.for()` call in a declaration
//!   position (`checker.go:8351`) and the `SymbolConstructor` merge arm of
//!   `widenTypeForVariableLikeDeclaration` (`checker.go:18247`).
//! - **Key identity and owner:** the declaration's merged [`SymbolId`],
//!   private to this Checker. The type node or call that asked is *not* part
//!   of the key: `declare const x: unique symbol` and `const x = Symbol()` in
//!   two merged declarations, or a `typeof x` read elsewhere, are one type.
//! - **Publication states:** absent or completed; the mint has no inputs that
//!   can be in progress, so there is no active state.
//! - **Expensive work boundary:** none: a store push per declaration symbol.
//!
//! A position that is not a valid unique-symbol declaration
//! (`isValidESSymbolDeclaration`, `utilities.go:961`) answers `esSymbolType`.
//! `docs/parity/notes/r5-instexpr.md` §3.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolId;

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::types::TypeId;

impl Checker<'_, '_> {
    /// `getESSymbolLikeTypeForNode(node)` for the declaration `node`.
    pub(crate) fn get_es_symbol_like_type_for_node(&mut self, declaration: NodeId) -> TypeId {
        if self.is_valid_es_symbol_declaration_node(declaration)
            && let Some(symbol) = self.binder.symbol_of(declaration)
        {
            let symbol: SymbolId = self.binder.merged_symbol(symbol);
            if let Some(&existing) = self.unique_es_symbol_types.get(&symbol) {
                return existing;
            }
            let minted = self.store.new_named(
                TypeFlags::UNIQUE_ES_SYMBOL,
                "unique symbol".to_string(),
                None,
            );
            self.unique_es_symbol_types.insert(symbol, minted);
            return minted;
        }
        self.intrinsics.es_symbol
    }

    /// The `getTypeFromTypeOperatorNode` caller: the declaration is
    /// `WalkUpParenthesizedTypes(node.Parent)`. A JSDoc `@type` is reparsed
    /// onto its host upstream (`reparser.go`, `KindJSDocTypeTag`): a variable
    /// statement's first declaration without a type, else the host itself.
    pub(crate) fn get_es_symbol_like_type_for_type_operator(&mut self, operator: NodeId) -> TypeId {
        if self.unique_symbol_awaits_printer_reuse(operator) {
            return self.legacy_unique_symbol_for_node(operator);
        }
        let mut current = operator;
        let mut parent = self.nodes.parent(current);
        while let Some(node) = parent
            && self.nodes.kind(node) == SyntaxKind::ParenthesizedType
        {
            current = node;
            parent = self.nodes.parent(current);
        }
        let Some(mut declaration) = parent else { return self.intrinsics.es_symbol };
        if self.nodes.kind(declaration) == SyntaxKind::JSDocTypeExpression {
            match self.jsdoc_type_tag_host(declaration) {
                Some(host) => declaration = host,
                None => return self.intrinsics.es_symbol,
            }
        }
        let resolved = self.get_es_symbol_like_type_for_node(declaration);
        // `typeNodeLinks.resolvedType`: the written operator's type, which
        // the declaration-emit tracker reads back to find `t.symbol`
        // (`symbol_access.rs`'s `unique_symbol_type_symbol`).
        if resolved != self.intrinsics.es_symbol {
            self.unique_symbol_nodes.insert(operator, resolved);
        }
        resolved
    }

    /// **The one decline left, and why.** In a signature's parameter, `this`
    /// or return annotation, a type predicate or a type parameter's
    /// constraint, `getESSymbolLikeTypeForNode` answers `esSymbolType`, yet
    /// the signature prints `unique symbol`: the node builder reuses the
    /// written node when it lies inside the print's enclosing declaration
    /// (`nodecopy.go:596`). This port's reuser refuses `unique symbol`
    /// outright (`node_reuse.rs`) and prints predicates and constraints from
    /// their types (`signatures.rs`), so answering `symbol` there turns 30
    /// RIGHT signature lines in `uniqueSymbolsErrors` WRONG. Until those
    /// printers port the rule (`docs/parity/notes/r5-instexpr.md` §3.3), these
    /// slots keep the previous per-node mint. A default (`<T = unique
    /// symbol>`) is not reused upstream and is not exempt.
    fn unique_symbol_awaits_printer_reuse(&self, operator: NodeId) -> bool {
        let mut child = operator;
        let mut current = self.nodes.parent(operator);
        while let Some(node) = current {
            match self.nodes.kind(node) {
                SyntaxKind::TypePredicate => return true,
                SyntaxKind::Parameter => return false,
                SyntaxKind::TypeParameter => {
                    return matches!(self.node_map.get(node),
                        Some(Node::TypeParameterDeclaration(parameter))
                            if parameter.constraint.and_then(|c| c.node_id()) == Some(child));
                }
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::FunctionType
                | SyntaxKind::ConstructorType => return false,
                SyntaxKind::VariableDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::IndexSignature
                | SyntaxKind::TypeAliasDeclaration
                | SyntaxKind::SourceFile => return false,
                _ => {}
            }
            child = node;
            current = self.nodes.parent(node);
        }
        false
    }

    /// The previous mint for [`Self::unique_symbol_awaits_printer_reuse`]'s
    /// slots: one `unique symbol` per written node.
    fn legacy_unique_symbol_for_node(&mut self, operator: NodeId) -> TypeId {
        if let Some(&existing) = self.unique_symbol_nodes.get(&operator) {
            return existing;
        }
        let minted =
            self.store.new_named(TypeFlags::UNIQUE_ES_SYMBOL, "unique symbol".to_string(), None);
        self.unique_symbol_nodes.insert(operator, minted);
        minted
    }

    /// The `resolveCallExpression` caller (`checker.go:8351`): the declaration
    /// is `WalkUpParenthesizedExpressions(node.Parent)`.
    pub(crate) fn get_es_symbol_like_type_for_call(&mut self, call: NodeId) -> TypeId {
        let mut parent = self.nodes.parent(call);
        while let Some(node) = parent
            && self.nodes.kind(node) == SyntaxKind::ParenthesizedExpression
        {
            parent = self.nodes.parent(node);
        }
        match parent {
            Some(declaration) => self.get_es_symbol_like_type_for_node(declaration),
            None => self.intrinsics.es_symbol,
        }
    }

    /// The declaration a JSDoc `@type {…}` expression is reparsed onto.
    fn jsdoc_type_tag_host(&self, expression: NodeId) -> Option<NodeId> {
        let tag = self.nodes.parent(expression)?;
        if self.nodes.kind(tag) != SyntaxKind::JSDocTypeTag {
            return None;
        }
        let mut comment = self.nodes.parent(tag)?;
        while self.nodes.kind(comment) != SyntaxKind::JSDoc {
            comment = self.nodes.parent(comment)?;
        }
        let host = *self.jsdoc_hosts.get(&comment)?;
        match self.node_map.get(host)? {
            Node::VariableStatement(statement) => statement
                .declaration_list?
                .declarations
                .iter()
                .find(|declaration| declaration.r#type.is_none())
                .and_then(|declaration| declaration.node_id),
            _ => Some(host),
        }
    }

    /// `isValidESSymbolDeclaration` (`utilities.go:961`).
    fn is_valid_es_symbol_declaration_node(&self, node: NodeId) -> bool {
        let has = |modifiers: &[tsr_ast::ModifierLike<'_>], kind: SyntaxKind| {
            modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == kind)
            })
        };
        match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) => {
                matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(_)))
                    && self.nodes.parent(node).is_some_and(|list| {
                        self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST)
                            && self.nodes.parent(list).is_some_and(|statement| {
                                self.nodes.kind(statement) == SyntaxKind::VariableStatement
                            })
                    })
            }
            Some(Node::PropertyDeclaration(property)) => {
                has(property.modifiers, SyntaxKind::ReadonlyKeyword)
                    && has(property.modifiers, SyntaxKind::StaticKeyword)
            }
            Some(Node::PropertySignatureDeclaration(property)) => {
                has(property.modifiers, SyntaxKind::ReadonlyKeyword)
            }
            _ => false,
        }
    }
}
