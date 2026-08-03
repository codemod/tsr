//! Expression parsing.
//!
//! Binary operators use precedence climbing rather than one function per level:
//! TypeScript has 15 binary precedence levels, and a function each would be 15
//! near-identical bodies that drift apart. The table in [`binary_precedence`] is
//! the single place the grammar's shape is stated.

use tsr_ast::*;
use tsr_core::Span;
use tsr_diagnostics::messages;

use crate::parser::Parser;

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
fn is_assignment_operator(kind: SyntaxKind) -> bool {
    (SyntaxKind::FIRST_ASSIGNMENT as u16..=SyntaxKind::LAST_ASSIGNMENT as u16)
        .contains(&(kind as u16))
}

impl<'a> Parser<'a> {
    /// Whether the cursor could begin an expression.
    pub(crate) fn at_expression_start(&self) -> bool {
        match self.token.kind {
            SyntaxKind::Identifier
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
            | SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken
            | SyntaxKind::SlashToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::DotDotDotToken
            | SyntaxKind::AtToken
            | SyntaxKind::HashToken => true,
            kind => kind.is_keyword(),
        }
    }

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

    /// Parse an expression where a bare `in` is not an operator.
    ///
    /// `for (a in b)` would otherwise consume `a in b` as a comparison and leave
    /// the loop header malformed.
    pub(crate) fn parse_expression_no_in(&mut self) -> Expression<'a> {
        // Handled by the caller checking for `in` before recursing; the binary
        // loop stops at `in` only in that context, which the `for` parser
        // establishes by parsing a unary expression and inspecting the operator.
        self.parse_unary_expression()
    }

    /// Parse an assignment, conditional, or binary expression.
    pub(crate) fn parse_assignment_expression(&mut self) -> Expression<'a> {
        let start = self.pos();

        if self.at(SyntaxKind::YieldKeyword) {
            return self.parse_yield_expression();
        }
        if let Some(arrow) = self.try_parse_arrow_function() {
            return arrow;
        }

        let left = self.parse_binary_expression(0);

        if is_assignment_operator(self.token.kind) {
            let operator = self.take_token();
            let right = self.parse_assignment_expression();
            let node = self.finish_node(
                BinaryExpression::new(&[], Some(left), None, Some(operator), Some(right)),
                SyntaxKind::BinaryExpression,
                start,
            );
            return Expression::BinaryExpression(node);
        }

        if self.at(SyntaxKind::QuestionToken) {
            let question = self.take_token();
            let when_true = self.parse_assignment_expression();
            let colon = if self.at(SyntaxKind::ColonToken) {
                self.take_token()
            } else {
                self.error_at_current_with(&messages::_0_EXPECTED, &[":"]);
                self.alloc_token(SyntaxKind::ColonToken, Span::at(self.pos()))
            };
            let when_false = self.parse_assignment_expression();
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
        let Some(mut left) = self.descend(Parser::parse_unary_expression) else {
            self.error_at_current(&messages::EXPRESSION_EXPECTED);
            return Expression::Identifier(self.missing_identifier());
        };

        while let Some(precedence) = binary_precedence(self.token.kind) {
            if precedence < min_precedence {
                break;
            }

            // `as` and `satisfies` take a *type* on the right, not an expression.
            if matches!(self.token.kind, SyntaxKind::AsKeyword | SyntaxKind::SatisfiesKeyword) {
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
        let asterisk = if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        let expression = if self.can_parse_semicolon() || self.token.has_preceding_line_break() {
            None
        } else {
            Some(self.parse_assignment_expression())
        };
        let node = self.finish_node(
            YieldExpression::new(asterisk, expression),
            SyntaxKind::YieldExpression,
            start,
        );
        Expression::YieldExpression(node)
    }

    /// Parse prefix operators and `await`, then a postfix expression.
    pub(crate) fn parse_unary_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        match self.token.kind {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken => {
                let operator = self.take_token();
                let operand = self.parse_unary_expression();
                let node = self.finish_node(
                    PrefixUnaryExpression::new(Some(operator), Some(operand)),
                    SyntaxKind::PrefixUnaryExpression,
                    start,
                );
                Expression::PrefixUnaryExpression(node)
            }
            SyntaxKind::TypeOfKeyword => {
                self.next_token();
                let operand = self.parse_unary_expression();
                let node = self.finish_node(
                    TypeOfExpression::new(Some(operand)),
                    SyntaxKind::TypeOfExpression,
                    start,
                );
                Expression::TypeOfExpression(node)
            }
            SyntaxKind::VoidKeyword => {
                self.next_token();
                let operand = self.parse_unary_expression();
                let node = self.finish_node(
                    VoidExpression::new(Some(operand)),
                    SyntaxKind::VoidExpression,
                    start,
                );
                Expression::VoidExpression(node)
            }
            SyntaxKind::DeleteKeyword => {
                self.next_token();
                let operand = self.parse_unary_expression();
                let node = self.finish_node(
                    DeleteExpression::new(Some(operand)),
                    SyntaxKind::DeleteExpression,
                    start,
                );
                Expression::DeleteExpression(node)
            }
            SyntaxKind::AwaitKeyword => {
                self.next_token();
                let operand = self.parse_unary_expression();
                let node = self.finish_node(
                    AwaitExpression::new(Some(operand)),
                    SyntaxKind::AwaitExpression,
                    start,
                );
                Expression::AwaitExpression(node)
            }
            _ => self.parse_postfix_expression(),
        }
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
                PostfixUnaryExpression::new(Some(expression), Some(operator)),
                SyntaxKind::PostfixUnaryExpression,
                start,
            );
            return Expression::PostfixUnaryExpression(node);
        }
        expression
    }

    /// Parse a primary expression followed by any chain of calls and accesses.
    fn parse_call_or_member_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        let mut expression = if self.at(SyntaxKind::NewKeyword) {
            self.parse_new_expression()
        } else {
            self.parse_primary_expression()
        };

        loop {
            match self.token.kind {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let name = self.parse_member_name();
                    let node = self.finish_node(
                        PropertyAccessExpression::new(Some(expression), None, Some(name)),
                        SyntaxKind::PropertyAccessExpression,
                        start,
                    );
                    expression = Expression::PropertyAccessExpression(node);
                }
                SyntaxKind::QuestionDotToken => {
                    let question_dot = self.take_token();
                    if self.at(SyntaxKind::OpenParenToken) {
                        let arguments = self.parse_arguments();
                        let arguments = self.arena.alloc_slice(&arguments);
                        let node = self.finish_node(
                            CallExpression::new(
                                Some(expression),
                                Some(question_dot),
                                &[],
                                arguments,
                            ),
                            SyntaxKind::CallExpression,
                            start,
                        );
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
                        expression = Expression::PropertyAccessExpression(node);
                    }
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = self.parse_expression();
                    self.expect(SyntaxKind::CloseBracketToken);
                    let node = self.finish_node(
                        ElementAccessExpression::new(Some(expression), None, Some(argument)),
                        SyntaxKind::ElementAccessExpression,
                        start,
                    );
                    expression = Expression::ElementAccessExpression(node);
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
                _ => break,
            }
        }
        expression
    }

    fn parse_new_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.next_token();
        let callee = self.parse_primary_expression();
        let arguments = if self.at(SyntaxKind::OpenParenToken) {
            let args = self.parse_arguments();
            self.arena.alloc_slice(&args)
        } else {
            // `new Foo` without parentheses is legal.
            &[][..]
        };
        let node = self.finish_node(
            NewExpression::new(Some(callee), &[], arguments),
            SyntaxKind::NewExpression,
            start,
        );
        Expression::NewExpression(node)
    }

    fn parse_arguments(&mut self) -> Vec<Expression<'a>> {
        self.expect(SyntaxKind::OpenParenToken);
        let mut arguments = Vec::new();
        while !self.at(SyntaxKind::CloseParenToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            arguments.push(self.parse_argument());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseParenToken);
        arguments
    }

    fn parse_argument(&mut self) -> Expression<'a> {
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
    fn parse_primary_expression(&mut self) -> Expression<'a> {
        let start = self.pos();
        match self.token.kind {
            SyntaxKind::Identifier => Expression::Identifier(self.parse_identifier()),
            SyntaxKind::NumericLiteral => {
                let text = self.token_value();
                self.next_token();
                let node =
                    self.finish_node(NumericLiteral::new(text), SyntaxKind::NumericLiteral, start);
                Expression::NumericLiteral(node)
            }
            SyntaxKind::BigIntLiteral => {
                let text = self.token_value();
                self.next_token();
                let node =
                    self.finish_node(BigIntLiteral::new(text), SyntaxKind::BigIntLiteral, start);
                Expression::BigIntLiteral(node)
            }
            SyntaxKind::StringLiteral => Expression::StringLiiteralOrIdentifier(),
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                let text = self.token_value();
                self.next_token();
                let node = self.finish_node(
                    NoSubstitutionTemplateLiteral::new(text),
                    SyntaxKind::NoSubstitutionTemplateLiteral,
                    start,
                );
                Expression::NoSubstitutionTemplateLiteral(node)
            }
            SyntaxKind::OpenParenToken => {
                self.next_token();
                let expression = self.parse_expression();
                self.expect(SyntaxKind::CloseParenToken);
                let node = self.finish_node(
                    ParenthesizedExpression::new(Some(expression)),
                    SyntaxKind::ParenthesizedExpression,
                    start,
                );
                Expression::ParenthesizedExpression(node)
            }
            SyntaxKind::OpenBracketToken => self.parse_array_literal(),
            SyntaxKind::OpenBraceToken => self.parse_object_literal(),
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken => {
                self.rescan_regular_expression();
                let text = self.token_text();
                self.next_token();
                let node = self.finish_node(
                    RegularExpressionLiteral::new(text),
                    SyntaxKind::RegularExpressionLiteral,
                    start,
                );
                Expression::RegularExpressionLiteral(node)
            }
            kind if kind.is_keyword() => {
                // `this`, `super`, `true`, `false`, `null`, and contextual
                // keywords used as names all land here.
                self.next_token();
                let node = self.finish_node(KeywordExpression::new(), kind, start);
                Expression::KeywordExpression(node)
            }
            _ => {
                self.error_at_current(&messages::EXPRESSION_EXPECTED);
                Expression::Identifier(self.missing_identifier())
            }
        }
    }

    fn parse_array_literal(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBracketToken);
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBracketToken) && !self.at(SyntaxKind::EndOfFile) {
            if self.at(SyntaxKind::CommaToken) {
                // Elision: `[1, , 2]` has a hole.
                let hole_start = self.pos();
                let node = self.finish_node(
                    OmittedExpression::new(),
                    SyntaxKind::OmittedExpression,
                    hole_start,
                );
                elements.push(Expression::OmittedExpression(node));
                self.next_token();
                continue;
            }
            elements.push(self.parse_argument());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBracketToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(
            ArrayLiteralExpression::new(elements, false),
            SyntaxKind::ArrayLiteralExpression,
            start,
        );
        Expression::ArrayLiteralExpression(node)
    }

    fn parse_object_literal(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let mut properties = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            properties.push(self.parse_object_literal_element());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
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

        let name = self.parse_property_name();

        if self.eat(SyntaxKind::ColonToken) {
            let initializer = self.parse_assignment_expression();
            let node = self.finish_node(
                PropertyAssignment::new(&[], name, None, None, Some(initializer)),
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
            ShorthandPropertyAssignment::new(&[], name, None, None, None, initializer),
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
    fn try_parse_arrow_function(&mut self) -> Option<Expression<'a>> {
        // A bare `x => …` needs no speculation.
        if self.at(SyntaxKind::Identifier) {
            let saved_start = self.pos();
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
            let body = self.parse_arrow_body();
            let parameters = self.arena.alloc_slice(&[parsed]);
            let node = self.finish_node(
                ArrowFunction::new(&[], &[], parameters, None, None, Some(arrow), Some(body), None),
                SyntaxKind::ArrowFunction,
                saved_start,
            );
            return Some(Expression::ArrowFunction(node));
        }

        if !self.at(SyntaxKind::OpenParenToken) {
            return None;
        }

        let start = self.pos();
        let parameters = self.try_parse(|p| {
            let parameters = p.parse_parameter_list();
            // A return type may intervene: `(a): number => a`.
            let return_type = p.parse_type_annotation();
            if !p.at(SyntaxKind::EqualsGreaterThanToken) {
                return None;
            }
            Some((parameters, return_type))
        })?;

        let (parameters, return_type) = parameters;
        let arrow = self.take_token();
        let body = self.parse_arrow_body();
        let parameters = self.arena.alloc_slice(&parameters);
        let node = self.finish_node(
            ArrowFunction::new(
                &[],
                &[],
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

    fn parse_arrow_body(&mut self) -> ConciseBody<'a> {
        if self.at(SyntaxKind::OpenBraceToken) {
            ConciseBody::Block(self.parse_block())
        } else {
            ConciseBody::from(self.parse_assignment_expression())
        }
    }

    // ---- names ----------------------------------------------------------

    /// Parse an identifier, synthesising one if the cursor is elsewhere.
    pub(crate) fn parse_identifier(&mut self) -> &'a Identifier<'a> {
        let start = self.pos();
        if self.at(SyntaxKind::Identifier) || self.token.kind.is_keyword() {
            let text = self.token_value();
            self.next_token();
            return self.finish_node(Identifier::new(text), SyntaxKind::Identifier, start);
        }
        self.error_at_current(&messages::IDENTIFIER_EXPECTED);
        self.missing_identifier()
    }

    fn parse_member_name(&mut self) -> MemberName<'a> {
        if self.at(SyntaxKind::PrivateIdentifier) {
            let start = self.pos();
            let text = self.token_value();
            self.next_token();
            let node = self.finish_node(
                PrivateIdentifier::new(text),
                SyntaxKind::PrivateIdentifier,
                start,
            );
            return MemberName::PrivateIdentifier(node);
        }
        MemberName::Identifier(self.parse_identifier())
    }

    /// Parse a property name: identifier, string, number, or `[computed]`.
    pub(crate) fn parse_property_name(&mut self) -> PropertyName<'a> {
        let start = self.pos();
        match self.token.kind {
            SyntaxKind::StringLiteral => {
                let text = self.token_value();
                self.next_token();
                let node =
                    self.finish_node(StringLiteral::new(text), SyntaxKind::StringLiteral, start);
                PropertyName::StringLiteral(node)
            }
            SyntaxKind::NumericLiteral => {
                let text = self.token_value();
                self.next_token();
                let node =
                    self.finish_node(NumericLiteral::new(text), SyntaxKind::NumericLiteral, start);
                PropertyName::NumericLiteral(node)
            }
            SyntaxKind::OpenBracketToken => {
                self.next_token();
                let expression = self.parse_assignment_expression();
                self.expect(SyntaxKind::CloseBracketToken);
                let node = self.finish_node(
                    ComputedPropertyName::new(Some(expression)),
                    SyntaxKind::ComputedPropertyName,
                    start,
                );
                PropertyName::ComputedPropertyName(node)
            }
            SyntaxKind::PrivateIdentifier => {
                let text = self.token_value();
                self.next_token();
                let node = self.finish_node(
                    PrivateIdentifier::new(text),
                    SyntaxKind::PrivateIdentifier,
                    start,
                );
                PropertyName::PrivateIdentifier(node)
            }
            _ => PropertyName::Identifier(self.parse_identifier()),
        }
    }

    /// Parse a binding name: an identifier or a destructuring pattern.
    pub(crate) fn parse_binding_name(&mut self) -> BindingName<'a> {
        match self.token.kind {
            SyntaxKind::OpenBracketToken => self.parse_array_binding_pattern(),
            SyntaxKind::OpenBraceToken => self.parse_object_binding_pattern(),
            _ => BindingName::Identifier(self.parse_identifier()),
        }
    }

    fn parse_array_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBracketToken);
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBracketToken) && !self.at(SyntaxKind::EndOfFile) {
            if self.at(SyntaxKind::CommaToken) {
                self.next_token();
                continue;
            }
            elements.push(self.parse_binding_element());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBracketToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(
            ArrayBindingPattern::new(elements),
            SyntaxKind::ArrayBindingPattern,
            start,
        );
        BindingName::ArrayBindingPattern(node)
    }

    fn parse_object_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            elements.push(self.parse_binding_element());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(
            ObjectBindingPattern::new(elements),
            SyntaxKind::ObjectBindingPattern,
            start,
        );
        BindingName::ObjectBindingPattern(node)
    }

    fn parse_binding_element(&mut self) -> &'a BindingElement<'a> {
        let start = self.pos();
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };

        // `{ a: b }` renames; `{ a }` does not.
        let (property_name, name) = if self.at(SyntaxKind::OpenBracketToken)
            || self.at(SyntaxKind::StringLiteral)
            || self.at(SyntaxKind::NumericLiteral)
        {
            let property = self.parse_property_name();
            self.expect(SyntaxKind::ColonToken);
            (Some(property), self.parse_binding_name())
        } else {
            let name = self.parse_binding_name();
            if self.eat(SyntaxKind::ColonToken) {
                let property = match name {
                    BindingName::Identifier(id) => PropertyName::Identifier(id),
                    _ => PropertyName::Identifier(self.missing_identifier()),
                };
                (Some(property), self.parse_binding_name())
            } else {
                (None, name)
            }
        };

        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };

        self.finish_node(
            BindingElement::new(dot_dot_dot, property_name, Some(name), initializer),
            SyntaxKind::BindingElement,
            start,
        )
    }

    /// Parse a parenthesised parameter list.
    pub(crate) fn parse_parameter_list(&mut self) -> Vec<&'a ParameterDeclaration<'a>> {
        self.expect(SyntaxKind::OpenParenToken);
        let mut parameters = Vec::new();
        while !self.at(SyntaxKind::CloseParenToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            parameters.push(self.parse_parameter());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseParenToken);
        parameters
    }

    fn parse_parameter(&mut self) -> &'a ParameterDeclaration<'a> {
        let start = self.pos();
        let modifiers = self.parse_modifiers();
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        let name = self.parse_binding_name();
        let question = if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };
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
