//! Expression parsing.
//!
//! Binary operators use precedence climbing rather than one function per level:
//! TypeScript has 15 binary precedence levels, and a function each would be 15
//! near-identical bodies that drift apart. The table in [`binary_precedence`] is
//! the single place the grammar's shape is stated.

use tsr_ast::*;
use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};

use crate::list::ParsingContext;
use crate::parser::Parser;

/// How many `>` a token represents.
///
/// The scanner produces `>>` and `>>>` as single shift tokens, but in a type
/// argument list each `>` closes a separate bracket.
fn greater_than_count(kind: SyntaxKind) -> u32 {
    match kind {
        SyntaxKind::GreaterThanToken | SyntaxKind::GreaterThanEqualsToken => 1,
        SyntaxKind::GreaterThanGreaterThanToken | SyntaxKind::GreaterThanGreaterThanEqualsToken => {
            2
        }
        SyntaxKind::GreaterThanGreaterThanGreaterThanToken
        | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => 3,
        _ => 0,
    }
}

/// Wrap an optional `async` modifier as an arena slice.
fn modifier_slice<'a>(
    arena: &'a tsr_core::Arena,
    modifier: Option<&'a Token<'a>>,
) -> &'a [ModifierLike<'a>] {
    match modifier {
        Some(token) => arena.alloc_slice(&[ModifierLike::Token(token)]),
        None => &[],
    }
}

/// Binding power of a binary operator, or `None` if `kind` is not one.
///
/// Higher binds tighter. Values mirror TypeScript's `OperatorPrecedence`.
fn binary_precedence(kind: SyntaxKind) -> Option<u8> {
    Some(match kind {
        SyntaxKind::QuestionQuestionToken => 1,
        SyntaxKind::BarBarToken => 2,
        SyntaxKind::AmpersandAmpersandToken => 3,
        SyntaxKind::BarToken => 4,
        SyntaxKind::CaretToken => 5,
        SyntaxKind::AmpersandToken => 6,
        SyntaxKind::EqualsEqualsToken
        | SyntaxKind::ExclamationEqualsToken
        | SyntaxKind::EqualsEqualsEqualsToken
        | SyntaxKind::ExclamationEqualsEqualsToken => 7,
        SyntaxKind::LessThanToken
        | SyntaxKind::GreaterThanToken
        | SyntaxKind::LessThanEqualsToken
        | SyntaxKind::GreaterThanEqualsToken
        | SyntaxKind::InstanceOfKeyword
        | SyntaxKind::InKeyword
        | SyntaxKind::AsKeyword
        | SyntaxKind::SatisfiesKeyword => 8,
        SyntaxKind::LessThanLessThanToken
        | SyntaxKind::GreaterThanGreaterThanToken
        | SyntaxKind::GreaterThanGreaterThanGreaterThanToken => 9,
        SyntaxKind::PlusToken | SyntaxKind::MinusToken => 10,
        SyntaxKind::AsteriskToken | SyntaxKind::SlashToken | SyntaxKind::PercentToken => 11,
        SyntaxKind::AsteriskAsteriskToken => 12,
        _ => return None,
    })
}

/// Whether `kind` is an assignment operator.
/// `ast.IsLeftHandSideExpression` — the set the assignment productions may
/// start with.
///
/// Transcribed rather than reused: `tsr_binder::narrowing` has the same
/// predicate and it is crate-private there, and the parser must not depend on
/// the binder.
fn is_left_hand_side_expression(expression: Expression<'_>) -> bool {
    matches!(
        expression,
        Expression::PropertyAccessExpression(_)
            | Expression::ElementAccessExpression(_)
            | Expression::NewExpression(_)
            | Expression::CallExpression(_)
            | Expression::JsxElement(_)
            | Expression::JsxSelfClosingElement(_)
            | Expression::JsxFragment(_)
            | Expression::TaggedTemplateExpression(_)
            | Expression::ArrayLiteralExpression(_)
            | Expression::ParenthesizedExpression(_)
            | Expression::ObjectLiteralExpression(_)
            | Expression::ClassExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::Identifier(_)
            | Expression::PrivateIdentifier(_)
            | Expression::RegularExpressionLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::NoSubstitutionTemplateLiteral(_)
            | Expression::TemplateExpression(_)
            | Expression::NonNullExpression(_)
            | Expression::ExpressionWithTypeArguments(_)
            | Expression::MetaProperty(_)
    ) || matches!(
        expression,
        Expression::KeywordExpression(keyword)
            if matches!(
                keyword.kind,
                SyntaxKind::FalseKeyword
                    | SyntaxKind::NullKeyword
                    | SyntaxKind::ThisKeyword
                    | SyntaxKind::TrueKeyword
                    | SyntaxKind::SuperKeyword
                    | SyntaxKind::ImportKeyword
            )
    )
}

fn is_assignment_operator(kind: SyntaxKind) -> bool {
    (SyntaxKind::FIRST_ASSIGNMENT as u16..=SyntaxKind::LAST_ASSIGNMENT as u16)
        .contains(&(kind as u16))
}

