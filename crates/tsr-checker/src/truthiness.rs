//! TS2872 / TS2873 — `This kind of expression is always truthy.` and
//! `… always falsy.` — plus TS1345, the `void` early return they share.
//!
//! `checkTruthinessOfType` (`checker.go:12865`) and
//! `getSyntacticTruthySemantics` (`checker.go:12886`).
//!
//! # There are no types in this rule
//!
//! Apart from the `void` early return and one identifier case, the predicate
//! reads **node kinds and literal text**. §14's ordering rule — a rule that
//! reports on a syntactic fact has no incompleteness to leak — applies here
//! more completely than to any rule since TS2369.
//!
//! # The site list *is* the rule
//!
//! Because the predicate is context-free, everything this rule gets right or
//! wrong is the set of positions it is asked about. Upstream calls
//! `checkTruthinessExpression` at seven: `if`, `do`, `while`, a `for`
//! statement's condition, a conditional expression's condition, the operand of
//! `!`, and the **left** operand of `&&` or `||` — the last only for
//! `IsLogicalBinaryOperator`, so `??` is excluded (`checker.go:12356`).
//!
//! `docs/architecture/checker-notes-diag2.md` §47.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, flags::TypeFlags};

/// `PredicateSemantics` (`checker.go:12877`), a two-bit lattice: the union of
/// `Always` and `Never` is `Sometimes`, which is what makes `c ? 1 : 0` silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PredicateSemantics {
    always: bool,
    never: bool,
}

impl PredicateSemantics {
    const ALWAYS: Self = Self { always: true, never: false };
    const NEVER: Self = Self { always: false, never: true };
    const SOMETIMES: Self = Self { always: true, never: true };

    fn union(self, other: Self) -> Self {
        Self { always: self.always || other.always, never: self.never || other.never }
    }
}

