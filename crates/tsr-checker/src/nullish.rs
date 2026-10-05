//! `checkNullishCoalesceOperands` (`checker.go:12915`): TS5076 for `??` mixed
//! with `||`/`&&` without parentheses, then TS2871 / TS2869 from
//! `getSyntacticNullishnessSemantics` (`checker.go:12948`).
//!
//! Upstream calls it from `checkBinaryLikeExpression`'s `??` arm
//! (`checker.go:12520`) once per `??` expression; this port reaches it from the
//! check traversal's binary-expression dispatch. Like
//! `getSyntacticTruthySemantics` (`truthiness.rs`) the predicate reads node
//! kinds only, plus whether an identifier resolves to the global `undefined`.
//! No cache, side table or traversal beyond the operands' own spines.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::checker::Checker;

/// `PredicateSemantics` (`checker.go:12877`) as upstream's bit set.
const ALWAYS: u8 = 1;
const NEVER: u8 = 2;
const SOMETIMES: u8 = ALWAYS | NEVER;

impl Checker<'_, '_> {
    /// `checkNullishCoalesceOperands` (`checker.go:12915`) for one `??`
    /// binary expression.
    pub(crate) fn check_nullish_coalesce_operands(&mut self, node: NodeId) {
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|t| t.kind != SyntaxKind::QuestionQuestionToken) {
            return;
        }
        let (Some(left), Some(right)) =
            (binary.left.and_then(|e| e.node_id()), binary.right.and_then(|e| e.node_id()))
        else {
            return;
        };
        // `left.Parent.Parent` is this expression's parent.
        if let Some(Node::BinaryExpression(grandparent)) =
            self.nodes.parent(node).and_then(|parent| self.node_map.get(parent))
        {
            if let Some(grandparent_left) = grandparent.left.and_then(|e| e.node_id())
                && self.nodes.kind(grandparent_left) == SyntaxKind::BinaryExpression
                && let Some(token) = grandparent.operator_token
                && token.kind == SyntaxKind::BarBarToken
            {
                self.report_mixed_without_parentheses(grandparent_left, "??", "||");
            }
        } else if let Some(Node::BinaryExpression(inner)) = self.node_map.get(left) {
            if let Some(token) = inner.operator_token {
                match token.kind {
                    SyntaxKind::BarBarToken => {
                        self.report_mixed_without_parentheses(left, "||", "??");
                    }
                    SyntaxKind::AmpersandAmpersandToken => {
                        self.report_mixed_without_parentheses(left, "&&", "??");
                    }
                    _ => {}
                }
            }
        } else if let Some(Node::BinaryExpression(inner)) = self.node_map.get(right)
            && inner.operator_token.is_some_and(|t| t.kind == SyntaxKind::AmpersandAmpersandToken)
        {
            self.report_mixed_without_parentheses(right, "??", "&&");
        }
        self.check_nullish_coalesce_operand_left(left);
    }

    /// TS5076 through `grammarErrorOnNode` (`grammarchecks.go:38`): silent in
    /// a file with parse diagnostics.
    fn report_mixed_without_parentheses(&mut self, at: NodeId, first: &str, second: &str) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_AND_1_OPERATIONS_CANNOT_BE_MIXED_WITHOUT_PARENTHESES,
                span,
                [first.to_string(), second.to_string()],
            ),
        );
    }

    /// `checkNullishCoalesceOperandLeft` (`checker.go:12936`).
    fn check_nullish_coalesce_operand_left(&mut self, left: NodeId) {
        let target = self.skip_all_outer_expressions(left);
        let semantics = self.syntactic_nullishness_semantics(target, 0);
        if semantics == SOMETIMES {
            return;
        }
        let message: &'static Message = if semantics == ALWAYS {
            &messages::THIS_EXPRESSION_IS_ALWAYS_NULLISH
        } else {
            &messages::RIGHT_OPERAND_OF_IS_UNREACHABLE_BECAUSE_THE_LEFT_OPERAND_IS_NEVER_NULLISH
        };
        let Some(file) = self.source_file_of_for_diagnostics(target) else { return };
        let span = self.error_span(target);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `ast.SkipOuterExpressions(node, ast.OEKAll)`: `truthiness.rs`'s
    /// [`Checker::skip_outer_expressions`] plus the `ExpressionWithTypeArguments`
    /// of an instantiation expression (`OEKExpressionsWithTypeArguments`), so
    /// `g<string> ?? x` reads `g`.
    fn skip_all_outer_expressions(&self, node: NodeId) -> NodeId {
        let mut current = node;
        for _ in 0..64 {
            let skipped = self.skip_outer_expressions(current);
            let Some(Node::ExpressionWithTypeArguments(instantiation)) = self.node_map.get(skipped)
            else {
                return skipped;
            };
            let Some(inner) = instantiation.expression.and_then(|e| e.node_id()) else {
                return skipped;
            };
            current = inner;
        }
        current
    }

    /// `getSyntacticNullishnessSemantics` (`checker.go:12948`).
    fn syntactic_nullishness_semantics(&self, node: NodeId, depth: u32) -> u8 {
        if depth > 64 {
            return SOMETIMES;
        }
        let node = self.skip_all_outer_expressions(node);
        match self.nodes.kind(node) {
            SyntaxKind::AwaitExpression
            | SyntaxKind::CallExpression
            | SyntaxKind::TaggedTemplateExpression
            | SyntaxKind::ElementAccessExpression
            | SyntaxKind::MetaProperty
            | SyntaxKind::NewExpression
            | SyntaxKind::PropertyAccessExpression
            | SyntaxKind::YieldExpression
            | SyntaxKind::ThisKeyword => SOMETIMES,
            SyntaxKind::BinaryExpression => {
                let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else {
                    return SOMETIMES;
                };
                let Some(token) = binary.operator_token else { return SOMETIMES };
                let operand =
                    |side: Option<tsr_ast::Expression<'_>>| side.and_then(|e| e.node_id());
                match token.kind {
                    SyntaxKind::BarBarToken
                    | SyntaxKind::BarBarEqualsToken
                    | SyntaxKind::AmpersandAmpersandToken
                    | SyntaxKind::AmpersandAmpersandEqualsToken => SOMETIMES,
                    SyntaxKind::CommaToken | SyntaxKind::EqualsToken => operand(binary.right)
                        .map_or(SOMETIMES, |right| {
                            self.syntactic_nullishness_semantics(right, depth + 1)
                        }),
                    SyntaxKind::QuestionQuestionToken | SyntaxKind::QuestionQuestionEqualsToken => {
                        let (Some(left), Some(right)) =
                            (operand(binary.left), operand(binary.right))
                        else {
                            return SOMETIMES;
                        };
                        let left_semantics = self.syntactic_nullishness_semantics(left, depth + 1);
                        let mut result = left_semantics & NEVER;
                        if left_semantics & ALWAYS != 0 {
                            result |= self.syntactic_nullishness_semantics(right, depth + 1);
                        }
                        result
                    }
                    _ => NEVER,
                }
            }
            SyntaxKind::ConditionalExpression => {
                let Some(Node::ConditionalExpression(conditional)) = self.node_map.get(node) else {
                    return SOMETIMES;
                };
                let branch =
                    |branch: Option<tsr_ast::Expression<'_>>| branch.and_then(|e| e.node_id());
                let (Some(when_true), Some(when_false)) =
                    (branch(conditional.when_true), branch(conditional.when_false))
                else {
                    return SOMETIMES;
                };
                self.syntactic_nullishness_semantics(when_true, depth + 1)
                    | self.syntactic_nullishness_semantics(when_false, depth + 1)
            }
            SyntaxKind::NullKeyword => ALWAYS,
            // `getResolvedSymbol(node) == c.undefinedSymbol`: the global
            // `undefined` is not in this binder's tables, so an unshadowed
            // `undefined` is the one that resolves to nothing — the same
            // reading as `truthiness.rs`'s identifier arm.
            SyntaxKind::Identifier => {
                let Some(Node::Identifier(identifier)) = self.node_map.get(node) else {
                    return SOMETIMES;
                };
                if identifier.text == "undefined"
                    && self
                        .binder
                        .resolve_name(
                            self.nodes,
                            self.node_map,
                            node,
                            identifier.text,
                            tsr_binder::SymbolFlags::VALUE,
                        )
                        .is_none()
                {
                    ALWAYS
                } else {
                    SOMETIMES
                }
            }
            _ => NEVER,
        }
    }
}
