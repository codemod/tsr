//! `checkThisExpression`'s diagnostics (`checker.go:12077`).
//!
//! One container walk decides every arm, in upstream's order: TS17009 through
//! `checkThisBeforeSuper`, the arrow/computed-name container adjustment, TS2816
//! (`checkThisInStaticClassFieldInitializerInDecoratedClass`), TS2465 /
//! TS2331 / TS2332 from the container switch, then the `noImplicitThis` arms
//! TS7041 and TS2683 from whether `tryGetThisTypeAtEx` answers a type. No cache,
//! side table or traversal beyond the node's ancestor chain; the one semantic
//! query is the contextual signature `check_this_expression` already asks for
//! the same container.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::checker::Checker;

/// What `tryGetThisTypeAtEx` (`checker.go:12146`) answers, reduced to the
/// distinctions `checkThisExpression`'s diagnostics read.
enum ThisTypeAnswer {
    /// `nil`: no annotation, contextual or class `this`.
    Absent,
    /// `getTypeOfSymbol(globalThisSymbol)` for a script's top level.
    GlobalThis,
    /// Any other type.
    Present,
}

impl Checker<'_, '_> {
    /// `checkThisExpression` (`checker.go:12077`) — the diagnostic arms only;
    /// the type it returns is `expressions.rs`'s `check_this_expression`.
    pub(crate) fn check_this_expression_diagnostics(&mut self, node: NodeId) {
        let Some(mut container) = self.this_container(node, true, true) else { return };
        if self.nodes.kind(container) == SyntaxKind::Constructor {
            self.check_this_before_super_in(
                node,
                container,
                &messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
            );
        }
        let mut captured_by_arrow_function = false;
        let mut this_in_computed_property_name = false;
        loop {
            // Skip arrow functions to the "real" owner of `this`.
            if self.nodes.kind(container) == SyntaxKind::ArrowFunction {
                let Some(next) =
                    self.this_container(container, false, !this_in_computed_property_name)
                else {
                    return;
                };
                container = next;
                captured_by_arrow_function = true;
            }
            if self.nodes.kind(container) == SyntaxKind::ComputedPropertyName {
                let Some(next) = self.this_container(container, !captured_by_arrow_function, false)
                else {
                    return;
                };
                container = next;
                this_in_computed_property_name = true;
                continue;
            }
            break;
        }
        self.check_this_in_static_class_field_initializer_in_decorated_class(node, container);
        if this_in_computed_property_name {
            self.report_this_error(
                node,
                &messages::THIS_CANNOT_BE_REFERENCED_IN_A_COMPUTED_PROPERTY_NAME,
            );
        } else {
            match self.nodes.kind(container) {
                SyntaxKind::ModuleDeclaration => self.report_this_error(
                    node,
                    &messages::THIS_CANNOT_BE_REFERENCED_IN_A_MODULE_OR_NAMESPACE_BODY,
                ),
                SyntaxKind::EnumDeclaration => self.report_this_error(
                    node,
                    &messages::THIS_CANNOT_BE_REFERENCED_IN_CURRENT_LOCATION,
                ),
                _ => {}
            }
        }
        if !self.no_implicit_this {
            return;
        }
        match self.try_get_this_type_answer(node, container) {
            ThisTypeAnswer::GlobalThis if captured_by_arrow_function => self.report_this_error(
                node,
                &messages::THE_CONTAINING_ARROW_FUNCTION_CAPTURES_THE_GLOBAL_VALUE_OF_THIS,
            ),
            ThisTypeAnswer::Absent => self.report_this_error(
                node,
                &messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
            ),
            _ => {}
        }
    }

