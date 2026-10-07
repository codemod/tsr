//! JSX elements, fragments, attributes, and children.
//!
//! JSX is the one place the parser must drive the scanner's mode explicitly.
//! Children are scanned as literal text, attribute values as raw strings, and
//! names may contain `-` — none of which the scanner can decide on its own,
//! because all three depend on where in a JSX element the cursor is.
//!
//! Ported function-for-function from typescript-go's JSX parsing in
//! `internal/parser/parser.go` (pinned 5b1047d,
//! `parseJsxElementOrSelfClosingElementOrFragment` through
//! `parseJsxClosingFragment`), including its error recovery: the children
//! list runs under `PCJsxChildren`, attributes under `PCJsxAttributes`, and an
//! unclosed child that swallowed its parent's closing tag is restructured
//! rather than reported twice. `docs/parity/notes/jsx.md`.
//!
//! One representational difference: upstream's scanner only ever scans a lone
//! `>` (compounds come from `reScanGreaterToken`), while this scanner joins
//! `>>`/`>=` greedily, so every test for `>` here splits a compound first
//! ([`Parser::at_jsx_greater_than`]).

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::list::ParsingContext;
use crate::parser::Parser;

/// The `openingTag` upstream threads through children parsing: an opening
/// element or an opening fragment.
#[derive(Clone, Copy)]
enum JsxOpeningTag<'a> {
    Element(&'a JsxOpeningElement<'a>),
    /// With the fragment's full start: upstream reports an unclosed fragment
    /// at `openingTag.Loc`, whose `pos` includes leading trivia, while this
    /// port's spans start at the token.
    Fragment(&'a JsxOpeningFragment<'a>, u32),
}

/// What `parseJsxOpeningOrSelfClosingElementOrOpeningFragment` produced.
enum JsxOpening<'a> {
    Element(&'a JsxOpeningElement<'a>),
    SelfClosing(&'a JsxSelfClosingElement<'a>),
    Fragment(&'a JsxOpeningFragment<'a>, u32),
}

