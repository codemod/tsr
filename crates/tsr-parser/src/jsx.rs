//! JSX elements, fragments, attributes, and children.
//!
//! JSX is the one place the parser must drive the scanner's mode explicitly.
//! Children are scanned as literal text, attribute values as raw strings, and
//! names may contain `-` — none of which the scanner can decide on its own,
//! because all three depend on where in a JSX element the cursor is.
//!
//! Ported from typescript-go's JSX parsing in `internal/parser/parser.go`.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a JSX element or fragment, starting at `<`.
    pub(crate) fn parse_jsx_element(&mut self) -> Expression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::LessThanToken);

        // `<>…</>` is a fragment: no tag name.
        if self.at(SyntaxKind::GreaterThanToken) {
            let opening =
                self.finish_node(JsxOpeningFragment::new(), SyntaxKind::JsxOpeningFragment, start);
            // The `>` closes the opening fragment and the next token is a child,
            // so it must be scanned under JSX rules.
            self.scan_jsx_token();

            let children = self.parse_jsx_children();
            let closing_start = self.pos();
            self.expect(SyntaxKind::LessThanSlashToken);
            self.expect(SyntaxKind::GreaterThanToken);
            let closing = self.finish_node(
                JsxClosingFragment::new(),
                SyntaxKind::JsxClosingFragment,
                closing_start,
            );
            let children = self.arena.alloc_slice(&children);
            return Expression::JsxFragment(self.finish_node(
                JsxFragment::new(Some(opening), children, Some(closing)),
                SyntaxKind::JsxFragment,
                start,
            ));
        }

        let tag_name = self.parse_jsx_tag_name();
        let type_arguments = if self.at(SyntaxKind::LessThanToken) {
            self.try_parse(Parser::parse_type_arguments_for_call).unwrap_or_default()
        } else {
            Vec::new()
        };
        let attributes = self.parse_jsx_attributes();
        let type_arguments = self.arena.alloc_slice(&type_arguments);

        // `<div />` — self-closing, no children.
        if self.at(SyntaxKind::SlashToken) {
            self.next_token();
            self.expect(SyntaxKind::GreaterThanToken);
            return Expression::JsxSelfClosingElement(self.finish_node(
                JsxSelfClosingElement::new(Some(tag_name), type_arguments, Some(attributes)),
                SyntaxKind::JsxSelfClosingElement,
                start,
            ));
        }

        let opening = self.finish_node(
            JsxOpeningElement::new(Some(tag_name), type_arguments, Some(attributes)),
            SyntaxKind::JsxOpeningElement,
            start,
        );
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.error_at_current_with(&messages::_0_EXPECTED, &[">"]);
        }
        // Past the `>`, everything is child content until `</`.
        self.scan_jsx_token();

        let children = self.parse_jsx_children();
        let closing_start = self.pos();
        self.expect(SyntaxKind::LessThanSlashToken);
        let closing_name = self.parse_jsx_tag_name();
        self.expect(SyntaxKind::GreaterThanToken);
        let closing = self.finish_node(
            JsxClosingElement::new(Some(closing_name)),
            SyntaxKind::JsxClosingElement,
            closing_start,
        );

        let children = self.arena.alloc_slice(&children);
        Expression::JsxElement(self.finish_node(
            JsxElement::new(Some(opening), children, Some(closing)),
            SyntaxKind::JsxElement,
            start,
        ))
    }

    /// `div`, `My.Component`, `svg:circle`, `this`.
    fn parse_jsx_tag_name(&mut self) -> JsxTagNameExpression<'a> {
        let start = self.pos();

        let mut expression = if self.at(SyntaxKind::ThisKeyword) {
            self.next_token();
            Expression::KeywordExpression(self.finish_node(
                KeywordExpression::new(SyntaxKind::ThisKeyword),
                SyntaxKind::ThisKeyword,
                start,
            ))
        } else {
            let name = self.parse_jsx_name();

            // `ns:name` — a namespaced name, distinct from a member access.
            if self.at(SyntaxKind::ColonToken) {
                self.next_token();
                let local = self.parse_jsx_name();
                return JsxTagNameExpression::JsxNamespacedName(self.finish_node(
                    JsxNamespacedName::new(Some(name), Some(local)),
                    SyntaxKind::JsxNamespacedName,
                    start,
                ));
            }

            Expression::Identifier(name)
        };

        // `A.B.C` and `this.dynamic` — member chains.
        while self.at(SyntaxKind::DotToken) {
            self.next_token();
            let member = self.parse_jsx_name();
            expression = Expression::PropertyAccessExpression(self.finish_node(
                PropertyAccessExpression::new(
                    Some(expression),
                    None,
                    Some(MemberName::Identifier(member)),
                ),
                SyntaxKind::PropertyAccessExpression,
                start,
            ));
        }

        match expression {
            Expression::Identifier(identifier) => JsxTagNameExpression::Identifier(identifier),
            Expression::KeywordExpression(keyword) => {
                JsxTagNameExpression::KeywordExpression(keyword)
            }
            Expression::PropertyAccessExpression(access) => {
                JsxTagNameExpression::PropertyAccessExpression(access)
            }
            _ => JsxTagNameExpression::Identifier(self.missing_identifier()),
        }
    }

    /// An identifier that may contain `-`.
    fn parse_jsx_name(&mut self) -> &'a Identifier<'a> {
        let start = self.pos();
        if !self.at(SyntaxKind::Identifier) && !self.token.kind.is_keyword() {
            self.error_at_current(&messages::IDENTIFIER_EXPECTED);
            return self.missing_identifier();
        }
        // Extend the token in place before consuming it: `data-foo` is one name.
        self.scan_jsx_identifier();
        let text = self.token_value();
        self.next_token();
        self.finish_node(Identifier::new(text), SyntaxKind::Identifier, start)
    }

    /// `class="a" {...rest} disabled`.
    fn parse_jsx_attributes(&mut self) -> &'a JsxAttributes<'a> {
        let start = self.pos();
        let mut properties = Vec::new();

        while !matches!(
            self.token.kind,
            SyntaxKind::GreaterThanToken | SyntaxKind::SlashToken | SyntaxKind::EndOfFile
        ) {
            let before = self.pos();
            let attribute_start = self.pos();

            if self.at(SyntaxKind::OpenBraceToken) {
                self.next_token();
                self.expect(SyntaxKind::DotDotDotToken);
                let expression = self.parse_assignment_expression();
                self.expect(SyntaxKind::CloseBraceToken);
                properties.push(JsxAttributeLike::JsxSpreadAttribute(self.finish_node(
                    JsxSpreadAttribute::new(Some(expression)),
                    SyntaxKind::JsxSpreadAttribute,
                    attribute_start,
                )));
            } else {
                let name = self.parse_jsx_attribute_name();
                // A bare attribute means `true`: `<input disabled />`.
                let initializer = if self.at(SyntaxKind::EqualsToken) {
                    self.next_token();
                    Some(self.parse_jsx_attribute_value())
                } else {
                    None
                };
                properties.push(JsxAttributeLike::JsxAttribute(self.finish_node(
                    JsxAttribute::new(Some(name), initializer),
                    SyntaxKind::JsxAttribute,
                    attribute_start,
                )));
            }

            if self.pos() == before {
                self.error_at_current(&messages::UNEXPECTED_TOKEN);
                self.next_token();
            }
        }

        let properties = self.arena.alloc_slice(&properties);
        self.finish_node(JsxAttributes::new(properties), SyntaxKind::JsxAttributes, start)
    }

    fn parse_jsx_attribute_name(&mut self) -> JsxAttributeName<'a> {
        let start = self.pos();
        let name = self.parse_jsx_name();
        if self.at(SyntaxKind::ColonToken) {
            self.next_token();
            let local = self.parse_jsx_name();
            return JsxAttributeName::JsxNamespacedName(self.finish_node(
                JsxNamespacedName::new(Some(name), Some(local)),
                SyntaxKind::JsxNamespacedName,
                start,
            ));
        }
        JsxAttributeName::Identifier(name)
    }

    /// `"text"`, `{expr}`, or a nested element.
    fn parse_jsx_attribute_value(&mut self) -> JsxAttributeValue<'a> {
        // Re-scanned under JSX rules: a quoted value is raw, so `"a\b"` keeps its
        // backslash rather than reporting a bad escape. A rescan rather than a
        // scan because the parser's lookahead has already read the token.
        self.rescan_jsx_attribute_value();

        let start = self.pos();
        match self.token.kind {
            SyntaxKind::StringLiteral => {
                let text = self.token_value();
                let flags = self.token.ast_flags();
                self.next_token();
                JsxAttributeValue::StringLiteral(self.finish_node(
                    StringLiteral::new(text, flags),
                    SyntaxKind::StringLiteral,
                    start,
                ))
            }
            SyntaxKind::OpenBraceToken => {
                JsxAttributeValue::JsxExpression(self.parse_jsx_expression())
            }
            SyntaxKind::LessThanToken => match self.parse_jsx_element() {
                Expression::JsxElement(element) => JsxAttributeValue::JsxElement(element),
                Expression::JsxSelfClosingElement(element) => {
                    JsxAttributeValue::JsxSelfClosingElement(element)
                }
                Expression::JsxFragment(fragment) => JsxAttributeValue::JsxFragment(fragment),
                _ => unreachable!("parse_jsx_element yields only JSX"),
            },
            _ => {
                self.error_at_current(&messages::EXPRESSION_EXPECTED);
                JsxAttributeValue::JsxExpression(self.finish_node(
                    JsxExpression::new(None, None),
                    SyntaxKind::JsxExpression,
                    start,
                ))
            }
        }
    }

    /// `{expr}`, `{...expr}`, or `{}`.
    ///
    /// The cursor is on `{`, and the token after `}` belongs to whatever encloses
    /// the expression — a child position or an attribute list — so the caller
    /// rescans rather than this function guessing.
    fn parse_jsx_expression(&mut self) -> &'a JsxExpression<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);

        let dot_dot_dot =
            if self.at(SyntaxKind::DotDotDotToken) { Some(self.take_token()) } else { None };
        // `{}` is legal and means nothing, which is why the expression is optional.
        let expression =
            if self.at(SyntaxKind::CloseBraceToken) { None } else { Some(self.parse_expression()) };
        self.expect(SyntaxKind::CloseBraceToken);

        self.finish_node(
            JsxExpression::new(dot_dot_dot, expression),
            SyntaxKind::JsxExpression,
            start,
        )
    }

    /// Children, up to the closing `</`.
    ///
    /// Every iteration re-enters JSX scanning, because parsing an expression
    /// container or a nested element leaves the cursor holding a token scanned
    /// under expression rules.
    fn parse_jsx_children(&mut self) -> Vec<JsxChild<'a>> {
        let mut children = Vec::new();

        loop {
            match self.token.kind {
                SyntaxKind::LessThanSlashToken | SyntaxKind::EndOfFile => break,
                SyntaxKind::JsxText | SyntaxKind::JsxTextAllWhiteSpaces => {
                    let start = self.pos();
                    let only_whitespace = self.at(SyntaxKind::JsxTextAllWhiteSpaces);
                    let text = self.token_text();
                    let flags = self.token.ast_flags();
                    let kind = self.token.kind;
                    self.scan_jsx_token();
                    children.push(JsxChild::JsxText(self.finish_node(
                        JsxText::new(text, only_whitespace, flags),
                        kind,
                        start,
                    )));
                }
                SyntaxKind::OpenBraceToken => {
                    let expression = self.parse_jsx_expression();
                    children.push(JsxChild::JsxExpression(expression));
                    // The token after `}` is child content again.
                    self.rescan_jsx_token();
                }
                SyntaxKind::LessThanToken => {
                    match self.parse_jsx_element() {
                        Expression::JsxElement(element) => {
                            children.push(JsxChild::JsxElement(element));
                        }
                        Expression::JsxSelfClosingElement(element) => {
                            children.push(JsxChild::JsxSelfClosingElement(element));
                        }
                        Expression::JsxFragment(fragment) => {
                            children.push(JsxChild::JsxFragment(fragment));
                        }
                        _ => unreachable!("parse_jsx_element yields only JSX"),
                    }
                    self.rescan_jsx_token();
                }
                _ => {
                    // Not child content; the element is malformed. Bail rather
                    // than loop, and let the caller report the missing `</`.
                    break;
                }
            }
        }

        children
    }
}
