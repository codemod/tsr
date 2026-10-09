//! The pseudochecker's DIRECT answers: the written type node a declaration
//! or expression is syntactically typed by, ported from
//! `internal/pseudochecker/lookup.go` (pinned `5b1047d`).
//!
//! # What is ported, and what is not
//!
//! Upstream's pseudochecker maps a declaration to a `PseudoType`, a skeleton
//! of the type it *should* have built from syntax alone, and the node builder
//! reuses it when `pseudoTypeEquivalentToType` holds against the checker's
//! type (`checker/pseudotypenodebuilder.go:362`). Of its twenty kinds, only
//! `PseudoTypeKindDirect` (a written type node) changes what this port
//! prints: the other kinds rebuild text the type already prints the same way
//! (keywords, literals), or are structural (object literals, single call
//! signatures, tuples) and are not ported yet
//! (`docs/parity/notes/r5-nodereuse.md` §5). So each function here answers
//! `Some(node)` exactly where upstream answers `NewPseudoTypeDirect(node)`,
//! and `None` for every other kind.
//!
//! The Direct node is then handed to the existing reuse machinery
//! ([`crate::node_reuse`]): its type is `getTypeFromTypeNode(node)`
//! (`pseudoTypeToType`'s Direct arm, `pseudotypenodebuilder.go:694`), the
//! identity arm of the equivalence is asked at print time, and the visitor
//! re-emits the node.
//!
//! # Port-convention boundary (`docs/conventions.md`)
//!
//! - **Native operation:** `GetReturnTypeOfSignature` →
//!   `createReturnFromSignature` → `typeFromSingleReturnExpression`
//!   (`lookup.go:198`, `:211`), and `typeFromExpression`'s assertion arms
//!   (`:254`, `:510`), consumed by `serializeReturnTypeForSignature`
//!   (`nodebuilderimpl.go:2023`).
//! - **Key identity and owner:** none. No cache, no side table: the answer is
//!   a pure function of the syntax tree, as upstream's is.
//! - **Publication states:** none; nothing is stored.
//! - **Receiver/alias context:** the node is asked of the signature's own
//!   declaration; an instantiated signature whose return type the
//!   instantiation moved fails the print-time identity gate, as upstream's
//!   equivalence does.
//! - **Expensive work boundary:** upstream asks at PRINT time, and so does this
//!   port ([`crate::Checker::reused_return_text`]): the body's return
//!   statements are walked once per printed unannotated signature, never on
//!   the checking path.

use crate::Checker;
use tsr_ast::{
    ConciseBody, Expression, FunctionBody, ModifierLike, Node, NodeId, SyntaxKind, TypeNode,
};

