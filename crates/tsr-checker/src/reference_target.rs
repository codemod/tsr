//! `checkReferenceExpression` (`checker.go:13130`) at the callers the
//! assignment/update arm in `check.rs` (`check_reference_expression`) does
//! not reach: destructuring-assignment targets (`checkReferenceAssignment`,
//! `checker.go:12703`) and the expression left-hand side of `for...of`
//! (`checker.go:4064`) and `for...in` (`checker.go:4013`).
//!
//! `checkReferenceExpression` is two syntactic arms over
//! `SkipOuterExpressions(expr, OEKAssertions|OEKParentheses)`: not an
//! identifier or access expression → the caller's "must be a variable"
//! message; an access carrying `NodeFlagsOptionalChain` → the caller's "may
//! not be an optional property access" message. Both report at the unskipped
//! `expr`. No types are read; no cache, side table or traversal beyond the
//! destructuring pattern's own elements. `docs/parity/notes/misc-checks.md` §9.

use tsr_ast::{Node, NodeFlags, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `node.Flags&ast.NodeFlagsOptionalChain != 0` on the reference spine
    /// of `expr` — the second arm of `checkReferenceExpression`.
    pub(crate) fn is_optional_chain_reference(&self, expr: NodeId) -> bool {
        let spine = self.skip_outer_reference_expressions(expr);
        matches!(
            self.nodes.kind(spine),
            SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
        ) && self.nodes.flags(spine).contains(NodeFlags::OPTIONAL_CHAIN)
    }

    /// `SkipOuterExpressions(expr, OEKAssertions|OEKParentheses)`.
    fn skip_outer_reference_expressions(&self, mut node: NodeId) -> NodeId {
        for _ in 0..64 {
            let next = match self.node_map.get(node) {
                Some(Node::ParenthesizedExpression(inner)) => inner.expression,
                Some(Node::AsExpression(inner)) => inner.expression,
                Some(Node::TypeAssertion(inner)) => inner.expression,
                Some(Node::SatisfiesExpression(inner)) => inner.expression,
                Some(Node::NonNullExpression(inner)) => inner.expression,
                _ => return node,
            };
            let Some(next) = next.and_then(|expression| expression.node_id()) else { return node };
            node = next;
        }
        node
    }

    /// `c.error(expr, message)` at the expression's own span.
    pub(crate) fn report_reference_error(&mut self, expr: NodeId, message: &'static Message) {
        let Some(file) = self.source_file_of_for_diagnostics(expr) else { return };
        let span = self.nodes.span(expr);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `checkReferenceExpression(expr, invalid, optional)` (`checker.go:13130`).
    fn check_reference_expression_arms(
        &mut self,
        expr: NodeId,
        invalid: &'static Message,
        optional: &'static Message,
    ) {
        let spine = self.skip_outer_reference_expressions(expr);
        let kind = self.nodes.kind(spine);
        if kind != SyntaxKind::Identifier
            && !matches!(
                kind,
                SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
            )
        {
            self.report_reference_error(expr, invalid);
        } else if self.nodes.flags(spine).contains(NodeFlags::OPTIONAL_CHAIN) {
            self.report_reference_error(expr, optional);
        }
    }

    /// The reference checks of a destructuring assignment `[..] = x` /
    /// `({..} = x)`, reached from the `=` arm of the binary dispatch.
    ///
    /// `checkBinaryLikeExpression` (`checker.go:12338`) sends an `=` whose left
    /// is an object or array literal to `checkDestructuringAssignment`
    /// (`checker.go:12552`), which recurses through the literal and ends at
    /// `checkReferenceAssignment` for every leaf target.
    pub(crate) fn check_destructuring_assignment_targets(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|t| t.kind != SyntaxKind::EqualsToken) {
            return;
        }
        let Some(left) = binary.left.and_then(|left| left.node_id()) else { return };
        self.check_destructuring_pattern_targets(left);
        // The relation half of the same walk (`destructuring_assignment.rs`).
        self.check_destructuring_assignment_relations(node);
    }

    /// `checkForOfStatement`'s expression arm (`checker.go:4050`): a literal
    /// left-hand side is a destructuring assignment; anything else goes to
    /// `checkReferenceExpression` with the `for...of` messages.
    pub(crate) fn check_for_of_reference_target(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForOfStatement {
            return;
        }
        let Some(initializer) = statement.initializer.and_then(|i| i.node_id()) else { return };
        match self.nodes.kind(initializer) {
            SyntaxKind::VariableDeclarationList => {}
            SyntaxKind::ArrayLiteralExpression | SyntaxKind::ObjectLiteralExpression => {
                self.check_destructuring_pattern_targets(initializer);
                self.check_for_of_destructuring_relations(node);
            }
            _ => self.check_reference_expression_arms(
                initializer,
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
            ),
        }
    }

    /// `checkObjectLiteralAssignment` / `checkArrayLiteralAssignment` over one
    /// literal: each element's target goes through
    /// [`Self::check_destructuring_target`].
    fn check_destructuring_pattern_targets(&mut self, literal: NodeId) {
        match self.node_map.get(literal) {
            Some(Node::ObjectLiteralExpression(object)) => {
                let count = object.properties.len();
                for (index, property) in object.properties.iter().enumerate() {
                    let Some(id) = property.node_id() else { continue };
                    match self.node_map.get(id) {
                        // `checkObjectLiteralDestructuringPropertyAssignment`
                        // (`checker.go:12597`): the initializer is the target.
                        Some(Node::PropertyAssignment(assignment)) => {
                            if let Some(target) = assignment.initializer.and_then(|e| e.node_id()) {
                                self.check_destructuring_target(target, false);
                            }
                        }
                        // A shorthand's target is its name, an identifier,
                        // which is always a reference.
                        Some(Node::SpreadAssignment(spread)) if index + 1 == count => {
                            if let Some(target) = spread.expression.and_then(|e| e.node_id()) {
                                self.check_destructuring_target(target, true);
                            }
                        }
                        // A rest that is not last reports TS2462 and returns.
                        _ => {}
                    }
                }
            }
            Some(Node::ArrayLiteralExpression(array)) => {
                let count = array.elements.len();
                for (index, element) in array.elements.iter().enumerate() {
                    let Some(id) = element.node_id() else { continue };
                    match self.node_map.get(id) {
                        Some(Node::OmittedExpression(_)) => {}
                        // `checkArrayLiteralDestructuringElementAssignment`
                        // (`checker.go:12663`): a rest is checked only when
                        // last and without an initializer (TS2462 / TS1186).
                        Some(Node::SpreadElement(spread)) => {
                            if index + 1 != count {
                                continue;
                            }
                            let Some(target) = spread.expression.and_then(|e| e.node_id()) else {
                                continue;
                            };
                            if self.is_simple_assignment(target) {
                                continue;
                            }
                            self.check_destructuring_target(target, false);
                        }
                        _ => self.check_destructuring_target(id, false),
                    }
                }
            }
            _ => {}
        }
    }

    /// `checkDestructuringAssignment` (`checker.go:12552`) for one target.
    ///
    /// A target `x = default` is checked by upstream as a binary expression
    /// first (`checkBinaryExpression(target)`), which runs this same
    /// destructuring or reference check on its left; this port's traversal
    /// visits that nested `=` as its own node, so it is skipped here rather
    /// than reported twice.
    fn check_destructuring_target(&mut self, target: NodeId, object_rest: bool) {
        if self.is_simple_assignment(target) {
            return;
        }
        match self.nodes.kind(target) {
            SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression => {
                self.check_destructuring_pattern_targets(target);
            }
            // `checkReferenceAssignment` (`checker.go:12703`): the object-rest
            // messages when `target.Parent` is a spread assignment.
            _ if object_rest => self.check_reference_expression_arms(
                target,
                &messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                &messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
            ),
            _ => self.check_reference_expression_arms(
                target,
                &messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                &messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
            ),
        }
    }

    fn is_simple_assignment(&self, node: NodeId) -> bool {
        matches!(
            self.node_map.get(node),
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
        )
    }
}
