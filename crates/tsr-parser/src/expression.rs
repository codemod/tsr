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
            | SyntaxKind::PrivateIdentifier
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
            | SyntaxKind::DotDotDotToken => true,
            // `@` is parsed only where decorators are legal. Claiming it as an
            // expression start would produce a node covering no text.
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
            if self.no_in > 0 && self.at(SyntaxKind::InKeyword) {
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
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        // `yield` may stand alone. Besides the usual statement enders, a closing
        // delimiter ends it too: `{ [yield]: 1 }` and `f(yield)` are both legal.
        let has_operand = !self.can_parse_semicolon()
            && !self.token.has_preceding_line_break()
            && !matches!(
                self.token.kind,
                SyntaxKind::CloseBracketToken
                    | SyntaxKind::CloseParenToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::ColonToken
            );
        let expression = if has_operand { Some(self.parse_assignment_expression()) } else { None };
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
                    PrefixUnaryExpression::new(operator, Some(operand)),
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
                PostfixUnaryExpression::new(Some(expression), operator),
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
                // `f<T>(x)`. `<` is also less-than, so the type arguments are
                // only accepted when a call follows them.
                SyntaxKind::LessThanToken | SyntaxKind::LessThanLessThanToken => {
                    // `f<T>(x)` is a generic call. `f<T>` alone is an
                    // *instantiation expression*, legal since TS 4.7 — but `a < b
                    // > c` is a comparison, so the type arguments only stand
                    // without a call when what follows cannot continue an
                    // expression.
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
                        continue;
                    }
                    let arguments = self.parse_arguments();
                    let arguments = self.arena.alloc_slice(&arguments);
                    let node = self.finish_node(
                        CallExpression::new(Some(expression), None, type_arguments, arguments),
                        SyntaxKind::CallExpression,
                        start,
                    );
                    expression = Expression::CallExpression(node);
                }
                // `` tag`…` `` — a tagged template. The template is an operand of
                // the tag, not a separate expression.
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal();
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
        let mut callee = self.parse_primary_expression();
        // `new a.b.C()` — the callee is a member chain, but not a call, since the
        // parentheses belong to `new`.
        while self.at(SyntaxKind::DotToken) {
            self.next_token();
            let name = self.parse_member_name();
            let node = self.finish_node(
                PropertyAccessExpression::new(Some(callee), None, Some(name)),
                SyntaxKind::PropertyAccessExpression,
                callee_start,
            );
            callee = Expression::PropertyAccessExpression(node);
        }
        let type_arguments = if self.at(SyntaxKind::LessThanToken) {
            self.try_parse(Parser::parse_type_arguments_for_call).unwrap_or_default()
        } else {
            Vec::new()
        };
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

    pub(crate) fn parse_arguments(&mut self) -> Vec<Expression<'a>> {
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
                let raw = self.token_text();
                let (text, flags) = self.take_literal();
                let node = self.finish_node(
                    NoSubstitutionTemplateLiteral::new(text, flags, flags, raw),
                    SyntaxKind::NoSubstitutionTemplateLiteral,
                    start,
                );
                Expression::NoSubstitutionTemplateLiteral(node)
            }
            SyntaxKind::TemplateHead => self.parse_template_expression(),
            // `<` is a type assertion in `.ts` and a JSX element in `.tsx`. The
            // two readings are mutually exclusive, which is why TypeScript ties
            // them to the file extension rather than to a lookahead.
            SyntaxKind::LessThanToken => {
                if self.script_kind.allows_jsx() {
                    self.parse_jsx_element()
                } else {
                    self.parse_type_assertion()
                }
            }
            SyntaxKind::FunctionKeyword => self.parse_function_expression(None),
            SyntaxKind::AsyncKeyword if self.next_is_function_keyword() => {
                let modifier = self.take_token();
                self.parse_function_expression(Some(modifier))
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
            kind if kind.is_keyword() && crate::statement::is_reserved_word(kind) => {
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
            kind if kind.is_keyword() => Expression::Identifier(self.parse_identifier()),
            _ => {
                self.error_at_current(&messages::EXPRESSION_EXPECTED);
                Expression::Identifier(self.missing_identifier())
            }
        }
    }

    pub(crate) fn parse_array_literal(&mut self) -> Expression<'a> {
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

    pub(crate) fn parse_object_literal(&mut self) -> Expression<'a> {
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

        // `async`, `*`, `get`, and `set` all introduce a member rather than a
        // name — unless what follows says otherwise, since each is also a legal
        // property name on its own.
        let modifiers =
            if self.at(SyntaxKind::AsyncKeyword) && self.next_starts_property_name_or_star() {
                vec![ModifierLike::Token(self.take_token())]
            } else {
                Vec::new()
            };
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };

        if matches!(self.token.kind, SyntaxKind::GetKeyword | SyntaxKind::SetKeyword)
            && self.next_starts_property_name()
        {
            let is_getter = self.at(SyntaxKind::GetKeyword);
            self.next_token();
            let name = self.parse_property_name();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            let body = FunctionBody::Block(self.parse_block());
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
                        Some(body),
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
                        Some(body),
                        None,
                        None,
                    ),
                    SyntaxKind::SetAccessor,
                    start,
                ))
            };
        }

        let name = self.parse_property_name();

        // `{ m() {} }` and `{ m<T>() {} }` are methods.
        if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            let body = FunctionBody::Block(self.parse_block());
            let modifiers = self.arena.alloc_slice(&modifiers);
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            return ObjectLiteralElementLike::MethodDeclaration(self.finish_node(
                MethodDeclaration::new(
                    modifiers,
                    asterisk,
                    name,
                    None,
                    type_parameters,
                    parameters,
                    return_type,
                    None,
                    Some(body),
                ),
                SyntaxKind::MethodDeclaration,
                start,
            ));
        }

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

    fn next_starts_property_name_or_star(&mut self) -> bool {
        self.peek_kind(|kind| {
            kind == SyntaxKind::AsteriskToken
                || matches!(
                    kind,
                    SyntaxKind::Identifier
                        | SyntaxKind::StringLiteral
                        | SyntaxKind::NumericLiteral
                        | SyntaxKind::OpenBracketToken
                )
                || kind.is_keyword()
        })
    }

    /// Arrow functions, when the lookahead confirms one.
    ///
    /// `(a)` is a parenthesised expression and `(a) => a` is an arrow function;
    /// they diverge only at the `=>`. Rather than encode a lookahead predicate for
    /// every parameter-list shape, this speculatively parses a parameter list and
    /// rewinds if no arrow follows.
    fn try_parse_arrow_function(&mut self) -> Option<Expression<'a>> {
        // `async` prefixes an arrow but is also an ordinary identifier, so it is
        // only consumed once the arrow is confirmed.
        let async_modifier = if self.at(SyntaxKind::AsyncKeyword) && self.async_starts_arrow() {
            Some(self.take_token())
        } else {
            None
        };

        // A bare `x => …` needs no speculation. The name may be a contextual
        // keyword — `async => async` names its parameter `async`.
        if self.at(SyntaxKind::Identifier)
            || crate::statement::is_contextual_keyword(self.token.kind)
        {
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
            return Some(Expression::ArrowFunction(node));
        }

        if !self.at(SyntaxKind::OpenParenToken) && !self.at(SyntaxKind::LessThanToken) {
            return None;
        }
        // Decide with a token-only scan before committing to a real parse. See
        // `is_arrow_function_ahead` for why speculating directly is catastrophic.
        if !self.is_arrow_function_ahead() {
            return None;
        }

        let start = self.pos();
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameter_list();
        // A return type may intervene: `(a): number => a`.
        let return_type = self.parse_return_type_annotation();
        let arrow = self.take_token();
        let body = self.parse_arrow_body();
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

    /// Whether the token after `f<T>` rules out a comparison.
    ///
    /// `a < b > c` is arithmetic; `f<T>;` is an instantiation expression. The
    /// difference is whether an operand could follow — TypeScript decides the same
    /// way.
    fn at_instantiation_terminator(&self) -> bool {
        matches!(
            self.token.kind,
            SyntaxKind::SemicolonToken
                | SyntaxKind::CommaToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::QuestionDotToken
                | SyntaxKind::ColonToken
                | SyntaxKind::EndOfFile
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
        ) || self.token.has_preceding_line_break()
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
                    p.is_arrow_function_ahead()
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
    fn is_arrow_function_ahead(&mut self) -> bool {
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

    fn parse_arrow_body(&mut self) -> ConciseBody<'a> {
        if self.at(SyntaxKind::OpenBraceToken) {
            ConciseBody::Block(self.parse_block())
        } else {
            ConciseBody::from(self.parse_assignment_expression())
        }
    }

    /// Parse `` `a${x}b` ``.
    ///
    /// The scanner cannot know where a substitution ends: the `}` closing it is
    /// lexically a close-brace, and only the parser's bracket tracking
    /// distinguishes the two. So each span is driven explicitly, re-scanning the
    /// `}` as template text via [`Parser::rescan_template_continuation`].
    fn parse_template_literal(&mut self) -> TemplateLiteral<'a> {
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
                    let name = self.parse_identifier();
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
                    let template = self.parse_template_literal();
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
    ) -> Expression<'a> {
        let start = self.pos();
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
        let parameters = self.parse_parameter_list();
        let return_type = self.parse_return_type_annotation();
        let body = FunctionBody::Block(self.parse_block());

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
            let text = self.private_identifier_text();
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
                let text = self.private_identifier_text();
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

    fn parse_array_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        let kind_token = self.alloc_token(SyntaxKind::OpenBracketToken, self.token.span);
        self.expect(SyntaxKind::OpenBracketToken);
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBracketToken) && !self.at(SyntaxKind::EndOfFile) {
            if self.at(SyntaxKind::CommaToken) {
                // A hole, `[, a]`. Upstream's `parseArrayBindingElement`
                // (`parser.go:1656`) represents it as a `BindingElement` whose
                // fields are all nil — "These are all nil for a missing
                // element" — and `getBindingElementTypeFromParentType` counts
                // it in `slices.Index(pattern.Elements(), declaration)`
                // (`checker.go:17750`). Skipping the hole, as this loop did
                // before `bd tsr-o00`, silently renumbered every element after
                // it.
                let hole_start = self.pos();
                let hole = self.finish_node(
                    tsr_ast::BindingElement::new(None, None, None, None),
                    SyntaxKind::BindingElement,
                    hole_start,
                );
                elements.push(hole);
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
            BindingPattern::new(kind_token, elements),
            SyntaxKind::ArrayBindingPattern,
            start,
        );
        BindingName::BindingPattern(node)
    }

    fn parse_object_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        let kind_token = self.alloc_token(SyntaxKind::OpenBraceToken, self.token.span);
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
            BindingPattern::new(kind_token, elements),
            SyntaxKind::ObjectBindingPattern,
            start,
        );
        BindingName::BindingPattern(node)
    }

    fn parse_binding_element(&mut self) -> &'a BindingElement<'a> {
        let start = self.pos();
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };

        // `{ [k]: v }` renames via a computed key; `[a, b]` is a nested array
        // pattern. Both start with `[`, and only the `:` after the closing
        // bracket tells them apart — reading `[` as a key unconditionally breaks
        // every nested array destructuring.
        let bracket_is_computed_key = self.at(SyntaxKind::OpenBracketToken) && {
            let mut matched = false;
            self.try_parse(|p| {
                if p.skip_balanced(SyntaxKind::OpenBracketToken) {
                    matched = p.at(SyntaxKind::ColonToken);
                }
                None::<()>
            });
            matched
        };

        // `{ a: b }` renames; `{ a }` does not.
        let (property_name, name) = if bracket_is_computed_key
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
                    BindingName::BindingPattern(_) => {
                        PropertyName::Identifier(self.missing_identifier())
                    }
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
        let docs = self.parse_leading_jsdoc();
        let parameter = self.parse_parameter_worker();
        self.attach_jsdoc(tsr_ast::Node::ParameterDeclaration(parameter), docs);
        parameter
    }

    fn parse_parameter_worker(&mut self) -> &'a ParameterDeclaration<'a> {
        let start = self.pos();
        let modifiers = self.parse_modifiers();
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        let name = self.parse_binding_name();
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