impl<'a> Checker<'a, '_> {
    /// `createReturnFromSignature` (`lookup.go:198`) restricted to its Direct
    /// answers: a function-like declaration's written return annotation,
    /// else, for a value-signature declaration, the body's
    /// `typeFromSingleReturnExpression` (`:211`).
    pub(crate) fn pseudo_direct_return_node(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        if let Some(annotation) = self.function_like_return_annotation(declaration) {
            return Some(annotation);
        }
        // `isValueSignatureDeclaration` (`lookup.go:194`), with the body.
        let (modifiers, asterisk, body): (&[ModifierLike<'a>], bool, _) =
            match self.node_map.get(declaration)? {
                Node::FunctionDeclaration(node) if node.r#type.is_none() => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Node::FunctionExpression(node) if node.r#type.is_none() => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Node::MethodDeclaration(node) if node.r#type.is_none() => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Node::ArrowFunction(node) if node.r#type.is_none() => {
                    (node.modifiers, false, node.body)
                }
                _ => return None,
            };
        let body = body?;
        // `FunctionFlagsAsyncGenerator`: an async generator is Inferred.
        let is_async = modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
        });
        if is_async && asterisk {
            return None;
        }
        let candidate = match body {
            ConciseBody::Block(block) => self.single_direct_return_expression(block)?,
            expression => Expression::try_from(Node::from(expression)).ok()?,
        };
        let candidate_id = Node::from(candidate).node_id()?;
        if self.pseudo_is_contextually_typed(candidate_id) {
            // A contextually typed candidate is Direct only when it is itself
            // a non-`const` assertion.
            let annotation = match candidate {
                Expression::AsExpression(node) => node.r#type?,
                Expression::TypeAssertion(node) => node.r#type?,
                _ => return None,
            };
            return (!crate::assertions::is_const_type_reference(annotation)).then_some(annotation);
        }
        self.pseudo_direct_type_of_expression(candidate)
    }

    /// `GetTypeOfDeclaration` (`lookup.go:34`) restricted to its Direct
    /// answers, for the declarations a printed property symbol can have:
    ///
    /// - a property signature or declaration: its annotation; an
    ///   unannotated property declaration's non-contextual initializer
    ///   (`typeFromProperty`, `:96`);
    /// - an object literal's property assignment: its initializer
    ///   (`typeFromPropertyAssignment`, `:66`);
    /// - a variable: its annotation, else a non-contextual initializer of a
    ///   variable with a single declaration (`typeFromVariable`, `:120`);
    /// - a parameter without an initializer: its annotation
    ///   (`typeFromParameter`'s fast path, `:620`). An initialized parameter
    ///   may need `| undefined` added (`typeFromParameterWorker`), which a
    ///   Direct node cannot express, so it answers none here.
    pub(crate) fn pseudo_direct_declaration_node(
        &self,
        declaration: NodeId,
    ) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::PropertySignatureDeclaration(node) => node.r#type,
            Node::PropertyDeclaration(node) => {
                if let Some(annotation) = node.r#type {
                    return Some(annotation);
                }
                let initializer = node.initializer?;
                if self.pseudo_is_contextually_typed(declaration) {
                    return None;
                }
                self.pseudo_direct_type_of_expression(initializer)
            }
            Node::PropertyAssignment(node) => {
                node.r#type.or_else(|| self.pseudo_direct_type_of_expression(node.initializer?))
            }
            Node::VariableDeclaration(node) => {
                if let Some(annotation) = node.r#type {
                    return Some(annotation);
                }
                let initializer = node.initializer?;
                let symbol = self.binder.symbol_of(declaration)?;
                let declarations = &self.binder.symbols().get(symbol).declarations;
                let single = declarations.len() == 1
                    || declarations
                        .iter()
                        .filter(|&&d| self.nodes.kind(d) == SyntaxKind::VariableDeclaration)
                        .count()
                        == 1;
                if !single || self.pseudo_is_contextually_typed(declaration) {
                    return None;
                }
                self.pseudo_direct_type_of_expression(initializer)
            }
            Node::ParameterDeclaration(node) if node.initializer.is_none() => {
                // A setter's parameter is typed by the accessor pair
                // (`GetTypeOfAccessor`), which is not ported here.
                if self
                    .nodes
                    .parent(declaration)
                    .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::SetAccessor)
                {
                    return None;
                }
                node.r#type
            }
            _ => None,
        }
    }

    /// The written return annotation (`FunctionLikeData().Type`) of a
    /// function-like declaration.
    pub(crate) fn function_like_return_annotation(
        &self,
        declaration: NodeId,
    ) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::FunctionDeclaration(node) => node.r#type,
            Node::FunctionExpression(node) => node.r#type,
            Node::ArrowFunction(node) => node.r#type,
            Node::MethodDeclaration(node) => node.r#type,
            Node::MethodSignatureDeclaration(node) => node.r#type,
            Node::CallSignatureDeclaration(node) => node.r#type,
            Node::ConstructSignatureDeclaration(node) => node.r#type,
            Node::FunctionTypeNode(node) => node.r#type,
            Node::ConstructorTypeNode(node) => node.r#type,
            Node::GetAccessorDeclaration(node) => node.r#type,
            Node::SetAccessorDeclaration(node) => node.r#type,
            Node::IndexSignatureDeclaration(node) => node.r#type,
            Node::ConstructorDeclaration(node) => node.r#type,
            _ => None,
        }
    }

    /// `ast.ForEachReturnStatement` with `typeFromSingleReturnExpression`'s
    /// callback (`lookup.go:221`): the expression of the block's ONLY return
    /// statement, provided it is a direct child of the block. A nested return
    /// (inside `if`, a loop, …) or a second return yields none, as does a
    /// bare `return;`. Only statement containers are entered, so a function
    /// or class never contributes its own returns.
    fn single_direct_return_expression(
        &self,
        block: &'a tsr_ast::Block<'a>,
    ) -> Option<Expression<'a>> {
        let block_id = block.node_id?;
        let mut candidate = None;
        let mut stack: Vec<Node<'a>> =
            block.statements.iter().rev().map(|&s| Node::from(s)).collect();
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            match node {
                Node::ReturnStatement(statement) => {
                    let id = statement.node_id?;
                    if self.nodes.parent(id) != Some(block_id) || candidate.is_some() {
                        return None;
                    }
                    candidate = Some(statement.expression);
                    continue;
                }
                // `ForEachReturnStatement`'s traversed kinds (`ast/utilities.go:1163`).
                Node::CaseBlock(_)
                | Node::Block(_)
                | Node::IfStatement(_)
                | Node::DoStatement(_)
                | Node::WhileStatement(_)
                | Node::ForStatement(_)
                | Node::ForInOrOfStatement(_)
                | Node::WithStatement(_)
                | Node::SwitchStatement(_)
                | Node::CaseOrDefaultClause(_)
                | Node::LabeledStatement(_)
                | Node::TryStatement(_)
                | Node::CatchClause(_) => {}
                _ => continue,
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().rev().copied());
        }
        candidate.flatten()
    }

    /// `typeFromExpression` (`lookup.go:254`) restricted to its Direct
    /// answers: a non-`const` type assertion (`typeFromTypeAssertion`,
    /// `:510`), through parentheses. A `const` assertion answers whatever its
    /// operand answers.
    pub(crate) fn pseudo_direct_type_of_expression(
        &self,
        expression: Expression<'a>,
    ) -> Option<TypeNode<'a>> {
        match expression {
            Expression::ParenthesizedExpression(node) => {
                self.pseudo_direct_type_of_expression(node.expression?)
            }
            Expression::AsExpression(node) => {
                self.pseudo_direct_type_of_assertion(node.expression?, node.r#type?)
            }
            Expression::TypeAssertion(node) => {
                self.pseudo_direct_type_of_assertion(node.expression?, node.r#type?)
            }
            _ => None,
        }
    }

    fn pseudo_direct_type_of_assertion(
        &self,
        expression: Expression<'a>,
        annotation: TypeNode<'a>,
    ) -> Option<TypeNode<'a>> {
        if crate::assertions::is_const_type_reference(annotation) {
            return self.pseudo_direct_type_of_expression(expression);
        }
        Some(annotation)
    }

    /// The pseudochecker's `isContextuallyTyped` (`lookup.go:713`): some
    /// ancestor (not stopping at function boundaries) is a call, a
    /// `satisfies`, a JSX element or expression, or an annotated variable,
    /// parameter, property or non-`const` assertion.
    pub(crate) fn pseudo_is_contextually_typed(&self, node: NodeId) -> bool {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            let contextual = match self.node_map.get(id) {
                Some(
                    Node::CallExpression(_)
                    | Node::SatisfiesExpression(_)
                    | Node::JsxElement(_)
                    | Node::JsxExpression(_),
                ) => true,
                Some(Node::VariableDeclaration(node)) => node.r#type.is_some(),
                Some(Node::ParameterDeclaration(node)) => node.r#type.is_some(),
                Some(Node::PropertySignatureDeclaration(node)) => node.r#type.is_some(),
                Some(Node::PropertyDeclaration(node)) => node.r#type.is_some(),
                Some(Node::AsExpression(node)) => {
                    node.r#type.is_some_and(|t| !crate::assertions::is_const_type_reference(t))
                }
                Some(Node::TypeAssertion(node)) => {
                    node.r#type.is_some_and(|t| !crate::assertions::is_const_type_reference(t))
                }
                _ => false,
            };
            if contextual {
                return true;
            }
            current = self.nodes.parent(id);
        }
        false
    }
}