impl Checker<'_, '_> {
    /// Every truthiness-tested position under `node`, dispatched from the
    /// check traversal.
    pub(crate) fn check_truthiness_sites(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let tested = match self.node_map.get(node) {
            Some(Node::IfStatement(statement)) => statement.expression,
            Some(Node::WhileStatement(statement)) => statement.expression,
            Some(Node::DoStatement(statement)) => statement.expression,
            Some(Node::ForStatement(statement)) => statement.condition,
            Some(Node::ConditionalExpression(conditional)) => conditional.condition,
            Some(Node::PrefixUnaryExpression(unary))
                if unary.operator.kind == SyntaxKind::ExclamationToken =>
            {
                unary.operand
            }
            // `IsLogicalBinaryOperator` (`checker.go:12355`) — `&&` and `||`
            // and **not** `??`, whose left operand is tested for nullishness
            // by a different check with different messages.
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|token| {
                    matches!(
                        token.kind,
                        SyntaxKind::BarBarToken | SyntaxKind::AmpersandAmpersandToken
                    )
                }) =>
            {
                binary.left
            }
            _ => return,
        };
        let Some(tested) = tested else { return };
        self.check_truthiness_of(tested);
    }

    /// `checkTruthinessOfType` (`checker.go:12865`), against the expression's
    /// own node — which is also the error node.
    fn check_truthiness_of(&mut self, expression: Expression<'_>) {
        let Some(node) = expression.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        let tested = self.check_expression(expression);
        if self.type_of(tested).flags.intersects(TypeFlags::VOID) {
            self.report(
                file,
                Diagnostic::new(
                    &messages::AN_EXPRESSION_OF_TYPE_VOID_CANNOT_BE_TESTED_FOR_TRUTHINESS,
                    span,
                ),
            );
            return;
        }
        let semantics = self.syntactic_truthy_semantics(node, 0);
        if semantics == PredicateSemantics::SOMETIMES {
            return;
        }
        let message = if semantics.always {
            &messages::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY
        } else {
            &messages::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY
        };
        self.report(file, Diagnostic::new(message, span));
    }

    /// `getSyntacticTruthySemantics` (`checker.go:12886`).
    fn syntactic_truthy_semantics(&mut self, node: NodeId, depth: u32) -> PredicateSemantics {
        if depth > 64 {
            return PredicateSemantics::SOMETIMES;
        }
        let node = self.skip_outer_expressions(node);
        match self.node_map.get(node) {
            // `while (0)` and `while (1)` are allowed deliberately.
            Some(Node::NumericLiteral(literal)) => {
                if numeric_literal_is_zero_or_one(literal.text) {
                    PredicateSemantics::SOMETIMES
                } else {
                    PredicateSemantics::ALWAYS
                }
            }
            Some(
                Node::ArrayLiteralExpression(_)
                | Node::ArrowFunction(_)
                | Node::BigIntLiteral(_)
                | Node::ClassExpression(_)
                | Node::FunctionExpression(_)
                | Node::JsxElement(_)
                | Node::JsxSelfClosingElement(_)
                | Node::ObjectLiteralExpression(_)
                | Node::RegularExpressionLiteral(_),
            ) => PredicateSemantics::ALWAYS,
            Some(Node::VoidExpression(_)) => PredicateSemantics::NEVER,
            Some(Node::StringLiteral(literal)) => {
                if literal.text.is_empty() {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::ALWAYS
                }
            }
            Some(Node::NoSubstitutionTemplateLiteral(literal)) => {
                if literal.text.is_empty() {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::ALWAYS
                }
            }
            Some(Node::ConditionalExpression(conditional)) => {
                let branch = |branch: Option<Expression<'_>>| branch.and_then(|e| e.node_id());
                let (Some(when_true), Some(when_false)) =
                    (branch(conditional.when_true), branch(conditional.when_false))
                else {
                    return PredicateSemantics::SOMETIMES;
                };
                self.syntactic_truthy_semantics(when_true, depth + 1)
                    .union(self.syntactic_truthy_semantics(when_false, depth + 1))
            }
            // `undefined` is a value the global scope declares and no user
            // declaration shadows here; a shadowed one resolves and is
            // `Sometimes`, which is upstream's `symbol != c.undefinedSymbol`.
            Some(Node::Identifier(identifier)) if identifier.text == "undefined" => {
                if self
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
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::SOMETIMES
                }
            }
            _ => {
                if self.nodes.kind(node) == SyntaxKind::NullKeyword {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::SOMETIMES
                }
            }
        }
    }

    /// `ast.SkipOuterExpressions(node, ast.OEKAll)` — parentheses, the two
    /// assertion spellings, `satisfies`, and `!`-assertions all pass the
    /// question through to what they wrap.
    fn skip_outer_expressions(&self, node: NodeId) -> NodeId {
        let mut current = node;
        for _ in 0..64 {
            let inner = match self.node_map.get(current) {
                Some(Node::ParenthesizedExpression(wrapper)) => wrapper.expression,
                Some(Node::AsExpression(wrapper)) => wrapper.expression,
                Some(Node::TypeAssertion(wrapper)) => wrapper.expression,
                Some(Node::SatisfiesExpression(wrapper)) => wrapper.expression,
                Some(Node::NonNullExpression(wrapper)) => wrapper.expression,
                Some(Node::PartiallyEmittedExpression(wrapper)) => wrapper.expression,
                _ => return current,
            };
            let Some(inner) = inner.and_then(|expression| expression.node_id()) else {
                return current;
            };
            current = inner;
        }
        current
    }
}

/// Upstream compares `node.Text()` against `"0"` and `"1"`, and a numeric
/// literal's `Text` is the scanner's **normalised** value rather than the
/// source spelling: `0.0`, `0x0` and `0e5` all read `"0"`.
///
/// This port keeps the written text on the node, so the comparison is done on
/// the value instead. `ifDoWhileStatements`' `if (0.0) { }` was three wrong
/// lines before this — `checker-notes-diag2.md` §47.
fn numeric_literal_is_zero_or_one(text: &str) -> bool {
    let bare = text.replace('_', "");
    match bare.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("0x") => matches!(u128::from_str_radix(&bare[2..], 16), Ok(0 | 1)),
        Some("0o") => matches!(u128::from_str_radix(&bare[2..], 8), Ok(0 | 1)),
        Some("0b") => matches!(u128::from_str_radix(&bare[2..], 2), Ok(0 | 1)),
        // The decimal form is normalised the way the scanner would: format the
        // parsed value back out and compare the text, which is upstream's
        // comparison exactly and avoids an equality test on a float.
        _ => {
            matches!(bare.parse::<f64>().map(|value| format!("{value}")).as_deref(), Ok("0" | "1"))
        }
    }
}
