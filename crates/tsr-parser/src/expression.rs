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

/// The unambiguous half of `isStartOfParameter` (`parser.go:886`).
///
/// Upstream's predicate ends in `isStartOfType`, which this port does not have;
/// omitting it makes this a **subset**, and a subset is safe here because every
/// token it rejects keeps the caller's `break`. See `checker-notes-diag2.md`
/// §200 for why the complete predicate is priced separately.
fn starts_parameter(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::DotDotDotToken
            | SyntaxKind::Identifier
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::AtToken
            | SyntaxKind::ThisKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::OverrideKeyword
    )
}

/// The unambiguous half of `isListElement(PCArrayLiteralMembers)`
/// (`parser.go`): upstream admits `,`, `...` or `isStartOfExpression`.
///
/// `isStartOfExpression` is not ported, so this is a **subset** and every token
/// it rejects keeps the caller's `break` — the §200 shape.
fn starts_array_element(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CommaToken
            | SyntaxKind::DotDotDotToken
            | SyntaxKind::Identifier
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
            | SyntaxKind::RegularExpressionLiteral
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::OpenParenToken
    )
}

/// `isListElement(PCObjectLiteralMembers)` (`parser.go:845`).
///
/// `[`, `*`, `...` and `.` are admitted verbatim from upstream — the last is
/// *not* a member, and upstream's comment says it is there so a trailing dot
/// does not close the literal. The rest is `isLiteralPropertyName`: an
/// identifier or keyword, a string, or a number.
fn starts_object_literal_member(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::OpenBracketToken
            | SyntaxKind::AsteriskToken
            | SyntaxKind::DotDotDotToken
            | SyntaxKind::DotToken
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
    ) || kind == SyntaxKind::Identifier
        || kind.is_keyword()
}

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

        // `ast.IsLeftHandSideExpression(expr) && ast.IsAssignmentOperator(...)`
        // (`parser.go:4143`) — **both** conjuncts. The assignment productions
        // can only start with a `LeftHandSideExpression`, which is upstream's
        // own comment three lines above, so `x++ = 4` is not an assignment
        // there: the `=` falls through and the statement gets `';' expected`.
        // §190 found the omission; §191 refused it at `diagnostics −6` while
        // the reporting sink still double-reported, and §193 fixed the sink.
        if is_left_hand_side_expression(left) && is_assignment_operator(self.token.kind) {
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
            // `await` is CONTEXTUAL — see [`Self::is_await_expression`].
            SyntaxKind::AwaitKeyword if self.is_await_expression() => {
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
                    let template = self.parse_template_literal(true);
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
            // `isListElement(PCArgumentExpressions)` (`parser.go:882`), applied
            // only where the two-way break cannot disagree with upstream's
            // three-way one. §197.
            //
            // Upstream, on a token that is not an argument, first asks
            // `isListTerminator` — `)` or `;` for this context (`:938`) — and
            // then `abortParsingListOrMoveToNextToken` (`:698`), which breaks if
            // the token belongs to an ENCLOSING list and otherwise reports
            // `Argument_expression_expected`, skips one token and retries. That
            // third arm needs the `parsingContexts` bitmask, which only exists
            // if `parseDelimitedList` runs every list — see
            // `docs/architecture/checker-notes-nearmiss.md` §191.
            //
            // So the guard is deliberately narrow: it fires only on the closers,
            // where upstream breaks under *either* arm — `;` by the terminator
            // test, `}` and `]` by the abort test, since both certainly close an
            // enclosing block, object, array or index. Every other non-argument
            // token keeps this parser's existing behaviour rather than taking a
            // recovery decision this port cannot yet make faithfully.
            //
            // These three are `isStartOfExpression`'s answer already, and the
            // first draft said so out loud with a `&& !self.is_start_of_expression()`
            // beside them. **The mutation run reddened nothing when that clause
            // was deleted** — a closer cannot begin an expression, so the test
            // could not change the answer. A condition that cannot change the
            // answer is not a guard, and keeping it would have read as though
            // the general predicate were in force here when only three tokens
            // are.
            if matches!(
                self.token.kind,
                SyntaxKind::SemicolonToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::CloseBracketToken
            ) {
                break;
            }
            // §235: upstream's THIRD arm, for this context only.
            // `abortParsingListOrMoveToNextToken` (`parser.go:698`) reports,
            // **skips one token, and retries** — creating no node — when the
            // token neither starts an element nor terminates the list nor
            // belongs to an enclosing one. This port parsed an argument
            // regardless, so `Foo(,` minted a zero-width missing identifier and
            // rendered one assertion more than upstream.
            //
            // The `parsingContexts` bitmask this arm needs in general does not
            // exist here — see `checker-notes-nearmiss.md` §191/§228. What
            // stands in for it is the break above, which already leaves on the
            // three closers that certainly end an enclosing block, object or
            // index. A token that starts no expression and is none of those is
            // one upstream skips.
            // `isListElement(PCArgumentExpressions)` is
            // `token == KindDotDotDotToken || isStartOfExpression()`
            // (`parser.go:884`) — **both halves**. The first draft ported only
            // the second and measured +2 cases against roughly **three thousand
            // lines lost**, every one of them a spread argument:
            // `variadicTuples1` −466, `genericRestParameters1` −310,
            // `callWithSpread` −218. Corollary 30, by the author of the commit
            // that had just cited it.
            if !self.at(SyntaxKind::DotDotDotToken) && !self.is_start_of_expression() {
                self.next_token();
                continue;
            }
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
            SyntaxKind::LessThanToken => {
                if self.script_kind.allows_jsx() {
                    self.parse_jsx_element()
                } else {
                    self.parse_type_assertion()
                }
            }
            SyntaxKind::FunctionKeyword => self.parse_function_expression(None, None),
            SyntaxKind::AsyncKeyword if self.next_is_function_keyword() => {
                // §205: the span starts at the `async`, not at `function`.
                let modifier_start = self.pos();
                let modifier = self.take_token();
                self.parse_function_expression(Some(modifier), Some(modifier_start))
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
            // §237: `isListElement(PCArrayLiteralMembers)` — the same third arm
            // §235 gave argument lists, in the context that falls through to
            // the same predicate. Upstream (`parser.go:877-883`) answers `true`
            // for `,` and `.` and otherwise falls through to
            // `token == KindDotDotDotToken || isStartOfExpression()`; a token
            // failing all of that is reported, **skipped, and the list retries**
            // (`abortParsingListOrMoveToNextToken`, `:698`), creating no node.
            //
            // `.` is upstream's completion affordance — *"not an array literal
            // member, but don't want to close the array"* — and is deliberately
            // NOT skipped here, so it keeps reaching `parse_argument` as it does
            // today. The comma is handled by the elision arm above.
            // **The break comes first, and the first draft omitted it** — which
            // reproduced §191's exact prediction, that a skip without the
            // `parsingContexts` mask eats a token belonging to an outer
            // construct. `new DisplayPosition([), 3, …], NoMove, 0)`
            // (`conformance/parser0_004152`): upstream ends the array at `[`
            // because `)` closes the enclosing ARGUMENT list, and the skip
            // swallowed it and consumed the rest of the call — **−8 lines**.
            //
            // So the stand-in for `isInSomeParsingContext` here is the same one
            // §235 relies on: the closers that certainly end an enclosing
            // construct. For an array element that is `)`, `}` and `;`; `]` is
            // this list's own terminator and is handled by the `while`.
            if matches!(
                self.token.kind,
                SyntaxKind::CloseParenToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::SemicolonToken
            ) {
                break;
            }
            if !self.at(SyntaxKind::DotDotDotToken)
                && !self.at(SyntaxKind::DotToken)
                && !self.is_start_of_expression()
            {
                self.next_token();
                continue;
            }
            let before = self.pos();
            elements.push(self.parse_argument());
            if self.eat(SyntaxKind::CommaToken) {
                continue;
            }
            if self.at(SyntaxKind::CloseBracketToken) || self.at(SyntaxKind::EndOfFile) {
                break;
            }
            // `parseDelimitedList` (`parser.go:664`) reports the missing
            // separator and continues. `var v = [1, 2, 3\n4, 5, 6, 7];` is one
            // `',' expected` upstream and was two here: this loop left the
            // list, so the `]` was reported missing as well.
            //
            // Guarded by a **subset** of `isListElement(PCArrayLiteralMembers)`
            // — upstream's is `,`, `...` or `isStartOfExpression`, and the last
            // is a sixty-kind predicate this port does not have. Everything the
            // subset rejects keeps the `break`, so this can only turn an abort
            // into a continue where an element genuinely follows (§200). §219.
            if !starts_array_element(self.token.kind) {
                break;
            }
            self.expect(SyntaxKind::CommaToken);
            if self.pos() == before {
                self.next_token();
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
            if self.eat(SyntaxKind::CommaToken) {
                if self.pos() == before {
                    break;
                }
                continue;
            }
            if self.at(SyntaxKind::CloseBraceToken) || self.at(SyntaxKind::EndOfFile) {
                break;
            }
            // `parseDelimitedList` (`parser.go:664`) reports the missing
            // separator and continues. **And for object-literal members it then
            // skips a `;`**, with upstream's own reason at `:678`: *"If the
            // token was a semicolon, and the caller allows that, then skip it
            // and continue. This ensures we get back on track and don't result
            // in tons of parse errors. For example, this can happen when people
            // do things like use a semicolon to delimit object literal
            // members."*
            //
            // `var v = { foo(); }` is exactly that: one `',' expected` upstream,
            // and two diagnostics here because this loop left the list. §218.
            self.expect(SyntaxKind::CommaToken);
            if self.at(SyntaxKind::SemicolonToken) && !self.token.has_preceding_line_break() {
                self.next_token();
            }
            // `isListElement(PCObjectLiteralMembers)` (`parser.go:845`):
            // `[`, `*`, `...`, `.`, or a literal property name. Continuing
            // without it is what §198 measured at −64 parser files — the guard
            // is what makes "recover by continuing" safe (§200).
            if !starts_object_literal_member(self.token.kind) {
                break;
            }
            if self.pos() == before {
                self.next_token();
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
            // `{ async m() { await x } }` — an object-literal method's await
            // context is its own, exactly as a class method's is. §193.
            let is_async = Self::is_async(&modifiers);
            let (parameters, return_type, body) = self.with_await_context(is_async, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                (parameters, return_type, FunctionBody::Block(parser.parse_block()))
            });
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

        // **The shorthand form needs an identifier.** Upstream's
        // `isShorthandPropertyAssignment` is `tokenIsIdentifier && token != ':'`,
        // so `{ [e] }` is not a shorthand — it reaches `parseExpected(':')` and
        // reports there. A computed name has no shorthand spelling, because the
        // shorthand *is* the identifier.
        // `docs/architecture/checker-notes-diag2.md` §576.
        if !matches!(name, tsr_ast::PropertyName::Identifier(_)) {
            self.expect(SyntaxKind::ColonToken);
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
        // **Before the `async`.** Upstream takes `pos := p.nodePos()` at the top
        // of `parseParenthesizedArrowFunctionExpression` and
        // `parseSimpleArrowFunctionExpression` (`parser.go:4541`), so the node's
        // span COVERS its modifier. This port took it after `take_token()`, and
        // the `.types` walker prints a node's source text from its span — so
        // `async (): Promise<void> => {}` rendered as
        // `(): Promise<void> => {}`: the right type under the wrong
        // expression, which fails the line exactly as a wrong type does. §205.
        let modifier_start = self.pos();
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
            let body =
                self.with_await_context(async_modifier.is_some(), Self::parse_arrow_body_inner);
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

        let start = modifier_start;
        let is_async = async_modifier.is_some();
        let type_parameters = self.parse_type_parameters();
        // Parameters take the signature's await context (`parser.go:3299`), the
        // body the same one (`:4484`); the `=>` between them is neither's.
        let (parameters, return_type) = self.with_await_context(is_async, |parser| {
            let parameters = parser.parse_parameter_list();
            // A return type may intervene: `(a): number => a`.
            (parameters, parser.parse_return_type_annotation())
        });
        let arrow = self.take_token();
        let body = self.with_await_context(is_async, Self::parse_arrow_body_inner);
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

    /// [`Self::parse_arrow_body`]'s body, taken as a function pointer so the
    /// two call sites can hand it to [`Parser::with_await_context`].
    fn parse_arrow_body_inner(&mut self) -> ConciseBody<'a> {
        self.parse_arrow_body()
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
                    let name = self.parse_identifier_name();
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

    /// Whether an expression can start at the cursor.
    ///
    /// Upstream's `isStartOfExpression` (`parser.go:6144`). The one deviation is
    /// upstream's error-tolerance arm, which treats the start of *any* binary
    /// operator as the start of an expression so it can parse out a missing
    /// identifier and give a good message. This port has no `isBinaryOperator`;
    /// every operator that also begins a **unary** expression (`+`, `-`, `~`,
    /// `!`, `<`) is listed here on its own account, so only the genuinely binary
    /// ones (`*`, `&&`, `instanceof`) are missed — and only in the direction of
    /// answering `false` where upstream answers `true`.
    pub(crate) fn is_start_of_expression(&mut self) -> bool {
        if self.is_start_of_left_hand_side_expression() {
            return true;
        }
        matches!(
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
        )
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
    pub(crate) fn parse_identifier(&mut self) -> &'a Identifier<'a> {
        let start = self.pos();
        if self.is_binding_identifier() {
            let text = self.token_value();
            self.next_token();
            return self.finish_node(Identifier::new(text), SyntaxKind::Identifier, start);
        }
        self.error_at_current(&messages::IDENTIFIER_EXPECTED);
        self.missing_identifier()
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

    fn parse_array_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        let kind_token = self.alloc_token(SyntaxKind::OpenBracketToken, self.token.span);
        self.expect(SyntaxKind::OpenBracketToken);
        let mut elements = Vec::new();
        let mut has_trailing_comma = false;
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
                has_trailing_comma = self.at(SyntaxKind::CloseBracketToken);
                continue;
            }
            elements.push(self.parse_binding_element());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            has_trailing_comma = self.at(SyntaxKind::CloseBracketToken);
        }
        self.expect(SyntaxKind::CloseBracketToken);
        let elements = self.arena.alloc_slice(&elements);
        let end = self.node_end();
        let flags = if has_trailing_comma {
            tsr_ast::NodeFlags::HAS_TRAILING_COMMA
        } else {
            tsr_ast::NodeFlags::empty()
        };
        let node = self.finish_node_with_flags(
            BindingPattern::new(kind_token, elements),
            SyntaxKind::ArrayBindingPattern,
            start,
            end,
            flags,
        );
        BindingName::BindingPattern(node)
    }

    fn parse_object_binding_pattern(&mut self) -> BindingName<'a> {
        let start = self.pos();
        let kind_token = self.alloc_token(SyntaxKind::OpenBraceToken, self.token.span);
        self.expect(SyntaxKind::OpenBraceToken);
        let mut elements = Vec::new();
        let mut has_trailing_comma = false;
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            elements.push(self.parse_binding_element());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            has_trailing_comma = self.at(SyntaxKind::CloseBraceToken);
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);
        let elements = self.arena.alloc_slice(&elements);
        let end = self.node_end();
        let flags = if has_trailing_comma {
            tsr_ast::NodeFlags::HAS_TRAILING_COMMA
        } else {
            tsr_ast::NodeFlags::empty()
        };
        let node = self.finish_node_with_flags(
            BindingPattern::new(kind_token, elements),
            SyntaxKind::ObjectBindingPattern,
            start,
            end,
            flags,
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

        // `{ a: b }` renames; `{ a }` does not. A keyword property that
        // renames is legal — `{ enum: e }` — because upstream reads the
        // property with `parsePropertyName`, an identifier-name position
        // (`declarationEmitKeywordDestructuring`).
        //
        // **And a RESERVED word takes this branch whatever follows it.**
        // `parseObjectBindingElement` (`parser.go:1687`) branches on
        // `tokenIsIdentifier && p.token != KindColonToken`, where
        // `tokenIsIdentifier` is `isBindingIdentifier()` (`:6262`) — *"the
        // token is an identifier, or is past the last reserved word"* — and is
        // read **before** the property name is parsed. So `{ while }` takes the
        // `else`, which reports a single `':' expected` on the `}`; this port
        // asked only whether a colon followed, sent `while` to
        // `parse_binding_name`, and got `Identifier expected` at the keyword
        // instead. §202.
        let binding_identifier = self.token.kind == SyntaxKind::Identifier
            || (self.token.kind as u16) > (SyntaxKind::LAST_RESERVED_WORD as u16);
        let keyword_renames = self.token.kind.is_keyword()
            && (!binding_identifier || self.peek_kind(|kind| kind == SyntaxKind::ColonToken));
        let (property_name, name) = if bracket_is_computed_key
            || keyword_renames
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
        let mut parameters = Vec::new();
        while !self.at(SyntaxKind::CloseParenToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            parameters.push(self.parse_parameter());
            if self.eat(SyntaxKind::CommaToken) {
                if self.pos() == before {
                    break;
                }
                continue;
            }
            // `parseDelimitedList` (`parser.go:664`) reports the missing
            // separator and **continues the list**; this loop used to leave it,
            // which turned one recovery into a cascade —
            // `constructor(...public rest: string[])` is a single `',' expected`
            // upstream and was four errors here (§198).
            //
            // **Continuing is guarded, and the guard is a deliberate SUBSET of
            // upstream's.** `isListElement` for `PCParameters` is
            // `isStartOfParameter` (`parser.go:886`), whose last disjunct is
            // `isStartOfType` — a predicate this port does not have. Porting the
            // continue *without* any guard was §198, and it invented a parameter
            // from whatever token was there: 64 valid files gained
            // `TS1003 Identifier expected`.
            //
            // What is admitted here is the unambiguous half. Everything else
            // falls back to the `break` this loop already did, so the change can
            // only turn an abort into a continue where a parameter genuinely
            // follows — it cannot manufacture one. §200.
            if !starts_parameter(self.token.kind) {
                break;
            }
            self.expect(SyntaxKind::CommaToken);
            // `if startPos == p.nodePos() { p.nextToken() }` (`parser.go:686`):
            // an element that consumed nothing must not spin the loop.
            if self.pos() == before {
                self.next_token();
            }
        }
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
        let modifiers = self.parse_modifiers();
        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        // A `this` parameter is the one reserved word a parameter name admits:
        // upstream's `parseParameter` reads it with `parseIdentifierName`
        // (`parser.go`, the `KindThisKeyword` arm).
        // Only as the plain first parameter: `...this` is upstream's missing
        // identifier plus an error (`thisTypeInFunctionsNegative`).
        let name = if self.at(SyntaxKind::ThisKeyword) && dot_dot_dot.is_none() {
            BindingName::Identifier(self.parse_identifier_name())
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
