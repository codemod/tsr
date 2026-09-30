//! Context-sensitivity used by inference and contextual parameter typing.
//!
//! Ported from `Checker.isContextSensitive` (`internal/checker/checker.go`).

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};

use crate::Checker;

impl<'a> Checker<'a, '_> {
    /// Ported from `Checker.isContextSensitive` (`internal/checker/checker.go`).
    pub(crate) fn is_context_sensitive_argument(&self, argument: &Expression<'_>) -> bool {
        argument
            .node_id()
            .and_then(|id| self.node_map.get(id))
            .is_some_and(|node| self.is_context_sensitive_node(node))
    }

    fn is_context_sensitive_node(&self, root: Node<'a>) -> bool {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if is_function_like(node) {
                if node.node_id().is_some_and(|id| self.is_context_sensitive_function_like(id)) {
                    return true;
                }
            } else {
                self.append_context_sensitive_children(node, &mut pending);
            }
        }
        false
    }

    /// Functions whose contextual signatures are consumed while checking an
    /// argument. Uses the same expression boundaries as isContextSensitive.
    pub(crate) fn context_sensitive_functions(&self, argument: Expression<'_>) -> Vec<NodeId> {
        let Some(root) = argument.node_id().and_then(|id| self.node_map.get(id)) else {
            return Vec::new();
        };
        let mut functions = Vec::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if is_function_like(node) {
                if let Some(id) = node.node_id()
                    && self.is_context_sensitive_function_like(id)
                {
                    functions.push(id);
                    pending.extend(
                        self.context_sensitive_function_contents(id).into_iter().map(Node::from),
                    );
                }
            } else {
                self.append_context_sensitive_children(node, &mut pending);
            }
        }
        functions
    }

    /// Child positions inspected by isContextSensitive. In particular, a
    /// conditional's condition and a JSX spread attribute are not inspected.
    fn append_context_sensitive_children(&self, node: Node<'a>, pending: &mut Vec<Node<'a>>) {
        match node {
            Node::ObjectLiteralExpression(node) => {
                pending.extend(node.properties.iter().copied().map(Node::from));
            }
            Node::ArrayLiteralExpression(node) => {
                pending.extend(node.elements.iter().copied().map(Node::from));
            }
            Node::ConditionalExpression(node) => {
                pending.extend(
                    [node.when_true, node.when_false].into_iter().flatten().map(Node::from),
                );
            }
            Node::BinaryExpression(node)
                if node.operator_token.is_some_and(|token| {
                    matches!(
                        token.kind,
                        SyntaxKind::BarBarToken | SyntaxKind::QuestionQuestionToken
                    )
                }) =>
            {
                pending.extend([node.left, node.right].into_iter().flatten().map(Node::from));
            }
            Node::PropertyAssignment(node) => {
                pending.extend(node.initializer.map(Node::from));
            }
            Node::ParenthesizedExpression(node) => {
                pending.extend(node.expression.map(Node::from));
            }
            Node::JsxAttribute(node) => {
                pending.extend(node.initializer.map(Node::from));
            }
            Node::JsxExpression(node) => {
                pending.extend(node.expression.map(Node::from));
            }
            Node::YieldExpression(node) => {
                pending.extend(node.expression.map(Node::from));
            }
            Node::JsxAttributes(node) => {
                pending.extend(node.properties.iter().copied().map(Node::from));
                if let Some(opening) = node.node_id.and_then(|id| self.nodes.parent(id))
                    && self.nodes.kind(opening) == SyntaxKind::JsxOpeningElement
                    && let Some(parent) = self.nodes.parent(opening)
                    && let Some(Node::JsxElement(element)) = self.node_map.get(parent)
                {
                    pending.extend(element.children.iter().copied().map(Node::from));
                }
            }
            _ => {}
        }
    }
}

fn is_function_like(node: Node<'_>) -> bool {
    matches!(
        node,
        Node::FunctionExpression(_)
            | Node::ArrowFunction(_)
            | Node::MethodDeclaration(_)
            | Node::FunctionDeclaration(_)
    )
}