    /// `tryGetThisTypeAtEx(node, includeGlobalThis: true, container)`
    /// (`checker.go:12146`), answering only whether a type exists.
    ///
    /// Under `noImplicitThis` (the only caller's mode) the object-literal and
    /// `obj.x = function` arms of `getContextualThisParameterType`
    /// (`checker.go:12021`) always produce a type, so they are decided by
    /// shape; the contextual-signature arm needs the signature.
    fn try_get_this_type_answer(&mut self, node: NodeId, container: NodeId) -> ThisTypeAnswer {
        if self.is_function_like(container)
            && (!self.is_in_parameter_initializer_before_containing_function(node)
                || self.has_this_parameter(container))
        {
            // getThisTypeOfSignature: a declared `this` parameter, or a JSDoc
            // `@this` in a JS file.
            if self.has_this_parameter(container)
                || (self.in_js_file(container)
                    && self.jsdoc_this_parameter_type(container).is_some())
            {
                return ThisTypeAnswer::Present;
            }
            if self.contextual_this_type_exists(container) {
                return ThisTypeAnswer::Present;
            }
        }
        if self.nodes.parent(container).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        }) {
            return ThisTypeAnswer::Present;
        }
        if let Some(Node::SourceFile(source)) = self.node_map.get(container) {
            // A module's top-level `this` is `undefined`.
            return if tsr_binder::is_external_module(source) {
                ThisTypeAnswer::Present
            } else {
                ThisTypeAnswer::GlobalThis
            };
        }
        ThisTypeAnswer::Absent
    }

    /// Whether `getContextualThisParameterType` (`checker.go:12021`) answers a
    /// type for `function`.
    fn contextual_this_type_exists(&mut self, function: NodeId) -> bool {
        let kind = self.nodes.kind(function);
        if kind == SyntaxKind::ArrowFunction {
            return false;
        }
        // The contextual signature's `this` parameter.
        if self.contextual_this_parameter_type(function).is_some() {
            return true;
        }
        if !(self.no_implicit_this || self.in_js_file(function)) {
            return false;
        }
        let Some(parent) = self.nodes.parent(function) else { return false };
        // getContainingObjectLiteral: the literal's contextual type or its own
        // type.
        let containing_literal = match kind {
            SyntaxKind::MethodDeclaration | SyntaxKind::GetAccessor | SyntaxKind::SetAccessor => {
                self.nodes.kind(parent) == SyntaxKind::ObjectLiteralExpression
            }
            SyntaxKind::FunctionExpression => {
                self.nodes.kind(parent) == SyntaxKind::PropertyAssignment
            }
            _ => false,
        };
        if containing_literal {
            return true;
        }
        // `obj.xxx = function (...)` or `obj[xxx] = function (...)`.
        let mut parent = parent;
        while self.nodes.kind(parent) == SyntaxKind::ParenthesizedExpression {
            let Some(next) = self.nodes.parent(parent) else { return false };
            parent = next;
        }
        let Some(Node::BinaryExpression(assignment)) = self.node_map.get(parent) else {
            return false;
        };
        if assignment.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken) {
            return false;
        }
        let receiver = match assignment.left {
            Some(Expression::PropertyAccessExpression(access)) => access.expression,
            Some(Expression::ElementAccessExpression(access)) => access.expression,
            _ => return false,
        };
        let Some(receiver) = receiver else { return false };
        // `exports.Point = function (x, y) { this.x = x }` is not contextually
        // typed as `exports` in a CommonJS JS file.
        if self.in_js_file(function)
            && let Expression::Identifier(identifier) = receiver
            && let Some(id) = identifier.node_id
            && let Some(symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                id,
                identifier.text,
                tsr_binder::SymbolFlags::VALUE,
            )
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .contains(tsr_binder::SymbolFlags::MODULE_EXPORTS)
        {
            return false;
        }
        true
    }

    /// `checkThisInStaticClassFieldInitializerInDecoratedClass`
    /// (`checker.go:12254`).
    fn check_this_in_static_class_field_initializer_in_decorated_class(
        &mut self,
        node: NodeId,
        container: NodeId,
    ) {
        if !self.legacy_decorators {
            return;
        }
        let Some(Node::PropertyDeclaration(property)) = self.node_map.get(container) else {
            return;
        };
        let is_static = property.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::StaticKeyword)
        });
        if !is_static {
            return;
        }
        let Some(initializer) = property.initializer.and_then(|e| e.node_id()) else { return };
        let span = self.nodes.span(initializer);
        let at = self.nodes.span(node).start;
        if at < span.start || at > span.end {
            return;
        }
        let Some(class) = self.nodes.parent(container) else { return };
        let decorated = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(class)) => class.modifiers,
            Some(Node::ClassExpression(class)) => class.modifiers,
            _ => return,
        }
        .iter()
        .any(|modifier| matches!(modifier, tsr_ast::ModifierLike::Decorator(_)));
        if decorated {
            self.report_this_error(
                node,
                &messages::CANNOT_USE_THIS_IN_A_STATIC_PROPERTY_INITIALIZER_OF_A_DECORATED_CLASS,
            );
        }
    }

    /// `ast.GetThisContainer` (`ast/utilities.go:1790`).
    pub(crate) fn this_container(
        &self,
        node: NodeId,
        include_arrow_functions: bool,
        include_class_computed_property_name: bool,
    ) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            let mut next = id;
            match self.nodes.kind(id) {
                SyntaxKind::ComputedPropertyName => {
                    let owner = self.nodes.parent(id).and_then(|member| self.nodes.parent(member));
                    if include_class_computed_property_name
                        && owner.is_some_and(|owner| {
                            matches!(
                                self.nodes.kind(owner),
                                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                            )
                        })
                    {
                        return Some(id);
                    }
                    next = owner?;
                }
                SyntaxKind::Decorator => {
                    let parent = self.nodes.parent(id)?;
                    if self.nodes.kind(parent) == SyntaxKind::Parameter
                        && self.nodes.parent(parent).is_some_and(|it| self.is_class_element(it))
                    {
                        next = self.nodes.parent(parent)?;
                    } else if self.is_class_element(parent) {
                        next = parent;
                    }
                }
                SyntaxKind::ArrowFunction if include_arrow_functions => return Some(id),
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::ClassStaticBlockDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::EnumDeclaration
                | SyntaxKind::SourceFile => return Some(id),
                _ => {}
            }
            current = self.nodes.parent(next);
        }
        None
    }

    /// `ast.IsFunctionLike`: every signature-bearing kind.
    fn is_function_like(&self, node: NodeId) -> bool {
        self.is_function_like_or_static_block(node)
            && self.nodes.kind(node) != SyntaxKind::ClassStaticBlockDeclaration
    }

    /// `ast.GetThisParameter(container) != nil`: a first parameter named
    /// `this`.
    fn has_this_parameter(&self, container: NodeId) -> bool {
        let parameters = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(node)) => node.parameters,
            Some(Node::FunctionExpression(node)) => node.parameters,
            Some(Node::MethodDeclaration(node)) => node.parameters,
            Some(Node::MethodSignatureDeclaration(node)) => node.parameters,
            Some(Node::CallSignatureDeclaration(node)) => node.parameters,
            Some(Node::ConstructSignatureDeclaration(node)) => node.parameters,
            Some(Node::IndexSignatureDeclaration(node)) => node.parameters,
            Some(Node::GetAccessorDeclaration(node)) => node.parameters,
            Some(Node::SetAccessorDeclaration(node)) => node.parameters,
            Some(Node::ConstructorDeclaration(node)) => node.parameters,
            Some(Node::FunctionTypeNode(node)) => node.parameters,
            Some(Node::ConstructorTypeNode(node)) => node.parameters,
            _ => return false,
        };
        parameters.first().is_some_and(|first| {
            matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        })
    }

    /// `c.error(node, message)` at the keyword.
    fn report_this_error(&mut self, node: NodeId, message: &'static Message) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(message, span));
    }
}