impl<'a> Parser<'a> {
    /// Parse a comma expression.
    pub(crate) fn parse_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        let mut expression = self.parse_assignment_expression();
        while self.at(SyntaxKind::CommaToken) {
            let operator = self.take_token();
            let right = self.parse_assignment_expression();
            let node = self.finish_node(
                BinaryExpression::new(&[], Some(expression), None, Some(operator), Some(right)),
                SyntaxKind::BinaryExpression,
                start,
            );
            expression = Expression::BinaryExpression(node);
        }
        expression
    }

    /// Parse a full expression in which a bare `in` is not a binary operator.
    ///
    /// Used for a `for` statement's initializer, where `in` introduces the
    /// `for…in` form. This must still be a *complete* expression — `for (i = 0;
    /// …)` is an assignment — which an earlier version got wrong by parsing only
    /// a unary expression.
    pub(crate) fn parse_expression_no_in(&mut self) -> Expression<'a> {
        self.no_in += 1;
        let expression = self.parse_expression();
        self.no_in -= 1;
        expression
    }

    /// Parse an assignment, conditional, or binary expression —
    /// typescript-go's `parseAssignmentExpressionOrHigher`, which allows an
    /// arrow function's return type.
    pub(crate) fn parse_assignment_expression(&mut self) -> Expression<'a> {
        self.parse_assignment_expression_worker(true)
    }

    /// typescript-go's `parseAssignmentExpressionOrHigherWorker`
    /// (`parser.go:4081`). `allow_return_type_in_arrow_function` is false only
    /// on the true branch of a conditional (and what that branch's arrows
    /// and assignments pass down), where `a ? (b) : c => d` must not read
    /// `(b) : c` as an arrow signature with a return type.
    fn parse_assignment_expression_worker(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Expression<'a> {
        let start = self.pos();

        if self.at(SyntaxKind::YieldKeyword) {
            return self.parse_yield_expression();
        }
        if let Some(arrow) = self.try_parse_arrow_function(allow_return_type_in_arrow_function) {
            return arrow;
        }

        let left = self.parse_binary_expression(0);

        // `ast.IsLeftHandSideExpression(expr) && ast.IsAssignmentOperator(...)`
        // (`parser.go:4143`) — **both** conjuncts. The assignment productions
        // can only start with a `LeftHandSideExpression`, which is upstream's
        // own comment three lines above, so `x++ = 4` is not an assignment
        // there: the `=` falls through and the statement gets `';' expected`.
        // §190 found the omission; §191 refused it at `diagnostics −6` while
        // the reporting sink still double-reported, and §193 fixed the sink.
        if is_left_hand_side_expression(left) && is_assignment_operator(self.token.kind) {
            let operator = self.take_token();
            let right =
                self.parse_assignment_expression_worker(allow_return_type_in_arrow_function);
            let node = self.finish_node(
                BinaryExpression::new(&[], Some(left), None, Some(operator), Some(right)),
                SyntaxKind::BinaryExpression,
                start,
            );
            return Expression::BinaryExpression(node);
        }

        if self.at(SyntaxKind::QuestionToken) {
            // `parseConditionalExpressionRest` (`parser.go:4552`).
            let question = self.take_token();
            let when_true = self.parse_assignment_expression_worker(false);
            let colon = if self.at(SyntaxKind::ColonToken) {
                self.take_token()
            } else {
                self.error_at_current_with(&messages::_0_EXPECTED, &[":"]);
                self.alloc_token(SyntaxKind::ColonToken, Span::at(self.pos()))
            };
            let when_false =
                self.parse_assignment_expression_worker(allow_return_type_in_arrow_function);
            let node = self.finish_node(
                ConditionalExpression::new(
                    Some(left),
                    Some(question),
                    Some(when_true),
                    Some(colon),
                    Some(when_false),
                ),
                SyntaxKind::ConditionalExpression,
                start,
            );
            return Expression::ConditionalExpression(node);
        }

        left
    }

    /// Precedence-climbing loop over binary operators.
    fn parse_binary_expression(&mut self, min_precedence: u8) -> Expression<'a> {
        let start = self.pos();
        let Some(mut left) = self.descend(Parser::parse_unary_expression_or_higher) else {
            self.error_at_current(&messages::EXPRESSION_EXPECTED);
            return Expression::Identifier(self.missing_identifier());
        };

        while let Some(precedence) = binary_precedence(self.token.kind) {
            if precedence < min_precedence {
                break;
            }
            if self.no_in > 0 && self.at(SyntaxKind::InKeyword) {
                break;
            }

            // `as` and `satisfies` take a *type* on the right, not an expression.
            if matches!(self.token.kind, SyntaxKind::AsKeyword | SyntaxKind::SatisfiesKeyword) {
                // `parseBinaryExpressionRest` (`parser.go:4619`): ASI applies,
                // so `var x = foo` followed by `as (Bar)` on the next line is a
                // call to a function named `as`, not an assertion.
                if self.token.has_preceding_line_break() {
                    break;
                }
                let is_as = self.at(SyntaxKind::AsKeyword);
                self.next_token();
                let type_node = self.parse_type();
                left = if is_as {
                    let node = self.finish_node(
                        AsExpression::new(Some(left), Some(type_node)),
                        SyntaxKind::AsExpression,
                        start,
                    );
                    Expression::AsExpression(node)
                } else {
                    let node = self.finish_node(
                        SatisfiesExpression::new(Some(left), Some(type_node)),
                        SyntaxKind::SatisfiesExpression,
                        start,
                    );
                    Expression::SatisfiesExpression(node)
                };
                continue;
            }

            let operator = self.take_token();
            // `**` is right-associative; everything else is left-associative.
            let next_min = if operator.kind == SyntaxKind::AsteriskAsteriskToken {
                precedence
            } else {
                precedence + 1
            };
            let right = self.parse_binary_expression(next_min);
            let node = self.finish_node(
                BinaryExpression::new(&[], Some(left), None, Some(operator), Some(right)),
                SyntaxKind::BinaryExpression,
                start,
            );
            left = Expression::BinaryExpression(node);
        }

        left
    }

    fn parse_yield_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.next_token();
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        // `yield` may stand alone. Besides the usual statement enders, a closing
        // delimiter ends it too: `{ [yield]: 1 }` and `f(yield)` are both legal.
        // **`yield` may stand alone; `yield*` may not.** Once upstream takes the
        // asterisk it calls `parseAssignmentExpression` unconditionally, and
        // that call is what reports `Expression expected` for `yield*` with
        // nothing after it. The test below is right for the bare form and was
        // applied to both because they share a function.
        // `docs/architecture/checker-notes-diag2.md` §572.
        let has_operand = asterisk.is_some()
            || (!self.can_parse_semicolon()
                && !self.token.has_preceding_line_break()
                && !matches!(
                    self.token.kind,
                    SyntaxKind::CloseBracketToken
                        | SyntaxKind::CloseParenToken
                        | SyntaxKind::CommaToken
                        | SyntaxKind::ColonToken
                ));
        let expression = if has_operand { Some(self.parse_assignment_expression()) } else { None };
        let node = self.finish_node(
            YieldExpression::new(asterisk, expression),
            SyntaxKind::YieldExpression,
            start,
        );
        Expression::YieldExpression(node)
    }

    /// `parseUnaryExpressionOrHigher` (`parser.go:4660`): the binary
    /// operand, with its report on a simple unary expression written as the
    /// left operand of `**`.
    ///
    /// An update expression (`isUpdateExpression`) may be followed by `**`;
    /// a prefix `+ - ~ ! delete typeof void await` expression or a type
    /// assertion may not, and the report spans it from its first token to
    /// its end (`SkipTrivia(pos)`, `End()`). The tree is the same either way:
    /// the binary loop takes the `**` next. Only this outermost entry
    /// reports, as only upstream's `…OrHigher` does; an operand of a prefix
    /// operator goes through `parseSimpleUnaryExpression`.
    fn parse_unary_expression_or_higher(&mut self) -> Expression<'a> {
        let start = self.pos();
        let operator = self.token_text();
        let is_update_expression = match self.token.kind {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::DeleteKeyword
            | SyntaxKind::TypeOfKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::AwaitKeyword => false,
            SyntaxKind::LessThanToken => self.script_kind.allows_jsx(),
            _ => true,
        };
        let expression = self.parse_unary_expression();
        if !is_update_expression && self.at(SyntaxKind::AsteriskAsteriskToken) {
            let span = Span::new(start, self.node_end());
            if !self.would_repeat_last_error(span) {
                let diagnostic = if matches!(expression, Expression::TypeAssertion(_)) {
                    Diagnostic::new(
                        &messages::A_TYPE_ASSERTION_EXPRESSION_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTIATION_EXPRESSION_CONSIDER_ENCLOSING_THE_EXPRESSION_IN_PARENTHESES,
                        span,
                    )
                } else {
                    Diagnostic::with_args(
                        &messages::AN_UNARY_EXPRESSION_WITH_THE_0_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTIATION_EXPRESSION_CONSIDER_ENCLOSING_THE_EXPRESSION_IN_PARENTHESES,
                        span,
                        [operator.to_string()],
                    )
                };
                self.diagnostics.push(diagnostic);
            }
        }
        expression
    }

    /// Parse prefix operators and `await`, then a postfix expression.
    pub(crate) fn parse_unary_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        // Bound before matching: the `await` arm's guard needs `&mut self` for
        // lookahead, which a match on `self.token.kind` directly would forbid.
        let kind = self.token.kind;
        match kind {
            // **`++` and `--` take a *left-hand-side* expression**, not a unary
            // one: `UpdateExpression : ++ LeftHandSideExpression`. Upstream
            // routes them through `parseUpdateExpression`, so `++ delete x`
            // reaches `parsePrimaryExpression` and fails there with
            // `Expression expected` (`parser.go:5591`) — the diagnostic comes
            // from not finding a primary, not from a test for `delete`.
            // `docs/architecture/checker-notes-diag2.md` §574.
            SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken => {
                let operator = self.take_token();
                let operand = self.parse_call_or_member_expression();
                let node = self.finish_node(
                    PrefixUnaryExpression::new(operator, Some(operand)),
                    SyntaxKind::PrefixUnaryExpression,
                    start,
                );
                Expression::PrefixUnaryExpression(node)
            }
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken => {
                let operator = self.take_token();
                let operand = self.parse_simple_unary_operand();
                let node = self.finish_node(
                    PrefixUnaryExpression::new(operator, Some(operand)),
                    SyntaxKind::PrefixUnaryExpression,
                    start,
                );
                Expression::PrefixUnaryExpression(node)
            }
            SyntaxKind::TypeOfKeyword => {
                self.next_token();
                let operand = self.parse_simple_unary_operand();
                let node = self.finish_node(
                    TypeOfExpression::new(Some(operand)),
                    SyntaxKind::TypeOfExpression,
                    start,
                );
                Expression::TypeOfExpression(node)
            }
            SyntaxKind::VoidKeyword => {
                self.next_token();
                let operand = self.parse_simple_unary_operand();
                let node = self.finish_node(
                    VoidExpression::new(Some(operand)),
                    SyntaxKind::VoidExpression,
                    start,
                );
                Expression::VoidExpression(node)
            }
            SyntaxKind::DeleteKeyword => {
                self.next_token();
                let operand = self.parse_simple_unary_operand();
                let node = self.finish_node(
                    DeleteExpression::new(Some(operand)),
                    SyntaxKind::DeleteExpression,
                    start,
                );
                Expression::DeleteExpression(node)
            }
            // `await` is CONTEXTUAL — see [`Self::is_await_expression`].
            SyntaxKind::AwaitKeyword if self.is_await_expression() => {
                self.next_token();
                let operand = self.parse_simple_unary_operand();
                let node = self.finish_node(
                    AwaitExpression::new(Some(operand)),
                    SyntaxKind::AwaitExpression,
                    start,
                );
                Expression::AwaitExpression(node)
            }
            // `parseUpdateExpression` (`parser.go:4716`): JSX is part of the
            // primary expression only when `<` is followed by a name or `>`;
            // otherwise the `<` falls through to the left-hand side.
            SyntaxKind::LessThanToken
                if self.script_kind.allows_jsx()
                    && self
                        .look_ahead(Self::next_token_is_identifier_or_keyword_or_greater_than) =>
            {
                self.parse_jsx_element_or_self_closing_element_or_fragment(true, None, false)
            }
            _ => self.parse_postfix_expression(),
        }
    }

    /// `nextTokenIsIdentifierOrKeywordOrGreaterThan` (`parser.go`).
    fn next_token_is_identifier_or_keyword_or_greater_than(&mut self) -> bool {
        self.next_token();
        crate::list::token_is_identifier_or_keyword(self.token.kind)
            || self.at(SyntaxKind::GreaterThanToken)
    }

    /// The operand of a prefix operator: upstream's `parseSimpleUnaryExpression`
    /// (`parser.go:5071`) differs from the update-expression entry only in its
    /// JSX arm — any `<` is JSX, parsed with `mustBeUnary` so the
    /// sibling-element recovery cannot wrap it in a binary.
    fn parse_simple_unary_operand(&mut self) -> Expression<'a> {
        if self.script_kind.allows_jsx() && self.at(SyntaxKind::LessThanToken) {
            return self.parse_jsx_element_or_self_closing_element_or_fragment(true, None, true);
        }
        self.parse_unary_expression()
    }

    fn parse_postfix_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        let expression = self.parse_call_or_member_expression();

        // `a\n++b` is two statements, not a postfix increment, so a line break
        // suppresses the operator.
        if matches!(self.token.kind, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
            && !self.token.has_preceding_line_break()
        {
            let operator = self.take_token();
            let node = self.finish_node(
                PostfixUnaryExpression::new(Some(expression), operator),
                SyntaxKind::PostfixUnaryExpression,
                start,
            );
            return Expression::PostfixUnaryExpression(node);
        }
        expression
    }

    /// typescript-go's `Parser.parseLeftHandSideExpressionOrHigher`
    /// (`parser.go`), which a heritage clause element starts with.
    pub(crate) fn parse_left_hand_side_expression_or_higher(&mut self) -> Expression<'a> {
        self.parse_call_or_member_expression()
    }

    /// Parse a primary expression followed by any chain of calls and accesses.
    fn parse_call_or_member_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        let mut expression = if self.at(SyntaxKind::NewKeyword) {
            self.parse_new_expression()
        } else if self.at(SyntaxKind::SuperKeyword) {
            self.parse_super_expression()
        } else if self.at(SyntaxKind::ImportKeyword)
            && !self.peek_kind(|kind| {
                matches!(kind, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken)
            })
            && self.peek_kind(|kind| kind == SyntaxKind::DotToken)
        {
            // `parseLeftHandSideExpressionOrHigher` (`parser.go:5176`):
            // `import` followed by `.` (and not by `(`/`<`, the import call)
            // is an `import.*` meta-property, `import.meta` or `import.defer`,
            // named by `parseIdentifierName`. The checker answers its type and
            // module-kind errors (`tsr-checker/src/import_meta.rs`). bd
            // tsr-9or.4 / tsr-2zk.990.
            let import_token = self.token;
            self.next_token();
            self.next_token();
            let name = self.parse_identifier_name();
            if name.text != "defer" {
                self.source_flags |= tsr_ast::NodeFlags::POSSIBLY_CONTAINS_IMPORT_META;
            }
            let keyword_token = self.alloc_token(import_token.kind, import_token.span);
            let node = self.finish_node(
                MetaProperty::new(keyword_token, Some(name)),
                SyntaxKind::MetaProperty,
                start,
            );
            Expression::MetaProperty(node)
        } else {
            self.parse_primary_expression()
        };
        // The `<`…`>` range of the instantiation expression `expression` is,
        // while it is one: `parsePropertyAccessExpressionRest`'s TS1477 spans
        // `typeArguments.Pos()-1` to `SkipTrivia(typeArguments.End())+1`, and
        // this AST keeps neither bracket.
        let mut instantiation_brackets: Option<Span> = None;

        loop {
            match self.token.kind {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let name = if self.right_side_of_dot_is_missing() {
                        self.report_missing_right_side_of_dot();
                        MemberName::Identifier(self.missing_identifier())
                    } else {
                        self.parse_member_name()
                    };
                    if let (Expression::ExpressionWithTypeArguments(_), Some(brackets)) =
                        (expression, instantiation_brackets.take())
                    {
                        self.error_at(
                            &messages::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                            brackets,
                        );
                    }
                    let is_chain = self.try_reparse_optional_chain(expression);
                    let node = self.finish_node(
                        PropertyAccessExpression::new(Some(expression), None, Some(name)),
                        SyntaxKind::PropertyAccessExpression,
                        start,
                    );
                    self.mark_optional_chain(node.node_id(), is_chain);
                    expression = Expression::PropertyAccessExpression(node);
                }
                SyntaxKind::QuestionDotToken => {
                    let question_dot = self.take_token();
                    if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
                        // `a?.<T>()` — type arguments after the optional-chain dot.
                        let type_arguments = if self.at(SyntaxKind::LessThanToken) {
                            self.try_parse(Parser::parse_type_arguments_for_call)
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let arguments = self.parse_arguments();
                        let arguments = self.arena.alloc_slice(&arguments);
                        let type_arguments = self.arena.alloc_slice(&type_arguments);
                        let node = self.finish_node(
                            CallExpression::new(
                                Some(expression),
                                Some(question_dot),
                                type_arguments,
                                arguments,
                            ),
                            SyntaxKind::CallExpression,
                            start,
                        );
                        self.mark_optional_chain(node.node_id(), true);
                        expression = Expression::CallExpression(node);
                    } else if self.at(SyntaxKind::OpenBracketToken) {
                        self.next_token();
                        let argument = self.parse_expression();
                        self.expect(SyntaxKind::CloseBracketToken);
                        let node = self.finish_node(
                            ElementAccessExpression::new(
                                Some(expression),
                                Some(question_dot),
                                Some(argument),
                            ),
                            SyntaxKind::ElementAccessExpression,
                            start,
                        );
                        self.mark_optional_chain(node.node_id(), true);
                        expression = Expression::ElementAccessExpression(node);
                    } else {
                        let name = self.parse_member_name();
                        let node = self.finish_node(
                            PropertyAccessExpression::new(
                                Some(expression),
                                Some(question_dot),
                                Some(name),
                            ),
                            SyntaxKind::PropertyAccessExpression,
                            start,
                        );
                        self.mark_optional_chain(node.node_id(), true);
                        expression = Expression::PropertyAccessExpression(node);
                    }
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = self.parse_expression();
                    self.expect(SyntaxKind::CloseBracketToken);
                    let is_chain = self.try_reparse_optional_chain(expression);
                    let node = self.finish_node(
                        ElementAccessExpression::new(Some(expression), None, Some(argument)),
                        SyntaxKind::ElementAccessExpression,
                        start,
                    );
                    self.mark_optional_chain(node.node_id(), is_chain);
                    expression = Expression::ElementAccessExpression(node);
                }
                SyntaxKind::OpenParenToken => {
                    let arguments = self.parse_arguments();
                    let arguments = self.arena.alloc_slice(&arguments);
                    // `parseCallExpressionRest` absorbs an instantiation into
                    // the call; notably `parseSuperExpression` can return one.
                    let (callee, type_arguments) = match expression {
                        Expression::ExpressionWithTypeArguments(instantiation) => (
                            instantiation.expression.unwrap_or(expression),
                            instantiation.type_arguments,
                        ),
                        _ => (expression, &[] as &[TypeNode<'a>]),
                    };
                    let is_chain = self.try_reparse_optional_chain(expression);
                    let node = self.finish_node(
                        CallExpression::new(Some(callee), None, type_arguments, arguments),
                        SyntaxKind::CallExpression,
                        start,
                    );
                    self.mark_optional_chain(node.node_id(), is_chain);
                    expression = Expression::CallExpression(node);
                }
                // `f<T>(x)`. `<` is also less-than, so the type arguments are
                // only accepted when a call follows them.
                SyntaxKind::LessThanToken | SyntaxKind::LessThanLessThanToken => {
                    // `f<T>(x)` is a generic call. `f<T>` alone is an
                    // *instantiation expression*, legal since TS 4.7 — but `a < b
                    // > c` is a comparison, so the type arguments only stand
                    // without a call when what follows cannot continue an
                    // expression.
                    let open_bracket = self.token.span.start;
                    let Some(type_arguments) = self.try_parse(|p| {
                        // `f<<T>() => U>(g)` starts a generic call whose first
                        // type argument is a generic arrow. The scanner sees
                        // the adjacent opening brackets as `<<`; split them
                        // only inside the speculative parse so a real shift
                        // expression still rewinds intact.
                        if p.at(SyntaxKind::LessThanLessThanToken) {
                            p.rescan_less_than();
                        }
                        let arguments = p.parse_type_arguments_for_call()?;
                        (p.at(SyntaxKind::OpenParenToken) || p.at_instantiation_terminator())
                            .then_some(arguments)
                    }) else {
                        break;
                    };
                    let type_arguments = self.arena.alloc_slice(&type_arguments);
                    if !self.at(SyntaxKind::OpenParenToken) {
                        let node = self.finish_node(
                            ExpressionWithTypeArguments::new(Some(expression), type_arguments),
                            SyntaxKind::ExpressionWithTypeArguments,
                            start,
                        );
                        expression = Expression::ExpressionWithTypeArguments(node);
                        instantiation_brackets = Some(Span::new(open_bracket, self.node_end()));
                        continue;
                    }
                    let arguments = self.parse_arguments();
                    let arguments = self.arena.alloc_slice(&arguments);
                    let is_chain = self.try_reparse_optional_chain(expression);
                    let node = self.finish_node(
                        CallExpression::new(Some(expression), None, type_arguments, arguments),
                        SyntaxKind::CallExpression,
                        start,
                    );
                    self.mark_optional_chain(node.node_id(), is_chain);
                    expression = Expression::CallExpression(node);
                }
                // `` tag`…` `` — a tagged template. The template is an operand of
                // the tag, not a separate expression.
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal(true);
                    let (tag, type_arguments) = match expression {
                        Expression::ExpressionWithTypeArguments(instantiation) => (
                            instantiation.expression.unwrap_or(expression),
                            instantiation.type_arguments,
                        ),
                        _ => (expression, &[] as &[TypeNode<'a>]),
                    };
                    // `parser.go:5520`: the flag is inherited from the tag
                    // directly, without the non-null reparse.
                    let is_chain = self.has_optional_chain_flag(tag);
                    let node = self.finish_node(
                        TaggedTemplateExpression::new(
                            Some(tag),
                            None,
                            type_arguments,
                            Some(template),
                        ),
                        SyntaxKind::TaggedTemplateExpression,
                        start,
                    );
                    self.mark_optional_chain(node.node_id(), is_chain);
                    expression = Expression::TaggedTemplateExpression(node);
                }
                SyntaxKind::ExclamationToken if !self.token.has_preceding_line_break() => {
                    self.next_token();
                    let node = self.finish_node(
                        NonNullExpression::new(Some(expression)),
                        SyntaxKind::NonNullExpression,
                        start,
                    );
                    expression = Expression::NonNullExpression(node);
                }
                _ => break,
            }
        }
        expression
    }

    /// Pinned `parseSuperExpression` (`parser.go:5215`, 5b1047d10d32e7d5b446be4de56b126ff42f82bb).
    /// Recovery owns a property-access node and its missing/name child, rather
    /// than leaving a bare keyword. Nodes remain in the existing parser arena
    /// and parent table; no semantic query or additional traversal is introduced.
    fn parse_super_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        let mut expression = self.parse_primary_expression();
        if self.at(SyntaxKind::LessThanToken) {
            let arguments_start = self.node_end();
            if let Some(arguments) = self.try_parse(|parser| {
                let arguments = parser.parse_type_arguments_for_call()?;
                parser.at_instantiation_terminator().then_some(arguments)
            }) {
                self.error_at(
                    &messages::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                    Span::new(arguments_start, self.node_end()),
                );
                // Native discards the arguments before a tagged template.
                if !matches!(
                    self.token.kind,
                    SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead
                ) {
                    let arguments = self.arena.alloc_slice(&arguments);
                    expression = Expression::ExpressionWithTypeArguments(self.finish_node(
                        ExpressionWithTypeArguments::new(Some(expression), arguments),
                        SyntaxKind::ExpressionWithTypeArguments,
                        start,
                    ));
                }
            }
        }
        if matches!(
            self.token.kind,
            SyntaxKind::OpenParenToken | SyntaxKind::DotToken | SyntaxKind::OpenBracketToken
        ) {
            return expression;
        }
        self.error_at_current(
            &messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
        );
        let name = if self.right_side_of_dot_is_missing() {
            // `parseRightSideOfDot` reports at TokenFullStart, before the
            // following line's trivia, distinct from TS1034 on that token.
            self.error_at(&messages::IDENTIFIER_EXPECTED, Span::at(self.node_end()));
            MemberName::Identifier(self.missing_identifier())
        } else {
            self.parse_member_name()
        };
        Expression::PropertyAccessExpression(self.finish_node(
            PropertyAccessExpression::new(Some(expression), None, Some(name)),
            SyntaxKind::PropertyAccessExpression,
            start,
        ))
    }

    /// `parser.go:5414` `tryReparseOptionalChain`: is `node` already part of
    /// an optional chain? A `NonNullExpression` (or a run of them) over a
    /// chain is stamped as a chain member here, retroactively, exactly as
    /// upstream mutates `node.Flags` — `a?.b!.c` makes `a?.b!` a chain link
    /// only once `.c` follows. §748.
    fn try_reparse_optional_chain(&mut self, node: Expression<'a>) -> bool {
        if self.has_optional_chain_flag(node) {
            return true;
        }
        if let Expression::NonNullExpression(_) = node {
            let mut expr = node;
            while let Expression::NonNullExpression(non_null) = expr
                && !self.has_optional_chain_flag(expr)
            {
                let Some(inner) = non_null.expression else { return false };
                expr = inner;
            }
            if self.has_optional_chain_flag(expr) {
                let mut current = node;
                while let Expression::NonNullExpression(non_null) = current {
                    self.mark_optional_chain(current.node_id(), true);
                    let Some(inner) = non_null.expression else { break };
                    current = inner;
                }
                return true;
            }
        }
        false
    }

    fn has_optional_chain_flag(&self, node: Expression<'a>) -> bool {
        node.node_id()
            .is_some_and(|id| self.nodes.flags(id).contains(tsr_ast::NodeFlags::OPTIONAL_CHAIN))
    }

    /// Stamp [`tsr_ast::NodeFlags::OPTIONAL_CHAIN`] when `is_chain`, the
    /// `core.IfElse(isOptionalChain, ast.NodeFlagsOptionalChain, …)` of every
    /// member-rest constructor in `parser.go:5399-5521`.
    fn mark_optional_chain(&mut self, id: Option<tsr_ast::NodeId>, is_chain: bool) {
        if is_chain && let Some(id) = id {
            self.nodes.add_flags(id, tsr_ast::NodeFlags::OPTIONAL_CHAIN);
        }
    }

    /// Ported from typescript-go's `Parser.parseNewExpressionOrNewDotTarget`
    /// and `Parser.parseMemberExpressionRest` (`internal/parser/parser.go`) at
    /// 5b1047d10d32e7d5b446be4de56b126ff42f82bb, with optional chains disabled.
    /// Callees use the parser's existing arena and node table. Ordinary generic
    /// constructors absorb their final type arguments without allocating an
    /// unused wrapper identity; nested members keep their actual AST owners.
    fn parse_new_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        // The `new` token's kind and span, captured WITHOUT allocating. Using
        // `take_token` here — the obvious spelling — allocates and registers a
        // token node on **every** `new` expression, not just the meta-property
        // one, which shifts every subsequent `NodeId` and measured **−2 lines
        // in `compiler/valueOfTypedArray`**, a case with no `new.target` in it
        // at all. Allocate on the meta path only.
        let new_token = self.token;
        self.next_token();
        // §233: `new.target` is a `MetaProperty`, not a `NewExpression` whose
        // callee begins with a dot. `parseNewExpressionOrNewDotTarget`
        // (`parser.go:5746`) tests for the dot immediately after `new` and
        // returns before any callee is parsed.
        //
        // Without this, `parse_primary_expression` met `.`, manufactured a
        // missing identifier, and the member-chain loop below then ate
        // `.target` as a property access — so the tree held a `NewExpression`
        // over a zero-width callee where upstream holds one node.
        //
        // `parse_identifier_name`, not `parse_identifier`: `target` is a plain
        // identifier here but the grammar admits any identifier NAME, and
        // upstream uses the name form so that `new.default` parses (and is
        // rejected later by the checker) rather than failing in the parser.
        if self.at(SyntaxKind::DotToken) {
            self.next_token();
            let name = self.parse_identifier_name();
            let keyword_token = self.alloc_token(new_token.kind, new_token.span);
            let node = self.finish_node(
                MetaProperty::new(keyword_token, Some(name)),
                SyntaxKind::MetaProperty,
                start,
            );
            return Expression::MetaProperty(node);
        }
        // Where the *callee* begins, after `new`. The member chain below is
        // finished from here rather than from `start`, because the `new` keyword
        // belongs to the `NewExpression` and not to its callee: in
        // `new provide.Provide()` the property access spans `provide.Provide`,
        // and upstream's `.types` baseline records exactly that
        // (`compiler/aliasBug.types`: `>provide.Provide : typeof provide.Provide`).
        // Finishing from `start` gave it `new provide.Provide`, which was the
        // largest single source of `.types` walker divergence once the
        // predicates were right.
        let callee_start = self.pos();
        let mut callee = if self.at(SyntaxKind::NewKeyword) {
            let Some(callee) = self.descend(|parser| {
                tsr_core::stack::ensure_sufficient(|| parser.parse_new_expression())
            }) else {
                self.error_at_current(&messages::EXPRESSION_EXPECTED);
                return Expression::Identifier(self.missing_identifier());
            };
            callee
        } else {
            self.parse_primary_expression()
        };
        // `parseNewExpressionOrNewDotTarget` calls `parseMemberExpressionRest`
        // with allowOptionalChain=false (pinned parser.go). Brackets, tags and
        // non-null assertions bind to the constructor just as dots do; calls
        // and optional chains do not. In `new a[0].C(x)` the old dot-only loop
        // instead built `(new a)[0].C(x)`, changing both types and source spans.
        let mut type_arguments = Vec::new();
        loop {
            match self.token.kind {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let name = if self.right_side_of_dot_is_missing() {
                        self.report_missing_right_side_of_dot();
                        MemberName::Identifier(self.missing_identifier())
                    } else {
                        self.parse_member_name()
                    };
                    callee = Expression::PropertyAccessExpression(self.finish_node(
                        PropertyAccessExpression::new(Some(callee), None, Some(name)),
                        SyntaxKind::PropertyAccessExpression,
                        callee_start,
                    ));
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = if self.at(SyntaxKind::CloseBracketToken) {
                        self.error_at(
                            &messages::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                            Span::at(self.node_end()),
                        );
                        Expression::Identifier(self.missing_identifier())
                    } else {
                        let saved_no_in = std::mem::take(&mut self.no_in);
                        let argument = self.parse_expression();
                        self.no_in = saved_no_in;
                        argument
                    };
                    self.expect(SyntaxKind::CloseBracketToken);
                    callee = Expression::ElementAccessExpression(self.finish_node(
                        ElementAccessExpression::new(Some(callee), None, Some(argument)),
                        SyntaxKind::ElementAccessExpression,
                        callee_start,
                    ));
                }
                SyntaxKind::ExclamationToken if !self.token.has_preceding_line_break() => {
                    self.next_token();
                    callee = Expression::NonNullExpression(self.finish_node(
                        NonNullExpression::new(Some(callee)),
                        SyntaxKind::NonNullExpression,
                        callee_start,
                    ));
                }
                SyntaxKind::LessThanToken | SyntaxKind::LessThanLessThanToken => {
                    let Some(arguments) = self.try_parse(|p| {
                        if p.at(SyntaxKind::LessThanLessThanToken) {
                            p.rescan_less_than();
                        }
                        let arguments = p.parse_type_arguments_for_call()?;
                        // `canFollowTypeArgumentsInExpression`: favor a type
                        // argument list at a binary operator or where another
                        // expression cannot start, except for the four tokens
                        // native explicitly treats as relational/unary.
                        let follows = match p.token.kind {
                            SyntaxKind::OpenParenToken
                            | SyntaxKind::NoSubstitutionTemplateLiteral
                            | SyntaxKind::TemplateHead => true,
                            SyntaxKind::LessThanToken
                            | SyntaxKind::GreaterThanToken
                            | SyntaxKind::PlusToken
                            | SyntaxKind::MinusToken => false,
                            kind => {
                                p.token.has_preceding_line_break()
                                    || (binary_precedence(kind).is_some()
                                        && !(p.no_in != 0 && kind == SyntaxKind::InKeyword))
                                    || !p.is_start_of_expression()
                            }
                        };
                        follows.then_some(arguments)
                    }) else {
                        break;
                    };
                    // The final instantiation is absorbed into NewExpression.
                    // Avoid allocating its discarded wrapper: ordinary generic
                    // `new C<T>()` keeps its existing program-local node IDs.
                    if !matches!(
                        self.token.kind,
                        SyntaxKind::DotToken
                            | SyntaxKind::OpenBracketToken
                            | SyntaxKind::ExclamationToken
                            | SyntaxKind::LessThanToken
                            | SyntaxKind::LessThanLessThanToken
                            | SyntaxKind::NoSubstitutionTemplateLiteral
                            | SyntaxKind::TemplateHead
                    ) {
                        type_arguments = arguments;
                        break;
                    }
                    let arguments = self.arena.alloc_slice(&arguments);
                    callee = Expression::ExpressionWithTypeArguments(self.finish_node(
                        ExpressionWithTypeArguments::new(Some(callee), arguments),
                        SyntaxKind::ExpressionWithTypeArguments,
                        callee_start,
                    ));
                }
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal(true);
                    let (tag, arguments) = match callee {
                        Expression::ExpressionWithTypeArguments(instantiation) => (
                            instantiation.expression.unwrap_or(callee),
                            instantiation.type_arguments,
                        ),
                        _ => (callee, &[] as &[TypeNode<'a>]),
                    };
                    callee = Expression::TaggedTemplateExpression(self.finish_node(
                        TaggedTemplateExpression::new(Some(tag), None, arguments, Some(template)),
                        SyntaxKind::TaggedTemplateExpression,
                        callee_start,
                    ));
                }
                _ => break,
            }
        }
        if let Expression::ExpressionWithTypeArguments(instantiation) = callee {
            callee = instantiation.expression.unwrap_or(callee);
            type_arguments = instantiation.type_arguments.to_vec();
        }
        if self.at(SyntaxKind::QuestionDotToken) {
            let span = self.nodes.span(callee.node_id().expect("a parsed callee has an id"));
            let text = &self.source[span.start as usize..span.end as usize];
            self.error_at_current_with(
                &messages::INVALID_OPTIONAL_CHAIN_FROM_NEW_EXPRESSION_DID_YOU_MEAN_TO_CALL_0,
                &[text],
            );
        }
        let arguments = if self.at(SyntaxKind::OpenParenToken) {
            let args = self.parse_arguments();
            self.arena.alloc_slice(&args)
        } else {
            // `new Foo` without parentheses is legal.
            &[][..]
        };
        let type_arguments = self.arena.alloc_slice(&type_arguments);
        let node = self.finish_node(
            NewExpression::new(Some(callee), type_arguments, arguments),
            SyntaxKind::NewExpression,
            start,
        );
        Expression::NewExpression(node)
    }

    /// typescript-go's `Parser.parseArgumentList` (`parser.go`):
    /// `parseDelimitedList(PCArgumentExpressions, parseArgumentExpression)`
    /// between parentheses. A token that is no argument is reported and
    /// skipped unless an enclosing list wants it (§191/§235/§197's stand-ins,
    /// now the real `isInSomeParsingContext`).
    pub(crate) fn parse_arguments(&mut self) -> Vec<Expression<'a>> {
        self.expect(SyntaxKind::OpenParenToken);
        // `parseArgumentExpression`: arguments are never in a disallow-in
        // context.
        let saved_no_in = std::mem::take(&mut self.no_in);
        let (arguments, _) =
            self.parse_delimited_list(ParsingContext::ArgumentExpressions, Self::parse_argument);
        self.no_in = saved_no_in;
        self.expect(SyntaxKind::CloseParenToken);
        arguments
    }

    /// typescript-go's `Parser.parseArgumentOrArrayLiteralElement`
    /// (`parser.go`).
    fn parse_argument(&mut self) -> Expression<'a> {
        if self.at(SyntaxKind::CommaToken) {
            let at = self.node_end();
            let node = self.finish_node_with_end(
                OmittedExpression::new(),
                SyntaxKind::OmittedExpression,
                at,
                at,
            );
            return Expression::OmittedExpression(node);
        }
        if self.at(SyntaxKind::DotDotDotToken) {
            let start = self.pos();
            self.next_token();
            let expression = self.parse_assignment_expression();
            let node = self.finish_node(
                SpreadElement::new(Some(expression)),
                SyntaxKind::SpreadElement,
                start,
            );
            return Expression::SpreadElement(node);
        }
        self.parse_assignment_expression()
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse_primary_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        // Bound before matching: a guard below needs `&mut self` for lookahead.
        let kind = self.token.kind;
        match kind {
            SyntaxKind::Identifier => Expression::Identifier(self.parse_identifier()),
            SyntaxKind::NumericLiteral => {
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    NumericLiteral::new(text, flags),
                    SyntaxKind::NumericLiteral,
                    start,
                );
                Expression::NumericLiteral(node)
            }
            SyntaxKind::BigIntLiteral => {
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    BigIntLiteral::new(text, flags),
                    SyntaxKind::BigIntLiteral,
                    start,
                );
                Expression::BigIntLiteral(node)
            }
            SyntaxKind::StringLiteral => {
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    StringLiteral::new(text, flags),
                    SyntaxKind::StringLiteral,
                    start,
                );
                Expression::StringLiteral(node)
            }
            SyntaxKind::PrivateIdentifier => {
                let text = self.private_identifier_text();
                self.next_token();
                let node = self.finish_node(
                    PrivateIdentifier::new(text),
                    SyntaxKind::PrivateIdentifier,
                    start,
                );
                Expression::PrivateIdentifier(node)
            }
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                // Untagged: re-scan reporting invalid escapes, since the first
                // pass is silent (`scanner.go:522`). §223.
                self.token = self.scanner.rescan_template(false);
                let raw = self.token_text();
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    NoSubstitutionTemplateLiteral::new(text, flags, flags, raw),
                    SyntaxKind::NoSubstitutionTemplateLiteral,
                    start,
                );
                Expression::NoSubstitutionTemplateLiteral(node)
            }
            SyntaxKind::TemplateHead => {
                self.token = self.scanner.rescan_template(false);
                self.parse_template_expression()
            }
            // `<` is a type assertion in `.ts` and a JSX element in `.tsx`. The
            // two readings are mutually exclusive, which is why TypeScript ties
            // them to the file extension rather than to a lookahead.
            // In `.tsx` a `<` never reaches here as JSX: upstream parses JSX
            // in `parseUpdateExpression`/`parseSimpleUnaryExpression` (see
            // [`Self::parse_unary_expression`]), and `parsePrimaryExpression`
            // has no `<` arm, so it falls to the missing-expression default.
            SyntaxKind::LessThanToken if !self.script_kind.allows_jsx() => {
                self.parse_type_assertion()
            }
            SyntaxKind::FunctionKeyword => {
                let jsdoc = self.leading_jsdoc_marker();
                self.parse_function_expression(None, None, jsdoc)
            }
            SyntaxKind::AsyncKeyword if self.next_is_function_keyword() => {
                // §205: the span starts at the `async`, not at `function`.
                let modifier_start = self.pos();
                let jsdoc = self.leading_jsdoc_marker();
                let modifier = self.take_token();
                self.parse_function_expression(Some(modifier), Some(modifier_start), jsdoc)
            }
            SyntaxKind::ClassKeyword => self.parse_class_expression(),
            // `(@dec class C {})` — a decorated class expression.
            SyntaxKind::AtToken => {
                let modifiers = self.parse_modifiers();
                let modifiers = self.arena.alloc_slice(&modifiers);
                let Expression::ClassExpression(class) = self.parse_class_expression() else {
                    unreachable!("parse_class_expression yields a class")
                };
                Expression::ClassExpression(self.finish_node(
                    ClassExpression::new(
                        modifiers,
                        class.name,
                        class.type_parameters,
                        class.heritage_clauses,
                        class.members,
                    ),
                    SyntaxKind::ClassExpression,
                    start,
                ))
            }
            SyntaxKind::OpenParenToken => {
                // §275: a `/** @type {T} */ (expr)` JSDoc CAST hangs its doc
                // off the parenthesized expression — upstream's
                // `parseParenthesizedExpression` is `withJSDoc`-wrapped, and
                // `isJSDocTypeAssertion` reads the tag back off this node.
                let docs = self.parse_leading_jsdoc();
                self.next_token();
                let saved_no_in = std::mem::take(&mut self.no_in);
                let expression = self.parse_expression();
                self.no_in = saved_no_in;
                self.expect(SyntaxKind::CloseParenToken);
                let node = self.finish_node(
                    ParenthesizedExpression::new(Some(expression)),
                    SyntaxKind::ParenthesizedExpression,
                    start,
                );
                self.attach_jsdoc(tsr_ast::Node::ParenthesizedExpression(node), docs);
                Expression::ParenthesizedExpression(node)
            }
            SyntaxKind::OpenBracketToken => self.parse_array_literal(),
            SyntaxKind::OpenBraceToken => self.parse_object_literal(),
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken => {
                self.rescan_regular_expression();
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    RegularExpressionLiteral::new(text, flags),
                    SyntaxKind::RegularExpressionLiteral,
                    start,
                );
                Expression::RegularExpressionLiteral(node)
            }
            // A *reserved* word in expression position is the keyword itself:
            // `this`, `super`, `true`, `false`, `null`.
            //
            // Only these: `parsePrimaryExpression` (`parser.go:5530`) takes
            // `this`/`super`/`null`/`true`/`false` as token nodes, and `import`
            // reaches here for `import(…)`/`import.meta`
            // (`parseMemberExpressionOrHigher`). Every other reserved word falls
            // to `parseIdentifierWithDiagnostic(Expression_expected)` below and
            // is left for the statement that follows — `1 +⏎return;` is TS1109
            // at `return`, not a keyword operand.
            SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::ImportKeyword => {
                let kind = self.token.kind;
                if kind == SyntaxKind::ImportKeyword {
                    // `parseMemberExpressionOrHigher`'s import-call arm
                    // (`parser.go:5183`). Set for every `import` keyword
                    // expression, a superset of upstream's `(`/`<` lookahead:
                    // the flag only gates the loader's dynamic-import walk
                    // (`references.rs`), which finds nothing extra.
                    self.source_flags |= tsr_ast::NodeFlags::POSSIBLY_CONTAINS_DYNAMIC_IMPORT;
                }
                self.next_token();
                let node = self.finish_node(KeywordExpression::new(kind), kind, start);
                Expression::KeywordExpression(node)
            }
            // Anything else that is a keyword is *contextual*, and in expression
            // position it is an ordinary name: `module.exports`, `const x =
            // type`, `of(1)`. Parsing it as a keyword expression loses the text
            // — a `KeywordExpression` has no name — so every such reference
            // became anonymous. Upstream falls through to `parseIdentifier()`
            // here for the same reason.
            kind if kind.is_keyword() && !crate::statement::is_reserved_word(kind) => {
                Expression::Identifier(self.parse_identifier())
            }
            _ => {
                // `parseIdentifierWithDiagnostic(Expression_expected)`: at end
                // of file the report sits zero-width at the token's full
                // start, as `report_missing_identifier`'s does.
                let span = if self.at(SyntaxKind::EndOfFile) {
                    Span::at(self.node_end())
                } else {
                    self.token.span
                };
                self.error_at(&messages::EXPRESSION_EXPECTED, span);
                Expression::Identifier(self.missing_identifier())
            }
        }
    }

    /// typescript-go's `Parser.parseArrayLiteralExpression` (`parser.go`):
    /// `parseDelimitedList(PCArrayLiteralMembers,
    /// parseArgumentOrArrayLiteralElement)`. An elision is an
    /// `OmittedExpression`; a token that is no element is reported and
    /// skipped unless an enclosing list wants it (§237's stand-in, now the
    /// real `isInSomeParsingContext`).
    pub(crate) fn parse_array_literal(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBracketToken);
        let saved_no_in = std::mem::take(&mut self.no_in);
        let (elements, _) =
            self.parse_delimited_list(ParsingContext::ArrayLiteralMembers, Self::parse_argument);
        self.no_in = saved_no_in;
        // `parseExpectedMatchingBrackets`.
        self.expect(SyntaxKind::CloseBracketToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(
            ArrayLiteralExpression::new(elements, false),
            SyntaxKind::ArrayLiteralExpression,
            start,
        );
        Expression::ArrayLiteralExpression(node)
    }

    /// typescript-go's `Parser.parseObjectLiteralExpression` (`parser.go:5615`):
    /// `parseDelimitedList(PCObjectLiteralMembers, parseObjectLiteralElement)`.
    /// The list machinery reports a stray `,` as "Property assignment
    /// expected" and skips it, skips a `;` used as a separator after the
    /// "',' expected", and — unlike the hand loop it replaces — keeps going
    /// past a token that is not a member unless an enclosing list wants it.
    pub(crate) fn parse_object_literal(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let (properties, _) = self.parse_delimited_list(
            ParsingContext::ObjectLiteralMembers,
            Self::parse_object_literal_element,
        );
        self.expect(SyntaxKind::CloseBraceToken);
        let properties = self.arena.alloc_slice(&properties);
        let node = self.finish_node(
            ObjectLiteralExpression::new(properties, false),
            SyntaxKind::ObjectLiteralExpression,
            start,
        );
        Expression::ObjectLiteralExpression(node)
    }

    fn parse_object_literal_element(&mut self) -> ObjectLiteralElementLike<'a> {
        // parseObjectLiteralElement saves JSDoc before parsing and attaches it
        // to every resulting property/method/accessor/spread declaration.
        let docs = self.parse_leading_jsdoc();
        let member = self.parse_object_literal_element_worker();
        self.attach_jsdoc(member.into(), docs);
        member
    }

    fn parse_object_literal_element_worker(&mut self) -> ObjectLiteralElementLike<'a> {
        let start = self.pos();

        if self.at(SyntaxKind::DotDotDotToken) {
            self.next_token();
            let expression = self.parse_assignment_expression();
            let node = self.finish_node(
                SpreadAssignment::new(Some(expression)),
                SyntaxKind::SpreadAssignment,
                start,
            );
            return ObjectLiteralElementLike::SpreadAssignment(node);
        }

        // `parseModifiersEx(allowDecorators: true)`: modifiers and decorators
        // are parsed on any member and rejected by the grammar checker
        // (TS1042, TS1206).
        let modifiers = self.parse_modifiers_ex(false, false);

        if matches!(self.token.kind, SyntaxKind::GetKeyword | SyntaxKind::SetKeyword)
            && self.next_starts_property_name()
        {
            let is_getter = self.at(SyntaxKind::GetKeyword);
            self.next_token();
            let name = self.parse_property_name();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            // Pinned `parseFunctionBlockOrSemicolon`/`parseBlock`: semicolon
            // or ASI means no body; a missing `{` instead owns an empty Block.
            // Neither recovery may consume the next object member as a body.
            let body = if self.at(SyntaxKind::OpenBraceToken) {
                Some(FunctionBody::Block(self.parse_block()))
            } else if self.can_parse_semicolon() {
                self.parse_semicolon();
                None
            } else {
                self.expect(SyntaxKind::OpenBraceToken);
                Some(FunctionBody::Block(self.finish_node(
                    Block::new(&[], false),
                    SyntaxKind::Block,
                    self.pos(),
                )))
            };
            let modifiers = self.arena.alloc_slice(&modifiers);
            let parameters = self.arena.alloc_slice(&parameters);
            return if is_getter {
                ObjectLiteralElementLike::GetAccessorDeclaration(self.finish_node(
                    GetAccessorDeclaration::new(
                        modifiers,
                        name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        body,
                        None,
                        None,
                    ),
                    SyntaxKind::GetAccessor,
                    start,
                ))
            } else {
                ObjectLiteralElementLike::SetAccessorDeclaration(self.finish_node(
                    SetAccessorDeclaration::new(
                        modifiers,
                        name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        body,
                        None,
                        None,
                    ),
                    SyntaxKind::SetAccessor,
                    start,
                ))
            };
        }

        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };

        // §720: upstream captures `tokenIsIdentifier := p.isIdentifier()`
        // BEFORE `parsePropertyName` (`parser.go:5642`) and keys the shorthand
        // test on it. A RESERVED WORD is a valid property NAME but not a valid
        // identifier, so `var v = { class };` is NOT a shorthand — upstream
        // falls through to `parseExpected(':')` and parses a MISSING
        // initializer, which the baseline records as a trailing `> : any`
        // (`parserShorthandPropertyAssignment2`,
        // `parserErrorRecovery_ObjectLiteral2`/`4`/`5`).
        //
        // Testing the PARSED name instead — as this port did — cannot see the
        // difference, because `parse_property_name` accepts every keyword and
        // hands back an `Identifier`.
        // TEMPLATE literals are excluded from this strictness. Upstream does not
        // reach the shorthand test for them at all: `var x = { `a`: 321 }` is
        // parsed as an empty object literal TAGGED by the template (§683's
        // instrumented finding, `KindTaggedTemplateExpression`), a divergence
        // this port does not reproduce. Applying the identifier test to them
        // changed their line counts and cost 6 cases with no gain; keeping them
        // on the old path leaves §683's separate defect exactly as it was.
        let token_is_identifier = self.is_binding_identifier()
            || matches!(
                self.token.kind,
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead
            );
        let name = self.parse_property_name();

        // Optional and definite markers are not supported on property
        // assignments and are reported by the grammar checker; upstream parses
        // them here for recovery. §405.
        let postfix = if self.at(SyntaxKind::QuestionToken) || self.at(SyntaxKind::ExclamationToken)
        {
            Some(self.take_token())
        } else {
            None
        };

        // `{ m() {} }`, `{ m<T>() {} }` and `{ *m() {} }` are methods.
        //
        // Upstream's `parseMethodDeclaration` ends in
        // `parseFunctionBlockOrSemicolon`, so `{ foo(); }` has no body, and
        // `checkGrammarMethod` reports the `'{' expected` at the `;`. That
        // grammar arm is not ported, and without it a bodiless member reads
        // as a missing implementation (TS2391); until it is, the body is
        // parsed as a block, whose missing `{` reports the same error.
        if asterisk.is_some()
            || self.at(SyntaxKind::OpenParenToken)
            || self.at(SyntaxKind::LessThanToken)
        {
            let type_parameters = self.parse_type_parameters();
            // `{ async m() { await x } }` — an object-literal method's await
            // context is its own, exactly as a class method's is. §193.
            let is_async = Self::is_async(&modifiers);
            let (parameters, return_type, body) = self.with_await_context(is_async, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                let body = FunctionBody::Block(parser.parse_block());
                (parameters, return_type, Some(body))
            });
            let modifiers = self.arena.alloc_slice(&modifiers);
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            return ObjectLiteralElementLike::MethodDeclaration(self.finish_node(
                MethodDeclaration::new(
                    modifiers,
                    asterisk,
                    name,
                    postfix,
                    type_parameters,
                    parameters,
                    return_type,
                    None,
                    body,
                ),
                SyntaxKind::MethodDeclaration,
                start,
            ));
        }

        let modifiers = self.arena.alloc_slice(&modifiers);
        if self.eat(SyntaxKind::ColonToken) {
            let initializer = self.parse_assignment_expression();
            let node = self.finish_node(
                PropertyAssignment::new(modifiers, name, postfix, None, Some(initializer)),
                SyntaxKind::PropertyAssignment,
                start,
            );
            return ObjectLiteralElementLike::PropertyAssignment(node);
        }

        // **The shorthand form needs an identifier.** Upstream's
        // `isShorthandPropertyAssignment` is `tokenIsIdentifier && token != ':'`,
        // so `{ [e] }` is not a shorthand — it reaches `parseExpected(':')` and
        // reports there. A computed name has no shorthand spelling, because the
        // shorthand *is* the identifier.
        // `docs/architecture/checker-notes-diag2.md` §576.
        if !token_is_identifier || !matches!(name, tsr_ast::PropertyName::Identifier(_)) {
            self.expect(SyntaxKind::ColonToken);
            let initializer = self.parse_assignment_expression();
            let node = self.finish_node(
                PropertyAssignment::new(modifiers, name, postfix, None, Some(initializer)),
                SyntaxKind::PropertyAssignment,
                start,
            );
            return ObjectLiteralElementLike::PropertyAssignment(node);
        }

        // `{ a }` and `{ a = 1 }` (the latter only valid as a destructuring
        // target, which the checker enforces).
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        let node = self.finish_node(
            ShorthandPropertyAssignment::new(modifiers, name, postfix, None, None, initializer),
            SyntaxKind::ShorthandPropertyAssignment,
            start,
        );
        ObjectLiteralElementLike::ShorthandPropertyAssignment(node)
    }

    /// Arrow functions, when the lookahead confirms one.
    ///
    /// `(a)` is a parenthesised expression and `(a) => a` is an arrow function;
    /// they diverge only at the `=>`. Rather than encode a lookahead predicate for
    /// every parameter-list shape, this speculatively parses a parameter list and
    /// rewinds if no arrow follows.
    fn try_parse_arrow_function(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Expression<'a>> {
        // **Before the `async`.** Upstream takes `pos := p.nodePos()` at the top
        // of `parseParenthesizedArrowFunctionExpression` and
        // `parseSimpleArrowFunctionExpression` (`parser.go:4541`), so the node's
        // span COVERS its modifier. This port took it after `take_token()`, and
        // the `.types` walker prints a node's source text from its span — so
        // `async (): Promise<void> => {}` rendered as
        // `(): Promise<void> => {}`: the right type under the wrong
        // expression, which fails the line exactly as a wrong type does. §205.
        let modifier_start = self.pos();
        // `jsdocScannerInfo` before the `async` (`parser.go:4341`,
        // `:4505`): the comment is parsed by `withJSDoc` only once the
        // arrow is finished.
        let jsdoc = self.leading_jsdoc_marker();
        // `async` prefixes an arrow but is also an ordinary identifier, so it is
        // only consumed once the arrow is confirmed.
        let async_modifier = if self.at(SyntaxKind::AsyncKeyword) && self.async_starts_arrow() {
            Some(self.take_token())
        } else {
            None
        };

        // A bare `x => …` needs no speculation. The name may be a contextual
        // keyword — `async => async` names its parameter `async`.
        // `isIdentifier`: `await` names no parameter inside an await
        // context (`isSimpleArrowFunction` / `parseSimpleArrowFunctionExpression`).
        if self.is_identifier() {
            let saved_start = modifier_start;
            let parsed = self.try_parse(|p| {
                let parameter_start = p.pos();
                let name = p.parse_identifier();
                if !p.at(SyntaxKind::EqualsGreaterThanToken) {
                    return None;
                }
                let parameter = p.finish_node(
                    ParameterDeclaration::new(
                        &[],
                        None,
                        Some(BindingName::Identifier(name)),
                        None,
                        None,
                        None,
                    ),
                    SyntaxKind::Parameter,
                    parameter_start,
                );
                Some(parameter)
            })?;
            let arrow = self.take_token();
            // `parseArrowFunctionExpressionBody` sets the await context from
            // `isAsync` (`parser.go:4484`) — the *body*'s context, which is why
            // it is entered after the `=>` rather than around the parameter.
            let body = self.with_await_context(async_modifier.is_some(), |p| {
                p.parse_arrow_body(allow_return_type_in_arrow_function)
            });
            let parameters = self.arena.alloc_slice(&[parsed]);
            let modifiers = modifier_slice(self.arena, async_modifier);
            let node = self.finish_node(
                ArrowFunction::new(
                    modifiers,
                    &[],
                    parameters,
                    None,
                    None,
                    Some(arrow),
                    Some(body),
                    None,
                ),
                SyntaxKind::ArrowFunction,
                saved_start,
            );
            let docs = self.parse_jsdoc_at(jsdoc);
            self.attach_jsdoc(tsr_ast::Node::ArrowFunction(node), docs);
            return Some(Expression::ArrowFunction(node));
        }

        if !self.at(SyntaxKind::OpenParenToken) && !self.at(SyntaxKind::LessThanToken) {
            return None;
        }
        // `isParenthesizedArrowFunctionExpression`: a definite answer is
        // upstream's; an ambiguous one is decided with a token-only scan
        // before committing to a real parse. See `is_arrow_function_ahead_given`
        // for why speculating directly is catastrophic.
        let tristate = self.look_ahead(Self::next_is_parenthesized_arrow_function_expression);
        if !self.is_arrow_function_ahead_given(tristate) {
            return None;
        }

        // `parseParenthesizedArrowFunctionExpression(allowAmbiguity)`: only a
        // definite arrow may have a parameter list that is not one — an
        // ambiguous one is abandoned (and the group reparsed as an
        // expression) at a parameter that starts with no parameter name.
        let allow_ambiguity = tristate == Some(true);
        // `tryParseParenthesizedArrowFunctionExpression` (`parser.go:4320`)
        // passes the caller's `allowReturnTypeInArrowFunction` only to the
        // ambiguous parse; a definite arrow always allows a return type. When
        // the ambiguous parse may still be refused after its body, the whole
        // parse is speculative. An `async` already consumed cannot be
        // rewound here, so an `async` arrow keeps its return type.
        let allow_return_type =
            allow_ambiguity || allow_return_type_in_arrow_function || async_modifier.is_some();
        let arrow = if allow_return_type {
            self.parse_parenthesized_arrow_function(
                modifier_start,
                async_modifier,
                allow_ambiguity,
                true,
            )
        } else {
            self.try_parse(|p| {
                p.parse_parenthesized_arrow_function(modifier_start, None, allow_ambiguity, false)
            })
        };
        // `parseParenthesizedArrowFunctionExpression`'s `withJSDoc`
        // (`parser.go:4434`), on the arrow that survived.
        if let Some(Expression::ArrowFunction(node)) = arrow {
            let docs = self.parse_jsdoc_at(jsdoc);
            self.attach_jsdoc(tsr_ast::Node::ArrowFunction(node), docs);
        }
        arrow
    }

    /// typescript-go's `Parser.parseParenthesizedArrowFunctionExpression`
    /// (`parser.go:4341`) from its type parameters on; `None` is upstream's
    /// `nil` (rewind).
    fn parse_parenthesized_arrow_function(
        &mut self,
        start: u32,
        async_modifier: Option<&'a Token<'a>>,
        allow_ambiguity: bool,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Expression<'a>> {
        let is_async = async_modifier.is_some();
        let signature = self.try_parse(|parser| {
            let type_parameters = parser.parse_type_parameters();
            // Parameters take the signature's await context (`parser.go:3299`),
            // the body the same one (`:4484`); the `=>` between them is
            // neither's.
            parser.with_await_context(is_async, |parser| {
                let parameters = if allow_ambiguity {
                    parser.parse_parameter_list()
                } else {
                    parser.parse_unambiguous_parameter_list()?
                };
                // A return type may intervene: `(a): number => a`.
                Some((type_parameters, parameters, parser.parse_return_type_annotation()))
            })
        });
        let (type_parameters, parameters, return_type) = signature?;
        // A definite arrow may be missing its `=>` (`() :void {}`): report it,
        // and parse a body only after `=>` or `{`.
        let last_token = self.token.kind;
        let arrow = if self.at(SyntaxKind::EqualsGreaterThanToken) {
            self.take_token()
        } else {
            self.error_at_current_with(&messages::_0_EXPECTED, &["=>"]);
            self.alloc_token(SyntaxKind::EqualsGreaterThanToken, Span::at(self.node_end()))
        };
        let body = if matches!(
            last_token,
            SyntaxKind::EqualsGreaterThanToken | SyntaxKind::OpenBraceToken
        ) {
            self.with_await_context(is_async, |p| {
                p.parse_arrow_body(allow_return_type_in_arrow_function)
            })
        } else {
            ConciseBody::from(Expression::Identifier(self.parse_identifier()))
        };
        // Given `x ? y => ({ y }) : z => ({ z })`, the first arrow's body
        // `({ y }) : z => ({ z })` is an arrow with return type `z`; on the
        // true side of a conditional that colon ends the branch, unless the
        // arrow is followed by yet another colon (`a ? (x): string => x : null`).
        if !allow_return_type_in_arrow_function
            && return_type.is_some()
            && !self.at(SyntaxKind::ColonToken)
        {
            return None;
        }
        let parameters = self.arena.alloc_slice(&parameters);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let modifiers = modifier_slice(self.arena, async_modifier);
        let node = self.finish_node(
            ArrowFunction::new(
                modifiers,
                type_parameters,
                parameters,
                return_type,
                None,
                Some(arrow),
                Some(body),
                None,
            ),
            SyntaxKind::ArrowFunction,
            start,
        );
        Some(Expression::ArrowFunction(node))
    }

    /// Pinned `canFollowTypeArgumentsInExpression` (`parser.go:5270`): binary
    /// operators and non-expression starters favor instantiation, but `<`, `>`,
    /// unary `+` and `-` never do, even after a line break. Keep the `NoIn` context.
    fn at_instantiation_terminator(&mut self) -> bool {
        match self.token.kind {
            SyntaxKind::OpenParenToken
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead => true,
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            // Native Scan returns a single `>`; it packs shifts only through
            // reScanGreaterThanToken. Our scanner packs them eagerly. A `>`
            // still following the speculative close must therefore reject
            // instantiation too (`0 < u >>> 0` is a shift/comparison, not `0<u>`).
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken => false,
            kind => {
                self.token.has_preceding_line_break()
                    || (binary_precedence(kind).is_some()
                        && !(self.no_in != 0 && kind == SyntaxKind::InKeyword))
                    || !self.is_start_of_expression()
            }
        }
    }

    /// Whether `async` here prefixes an arrow rather than naming something.
    fn next_is_function_keyword(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::FunctionKeyword)
    }

    fn async_starts_arrow(&mut self) -> bool {
        let mut matched = false;
        self.try_parse(|p| {
            // A line break ends the statement: `async\nx => y` is two things.
            p.next_token();
            if p.token.has_preceding_line_break() {
                return None::<()>;
            }
            matched = match p.token.kind {
                SyntaxKind::Identifier => p.peek_kind(|k| k == SyntaxKind::EqualsGreaterThanToken),
                SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken => {
                    // The same decision `try_parse_arrow_function` makes after
                    // the `async`, including an ambiguous list's abandonment,
                    // so the modifier is never consumed for a non-arrow.
                    let tristate =
                        p.look_ahead(Self::next_is_parenthesized_arrow_function_expression);
                    p.is_arrow_function_ahead_given(tristate)
                        && (tristate == Some(true)
                            || p.look_ahead(|p| {
                                p.parse_type_parameters();
                                p.with_await_context(true, |p| {
                                    p.parse_unambiguous_parameter_list().is_some()
                                })
                            }))
                }
                _ => false,
            };
            None
        });
        matched
    }

    /// Whether the parenthesised group at the cursor is an arrow function's
    /// parameter list.
    ///
    /// `(a)` and `(a) => a` are identical up to the `=>`, so the parser has to
    /// look ahead. The obvious implementation — speculatively parse a parameter
    /// list and rewind — is **exponential**: a parameter's initializer is parsed
    /// with `parse_assignment_expression`, which speculates again, so nested
    /// assignments like `E = (E = (E = …))` re-parse the whole tail once per
    /// level. The corpus contains exactly that shape
    /// (`parsingDeepParenthensizedExpression.ts`), and it took the parser from
    /// milliseconds to unbounded memory.
    ///
    /// This scan only moves the token cursor: no nodes are built and no
    /// expression parser is re-entered, so it is linear in the group's length.
    /// TypeScript resolves the same ambiguity the same way.
    ///
    /// Upstream's tristate comes first: `Some(answer)` is definite, `None`
    /// is decided by the scan.
    fn is_arrow_function_ahead_given(&mut self, tristate: Option<bool>) -> bool {
        if let Some(answer) = tristate {
            return answer;
        }
        self.scan_for_arrow_function()
    }

    /// typescript-go's `Parser.nextIsParenthesizedArrowFunctionExpression`
    /// (`parser.go`), from the `(` or `<` (the `async` is already consumed):
    /// `Some(true)`/`Some(false)` are its definite answers, `None` its
    /// `TSUnknown`.
    fn next_is_parenthesized_arrow_function_expression(&mut self) -> Option<bool> {
        let first = self.token.kind;
        self.next_token();
        let second = self.token.kind;
        if first == SyntaxKind::OpenParenToken {
            if second == SyntaxKind::CloseParenToken {
                // `() =>`, `():` and `() {` — the last is not an arrow
                // function, but probably what the user intended.
                self.next_token();
                return Some(matches!(
                    self.token.kind,
                    SyntaxKind::EqualsGreaterThanToken
                        | SyntaxKind::ColonToken
                        | SyntaxKind::OpenBraceToken
                ));
            }
            // `([` or `({` could start a binding pattern.
            if matches!(second, SyntaxKind::OpenBracketToken | SyntaxKind::OpenBraceToken) {
                return None;
            }
            // `(...` is a rest parameter.
            if second == SyntaxKind::DotDotDotToken {
                return Some(true);
            }
            // `(xxx yyy` with xxx a modifier: not allowed, but treated as a
            // lambda for a good error message.
            if second.is_modifier()
                && second != SyntaxKind::AsyncKeyword
                && self.look_ahead(|p| {
                    p.next_token();
                    p.is_identifier()
                })
            {
                self.next_token();
                return Some(!self.at(SyntaxKind::AsKeyword));
            }
            // `(` followed by something that is not an identifier is not a
            // lambda; `this` is parsed and given a semantic error.
            if !self.is_identifier() && second != SyntaxKind::ThisKeyword {
                return Some(false);
            }
            self.next_token();
            return match self.token.kind {
                // `(a:` — a type-annotated parameter.
                SyntaxKind::ColonToken => Some(true),
                // `(a?:`, `(a?,`, `(a?=`, `(a?)` — definitely; otherwise
                // definitely not.
                SyntaxKind::QuestionToken => {
                    self.next_token();
                    Some(matches!(
                        self.token.kind,
                        SyntaxKind::ColonToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::EqualsToken
                            | SyntaxKind::CloseParenToken
                    ))
                }
                // `(a,`, `(a=` or `(a)` could be an arrow function.
                SyntaxKind::CommaToken | SyntaxKind::EqualsToken | SyntaxKind::CloseParenToken => {
                    None
                }
                _ => Some(false),
            };
        }
        // `<` not followed by an identifier is not an arrow function.
        if !self.is_identifier() && second != SyntaxKind::ConstKeyword {
            return Some(false);
        }
        if self.script_kind.allows_jsx() {
            return Some(self.look_ahead(|p| {
                p.eat(SyntaxKind::ConstKeyword);
                p.next_token();
                match p.token.kind {
                    SyntaxKind::ExtendsKeyword => {
                        p.next_token();
                        !matches!(
                            p.token.kind,
                            SyntaxKind::EqualsToken
                                | SyntaxKind::GreaterThanToken
                                | SyntaxKind::SlashToken
                        )
                    }
                    SyntaxKind::CommaToken | SyntaxKind::EqualsToken => true,
                    _ => false,
                }
            }));
        }
        None
    }

    /// `parseParametersWorker(allowAmbiguity: false)` between parentheses:
    /// `None` when a parameter, past its modifiers and `...`, does not start
    /// with a parameter name (`isParameterNameStart`) or the `)` is missing —
    /// upstream's `nil` that abandons a speculative arrow function.
    fn parse_unambiguous_parameter_list(&mut self) -> Option<Vec<&'a ParameterDeclaration<'a>>> {
        if !self.expect(SyntaxKind::OpenParenToken) {
            return None;
        }
        let mut failed = false;
        let (parameters, _) = self.parse_delimited_list(ParsingContext::Parameters, |parser| {
            if !failed && !parser.look_ahead(Self::parameter_name_starts_after_prefix) {
                failed = true;
            }
            parser.parse_parameter()
        });
        if failed || !self.expect(SyntaxKind::CloseParenToken) {
            return None;
        }
        Some(parameters)
    }

    /// `parseParameterEx`'s `!allowAmbiguity` test: past modifiers, a `this`
    /// parameter is fine; otherwise past an optional `...` the token must be
    /// `isParameterNameStart`.
    fn parameter_name_starts_after_prefix(&mut self) -> bool {
        self.parse_modifiers();
        if self.at(SyntaxKind::ThisKeyword) {
            return true;
        }
        self.eat(SyntaxKind::DotDotDotToken);
        self.is_binding_identifier()
            || self.at(SyntaxKind::OpenBracketToken)
            || self.at(SyntaxKind::OpenBraceToken)
    }

    /// The token-only scan behind an ambiguous [`Self::is_arrow_function_ahead_given`].
    fn scan_for_arrow_function(&mut self) -> bool {
        let mut result = false;
        self.try_parse(|p| {
            // A generic arrow opens with type parameters: `<T>(a: T) => T`.
            if p.at(SyntaxKind::LessThanToken) && !p.skip_balanced(SyntaxKind::LessThanToken) {
                return None;
            }
            if !p.at(SyntaxKind::OpenParenToken) || !p.skip_balanced(SyntaxKind::OpenParenToken) {
                return None;
            }

            result = match p.token.kind {
                // `(a) => …`
                SyntaxKind::EqualsGreaterThanToken => true,
                // `(a): T => …`. Only a return type may sit between, so stop at
                // anything that would end the expression — otherwise a later,
                // unrelated `=>` would be mistaken for this one's.
                //
                // Depth-tracked, because a return type may itself contain both
                // brackets and arrows: in `(): (() => T) => null` the inner `)`
                // must not end the scan and the inner `=>` must not satisfy it.
                SyntaxKind::ColonToken => {
                    // Everything after `:` is a type, so `<` is always a type
                    // argument list here — and its commas must not end the scan:
                    // `(): Iterable<number, any> => …` is an arrow.
                    let mut depth = 0u32;
                    loop {
                        let kind = p.token.kind;
                        let closes = greater_than_count(kind);
                        match kind {
                            SyntaxKind::OpenParenToken
                            | SyntaxKind::OpenBracketToken
                            | SyntaxKind::OpenBraceToken
                            | SyntaxKind::LessThanToken => depth += 1,
                            SyntaxKind::EqualsGreaterThanToken if depth == 0 => break true,
                            SyntaxKind::CloseParenToken
                            | SyntaxKind::CloseBracketToken
                            | SyntaxKind::CloseBraceToken => {
                                if depth == 0 {
                                    break false;
                                }
                                depth -= 1;
                            }
                            _ if closes > 0 => depth = depth.saturating_sub(closes),
                            SyntaxKind::SemicolonToken | SyntaxKind::CommaToken if depth == 0 => {
                                break false;
                            }
                            SyntaxKind::EndOfFile => break false,
                            _ => {}
                        }
                        p.next_token();
                    }
                }
                _ => false,
            };
            // Always rewind: this is a lookahead, not a parse.
            None::<()>
        });
        result
    }

    /// Consume a bracketed group, leaving the cursor just past its close.
    ///
    /// Returns `false` if the group is unterminated, in which case the cursor is
    /// left at end of file.
    pub(crate) fn skip_balanced(&mut self, open: SyntaxKind) -> bool {
        debug_assert!(self.at(open));
        let close = match open {
            SyntaxKind::OpenParenToken => SyntaxKind::CloseParenToken,
            SyntaxKind::OpenBracketToken => SyntaxKind::CloseBracketToken,
            SyntaxKind::OpenBraceToken => SyntaxKind::CloseBraceToken,
            SyntaxKind::LessThanToken => SyntaxKind::GreaterThanToken,
            _ => return false,
        };

        let mut depth = 0u32;
        let mut brace_depth = 0u32;
        loop {
            let kind = self.token.kind;
            if kind == SyntaxKind::EndOfFile {
                return false;
            }
            // `<K extends Key<U>>` ends in a single `>>` token: the scanner has no
            // idea those are two closing brackets. Counting it as one leaves the
            // group unbalanced and the whole construct unrecognised.
            let closes = if open == SyntaxKind::LessThanToken {
                greater_than_count(kind)
            } else {
                u32::from(kind == close)
            };

            if kind == open {
                depth += 1;
            } else if open == SyntaxKind::LessThanToken && kind == SyntaxKind::OpenBraceToken {
                brace_depth += 1;
            } else if open == SyntaxKind::LessThanToken
                && kind == SyntaxKind::CloseBraceToken
                && brace_depth > 0
            {
                brace_depth -= 1;
            } else if closes > 0 {
                if closes >= depth {
                    self.next_token();
                    return true;
                }
                depth -= closes;
            } else if open == SyntaxKind::LessThanToken
                && kind == SyntaxKind::SemicolonToken
                && brace_depth == 0
            {
                // `<` is also a comparison operator; a statement boundary means
                // this was never a type-parameter list. An opening brace is not
                // such a boundary: object constraints make
                // `<T extends { key: value }>(x: T) => x` a generic arrow.
                return false;
            }
            self.next_token();
        }
    }

    /// typescript-go's `Parser.parseArrowFunctionExpressionBody` (`parser.go`).
    fn parse_arrow_body(&mut self, allow_return_type_in_arrow_function: bool) -> ConciseBody<'a> {
        if self.at(SyntaxKind::OpenBraceToken) {
            return ConciseBody::Block(self.parse_block());
        }
        // A plain statement (no expression statement, no function or class)
        // where a body belongs: the user probably left out the `{`, as in
        // `a =>⏎ let v = 0; }`. Parse a block so the next `}` does not close
        // the containing construct early.
        if !matches!(
            self.token.kind,
            SyntaxKind::SemicolonToken | SyntaxKind::FunctionKeyword | SyntaxKind::ClassKeyword
        ) && self.is_start_of_statement()
            && !self.is_start_of_expression_statement()
        {
            return ConciseBody::Block(self.parse_block_ex(true, None));
        }
        ConciseBody::from(
            self.parse_assignment_expression_worker(allow_return_type_in_arrow_function),
        )
    }

    /// typescript-go's `Parser.isStartOfExpressionStatement` (`parser.go`):
    /// none of `{`, `function`, `class` or `@` can start an expression
    /// statement.
    fn is_start_of_expression_statement(&mut self) -> bool {
        !matches!(
            self.token.kind,
            SyntaxKind::OpenBraceToken
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::AtToken
        ) && self.is_start_of_expression()
    }

    /// Parse `` `a${x}b` ``.
    ///
    /// The scanner cannot know where a substitution ends: the `}` closing it is
    /// lexically a close-brace, and only the parser's bracket tracking
    /// distinguishes the two. So each span is driven explicitly, re-scanning the
    /// `}` as template text via [`Parser::rescan_template_continuation`].
    /// `parseTemplateExpression(isTaggedTemplate)` (`parser.go`).
    ///
    /// The scanner's first pass over a template is **silent** about invalid
    /// escapes (`scanner.go:522`); every such diagnostic comes from
    /// `ReScanTemplateToken(!isTaggedTemplate)`, and a **tagged** template is
    /// permitted to contain them — the ES2018 revision, where the cooked value
    /// is `undefined` and the tag receives the raw text.
    ///
    /// This port reported eagerly and never re-scanned, so `` tag`\u` `` was
    /// four diagnostics upstream has none of. `templateLiteralEscapeSequence`
    /// invents thirty-two lines that way. §223.
    fn parse_template_literal(&mut self, is_tagged: bool) -> TemplateLiteral<'a> {
        self.token = self.scanner.rescan_template(is_tagged);
        if self.at(SyntaxKind::NoSubstitutionTemplateLiteral) {
            let start = self.pos();
            let raw = self.token_text();
            let (text, flags) = self.take_literal();
            return TemplateLiteral::NoSubstitutionTemplateLiteral(self.finish_node(
                NoSubstitutionTemplateLiteral::new(text, flags, flags, raw),
                SyntaxKind::NoSubstitutionTemplateLiteral,
                start,
            ));
        }
        match self.parse_template_expression() {
            Expression::TemplateExpression(template) => {
                TemplateLiteral::TemplateExpression(template)
            }
            _ => unreachable!("a template head yields a template expression"),
        }
    }

    fn parse_template_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        let head_start = self.pos();
        let raw = self.token_text();
        let (text, flags) = self.take_literal();
        let head = self.finish_node(
            TemplateHead::new(text, raw, flags, flags),
            SyntaxKind::TemplateHead,
            head_start,
        );

        let mut spans = Vec::new();
        loop {
            let span_start = self.pos();
            let expression = self.parse_expression();

            // The substitution must close with `}`; anything else means the
            // template is malformed and there is no continuation to re-scan.
            if !self.at(SyntaxKind::CloseBraceToken) {
                self.error_at_current_with(&messages::_0_EXPECTED, &["}"]);
                let literal_start = self.pos();
                let tail = self.finish_node(
                    TemplateTail::new("", "", flags, flags),
                    SyntaxKind::TemplateTail,
                    literal_start,
                );
                spans.push(self.finish_node(
                    TemplateSpan::new(
                        Some(expression),
                        Some(TemplateMiddleOrTail::TemplateTail(tail)),
                    ),
                    SyntaxKind::TemplateSpan,
                    span_start,
                ));
                break;
            }

            self.rescan_template_continuation();
            let literal_start = self.pos();
            let is_tail = self.at(SyntaxKind::TemplateTail);
            let raw = self.token_text();
            let (text, flags) = self.take_literal();
            let literal = if is_tail {
                TemplateMiddleOrTail::TemplateTail(self.finish_node(
                    TemplateTail::new(text, raw, flags, flags),
                    SyntaxKind::TemplateTail,
                    literal_start,
                ))
            } else {
                TemplateMiddleOrTail::TemplateMiddle(self.finish_node(
                    TemplateMiddle::new(text, raw, flags, flags),
                    SyntaxKind::TemplateMiddle,
                    literal_start,
                ))
            };

            spans.push(self.finish_node(
                TemplateSpan::new(Some(expression), Some(literal)),
                SyntaxKind::TemplateSpan,
                span_start,
            ));

            if is_tail {
                break;
            }
        }

        let spans = self.arena.alloc_slice(&spans);
        let node = self.finish_node(
            TemplateExpression::new(Some(head), spans),
            SyntaxKind::TemplateExpression,
            start,
        );
        Expression::TemplateExpression(node)
    }

    /// The expression after `@` in a decorator.
    ///
    /// Restricted to a call/member chain: parsing a full expression would let a
    /// following `class` or member be swallowed as an operand.
    pub(crate) fn parse_decorator_expression(&mut self) -> LeftHandSideExpression<'a> {
        let start = self.pos();
        let mut expression = if self.at(SyntaxKind::OpenParenToken) {
            // `@(expr)` — the parenthesised form takes an arbitrary expression.
            self.next_token();
            let inner = self.parse_expression();
            self.expect(SyntaxKind::CloseParenToken);
            Expression::ParenthesizedExpression(self.finish_node(
                ParenthesizedExpression::new(Some(inner)),
                SyntaxKind::ParenthesizedExpression,
                start,
            ))
        } else {
            Expression::Identifier(self.parse_identifier())
        };
        loop {
            match self.token.kind {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let name = if self.right_side_of_dot_is_missing() {
                        self.report_missing_right_side_of_dot();
                        self.missing_identifier()
                    } else {
                        self.parse_identifier_name()
                    };
                    let node = self.finish_node(
                        PropertyAccessExpression::new(
                            Some(expression),
                            None,
                            Some(MemberName::Identifier(name)),
                        ),
                        SyntaxKind::PropertyAccessExpression,
                        start,
                    );
                    expression = Expression::PropertyAccessExpression(node);
                }
                SyntaxKind::OpenParenToken => {
                    let arguments = self.parse_arguments();
                    let arguments = self.arena.alloc_slice(&arguments);
                    let node = self.finish_node(
                        CallExpression::new(Some(expression), None, &[], arguments),
                        SyntaxKind::CallExpression,
                        start,
                    );
                    expression = Expression::CallExpression(node);
                }
                SyntaxKind::ExclamationToken if !self.token.has_preceding_line_break() => {
                    self.next_token();
                    let node = self.finish_node(
                        NonNullExpression::new(Some(expression)),
                        SyntaxKind::NonNullExpression,
                        start,
                    );
                    expression = Expression::NonNullExpression(node);
                }
                SyntaxKind::LessThanToken => {
                    let Some(type_arguments) =
                        self.try_parse(Parser::parse_type_arguments_for_call)
                    else {
                        break;
                    };
                    let type_arguments = self.arena.alloc_slice(&type_arguments);
                    if self.at(SyntaxKind::OpenParenToken) {
                        let arguments = self.parse_arguments();
                        let arguments = self.arena.alloc_slice(&arguments);
                        let node = self.finish_node(
                            CallExpression::new(Some(expression), None, type_arguments, arguments),
                            SyntaxKind::CallExpression,
                            start,
                        );
                        expression = Expression::CallExpression(node);
                    } else {
                        let node = self.finish_node(
                            ExpressionWithTypeArguments::new(Some(expression), type_arguments),
                            SyntaxKind::ExpressionWithTypeArguments,
                            start,
                        );
                        expression = Expression::ExpressionWithTypeArguments(node);
                    }
                }
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal(true);
                    // Type arguments belong to the tagged-template node, not
                    // to an `ExpressionWithTypeArguments` wrapper around its
                    // tag (`parseMemberExpressionRest` upstream).
                    let (tag, type_arguments) = match expression {
                        Expression::ExpressionWithTypeArguments(instantiation) => (
                            instantiation.expression.unwrap_or(expression),
                            instantiation.type_arguments,
                        ),
                        _ => (expression, &[] as &[TypeNode<'a>]),
                    };
                    let node = self.finish_node(
                        TaggedTemplateExpression::new(
                            Some(tag),
                            None,
                            type_arguments,
                            Some(template),
                        ),
                        SyntaxKind::TaggedTemplateExpression,
                        start,
                    );
                    expression = Expression::TaggedTemplateExpression(node);
                }
                // Deliberately not `[`: an unparenthesised decorator takes a
                // dotted name with an optional call, so in `@dec ["1"]() {}` the
                // brackets are the *member's* computed name. Use `@(a["b"])` for
                // element access.
                _ => break,
            }
        }
        LeftHandSideExpression::try_from(tsr_ast::Node::from(expression))
            .unwrap_or_else(|_| LeftHandSideExpression::Identifier(self.missing_identifier()))
    }

    /// `<T>expr` — the pre-`as` cast syntax.
    ///
    /// Only valid in `.ts`; in `.tsx` the same tokens open a JSX element. The
    /// parser does not yet distinguish the two, so this always wins — which is
    /// wrong for `.tsx` and is why JSX support needs the file's script kind
    /// threaded through.
    fn parse_type_assertion(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::LessThanToken);
        let type_node = self.parse_type();
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.rescan_greater_than();
        }
        self.expect(SyntaxKind::GreaterThanToken);
        let expression = self.parse_unary_expression();
        let node = self.finish_node(
            TypeAssertion::new(Some(type_node), Some(expression)),
            SyntaxKind::TypeAssertionExpression,
            start,
        );
        Expression::TypeAssertion(node)
    }

    /// `function [name][<T>](params) { … }` in expression position.
    fn parse_function_expression(
        &mut self,
        async_modifier: Option<&'a Token<'a>>,
        modifier_start: Option<u32>,
        jsdoc: Option<(u32, u32)>,
    ) -> Expression<'a> {
        // §205: `modifier_start` is the position of the `async` the caller
        // already consumed. Without it the node's span begins at `function`
        // and the `.types` walker prints the expression without its modifier.
        let start = modifier_start.unwrap_or_else(|| self.pos());
        self.expect(SyntaxKind::FunctionKeyword);
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        // A function expression's name is optional and scoped to itself.
        let name = if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            None
        } else {
            Some(self.parse_identifier())
        };
        let type_parameters = self.parse_type_parameters();
        // The signature's own await context — see `parse_function_declaration`.
        let is_async = async_modifier.is_some();
        let (parameters, return_type, body) = self.with_await_context(is_async, |parser| {
            let parameters = parser.parse_parameter_list();
            let return_type = parser.parse_return_type_annotation();
            (parameters, return_type, FunctionBody::Block(parser.parse_block()))
        });

        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let parameters = self.arena.alloc_slice(&parameters);
        let node = self.finish_node(
            FunctionExpression::new(
                modifier_slice(self.arena, async_modifier),
                asterisk,
                name,
                type_parameters,
                parameters,
                return_type,
                None,
                Some(body),
            ),
            SyntaxKind::FunctionExpression,
            start,
        );
        // `parseFunctionExpression`'s `withJSDoc` (`parser.go:5711`): the
        // comment is reparsed onto the function's own parameters.
        let docs = self.parse_jsdoc_at(jsdoc);
        self.attach_jsdoc(tsr_ast::Node::FunctionExpression(node), docs);
        Expression::FunctionExpression(node)
    }

    /// `class [name] { … }` in expression position.
    pub(crate) fn parse_class_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        // Reuse the declaration parser and re-wrap: the grammars are identical
        // apart from the name being optional, which it already handles.
        let Statement::ClassDeclaration(declaration) = self.parse_class_declaration(start, &[])
        else {
            unreachable!("parse_class_declaration always yields a class")
        };
        let node = self.finish_node(
            ClassExpression::new(
                declaration.modifiers,
                declaration.name,
                declaration.type_parameters,
                declaration.heritage_clauses,
                declaration.members,
            ),
            SyntaxKind::ClassExpression,
            start,
        );
        Expression::ClassExpression(node)
    }

    /// Consume a literal token, returning its decoded text and how it was written.
    fn take_literal(&mut self) -> (&'a str, tsr_ast::TokenFlags) {
        let text = self.token_value();
        let flags = self.token.ast_flags();
        self.next_token();
        (text, flags)
    }

    // ---- names ----------------------------------------------------------

    /// Whether the cursor is on a token [`Self::parse_identifier`] will accept.
    ///
    /// Upstream's `isBindingIdentifier` (`parser.go:6262`), which is
    /// deliberately *not* `isIdentifier` (`:6248`): the latter also rejects
    /// `yield`/`await` inside a yield or await context, and upstream's own
    /// comment says `let await` is allowed here and refused later by the
    /// binder. Extracted from `parse_identifier`'s own admissibility test so
    /// that a list loop can ask *before* parsing whether an element is there —
    /// asking afterwards is what manufactures a missing identifier (§191).
    pub(crate) fn is_binding_identifier(&self) -> bool {
        self.at(SyntaxKind::Identifier)
            || (self.token.kind.is_keyword()
                && (self.token.kind as u16) > (SyntaxKind::LAST_RESERVED_WORD as u16))
    }

    /// typescript-go's `Parser.isIdentifier` (`parser.go`): an identifier or
    /// a contextual keyword, except `await` inside an await context. The
    /// `yield`-in-generator half is unported with the yield context (§193).
    pub(crate) fn is_identifier(&self) -> bool {
        if self.at(SyntaxKind::Identifier) {
            return true;
        }
        if self.at(SyntaxKind::AwaitKeyword) && self.in_await_context {
            return false;
        }
        self.token.kind.is_keyword()
            && (self.token.kind as u16) > (SyntaxKind::LAST_RESERVED_WORD as u16)
    }

    /// Whether the `await` under the cursor opens an await *expression* rather
    /// than naming an identifier.
    ///
    /// Upstream's `isAwaitExpression` (`parser.go:5115`), transcribed: inside an
    /// await context it always does; outside one, only when the next token is
    /// an identifier, keyword or literal on the same line.
    ///
    /// Both halves are load-bearing and the corpus proves it in one pair of
    /// neighbouring cases. `asyncFunctionDeclaration3_es6` is
    /// `function f(await = await) {}` — **not** async, so the initialiser is a
    /// plain identifier and upstream records three assertions.
    /// `asyncFunctionDeclaration6_es6` is
    /// `async function foo(a = await) {}` — async, so the initialiser IS an
    /// await expression, over a missing operand, and upstream records **four**,
    /// the fourth with empty source text. A port with only the lookahead half
    /// gets the first right and the second wrong; a port with neither, as this
    /// one had, gets the first wrong and the second right. §193.
    fn is_await_expression(&mut self) -> bool {
        if self.in_await_context {
            return true;
        }
        self.look_ahead(|parser| {
            parser.next_token();
            // `nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine`
            // (`parser.go:4011`).
            !parser.token.has_preceding_line_break()
                && (parser.token.kind == SyntaxKind::Identifier
                    || parser.token.kind.is_keyword()
                    || matches!(
                        parser.token.kind,
                        SyntaxKind::NumericLiteral
                            | SyntaxKind::BigIntLiteral
                            | SyntaxKind::StringLiteral
                    ))
        })
    }

    /// Whether a left-hand-side expression can start at the cursor.
    ///
    /// Upstream's `isStartOfLeftHandSideExpression` (`parser.go:6167`). The one
    /// deviation is the fallback: upstream's is `isIdentifier`, which refuses
    /// `yield`/`await` inside a yield or await context, and this port has no
    /// yield context (§193), so it uses [`Self::is_binding_identifier`] —
    /// upstream's own context-free variant of the same test.
    pub(crate) fn is_start_of_left_hand_side_expression(&mut self) -> bool {
        match self.token.kind {
            SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
            | SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::NewKeyword
            | SyntaxKind::SlashToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::Identifier => true,
            // `isNextTokenOpenParenOrLessThanOrDot` (`parser.go:6225`) — a bare
            // `import` is a declaration, `import(` / `import<` / `import.` an
            // expression.
            SyntaxKind::ImportKeyword => self.peek_kind(|kind| {
                matches!(
                    kind,
                    SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken | SyntaxKind::DotToken
                )
            }),
            _ => self.is_binding_identifier(),
        }
    }

    /// Whether an expression can start at the cursor — typescript-go's
    /// `Parser.isStartOfExpression` (`parser.go`).
    pub(crate) fn is_start_of_expression(&mut self) -> bool {
        if self.is_start_of_left_hand_side_expression() {
            return true;
        }
        if matches!(
            self.token.kind,
            SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::TildeToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::PrivateIdentifier
                | SyntaxKind::AtToken
        ) {
            return true;
        }
        // Error tolerance. If we see the start of some binary operator, we
        // consider that the start of an expression. That way we'll parse out
        // a missing identifier, give a good message about an identifier being
        // missing, and then consume the rest of the binary expression.
        if self.is_binary_operator() {
            return true;
        }
        self.is_identifier()
    }

    /// typescript-go's `Parser.isBinaryOperator` (`parser.go`).
    fn is_binary_operator(&self) -> bool {
        if self.no_in > 0 && self.at(SyntaxKind::InKeyword) {
            return false;
        }
        binary_precedence(self.token.kind).is_some()
    }

    /// Whether a *binding* can start here: a pattern, a private name, or a
    /// binding identifier.
    ///
    /// Upstream's `isBindingIdentifierOrPrivateIdentifierOrPattern`
    /// (`parser.go:6221`), which is `isListElement`'s answer for
    /// `PCVariableDeclarations` (`:871`).
    pub(crate) fn is_binding_identifier_or_private_identifier_or_pattern(&self) -> bool {
        self.at(SyntaxKind::OpenBraceToken)
            || self.at(SyntaxKind::OpenBracketToken)
            || self.at(SyntaxKind::PrivateIdentifier)
            || self.is_binding_identifier()
    }

    /// Parse an identifier, synthesising one if the cursor is elsewhere.
    ///
    /// Ported from `Parser.parseIdentifier` via `isIdentifier`
    /// (`parser.go:6248`): a reserved word — `enum`, `class`, `while` — is NOT
    /// an identifier and errors, producing a missing one; a contextual keyword
    /// above `LastReservedWord` is. Positions upstream reads with
    /// `parseIdentifierName` — the right of a dot, property names — use
    /// [`Self::parse_identifier_name`], where every keyword is a name.
    /// `parseRightSideOfDot`'s ASI recovery (`parser.go:2940`).
    ///
    /// `a.` followed by a NEWLINE and then `keyword identifier` is not a
    /// property access with that keyword as the name — ASI would have ended the
    /// statement at the dot, so upstream mints a MISSING identifier and lets
    /// the next line parse as its own statement. §701.
    ///
    /// ```text
    /// var x = IgnoreRulesSpecific.
    /// var y = Position.IgnoreRulesSpecific;
    /// ```
    ///
    /// Upstream records `IgnoreRulesSpecific. : any` and a separate
    /// `var y = … : Position`; this port took `var` as the member name and
    /// emitted one line too many (`enumConflictsWithGlobalIdentifier`).
    /// `parseRightSideOfDot(allowIdentifierNames: false, …)`: the dangling-dot
    /// case, else a plain identifier (a reserved word is reported).
    pub(crate) fn parse_right_side_of_dot_identifier(&mut self) -> &'a Identifier<'a> {
        if self.right_side_of_dot_is_missing() {
            self.report_missing_right_side_of_dot();
            return self.missing_identifier();
        }
        self.parse_identifier()
    }

    /// `parseRightSideOfDot`'s report for that case: `parseErrorAt(p.nodePos(),
    /// p.nodePos(), Identifier_expected)` — right after the dot, at the next
    /// token's full start, "because the next token might actually be an
    /// identifier and the error would be quite confusing".
    pub(crate) fn report_missing_right_side_of_dot(&mut self) {
        self.error_at(&messages::IDENTIFIER_EXPECTED, Span::at(self.node_end()));
    }

    pub(crate) fn right_side_of_dot_is_missing(&mut self) -> bool {
        self.token.flags.contains(tsr_scanner::TokenFlags::PRECEDING_LINE_BREAK)
            && (self.token.kind == SyntaxKind::Identifier || self.token.kind.is_keyword())
            && self.look_ahead(Self::next_token_is_identifier_or_keyword_on_same_line)
    }

    /// `nextTokenIsIdentifierOrKeywordOnSameLine` (`parser.go`). §701.
    pub(crate) fn next_token_is_identifier_or_keyword_on_same_line(&mut self) -> bool {
        // `Parser::next_token` returns the token it CONSUMED, not the one it
        // moved to (`parser.rs:348`) — reading its return value here tested the
        // wrong token and the whole arm measured as a no-op. §701.
        self.next_token();
        !self.token.flags.contains(tsr_scanner::TokenFlags::PRECEDING_LINE_BREAK)
            && (self.token.kind == SyntaxKind::Identifier || self.token.kind.is_keyword())
    }

    pub(crate) fn parse_identifier(&mut self) -> &'a Identifier<'a> {
        let start = self.pos();
        if self.is_binding_identifier() {
            let text = self.token_value();
            self.next_token();
            return self.finish_node(Identifier::new(text), SyntaxKind::Identifier, start);
        }
        self.report_missing_identifier()
    }

    /// Parse an identifier name: any keyword qualifies — `a.class` is legal.
    ///
    /// Upstream's `parseIdentifierName` (`parser.go:6316`).
    pub(crate) fn parse_identifier_name(&mut self) -> &'a Identifier<'a> {
        let start = self.pos();
        if self.at(SyntaxKind::Identifier) || self.token.kind.is_keyword() {
            let text = self.token_value();
            self.next_token();
            return self.finish_node(Identifier::new(text), SyntaxKind::Identifier, start);
        }
        self.report_missing_identifier()
    }

    /// The no-message tail of typescript-go's
    /// `Parser.createIdentifierWithDiagnostic` (`parser.go`): report why the
    /// token is not an identifier and mint a missing one. A private name is
    /// reported and consumed as the identifier; at end of file the report sits
    /// zero-width at the token's full start.
    fn report_missing_identifier(&mut self) -> &'a Identifier<'a> {
        if self.at(SyntaxKind::PrivateIdentifier) {
            self.error_at_current(
                &messages::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
            let start = self.pos();
            let text = self.token_value();
            self.next_token();
            return self.finish_node(Identifier::new(text), SyntaxKind::Identifier, start);
        }
        // Only for end of file because the error gets reported incorrectly on
        // embedded script tags.
        let span = if self.at(SyntaxKind::EndOfFile) {
            Span::at(self.node_end())
        } else {
            self.token.span
        };
        if crate::statement::is_reserved_word(self.token.kind) {
            let text = self.token_text();
            if !self.would_repeat_last_error(span) {
                self.diagnostics.push(Diagnostic::with_args(
                    &messages::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                    span,
                    [text.to_string()],
                ));
            }
        } else {
            self.error_at(&messages::IDENTIFIER_EXPECTED, span);
        }
        self.missing_identifier()
    }

    fn parse_member_name(&mut self) -> MemberName<'a> {
        if self.at(SyntaxKind::PrivateIdentifier) {
            let start = self.pos();
            let text = self.private_identifier_text();
            self.next_token();
            let node = self.finish_node(
                PrivateIdentifier::new(text),
                SyntaxKind::PrivateIdentifier,
                start,
            );
            return MemberName::PrivateIdentifier(node);
        }
        MemberName::Identifier(self.parse_identifier_name())
    }

    /// Parse a property name: identifier, string, number, or `[computed]`.
    pub(crate) fn parse_property_name(&mut self) -> PropertyName<'a> {
        let start = self.pos();
        match self.token.kind {
            SyntaxKind::StringLiteral => {
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    StringLiteral::new(text, flags),
                    SyntaxKind::StringLiteral,
                    start,
                );
                PropertyName::StringLiteral(node)
            }
            SyntaxKind::NumericLiteral => {
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    NumericLiteral::new(text, flags),
                    SyntaxKind::NumericLiteral,
                    start,
                );
                PropertyName::NumericLiteral(node)
            }
            // `parsePropertyNameWorker`: a bigint literal names a property
            // too (and the checker reports TS1539).
            SyntaxKind::BigIntLiteral => {
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    BigIntLiteral::new(text, flags),
                    SyntaxKind::BigIntLiteral,
                    start,
                );
                PropertyName::BigIntLiteral(node)
            }
            SyntaxKind::OpenBracketToken => {
                self.next_token();
                // `parseComputedPropertyName` (`parser.go:3467`) parses any
                // expression, comma included, with `in` allowed; the grammar
                // checker reports a comma expression (TS1171).
                let saved_no_in = std::mem::take(&mut self.no_in);
                let expression = self.parse_expression();
                self.no_in = saved_no_in;
                self.expect(SyntaxKind::CloseBracketToken);
                let node = self.finish_node(
                    ComputedPropertyName::new(Some(expression)),
                    SyntaxKind::ComputedPropertyName,
                    start,
                );
                PropertyName::ComputedPropertyName(node)
            }
            SyntaxKind::PrivateIdentifier => {
                let text = self.private_identifier_text();
                self.next_token();
                let node = self.finish_node(
                    PrivateIdentifier::new(text),
                    SyntaxKind::PrivateIdentifier,
                    start,
                );
                PropertyName::PrivateIdentifier(node)
            }
            _ => PropertyName::Identifier(self.parse_identifier_name()),
        }
    }

    /// The scanner's decoded value for an escaped private name excludes the
    /// leading `#`, while the AST invariant (and ordinary unescaped token value)
    /// includes it.
    fn private_identifier_text(&self) -> &'a str {
        let text = self.token_value();
        if text.starts_with('#') { text } else { self.arena.alloc_str(&format!("#{text}")) }
    }

    /// Parse a binding name: an identifier or a destructuring pattern.
    pub(crate) fn parse_binding_name(&mut self) -> BindingName<'a> {
        match self.token.kind {
            SyntaxKind::OpenBracketToken => self.parse_array_binding_pattern(),
            SyntaxKind::OpenBraceToken => self.parse_object_binding_pattern(),
            _ => BindingName::Identifier(self.parse_identifier()),
        }
    }

    /// typescript-go's `Parser.parseArrayBindingPattern` (`parser.go`):
    /// `parseDelimitedList(PCArrayBindingElements, parseArrayBindingElement)`
    /// outside any disallow-in context.
    fn parse_array_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        let kind_token = self.alloc_token(SyntaxKind::OpenBracketToken, self.token.span);
        self.expect(SyntaxKind::OpenBracketToken);
        let saved_no_in = std::mem::take(&mut self.no_in);
        let (elements, has_trailing_comma) = self.parse_delimited_list(
            ParsingContext::ArrayBindingElements,
            Self::parse_array_binding_element,
        );
        self.no_in = saved_no_in;
        self.expect(SyntaxKind::CloseBracketToken);
        self.finish_binding_pattern(
            kind_token,
            &elements,
            has_trailing_comma,
            SyntaxKind::ArrayBindingPattern,
            start,
        )
    }

    /// typescript-go's `Parser.parseObjectBindingPattern` (`parser.go`):
    /// `parseDelimitedList(PCObjectBindingElements, parseObjectBindingElement)`
    /// outside any disallow-in context.
    fn parse_object_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        let kind_token = self.alloc_token(SyntaxKind::OpenBraceToken, self.token.span);
        self.expect(SyntaxKind::OpenBraceToken);
        let saved_no_in = std::mem::take(&mut self.no_in);
        let (elements, has_trailing_comma) = self.parse_delimited_list(
            ParsingContext::ObjectBindingElements,
            Self::parse_object_binding_element,
        );
        self.no_in = saved_no_in;
        self.expect(SyntaxKind::CloseBraceToken);
        self.finish_binding_pattern(
            kind_token,
            &elements,
            has_trailing_comma,
            SyntaxKind::ObjectBindingPattern,
            start,
        )
    }

    /// The pattern node, with the trailing comma recorded as a flag: upstream
    /// reads it off the list's span, which this AST does not keep.
    fn finish_binding_pattern(
        &mut self,
        kind_token: &'a tsr_ast::Token<'a>,
        elements: &[&'a BindingElement<'a>],
        has_trailing_comma: bool,
        kind: SyntaxKind,
        start: u32,
    ) -> BindingName<'a> {
        let elements = self.arena.alloc_slice(elements);
        let end = self.node_end();
        let flags = if has_trailing_comma {
            tsr_ast::NodeFlags::HAS_TRAILING_COMMA
        } else {
            tsr_ast::NodeFlags::empty()
        };
        let node = self.finish_node_with_flags(
            BindingPattern::new(kind_token, elements),
            kind,
            start,
            end,
            flags,
        );
        BindingName::BindingPattern(node)
    }

    /// typescript-go's `Parser.parseArrayBindingElement` (`parser.go`). A
    /// hole, `[, a]`, is a `BindingElement` whose fields are all nil —
    /// "These are all nil for a missing element" — and
    /// `getBindingElementTypeFromParentType` counts it in
    /// `slices.Index(pattern.Elements(), declaration)` (`checker.go:17750`),
    /// so it must stay in the list (`bd tsr-o00`).
    fn parse_array_binding_element(&mut self) -> &'a BindingElement<'a> {
        let start = self.pos();
        if self.at(SyntaxKind::CommaToken) {
            return self.finish_node(
                tsr_ast::BindingElement::new(None, None, None, None),
                SyntaxKind::BindingElement,
                start,
            );
        }
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        let name = self.parse_binding_name();
        let initializer = self.parse_binding_initializer();
        self.finish_node(
            BindingElement::new(dot_dot_dot, None, Some(name), initializer),
            SyntaxKind::BindingElement,
            start,
        )
    }

    /// typescript-go's `Parser.parseObjectBindingElement` (`parser.go`).
    ///
    /// `tokenIsIdentifier` is read **before** the property name is parsed:
    /// a reserved word (`{ while }`) or a literal name takes the `:` branch
    /// and reports a single `':' expected` (§202), while a binding
    /// identifier not followed by `:` is a shorthand.
    fn parse_object_binding_element(&mut self) -> &'a BindingElement<'a> {
        let start = self.pos();
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        let token_is_identifier = self.is_binding_identifier();
        let property_name = self.parse_property_name();
        let (property_name, name) = match property_name {
            PropertyName::Identifier(id)
                if token_is_identifier && !self.at(SyntaxKind::ColonToken) =>
            {
                (None, BindingName::Identifier(id))
            }
            property => {
                self.expect(SyntaxKind::ColonToken);
                (Some(property), self.parse_binding_name())
            }
        };
        let initializer = self.parse_binding_initializer();
        self.finish_node(
            BindingElement::new(dot_dot_dot, property_name, Some(name), initializer),
            SyntaxKind::BindingElement,
            start,
        )
    }

    /// `parseInitializer` for a binding element.
    fn parse_binding_initializer(&mut self) -> Option<Expression<'a>> {
        self.eat(SyntaxKind::EqualsToken).then(|| self.parse_assignment_expression())
    }

    /// Parse a parenthesised parameter list.
    pub(crate) fn parse_parameter_list(&mut self) -> Vec<&'a ParameterDeclaration<'a>> {
        // §238: **no `(`, no parameters.** `parseParameters` (`parser.go:3274`)
        // guards the whole body on `parseExpected(KindOpenParenToken)` and
        // returns `createMissingList()` when it fails. This discarded that
        // boolean and fell into the loop regardless, so
        // `class Test { prop = 42; constructor }` — where the token after the
        // absent `(` is `}` — parsed a parameter and manufactured a zero-width
        // identifier. `classFieldsBrokenConstructorEmitNoCrash1` and
        // `parserConstructorDeclaration8` each rendered one assertion more than
        // upstream and failed on the count alone, every line otherwise right.
        //
        // The `expect` here already returned the answer; only the `if` was
        // missing.
        if !self.expect(SyntaxKind::OpenParenToken) {
            return Vec::new();
        }
        // `parseParametersWorker`: `parseDelimitedList(PCParameters,
        // parseParameter)`. A token that starts no parameter is reported
        // (TS1138, or TS1390 for a keyword) and skipped unless an enclosing
        // list wants it — the general form of §198/§200/§277's subsets.
        let (parameters, _) =
            self.parse_delimited_list(ParsingContext::Parameters, Self::parse_parameter);
        self.expect(SyntaxKind::CloseParenToken);
        parameters
    }

    pub(crate) fn parse_parameter(&mut self) -> &'a ParameterDeclaration<'a> {
        let docs = self.parse_leading_jsdoc();
        let parameter = self.parse_parameter_worker();
        self.attach_jsdoc(tsr_ast::Node::ParameterDeclaration(parameter), docs);
        parameter
    }

    fn parse_parameter_worker(&mut self) -> &'a ParameterDeclaration<'a> {
        let start = self.pos();
        // The first modifier's `Loc` starts at its full start, leading trivia
        // included: where the previous token ended.
        let modifiers_full_start = self.node_end();
        let modifiers = self.parse_modifiers();
        // `parseParameterWorker`'s `KindThisKeyword` arm (`parser.go:3334`):
        // a `this` parameter with modifiers is a parse error at
        // `modifiers.Nodes[0].Loc` — full start, no `SkipTrivia` — so the
        // checker's `grammarErrorOnFirstToken` for the same rule stays silent.
        if self.at(SyntaxKind::ThisKeyword)
            && let Some(first) = modifiers.first().and_then(ModifierLike::node_id)
        {
            let end = self.nodes.span(first).end;
            self.error_at(
                &messages::NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS,
                Span::new(modifiers_full_start, end),
            );
        }
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        // A `this` parameter is the one reserved word a parameter name admits:
        // upstream's `parseParameter` reads it with `parseIdentifierName`
        // (`parser.go`, the `KindThisKeyword` arm).
        // Only as the plain first parameter: `...this` is upstream's missing
        // identifier plus an error (`thisTypeInFunctionsNegative`).
        let name = if self.at(SyntaxKind::ThisKeyword) && dot_dot_dot.is_none() {
            BindingName::Identifier(self.parse_identifier_name())
        } else if self.at(SyntaxKind::PrivateIdentifier) {
            // Native parseParameterEx supplies this diagnostic to
            // createIdentifierWithDiagnostic, then consumes the private token
            // as an identifier rather than manufacturing a missing name.
            self.error_at_current(&messages::PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS);
            let start = self.pos();
            let text = self.token_value();
            self.next_token();
            BindingName::Identifier(self.finish_node(
                Identifier::new(text),
                SyntaxKind::Identifier,
                start,
            ))
        } else {
            self.parse_binding_name()
        };
        let question =
            if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };
        let type_node = self.parse_type_annotation();
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        let modifiers = self.arena.alloc_slice(&modifiers);
        self.finish_node(
            ParameterDeclaration::new(
                modifiers,
                dot_dot_dot,
                Some(name),
                question,
                type_node,
                initializer,
            ),
            SyntaxKind::Parameter,
            start,
        )
    }
}