impl<'a> Parser<'a> {
    /// typescript-go's `Parser.parseJsxElementOrSelfClosingElementOrFragment`
    /// (`parser.go:4736`). `top_invalid_node_position` is upstream's `-1`-or-
    /// position as an `Option`; `opening_tag` is the enclosing element when
    /// this is a child.
    pub(crate) fn parse_jsx_element_or_self_closing_element_or_fragment(
        &mut self,
        in_expression_context: bool,
        top_invalid_node_position: Option<u32>,
        must_be_unary: bool,
    ) -> Expression<'a> {
        self.parse_jsx_element_worker(
            in_expression_context,
            top_invalid_node_position,
            None,
            must_be_unary,
        )
    }

    fn parse_jsx_element_worker(
        &mut self,
        in_expression_context: bool,
        top_invalid_node_position: Option<u32>,
        opening_tag: Option<JsxOpeningTag<'a>>,
        must_be_unary: bool,
    ) -> Expression<'a> {
        let pos = self.pos();
        let opening = self
            .parse_jsx_opening_or_self_closing_element_or_opening_fragment(in_expression_context);
        let mut result = match opening {
            JsxOpening::Element(opening) => {
                let mut children = self.parse_jsx_children(JsxOpeningTag::Element(opening));
                let closing_element;
                let restructure = match children.last() {
                    Some(JsxChild::JsxElement(last)) => {
                        !tag_names_are_equivalent(
                            jsx_opening_tag_name(last.opening_element),
                            jsx_closing_tag_name(last.closing_element),
                        ) && tag_names_are_equivalent(
                            opening.tag_name,
                            jsx_closing_tag_name(last.closing_element),
                        )
                    }
                    _ => false,
                };
                if restructure {
                    let Some(JsxChild::JsxElement(last)) = children.pop() else { unreachable!() };
                    // When an unclosed JsxOpeningElement incorrectly parses its
                    // parent's JsxClosingElement, restructure
                    // (<div>(...<span>...</div>)) --> (<div>(...<span>...</>)</div>)
                    // (no need to error; the parent will error).
                    let end = self.jsx_children_end(last);
                    let missing = self.finish_node_with_end(
                        Identifier::new(""),
                        SyntaxKind::Identifier,
                        end,
                        end,
                    );
                    let new_closing = self.finish_node_with_end(
                        JsxClosingElement::new(Some(JsxTagNameExpression::Identifier(missing))),
                        SyntaxKind::JsxClosingElement,
                        end,
                        end,
                    );
                    let new_start = last
                        .opening_element
                        .and_then(|o| o.node_id)
                        .map_or(end, |id| self.nodes.span(id).start);
                    let new_last = self.finish_node_with_end(
                        JsxElement::new(last.opening_element, last.children, Some(new_closing)),
                        SyntaxKind::JsxElement,
                        new_start,
                        end,
                    );
                    children.push(JsxChild::JsxElement(new_last));
                    closing_element = last.closing_element.unwrap_or(new_closing);
                } else {
                    closing_element =
                        self.parse_jsx_closing_element(opening, in_expression_context);
                    if !tag_names_are_equivalent(opening.tag_name, closing_element.tag_name) {
                        let opening_text = self.jsx_tag_name_text(opening.tag_name);
                        match opening_tag {
                            Some(JsxOpeningTag::Element(parent))
                                if tag_names_are_equivalent(
                                    closing_element.tag_name,
                                    parent.tag_name,
                                ) =>
                            {
                                // Opening incorrectly matched with its parent's
                                // closing -- put error on opening.
                                let span = self.jsx_tag_name_span(opening.tag_name);
                                self.error_at_with(
                                    &messages::JSX_ELEMENT_0_HAS_NO_CORRESPONDING_CLOSING_TAG,
                                    span,
                                    &[opening_text],
                                );
                            }
                            _ => {
                                // Other opening/closing mismatches -- put error
                                // on closing.
                                let span = self.jsx_tag_name_span(closing_element.tag_name);
                                self.error_at_with(
                                    &messages::EXPECTED_CORRESPONDING_JSX_CLOSING_TAG_FOR_0,
                                    span,
                                    &[opening_text],
                                );
                            }
                        }
                    }
                }
                let children = self.arena.alloc_slice(&children);
                Expression::JsxElement(self.finish_node(
                    JsxElement::new(Some(opening), children, Some(closing_element)),
                    SyntaxKind::JsxElement,
                    pos,
                ))
            }
            JsxOpening::Fragment(opening, full_start) => {
                let children =
                    self.parse_jsx_children(JsxOpeningTag::Fragment(opening, full_start));
                let children = self.arena.alloc_slice(&children);
                let closing = self.parse_jsx_closing_fragment(in_expression_context);
                Expression::JsxFragment(self.finish_node(
                    JsxFragment::new(Some(opening), children, Some(closing)),
                    SyntaxKind::JsxFragment,
                    pos,
                ))
            }
            // Nothing else to do for self-closing elements.
            JsxOpening::SelfClosing(element) => Expression::JsxSelfClosingElement(element),
        };
        // If the user writes the invalid code '<div></div><div></div>' in an
        // expression context (i.e. not wrapped in an enclosing tag), we'll
        // naively try to parse this as a 'less than' operator and the
        // remainder of the tag as garbage. Perform a speculative parse of a
        // JSX element if we see a < token so that we can wrap it in a
        // synthetic binary expression and report a better error. In a unary
        // context the binary expression would not be a valid operand.
        if !must_be_unary && in_expression_context && self.at(SyntaxKind::LessThanToken) {
            let top_bad_pos = top_invalid_node_position.unwrap_or(pos);
            let invalid_element =
                self.parse_jsx_element_worker(true, Some(top_bad_pos), None, false);
            let invalid_start =
                invalid_element.node_id().map_or(self.pos(), |id| self.nodes.span(id).start);
            let operator =
                self.alloc_token(SyntaxKind::CommaToken, tsr_core::Span::at(invalid_start));
            let end = self.node_end();
            self.error_at(
                &messages::JSX_EXPRESSIONS_MUST_HAVE_ONE_PARENT_ELEMENT,
                tsr_core::Span::new(top_bad_pos, end),
            );
            result = Expression::BinaryExpression(self.finish_node(
                BinaryExpression::new(
                    &[],
                    Some(result),
                    None,
                    Some(operator),
                    Some(invalid_element),
                ),
                SyntaxKind::BinaryExpression,
                pos,
            ));
        }
        result
    }

    /// Where `last`'s children list ended: upstream's
    /// `lastChild.Children().End()`, the parser's position when its
    /// `parseJsxChildren` returned — the end of its last child, or of its
    /// opening element when it has none.
    fn jsx_children_end(&self, element: &'a JsxElement<'a>) -> u32 {
        let last = element.children.last().and_then(JsxChild::node_id);
        let opening = element.opening_element.and_then(|o| o.node_id);
        last.or(opening).map_or_else(|| self.node_end(), |id| self.nodes.span(id).end)
    }

    /// typescript-go's `Parser.parseJsxChildren` (`parser.go:4816`).
    fn parse_jsx_children(&mut self, opening_tag: JsxOpeningTag<'a>) -> Vec<JsxChild<'a>> {
        let saved = self.parsing_contexts;
        self.parsing_contexts |= ParsingContext::JsxChildren.bit();
        let mut list = Vec::new();
        loop {
            self.rescan_jsx_token();
            let Some(child) = self.parse_jsx_child(opening_tag) else { break };
            list.push(child);
            if let (JsxOpeningTag::Element(opening), JsxChild::JsxElement(child)) =
                (opening_tag, child)
                && !tag_names_are_equivalent(
                    jsx_opening_tag_name(child.opening_element),
                    jsx_closing_tag_name(child.closing_element),
                )
                && tag_names_are_equivalent(
                    opening.tag_name,
                    jsx_closing_tag_name(child.closing_element),
                )
            {
                // Stop after parsing a mismatched child like
                // <div>...(<span></div>) in order to reattach the </div> higher.
                break;
            }
        }
        self.parsing_contexts = saved;
        list
    }

    /// typescript-go's `Parser.parseJsxChild` (`parser.go:4839`).
    fn parse_jsx_child(&mut self, opening_tag: JsxOpeningTag<'a>) -> Option<JsxChild<'a>> {
        match self.token.kind {
            SyntaxKind::EndOfFile => {
                // If we hit EOF, issue the error at the tag that lacks the
                // closing element rather than at the end of the file.
                match opening_tag {
                    JsxOpeningTag::Fragment(fragment, full_start) => {
                        let end = fragment.node_id.map_or(full_start, |id| self.nodes.span(id).end);
                        let span = tsr_core::Span::new(full_start, end);
                        self.error_at(
                            &messages::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
                            span,
                        );
                    }
                    JsxOpeningTag::Element(element) => {
                        // Cover only 'Foo.Bar' in < Foo.Bar >.
                        let span = self.jsx_tag_name_span(element.tag_name);
                        let text = self.jsx_tag_name_text(element.tag_name);
                        self.error_at_with(
                            &messages::JSX_ELEMENT_0_HAS_NO_CORRESPONDING_CLOSING_TAG,
                            span,
                            &[text],
                        );
                    }
                }
                None
            }
            SyntaxKind::LessThanSlashToken | SyntaxKind::ConflictMarkerTrivia => None,
            SyntaxKind::JsxText | SyntaxKind::JsxTextAllWhiteSpaces => {
                Some(JsxChild::JsxText(self.parse_jsx_text()))
            }
            SyntaxKind::OpenBraceToken => {
                self.parse_jsx_expression(false).map(JsxChild::JsxExpression)
            }
            SyntaxKind::LessThanToken => {
                match self.parse_jsx_element_worker(false, None, Some(opening_tag), false) {
                    Expression::JsxElement(element) => Some(JsxChild::JsxElement(element)),
                    Expression::JsxSelfClosingElement(element) => {
                        Some(JsxChild::JsxSelfClosingElement(element))
                    }
                    Expression::JsxFragment(fragment) => Some(JsxChild::JsxFragment(fragment)),
                    _ => unreachable!("a child JSX element is never wrapped in a binary"),
                }
            }
            kind => unreachable!("rescanned JSX token {kind:?} is not a child"),
        }
    }

    /// typescript-go's `Parser.parseJsxText` (`parser.go:4867`).
    fn parse_jsx_text(&mut self) -> &'a JsxText<'a> {
        let pos = self.pos();
        let only_whitespace = self.at(SyntaxKind::JsxTextAllWhiteSpaces);
        let text = self.token_text();
        let flags = self.token.ast_flags();
        let kind = self.token.kind;
        self.scan_jsx_token();
        self.finish_node(JsxText::new(text, only_whitespace, flags), kind, pos)
    }

    /// typescript-go's `Parser.parseJsxExpression` (`parser.go:4874`).
    fn parse_jsx_expression(
        &mut self,
        in_expression_context: bool,
    ) -> Option<&'a JsxExpression<'a>> {
        let pos = self.pos();
        if !self.expect(SyntaxKind::OpenBraceToken) {
            return None;
        }
        let mut dot_dot_dot = None;
        let mut expression = None;
        if !self.at(SyntaxKind::CloseBraceToken) {
            if !in_expression_context && self.at(SyntaxKind::DotDotDotToken) {
                dot_dot_dot = Some(self.take_token());
            }
            // Only an AssignmentExpression is valid here per the JSX spec,
            // but we can unambiguously parse a comma sequence and provide a
            // better error message in grammar checking.
            expression = Some(self.parse_expression());
        }
        if in_expression_context {
            self.expect(SyntaxKind::CloseBraceToken);
        } else if self.expect_without_advancing(SyntaxKind::CloseBraceToken) {
            self.scan_jsx_token();
        }
        Some(self.finish_node(
            JsxExpression::new(dot_dot_dot, expression),
            SyntaxKind::JsxExpression,
            pos,
        ))
    }

    /// typescript-go's `Parser.parseJsxClosingElement` (`parser.go:4913`).
    fn parse_jsx_closing_element(
        &mut self,
        open: &'a JsxOpeningElement<'a>,
        in_expression_context: bool,
    ) -> &'a JsxClosingElement<'a> {
        let pos = self.pos();
        self.expect(SyntaxKind::LessThanSlashToken);
        let tag_name = self.parse_jsx_element_name();
        if self.expect_jsx_greater_than_without_advancing(None) {
            // Manually advance the scanner in order to look for JSX text
            // inside JSX.
            if in_expression_context || !tag_names_are_equivalent(open.tag_name, Some(tag_name)) {
                self.next_token();
            } else {
                self.scan_jsx_token();
            }
        }
        self.finish_node(JsxClosingElement::new(Some(tag_name)), SyntaxKind::JsxClosingElement, pos)
    }

    /// typescript-go's
    /// `Parser.parseJsxOpeningOrSelfClosingElementOrOpeningFragment`
    /// (`parser.go:4928`).
    fn parse_jsx_opening_or_self_closing_element_or_opening_fragment(
        &mut self,
        in_expression_context: bool,
    ) -> JsxOpening<'a> {
        let full_start = self.node_end();
        let pos = self.pos();
        self.expect(SyntaxKind::LessThanToken);
        if self.at_jsx_greater_than() {
            // See below for explanation of scanJsxText.
            self.scan_jsx_token();
            let fragment =
                self.finish_node(JsxOpeningFragment::new(), SyntaxKind::JsxOpeningFragment, pos);
            return JsxOpening::Fragment(fragment, full_start);
        }
        let tag_name = self.parse_jsx_element_name();
        // Native parseJsxOpeningOrSelfClosingElementOrOpeningFragment:
        // JS/JSX use JSX grammar but never consume TypeScript type arguments.
        let (type_arguments, list_span) = if self.script_kind.is_javascript() {
            (Vec::new(), None)
        } else {
            self.parse_type_arguments()
        };
        let type_arguments = self.arena.alloc_slice(&type_arguments);
        let attributes = self.parse_jsx_attributes();
        if self.at_jsx_greater_than() {
            // Closing tag, so scan the immediately-following text with the JSX
            // scanning instead of regular scanning to avoid treating illegal
            // characters (e.g. '#') as immediate scanning errors.
            self.scan_jsx_token();
            let node = self.finish_node(
                JsxOpeningElement::new(Some(tag_name), type_arguments, Some(attributes)),
                SyntaxKind::JsxOpeningElement,
                pos,
            );
            self.nodes.set_type_argument_list_span(node.node_id().unwrap(), list_span);
            return JsxOpening::Element(node);
        }
        self.expect(SyntaxKind::SlashToken);
        if self.expect_jsx_greater_than_without_advancing(None) {
            if in_expression_context {
                self.next_token();
            } else {
                self.scan_jsx_token();
            }
        }
        let node = self.finish_node(
            JsxSelfClosingElement::new(Some(tag_name), type_arguments, Some(attributes)),
            SyntaxKind::JsxSelfClosingElement,
            pos,
        );
        self.nodes.set_type_argument_list_span(node.node_id().unwrap(), list_span);
        JsxOpening::SelfClosing(node)
    }

    /// typescript-go's `Parser.parseJsxElementName` (`parser.go:4963`).
    fn parse_jsx_element_name(&mut self) -> JsxTagNameExpression<'a> {
        let pos = self.pos();
        // JsxElement can have name in the form of a property access
        // expression, or a primary expression in the form of an identifier
        // and "this" keyword.
        let initial = self.parse_jsx_tag_name();
        if let JsxTagNameExpression::JsxNamespacedName(_) = initial {
            // `a:b.c` is invalid syntax, don't even look for the `.` if we
            // parse `a:b`, and let `parseAttribute` report "unexpected :".
            return initial;
        }
        let mut expression = initial;
        while self.eat(SyntaxKind::DotToken) {
            let name = self.parse_jsx_right_side_of_dot();
            let object = match expression {
                JsxTagNameExpression::Identifier(identifier) => Expression::Identifier(identifier),
                JsxTagNameExpression::KeywordExpression(keyword) => {
                    Expression::KeywordExpression(keyword)
                }
                JsxTagNameExpression::PropertyAccessExpression(access) => {
                    Expression::PropertyAccessExpression(access)
                }
                JsxTagNameExpression::JsxNamespacedName(_) => unreachable!("returned above"),
            };
            expression = JsxTagNameExpression::PropertyAccessExpression(self.finish_node(
                PropertyAccessExpression::new(
                    Some(object),
                    None,
                    Some(MemberName::Identifier(name)),
                ),
                SyntaxKind::PropertyAccessExpression,
                pos,
            ));
        }
        expression
    }

    /// `parseRightSideOfDot(allowIdentifierNames=true,
    /// allowPrivateIdentifiers=false, allowUnicodeEscapeSequenceInIdentifierName=false)`
    /// (`parser.go:2920`), the arguments `parseJsxElementName` passes.
    fn parse_jsx_right_side_of_dot(&mut self) -> &'a Identifier<'a> {
        if self.right_side_of_dot_is_missing() {
            self.error_at(&messages::IDENTIFIER_EXPECTED, tsr_core::Span::at(self.node_end()));
            return self.missing_identifier();
        }
        if self.at(SyntaxKind::PrivateIdentifier) {
            self.next_token();
            self.error_at(&messages::IDENTIFIER_EXPECTED, tsr_core::Span::at(self.node_end()));
            return self.missing_identifier();
        }
        self.parse_identifier_name_error_on_unicode_escape_sequence()
    }

    /// typescript-go's `Parser.parseJsxTagName` (`parser.go:4981`).
    fn parse_jsx_tag_name(&mut self) -> JsxTagNameExpression<'a> {
        let pos = self.pos();
        self.scan_jsx_identifier();
        let is_this = self.at(SyntaxKind::ThisKeyword);
        let tag_name = self.parse_identifier_name_error_on_unicode_escape_sequence();
        if self.eat(SyntaxKind::ColonToken) {
            self.scan_jsx_identifier();
            let name = self.parse_identifier_name_error_on_unicode_escape_sequence();
            return JsxTagNameExpression::JsxNamespacedName(self.finish_node(
                JsxNamespacedName::new(Some(tag_name), Some(name)),
                SyntaxKind::JsxNamespacedName,
                pos,
            ));
        }
        if is_this {
            return JsxTagNameExpression::KeywordExpression(self.finish_node(
                KeywordExpression::new(SyntaxKind::ThisKeyword),
                SyntaxKind::ThisKeyword,
                pos,
            ));
        }
        JsxTagNameExpression::Identifier(tag_name)
    }

    /// typescript-go's `Parser.parseIdentifierNameErrorOnUnicodeEscapeSequence`
    /// (`parser.go:5803`).
    fn parse_identifier_name_error_on_unicode_escape_sequence(&mut self) -> &'a Identifier<'a> {
        if self.token.flags.contains(tsr_scanner::TokenFlags::UNICODE_ESCAPE) {
            self.error_at_current(&messages::UNICODE_ESCAPE_SEQUENCE_CANNOT_APPEAR_HERE);
        }
        self.parse_identifier_name()
    }

    /// typescript-go's `Parser.parseJsxAttributes` (`parser.go:4997`).
    fn parse_jsx_attributes(&mut self) -> &'a JsxAttributes<'a> {
        let pos = self.pos();
        let properties = self.parse_list(ParsingContext::JsxAttributes, Self::parse_jsx_attribute);
        let properties = self.arena.alloc_slice(&properties);
        self.finish_node(JsxAttributes::new(properties), SyntaxKind::JsxAttributes, pos)
    }

    /// typescript-go's `Parser.parseJsxAttribute` (`parser.go:5002`).
    fn parse_jsx_attribute(&mut self) -> JsxAttributeLike<'a> {
        if self.at(SyntaxKind::OpenBraceToken) {
            return JsxAttributeLike::JsxSpreadAttribute(self.parse_jsx_spread_attribute());
        }
        let pos = self.pos();
        let name = self.parse_jsx_attribute_name();
        let initializer = self.parse_jsx_attribute_value();
        JsxAttributeLike::JsxAttribute(self.finish_node(
            JsxAttribute::new(Some(name), initializer),
            SyntaxKind::JsxAttribute,
            pos,
        ))
    }

    /// typescript-go's `Parser.parseJsxSpreadAttribute` (`parser.go:5010`).
    fn parse_jsx_spread_attribute(&mut self) -> &'a JsxSpreadAttribute<'a> {
        let pos = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        self.expect(SyntaxKind::DotDotDotToken);
        let expression = self.parse_expression();
        self.expect(SyntaxKind::CloseBraceToken);
        self.finish_node(
            JsxSpreadAttribute::new(Some(expression)),
            SyntaxKind::JsxSpreadAttribute,
            pos,
        )
    }

    /// typescript-go's `Parser.parseJsxAttributeName` (`parser.go:5019`).
    fn parse_jsx_attribute_name(&mut self) -> JsxAttributeName<'a> {
        let pos = self.pos();
        self.scan_jsx_identifier();
        let name = self.parse_identifier_name_error_on_unicode_escape_sequence();
        if self.eat(SyntaxKind::ColonToken) {
            self.scan_jsx_identifier();
            let local = self.parse_identifier_name_error_on_unicode_escape_sequence();
            return JsxAttributeName::JsxNamespacedName(self.finish_node(
                JsxNamespacedName::new(Some(name), Some(local)),
                SyntaxKind::JsxNamespacedName,
                pos,
            ));
        }
        JsxAttributeName::Identifier(name)
    }

    /// typescript-go's `Parser.parseJsxAttributeValue` (`parser.go:5030`).
    fn parse_jsx_attribute_value(&mut self) -> Option<JsxAttributeValue<'a>> {
        if !self.at(SyntaxKind::EqualsToken) {
            return None;
        }
        // `scanJsxAttributeValue`: this parser has already scanned the token
        // after `=` under expression rules, so step onto it and rescan it as
        // a raw JSX string (dropping that scan's diagnostics).
        self.next_token();
        self.rescan_jsx_attribute_value();
        let pos = self.pos();
        match self.token.kind {
            SyntaxKind::StringLiteral => {
                let text = self.token_value();
                let flags = self.token.ast_flags();
                self.next_token();
                Some(JsxAttributeValue::StringLiteral(self.finish_node(
                    StringLiteral::new(text, flags),
                    SyntaxKind::StringLiteral,
                    pos,
                )))
            }
            SyntaxKind::OpenBraceToken => {
                self.parse_jsx_expression(true).map(JsxAttributeValue::JsxExpression)
            }
            SyntaxKind::LessThanToken => {
                match self.parse_jsx_element_worker(true, None, None, false) {
                    Expression::JsxElement(element) => Some(JsxAttributeValue::JsxElement(element)),
                    Expression::JsxSelfClosingElement(element) => {
                        Some(JsxAttributeValue::JsxSelfClosingElement(element))
                    }
                    Expression::JsxFragment(fragment) => {
                        Some(JsxAttributeValue::JsxFragment(fragment))
                    }
                    // `<a b=<c/><d/> />`: upstream stores the synthetic
                    // binary as the initializer. The AST's value union has no
                    // arm for it; the diagnostics are already reported.
                    _ => None,
                }
            }
            _ => {
                self.error_at_current(&messages::OR_JSX_ELEMENT_EXPECTED);
                None
            }
        }
    }

    /// typescript-go's `Parser.parseJsxClosingFragment` (`parser.go:5046`).
    fn parse_jsx_closing_fragment(
        &mut self,
        in_expression_context: bool,
    ) -> &'a JsxClosingFragment<'a> {
        let pos = self.pos();
        self.expect(SyntaxKind::LessThanSlashToken);
        if self.expect_jsx_greater_than_without_advancing(Some(
            &messages::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
        )) {
            // Manually advance the scanner in order to look for JSX text
            // inside JSX.
            if in_expression_context {
                self.next_token();
            } else {
                self.scan_jsx_token();
            }
        }
        self.finish_node(JsxClosingFragment::new(), SyntaxKind::JsxClosingFragment, pos)
    }

    /// `parseExpectedWithDiagnostic(KindGreaterThanToken, message, false)`
    /// (`parser.go:1002`), splitting a compound `>` first.
    fn expect_jsx_greater_than_without_advancing(
        &mut self,
        message: Option<&'static tsr_diagnostics::Message>,
    ) -> bool {
        if self.at_jsx_greater_than() {
            return true;
        }
        match message {
            Some(message) => self.error_at_current(message),
            None => self.error_at_current_with(&messages::_0_EXPECTED, &[">"]),
        }
        false
    }

    /// `parseExpectedWithoutAdvancing` (`parser.go:998`).
    fn expect_without_advancing(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            return true;
        }
        self.error_at_current_with(&messages::_0_EXPECTED, &[crate::parser::token_to_text(kind)]);
        false
    }

    /// Whether the cursor is on a single `>`, splitting a compound `>>`/`>=`
    /// first. typescript-go's scanner only ever scans a lone `>` (the
    /// compounds come from `reScanGreaterToken`), so in `<div>></div>` the
    /// second `>` is JSX text; this scanner joins them greedily.
    fn at_jsx_greater_than(&mut self) -> bool {
        self.rescan_greater_than();
        self.at(SyntaxKind::GreaterThanToken)
    }

    /// The span of a tag name, for `parseErrorAtRange(tagName.Loc)`.
    fn jsx_tag_name_span(&self, name: Option<JsxTagNameExpression<'a>>) -> tsr_core::Span {
        name.and_then(|name| name.node_id())
            .map_or(tsr_core::Span::at(self.pos()), |id| self.nodes.span(id))
    }

    /// `scanner.GetTextOfNodeFromSourceText(text, tagName, false)`.
    fn jsx_tag_name_text(&self, name: Option<JsxTagNameExpression<'a>>) -> &'a str {
        let span = self.jsx_tag_name_span(name);
        &self.source[span.start as usize..span.end as usize]
    }

    /// Report at `span` with substitution arguments, through the same
    /// same-position guard as every parser diagnostic.
    fn error_at_with(
        &mut self,
        message: &'static tsr_diagnostics::Message,
        span: tsr_core::Span,
        args: &[&str],
    ) {
        if self.would_repeat_last_error(span) {
            return;
        }
        self.diagnostics.push(tsr_diagnostics::Diagnostic::with_args(
            message,
            span,
            args.iter().map(|s| (*s).to_string()),
        ));
    }
}

