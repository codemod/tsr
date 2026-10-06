//! `checkSuperExpression`'s diagnostics (`checker.go:7854`) and the shared
//! `checkThisBeforeSuper` (`checker.go:12263`).
//!
//! The legality decision is `isLegalUsageOfSuperExpression` over the container
//! `getSuperContainer` returns, adjusted through arrow functions for property
//! access; an illegal use reports exactly one of TS2466, TS2337, TS2660 or
//! TS2338, in upstream's switch order. A legal use continues to the
//! constructor flow check (TS17011), the derived-class test (TS2335) and the
//! constructor-argument test (TS2336). No cache, side table or traversal beyond
//! the node's own ancestor chain.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `checkSuperExpression` (`checker.go:7854`) — the diagnostic arms only;
    /// the type it returns is `expressions.rs`'s business.
    pub(crate) fn check_super_expression_diagnostics(&mut self, node: NodeId) {
        let is_call_expression = self.nodes.parent(node).is_some_and(|parent| {
            matches!(self.node_map.get(parent), Some(Node::CallExpression(call))
                if call.expression.and_then(|e| e.node_id()) == Some(node))
        });
        let immediate_container = self.get_super_container(node, true);
        let mut container = immediate_container;
        // Adjust through arrow functions of arbitrary nesting, for property
        // access only.
        if !is_call_expression {
            while let Some(arrow) =
                container.filter(|&it| self.nodes.kind(it) == SyntaxKind::ArrowFunction)
            {
                container = self.get_super_container(arrow, true);
            }
        }
        let legal = container.is_some_and(|container| {
            self.is_legal_usage_of_super_expression(container, is_call_expression)
        });
        if !legal {
            // `FindAncestorOrQuit`: a computed property name strictly below
            // the container.
            let in_computed_name = self
                .nodes
                .ancestors(node)
                .take_while(|&it| Some(it) != container)
                .any(|it| self.nodes.kind(it) == SyntaxKind::ComputedPropertyName);
            let parent_is_member_host = container
                .and_then(|it| self.nodes.parent(it))
                .is_some_and(|parent| self.is_class_like_or_object_literal(parent));
            let message = if in_computed_name {
                &messages::SUPER_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME
            } else if is_call_expression {
                &messages::SUPER_CALLS_ARE_NOT_PERMITTED_OUTSIDE_CONSTRUCTORS_OR_IN_NESTED_FUNCTIONS_INSIDE_CONSTRUCTORS
            } else if !parent_is_member_host {
                &messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_MEMBERS_OF_DERIVED_CLASSES_OR_OBJECT_LITERAL_EXPRESSIONS
            } else {
                &messages::SUPER_PROPERTY_ACCESS_IS_PERMITTED_ONLY_IN_A_CONSTRUCTOR_MEMBER_FUNCTION_OR_MEMBER_ACCESSOR_OF_A_DERIVED_CLASS
            };
            self.report_super_error(node, message);
            return;
        }
        let Some(container) = container else { return };
        if !is_call_expression
            && let Some(immediate) = immediate_container
            && self.nodes.kind(immediate) == SyntaxKind::Constructor
        {
            self.check_this_before_super_in(
                node,
                immediate,
                &messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_A_PROPERTY_OF_SUPER_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
            );
        }
        let Some(class) = self.nodes.parent(container) else { return };
        // An object literal's `super` is `any`.
        if self.nodes.kind(class) == SyntaxKind::ObjectLiteralExpression {
            return;
        }
        if self.extends_clause_base(class).is_none() {
            self.report_super_error(
                node,
                &messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
            );
            return;
        }
        if self.class_declaration_extends_null(class) {
            return;
        }
        // `getBaseTypes(classType)[0] == nil` returns the error type silently.
        let Some(symbol) = self.binder.symbol_of(class) else { return };
        if self.first_base_type_of_class_symbol(symbol).is_none() {
            return;
        }
        if self.nodes.kind(container) == SyntaxKind::Constructor
            && self.is_in_constructor_argument_initializer(node, container)
        {
            self.report_super_error(
                node,
                &messages::SUPER_CANNOT_BE_REFERENCED_IN_CONSTRUCTOR_ARGUMENTS,
            );
        }
    }

    /// `checkThisBeforeSuper` (`checker.go:12263`) for a `this` or `super`
    /// whose container is `constructor`.
    ///
    /// Upstream's test is `!isPostSuperFlowNode(node.FlowNode)`
    /// (`flow.go:2611`): some flow path reaches the use without passing a
    /// `super(...)` call. This port answers it without the flow graph where
    /// the answer is certain in the reporting direction: a **parameter
    /// initializer** (evaluated before the body), and otherwise
    /// [`Checker::certainly_reached_before_super`]. §307; the structured walk
    /// replaced its statement-index test, which reported a use sharing a
    /// top-level statement with a `super()` before it
    /// (`docs/parity/notes/misc-checks.md` §17).
    pub(crate) fn check_this_before_super_in(
        &mut self,
        node: NodeId,
        constructor: NodeId,
        message: &'static Message,
    ) {
        let Some(class) = self.nodes.parent(constructor) else { return };
        if self.extends_clause_base(class).is_none() || self.class_declaration_extends_null(class) {
            return;
        }
        let Some(Node::ConstructorDeclaration(declaration)) = self.node_map.get(constructor) else {
            return;
        };
        let Some(body) = declaration.body.and_then(|body| body.node_id()) else { return };
        let Some(Node::Block(_)) = self.node_map.get(body) else { return };
        let in_parameter =
            self.nodes.ancestors(node).take_while(|&it| it != constructor).any(|it| {
                self.nodes.kind(it) == SyntaxKind::Parameter
                    && self.nodes.parent(it) == Some(constructor)
            });
        if !in_parameter && !self.certainly_reached_before_super(node, body) {
            return;
        }
        self.report_super_error(node, message);
    }

    /// Is there certainly a flow path from the constructor body's start to
    /// `node` that completes every statement before it without a `super()`
    /// call? Walks the ancestor chain top-down. At a statement list (block,
    /// case clause) every earlier sibling must have such a path
    /// ([`Checker::super_free_completion`]); an `if` is entered through a
    /// branch once its condition holds no `super()`; a `switch` clause is
    /// entered by the dispatch jump once the discriminant and the case labels
    /// hold none. Any other container is a leaf, certain when each of its
    /// `super()` calls is evaluated after the use
    /// ([`Checker::super_calls_follow`]).
    /// `false` means "not certain", never "post-super".
    fn certainly_reached_before_super(&self, node: NodeId, body: NodeId) -> bool {
        let mut chain: Vec<NodeId> =
            self.nodes.ancestors(node).take_while(|&it| it != body).collect();
        chain.reverse();
        if chain.iter().any(|&it| self.is_function_like_declaration(it)) {
            return false;
        }
        let mut container = body;
        for &child in chain.iter().chain(std::iter::once(&node)) {
            match self.node_map.get(container) {
                Some(Node::Block(_) | Node::CaseOrDefaultClause(_)) => {
                    let mut earlier = Vec::new();
                    if let Some(typed) = self.node_map.get(container) {
                        tsr_ast::for_each_child_id(typed, |it| earlier.push(it));
                    }
                    for sibling in earlier.into_iter().take_while(|&it| it != child) {
                        if !self.super_free_completion(sibling) {
                            return false;
                        }
                    }
                }
                Some(Node::IfStatement(statement))
                    if statement.expression.and_then(|e| e.node_id()) != Some(child) =>
                {
                    if statement
                        .expression
                        .and_then(|e| e.node_id())
                        .is_some_and(|condition| self.subtree_calls_super(condition))
                    {
                        return false;
                    }
                }
                Some(Node::SwitchStatement(statement))
                    if statement.expression.and_then(|e| e.node_id()) != Some(child) =>
                {
                    if statement
                        .expression
                        .and_then(|e| e.node_id())
                        .is_some_and(|discriminant| self.subtree_calls_super(discriminant))
                    {
                        return false;
                    }
                }
                Some(Node::CaseBlock(block)) => {
                    if block.clauses.iter().any(|clause| {
                        clause
                            .expression
                            .and_then(|e| e.node_id())
                            .is_some_and(|label| self.subtree_calls_super(label))
                    }) {
                        return false;
                    }
                }
                _ => return self.super_calls_follow(container, node),
            }
            container = child;
        }
        true
    }

    /// Every `super()` call in `container` (outside nested functions) is
    /// evaluated after `node`: it encloses `node` (arguments are evaluated
    /// before the call's flow node) or starts after `node` ends (expressions
    /// evaluate left to right; a loop is entered through its entry edge,
    /// which `isPostSuperFlowNode` follows alone at a loop label).
    fn super_calls_follow(&self, container: NodeId, node: NodeId) -> bool {
        let end = self.nodes.span(node).end;
        let mut stack = vec![container];
        while let Some(at) = stack.pop() {
            if matches!(self.node_map.get(at), Some(Node::CallExpression(call))
                if call.expression.and_then(|e| e.node_id())
                    .is_some_and(|callee| self.nodes.kind(callee) == SyntaxKind::SuperKeyword))
                && !self.nodes.ancestors(node).any(|it| it == at)
                && self.nodes.span(at).start < end
            {
                return false;
            }
            if let Some(typed) = self.node_map.get(at) {
                tsr_ast::for_each_child_id(typed, |child| {
                    if !self.is_function_like_or_static_block(child) {
                        stack.push(child);
                    }
                });
            }
        }
        true
    }

    /// Does `node` certainly have a path that completes normally without a
    /// `super()` call? Holds for a subtree with no `super()` call and no
    /// `return`/`throw`/`break`/`continue` (outside nested functions), an
    /// `if` whose condition holds no `super()` and one of whose branches
    /// (a missing `else` counts) has such a path, and a block whose
    /// statements all do. Anything else is uncertain.
    fn super_free_completion(&self, node: NodeId) -> bool {
        if !self.subtree_calls_super(node) {
            return !self.subtree_has_jump(node);
        }
        match self.node_map.get(node) {
            Some(Node::IfStatement(statement)) => {
                let condition = statement.expression.and_then(|e| e.node_id());
                if condition.is_some_and(|condition| self.subtree_calls_super(condition)) {
                    return false;
                }
                let branch = |branch: Option<tsr_ast::Statement<'_>>| {
                    branch.and_then(|b| b.node_id()).is_none_or(|b| self.super_free_completion(b))
                };
                branch(statement.else_statement)
                    || statement
                        .then_statement
                        .and_then(|b| b.node_id())
                        .is_some_and(|b| self.super_free_completion(b))
            }
            Some(Node::Block(_)) => {
                let mut statements = Vec::new();
                if let Some(typed) = self.node_map.get(node) {
                    tsr_ast::for_each_child_id(typed, |it| statements.push(it));
                }
                statements.into_iter().all(|it| self.super_free_completion(it))
            }
            _ => false,
        }
    }

    /// A `return`, `throw`, `break` or `continue` in the subtree, not
    /// counting nested functions.
    fn subtree_has_jump(&self, node: NodeId) -> bool {
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::ReturnStatement
                | SyntaxKind::ThrowStatement
                | SyntaxKind::BreakStatement
                | SyntaxKind::ContinueStatement
        ) {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| {
            !self.is_function_like_or_static_block(child) && self.subtree_has_jump(child)
        })
    }

    /// `ast.GetSuperContainer` (`ast/utilities.go:1825`).
    fn get_super_container(&self, node: NodeId, stop_on_functions: bool) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            let mut next = id;
            match self.nodes.kind(id) {
                SyntaxKind::ComputedPropertyName => next = self.nodes.parent(id)?,
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction => {
                    if stop_on_functions {
                        return Some(id);
                    }
                }
                SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::ClassStaticBlockDeclaration => return Some(id),
                SyntaxKind::Decorator => {
                    // Decorators apply outside the body of a class or method.
                    let parent = self.nodes.parent(id)?;
                    if self.nodes.kind(parent) == SyntaxKind::Parameter
                        && self.nodes.parent(parent).is_some_and(|it| self.is_class_element(it))
                    {
                        next = self.nodes.parent(parent)?;
                    } else if self.is_class_element(parent) {
                        next = parent;
                    }
                }
                _ => {}
            }
            current = self.nodes.parent(next);
        }
        None
    }

    /// `isLegalUsageOfSuperExpression`, the closure in `checkSuperExpression`
    /// (`checker.go:7865`).
    fn is_legal_usage_of_super_expression(&self, container: NodeId, is_call: bool) -> bool {
        let kind = self.nodes.kind(container);
        if is_call {
            return kind == SyntaxKind::Constructor;
        }
        let Some(parent) = self.nodes.parent(container) else { return false };
        if !self.is_class_like_or_object_literal(parent) {
            return false;
        }
        if self.is_static_member(container) {
            return matches!(
                kind,
                SyntaxKind::MethodDeclaration
                    | SyntaxKind::MethodSignature
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::PropertyDeclaration
                    | SyntaxKind::ClassStaticBlockDeclaration
            );
        }
        matches!(
            kind,
            SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::Constructor
        )
    }

    /// `isInConstructorArgumentInitializer` (`checker.go:7964`).
    fn is_in_constructor_argument_initializer(&self, node: NodeId, constructor: NodeId) -> bool {
        for ancestor in self.nodes.ancestors(node) {
            if self.is_function_like_declaration(ancestor) {
                return false;
            }
            if self.nodes.kind(ancestor) == SyntaxKind::Parameter
                && self.nodes.parent(ancestor) == Some(constructor)
            {
                return true;
            }
        }
        false
    }

    /// `ast.IsFunctionLikeDeclaration`: the function-like kinds that can carry
    /// a body.
    fn is_function_like_declaration(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
        )
    }

    /// `ast.IsClassLike(n) || ast.IsObjectLiteralExpression(n)`.
    fn is_class_like_or_object_literal(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::ClassDeclaration
                | SyntaxKind::ClassExpression
                | SyntaxKind::ObjectLiteralExpression
        )
    }

    /// `ast.IsStatic`: a `static` modifier, or a class static block.
    fn is_static_member(&self, member: NodeId) -> bool {
        let modifiers = match self.node_map.get(member) {
            Some(Node::ClassStaticBlockDeclaration(_)) => return true,
            Some(Node::MethodDeclaration(node)) => node.modifiers,
            Some(Node::PropertyDeclaration(node)) => node.modifiers,
            Some(Node::GetAccessorDeclaration(node)) => node.modifiers,
            Some(Node::SetAccessorDeclaration(node)) => node.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::StaticKeyword)
        })
    }

    /// `ast.GetExtendsHeritageClauseElement` for a class declaration or
    /// expression.
    fn extends_clause_base(&self, class: NodeId) -> Option<NodeId> {
        let clauses = match self.node_map.get(class)? {
            Node::ClassDeclaration(node) => node.heritage_clauses,
            Node::ClassExpression(node) => node.heritage_clauses,
            _ => return None,
        };
        clauses
            .iter()
            .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .and_then(|clause| clause.types.first())
            .and_then(|base| base.node_id)
    }

    /// `classDeclarationExtendsNull` (`checker.go:12280`), whose real test is
    /// *the base constructor type is `nullWideningType`*. Decided here by the
    /// written `extends null`; this parser makes that `null` an `Identifier`
    /// named `null`, which, being a reserved word, can only be the literal.
    /// §308.
    pub(crate) fn class_declaration_extends_null(&self, class: NodeId) -> bool {
        let clauses = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
            Some(Node::ClassExpression(node)) => node.heritage_clauses,
            _ => return false,
        };
        clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()
            .and_then(|base| base.expression)
            .and_then(|expression| expression.node_id())
            .is_some_and(|id| {
                self.nodes.kind(id) == SyntaxKind::NullKeyword
                    || self.identifier_text(id) == Some("null")
            })
    }

    /// `c.error(node, message)` at the keyword.
    fn report_super_error(&mut self, node: NodeId, message: &'static Message) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(message, span));
    }
}
