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
use tsr_binder::{SymbolFlags, SymbolId};
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
    /// ([`Checker::is_post_super_flow_node`]): some flow path reaches the use
    /// without passing a `super(...)` call node. The binder records a flow
    /// node for every `this` and `super` keyword and a `FlowFlags::CALL` node
    /// after every `super(...)` call, so the walk runs on the graph itself; a
    /// keyword with no flow node is in unreachable code, which upstream
    /// treats as post-super. This replaced §307's statement-index test and
    /// §17's structural walk (`docs/parity/notes/misc-checks.md` §21).
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
        let Some(flow) = self.binder.flow_of(node) else { return };
        if self.is_post_super_flow_node(flow) {
            return;
        }
        self.report_super_error(node, message);
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
    pub(crate) fn is_in_constructor_argument_initializer(
        &self,
        node: NodeId,
        constructor: NodeId,
    ) -> bool {
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

impl Checker<'_, '_> {
    /// `checkPropertyAccessibilityAtLocation`'s `isSuper` arm
    /// (`checker.go:11788`): a `super.x` access may not name an abstract
    /// member (TS2513) or a class field of the base (TS2855). Answers the
    /// message and its arguments, or `None` when the arm passes and the
    /// accessibility checks that follow decide.
    ///
    /// The flags are `getDeclarationModifierFlagsFromSymbolEx(prop, writing)`
    /// (`utilities.go:717`): the setter when writing, else the getter, else
    /// the value declaration; a symbol with no value declaration carries no
    /// `abstract`/`static` flag, so it passes.
    pub(crate) fn super_property_accessibility_error(
        &mut self,
        property: SymbolId,
        writing: bool,
        name: &str,
    ) -> Option<(&'static Message, Vec<String>)> {
        let entry = self.binder.symbols().get(property);
        let value_declaration = entry.value_declaration?;
        let accessor = |kind: SyntaxKind| {
            entry
                .declarations
                .iter()
                .copied()
                .find(|&declaration| self.nodes.kind(declaration) == kind)
        };
        let declaration = writing
            .then(|| accessor(SyntaxKind::SetAccessor))
            .flatten()
            .or_else(|| {
                entry
                    .flags
                    .intersects(SymbolFlags::GET_ACCESSOR)
                    .then(|| accessor(SyntaxKind::GetAccessor))
                    .flatten()
            })
            .unwrap_or(value_declaration);
        let declarations = entry.declarations.clone();
        let parent = entry.parent;
        if self.has_effective_modifier(declaration, SyntaxKind::AbstractKeyword) {
            // `getDeclaringClass(prop)`: the declared type of the parent class.
            let class = parent
                .filter(|&parent| {
                    self.binder.symbols().get(parent).flags.intersects(SymbolFlags::CLASS)
                })
                .map(|parent| self.get_declared_type_of_symbol(parent));
            let class = class.map(|class| self.type_to_string(class)).unwrap_or_default();
            return Some((
                &messages::ABSTRACT_METHOD_0_IN_CLASS_1_CANNOT_BE_ACCESSED_VIA_SUPER_EXPRESSION,
                vec![name.to_string(), class],
            ));
        }
        if !self.has_effective_modifier(declaration, SyntaxKind::StaticKeyword)
            && declarations.iter().any(|&declaration| self.is_class_instance_property(declaration))
        {
            return Some((
                &messages::CLASS_FIELD_0_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA_SUPER,
                vec![name.to_string()],
            ));
        }
        None
    }

    /// `isClassInstanceProperty` (`checker/utilities.go:1017`): a property
    /// declaration of a class without `accessor`, or in JS an expando
    /// assignment that is neither a prototype assignment nor a static one
    /// (`this.x = …` is an instance field).
    fn is_class_instance_property(&self, node: NodeId) -> bool {
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(node)
            && self.in_js_file(node)
        {
            let Some(left) = binary.left.and_then(|left| left.node_id()) else { return false };
            let receiver = self.access_receiver(left);
            return (!self.is_bindable_static_access_expression(left, false)
                || !receiver.is_some_and(|receiver| self.is_prototype_access(receiver)))
                && !self.is_bindable_static_name_expression(left, true);
        }
        self.nodes.kind(node) == SyntaxKind::PropertyDeclaration
            && self.nodes.parent(node).is_some_and(|parent| {
                matches!(
                    self.nodes.kind(parent),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            })
            && !self.has_effective_modifier(node, SyntaxKind::AccessorKeyword)
    }

    /// The `Expression()` of a property or element access.
    fn access_receiver(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::PropertyAccessExpression(access) => access.expression?.node_id(),
            Node::ElementAccessExpression(access) => access.expression?.node_id(),
            _ => None,
        }
    }

    /// `ast.IsBindableStaticAccessExpression` (`ast/utilities.go:1371`).
    fn is_bindable_static_access_expression(&self, node: NodeId, exclude_this: bool) -> bool {
        match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => {
                let Some(receiver) = access.expression.and_then(|e| e.node_id()) else {
                    return false;
                };
                (!exclude_this && self.nodes.kind(receiver) == SyntaxKind::ThisKeyword)
                    || (matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                        && self.is_bindable_static_name_expression(receiver, true))
            }
            Some(Node::ElementAccessExpression(_)) => {
                self.is_bindable_static_element_access_expression(node, exclude_this)
            }
            _ => false,
        }
    }

    /// `ast.IsBindableStaticElementAccessExpression` (`ast/utilities.go:1377`).
    fn is_bindable_static_element_access_expression(
        &self,
        node: NodeId,
        exclude_this: bool,
    ) -> bool {
        let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node) else {
            return false;
        };
        let literal = access
            .argument_expression
            .and_then(|argument| argument.node_id())
            .is_some_and(|argument| self.is_string_or_numeric_literal_like(argument));
        let Some(receiver) = access.expression.and_then(|e| e.node_id()) else { return false };
        literal
            && ((!exclude_this && self.nodes.kind(receiver) == SyntaxKind::ThisKeyword)
                || self.is_entity_name_expression(receiver)
                || self.is_bindable_static_access_expression(receiver, true))
    }

    /// `ast.IsBindableStaticNameExpression` (`ast/utilities.go:1397`).
    fn is_bindable_static_name_expression(&self, node: NodeId, exclude_this: bool) -> bool {
        self.is_entity_name_expression(node)
            || self.is_bindable_static_access_expression(node, exclude_this)
    }

    /// `ast.IsPrototypeAccess` (`ast/utilities.go:1384`), with
    /// `GetElementOrPropertyAccessName`'s parenthesis skip.
    fn is_prototype_access(&self, node: NodeId) -> bool {
        if !self.is_bindable_static_access_expression(node, false) {
            return false;
        }
        match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => {
                matches!(access.name, Some(tsr_ast::MemberName::Identifier(name)) if name.text == "prototype")
            }
            Some(Node::ElementAccessExpression(access)) => {
                let mut argument = access.argument_expression.and_then(|a| a.node_id());
                while let Some(Node::ParenthesizedExpression(inner)) =
                    argument.and_then(|a| self.node_map.get(a))
                {
                    argument = inner.expression.and_then(|e| e.node_id());
                }
                match argument.and_then(|a| self.node_map.get(a)) {
                    Some(Node::StringLiteral(literal)) => literal.text == "prototype",
                    Some(Node::NoSubstitutionTemplateLiteral(literal)) => {
                        literal.text == "prototype"
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// `ast.IsStringOrNumericLiteralLike`.
    fn is_string_or_numeric_literal_like(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::StringLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::NumericLiteral
        )
    }
}