fn jsx_opening_tag_name<'a>(
    opening: Option<&'a JsxOpeningElement<'a>>,
) -> Option<JsxTagNameExpression<'a>> {
    opening.and_then(|o| o.tag_name)
}

fn jsx_closing_tag_name<'a>(
    closing: Option<&'a JsxClosingElement<'a>>,
) -> Option<JsxTagNameExpression<'a>> {
    closing.and_then(|c| c.tag_name)
}

/// `ast.TagNamesAreEquivalent` (`internal/ast/utilities.go:4464`): compared
/// structurally on decoded names, so `<a></a>` matches.
fn tag_names_are_equivalent(
    lhs: Option<JsxTagNameExpression<'_>>,
    rhs: Option<JsxTagNameExpression<'_>>,
) -> bool {
    let (Some(lhs), Some(rhs)) = (lhs, rhs) else { return false };
    match (lhs, rhs) {
        (JsxTagNameExpression::Identifier(a), JsxTagNameExpression::Identifier(b)) => {
            a.text == b.text
        }
        (
            JsxTagNameExpression::KeywordExpression(_),
            JsxTagNameExpression::KeywordExpression(_),
        ) => true,
        (
            JsxTagNameExpression::JsxNamespacedName(a),
            JsxTagNameExpression::JsxNamespacedName(b),
        ) => {
            identifier_text(a.namespace) == identifier_text(b.namespace)
                && identifier_text(a.name) == identifier_text(b.name)
        }
        (
            JsxTagNameExpression::PropertyAccessExpression(a),
            JsxTagNameExpression::PropertyAccessExpression(b),
        ) => {
            member_text(a.name) == member_text(b.name)
                && tag_names_are_equivalent(
                    a.expression.and_then(expression_as_tag_name),
                    b.expression.and_then(expression_as_tag_name),
                )
        }
        _ => false,
    }
}

fn identifier_text<'a>(identifier: Option<&'a Identifier<'a>>) -> &'a str {
    identifier.map_or("", |i| i.text)
}

fn member_text(name: Option<MemberName<'_>>) -> &str {
    match name {
        Some(MemberName::Identifier(identifier)) => identifier.text,
        Some(MemberName::PrivateIdentifier(identifier)) => identifier.text,
        None => "",
    }
}

fn expression_as_tag_name(expression: Expression<'_>) -> Option<JsxTagNameExpression<'_>> {
    match expression {
        Expression::Identifier(identifier) => Some(JsxTagNameExpression::Identifier(identifier)),
        Expression::KeywordExpression(keyword) => {
            Some(JsxTagNameExpression::KeywordExpression(keyword))
        }
        Expression::PropertyAccessExpression(access) => {
            Some(JsxTagNameExpression::PropertyAccessExpression(access))
        }
        _ => None,
    }
}
