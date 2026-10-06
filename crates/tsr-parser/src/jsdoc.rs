//! Parsing `/** … */` comments.
//!
//! JSDoc sits outside the token stream: the scanner treats a `/** … */` comment
//! as trivia and only records, in a token flag, that one was there. When the
//! parser starts a construct whose first token carries that flag, it points the
//! scanner at each comment's byte range and parses the contents with the JSDoc
//! tokenizer, then puts the scanner back exactly where it was.
//!
//! Ported from `internal/parser/jsdoc.go` at the pinned commit. See
//! [`docs/architecture/jsdoc.md`](../../../../docs/architecture/jsdoc.md) for why
//! parsing is eager here and lazy upstream.

use tsr_ast::{
    EntityName, Expression, Identifier, JSDoc, JSDocAugmentsTag, JSDocComment, JSDocDeprecatedTag,
    JSDocImplementsTag, JSDocLink, JSDocLinkCode, JSDocLinkPlain, JSDocOverloadTag,
    JSDocOverrideTag, JSDocParameterOrPropertyTag, JSDocPrivateTag, JSDocProtectedTag,
    JSDocPublicTag, JSDocReadonlyTag, JSDocReturnTag, JSDocSatisfiesTag, JSDocSeeTag,
    JSDocTemplateTag, JSDocText, JSDocThisTag, JSDocThrowsTag, JSDocTypeExpression, JSDocTypeTag,
    JSDocTypedefTag, JSDocUnknownTag, Node, PropertyAccessExpression, SyntaxKind, TypeNode,
    TypeParameterDeclaration,
};
use tsr_core::Span;
use tsr_diagnostics::messages;
use tsr_scanner::{CommentRange, TokenFlags, jsdoc_ranges_in};

use crate::parser::Parser;

/// Where the comment scanner is within a line.
///
/// The distinction that matters is between "a tag could still start here" and
/// "this line is prose now": `/** * @type */` must *not* parse a tag, because the
/// second `*` is text rather than the line's leading decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing seen on this line yet.
    BeginningOfLine,
    /// The line's leading `*` has been consumed; a tag may still start.
    SawAsterisk,
    /// Collecting prose. A tag can no longer start until the next line.
    SavingComments,
    /// Collecting prose inside backticks, where `@` and `{` are literal.
    SavingBackticks,
}

/// `propertyLikeParse` (`parser/jsdoc.go`): which tag shapes a
/// `@param`/`@property` parse — or a child-tag scan — admits. A bit set,
/// because `tryParseChildTag` tests membership with `target & t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PropertyLike(u8);

impl PropertyLike {
    const PROPERTY: Self = Self(1);
    const PARAMETER: Self = Self(2);
    const CALLBACK_PARAMETER: Self = Self(4);

    const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl<'a> Parser<'a> {
    /// Parse the JSDoc comments attached to the construct starting here.
    ///
    /// Returns an empty slice — allocating nothing and touching no state — unless
    /// the current token's trivia actually contained a `/** … */`. That check is
    /// a bit test the scanner already computed, which is what makes it affordable
    /// to call at the head of every statement, member, and parameter.
    pub(crate) fn parse_leading_jsdoc(&mut self) -> &'a [&'a JSDoc<'a>] {
        if !self.parse_jsdoc || !self.token.flags.contains(TokenFlags::PRECEDING_JSDOC_COMMENT) {
            return &[];
        }
        let full_start = self.scanner.full_start();
        let token_start = self.pos();
        let ranges = jsdoc_ranges_in(self.source, full_start, token_start);
        if ranges.is_empty() {
            return &[];
        }

        let mut docs = Vec::with_capacity(ranges.len());
        for range in ranges {
            if let Some(doc) = self.parse_jsdoc_comment(range) {
                docs.push(doc);
            }
        }
        self.arena.alloc_slice(&docs)
    }

    /// Attach parsed JSDoc to a finished node.
    ///
    /// Separate from [`Parser::parse_leading_jsdoc`] because the comments are read
    /// before the construct is parsed and the node only exists afterwards. Doing
    /// nothing for an empty slice keeps the side table sparse.
    pub(crate) fn attach_jsdoc(&mut self, node: Node<'a>, docs: &'a [&'a JSDoc<'a>]) {
        if docs.is_empty() {
            return;
        }
        if let Some(id) = node.node_id() {
            // §269 note: the JSDoc root deliberately keeps NO parent edge to
            // its host. The first draft set one so `source_file_of` could
            // climb out of an `@import` tag — and every `resolve_name` from
            // inside a comment started climbing a scope chain it had never
            // seen, moving lines in `expandoFunctionContextualTypesJs`,
            // `returnTagTypeGuard` and the commonJS-require fixtures. The
            // checker bridges the one hop it needs with its own doc→host map
            // instead (`Checker::jsdoc_hosts`).
            self.jsdoc.push((id, docs));
        }
    }

    /// Parse one `/** … */` comment, restoring the scanner afterwards.
    fn parse_jsdoc_comment(&mut self, range: CommentRange) -> Option<&'a JSDoc<'a>> {
        let text = &self.source[range.start as usize..range.end as usize];
        if !tsr_scanner::is_jsdoc_like_text(text) {
            return None;
        }

        // The comment is parsed with the *main* scanner, retargeted: JSDoc type
        // expressions re-enter the ordinary type parser, which reads `self.token`
        // and calls `self.next_token()`, so a second scanner would have to be
        // threaded through all of it.
        let saved_scanner = self.scanner.save();
        let saved_token = self.token;
        let saved_diagnostics = self.diagnostics.len();

        // The initial indent counts from the line start through the leading
        // `/** `, so that continuation lines measure their margin against it.
        let line_start = self.source[..range.start as usize].rfind('\n').map_or(0, |i| i + 1);
        let indent = (range.start as usize + 4).saturating_sub(line_start);

        // `+3` past the opening `/**`, `-2` short of the closing `*/`: the window
        // is exactly the comment body, so end-of-window is end-of-comment.
        self.scanner.set_range(range.start + 3, range.end - 2);

        #[allow(clippy::cast_possible_truncation)]
        let doc = self.parse_jsdoc_comment_worker(range, indent as u32);

        // Moved aside rather than reported (`parseJSDocComment`, `jsdoc.go:171`).
        // In a `.ts` file the comment is not part of the program's syntax, so
        // complaining about its contents would turn a malformed comment into a
        // compile error; a checked `.js` file reports them with its semantic
        // diagnostics (`JSDocTable::diagnostics`). Upstream keeps them only for
        // JavaScript files; this parser has no JavaScript script kind, so the
        // gate is the consumer's.
        self.jsdoc_diagnostics.extend(self.diagnostics.drain(saved_diagnostics..));
        self.diagnostics.truncate(saved_diagnostics);
        self.scanner.restore(saved_scanner);
        self.token = saved_token;
        Some(doc)
    }

    /// The line-oriented state machine over a comment's body.
    fn parse_jsdoc_comment_worker(&mut self, range: CommentRange, indent: u32) -> &'a JSDoc<'a> {
        let mut tags: Vec<tsr_ast::JSDocTag<'a>> = Vec::new();
        let mut comment_parts: Vec<JSDocComment<'a>> = Vec::new();
        let mut comments: Vec<&'a str> = Vec::new();
        // Starting in `SawAsterisk` rather than `BeginningOfLine` is what makes
        // `/** * @type */` prose: the opening `/**` already supplied the line's
        // one permitted leading asterisk.
        let mut state = State::SawAsterisk;
        let mut backtick_count = 0u32;
        let mut in_fenced_code_block = false;
        let mut margin: Option<u32> = None;
        let mut indent = indent;
        let mut link_end = range.start;
        let mut comments_pos: Option<u32> = None;

        self.next_jsdoc_token();
        while self.at_jsdoc(SyntaxKind::WhitespaceTrivia) {
            self.next_jsdoc_token();
        }
        if self.at_jsdoc(SyntaxKind::NewLineTrivia) {
            self.next_jsdoc_token();
            state = State::BeginningOfLine;
            indent = 0;
        }

        loop {
            // Three or more backticks in a row open or close a fenced block, in
            // which `@` and `{` are literal text.
            if self.token.kind != SyntaxKind::BacktickToken && backtick_count > 0 {
                if backtick_count >= 3 {
                    in_fenced_code_block = !in_fenced_code_block;
                }
                backtick_count = 0;
            }

            match self.token.kind {
                SyntaxKind::EndOfFile => break,

                SyntaxKind::AtToken
                    if !in_fenced_code_block && self.scanner.can_follow_jsdoc_at() =>
                {
                    trim_trailing_whitespace(&mut comments);
                    if comments_pos.is_none() {
                        comments_pos = Some(self.pos());
                    }
                    let tag = self.parse_tag(indent);
                    tags.push(tag);
                    // A tag runs to the end of the line *in principle*; real
                    // comments break that rule (`@param {string} x @returns …`),
                    // so the next tag is allowed to start immediately.
                    state = State::BeginningOfLine;
                    margin = None;
                    continue;
                }

                SyntaxKind::NewLineTrivia => {
                    comments.push(self.jsdoc_token_text());
                    state = State::BeginningOfLine;
                    indent = 0;
                }

                SyntaxKind::AsteriskToken => {
                    let asterisk = self.jsdoc_token_text();
                    #[allow(clippy::cast_possible_truncation)]
                    let width = asterisk.len() as u32;
                    if state == State::SawAsterisk {
                        // A second asterisk is text, and ends any chance of a tag.
                        state = State::SavingComments;
                        push_comment(&mut comments, &mut margin, &mut indent, asterisk);
                    } else {
                        state = State::SawAsterisk;
                        indent += width;
                    }
                }

                SyntaxKind::WhitespaceTrivia => {
                    // Whitespace is kept only past the margin: the indentation
                    // that lines the comment up under the `*` is decoration, and
                    // anything beyond it is the author's own formatting.
                    let whitespace = self.jsdoc_token_text();
                    #[allow(clippy::cast_possible_truncation)]
                    let width = whitespace.len() as u32;
                    if let Some(margin_value) = margin
                        && indent + width > margin_value
                    {
                        let existing = margin_value.saturating_sub(indent);
                        let keep_from = (existing as usize).min(whitespace.len());
                        comments.push(&whitespace[keep_from..]);
                    }
                    indent += width;
                }

                SyntaxKind::BacktickToken => {
                    backtick_count += 1;
                    state = if state == State::SavingBackticks {
                        State::SavingComments
                    } else {
                        State::SavingBackticks
                    };
                    let text = self.jsdoc_token_text();
                    push_comment(&mut comments, &mut margin, &mut indent, text);
                }

                SyntaxKind::OpenBraceToken if !in_fenced_code_block => {
                    state = State::SavingComments;
                    let comment_end = self.scanner.full_start();
                    let link_start = self.token.span.end - 1;
                    if let Some(link) = self.parse_jsdoc_link(link_start) {
                        if link_end == range.start {
                            remove_leading_newlines(&mut comments);
                        }
                        let text = self.finish_jsdoc_text(&comments, link_end, comment_end);
                        comment_parts.push(JSDocComment::JSDocText(text));
                        comment_parts.push(link);
                        comments.clear();
                        link_end = self.token.span.start;
                        // `parse_jsdoc_link` has already consumed the closing
                        // brace and scanned what follows; scanning again at the
                        // bottom of the loop would drop that token.
                        continue;
                    }
                    let text = self.jsdoc_token_text();
                    push_comment(&mut comments, &mut margin, &mut indent, text);
                }

                _ => {
                    // Everything else — including a `@` that cannot start a tag,
                    // and a `{` inside a fenced block — is prose.
                    if state != State::SavingBackticks {
                        state = if in_fenced_code_block {
                            State::SavingBackticks
                        } else {
                            State::SavingComments
                        };
                    }
                    let text = self.jsdoc_token_text();
                    push_comment(&mut comments, &mut margin, &mut indent, text);
                }
            }

            if matches!(state, State::SavingComments | State::SavingBackticks) {
                self.next_jsdoc_comment_text_token(state == State::SavingBackticks);
            } else {
                self.next_jsdoc_token();
            }
        }

        let comments_end = comments_pos.unwrap_or_else(|| self.scanner.full_start());
        if !comments.is_empty() {
            if let Some(last) = comments.last_mut() {
                *last = last.trim_end();
            }
            let text = self.finish_jsdoc_text(&comments, link_end, comments_end);
            comment_parts.push(JSDocComment::JSDocText(text));
        }

        let comment = self.arena.alloc_slice(&comment_parts);
        let tags = self.arena.alloc_slice(&tags);
        let node = JSDoc::new(comment, tags);
        self.finish_node_with_end(node, SyntaxKind::JSDoc, range.start, range.end)
    }

    // ---- tags -----------------------------------------------------------

    /// Parse one `@tag`, positioned on the `@`.
    fn parse_tag(&mut self, margin: u32) -> tsr_ast::JSDocTag<'a> {
        debug_assert_eq!(self.token.kind, SyntaxKind::AtToken);
        let start = self.pos();
        self.next_jsdoc_token();

        let tag_name = self.parse_jsdoc_identifier_name();
        let indent_text = self.skip_whitespace_or_asterisk();
        let name = tag_name.text;

        match name {
            "implements" => {
                let class_name = self.parse_expression_with_type_arguments_for_augments();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                let node = JSDocImplementsTag::new(tag_name, class_name, comment);
                tsr_ast::JSDocTag::JSDocImplementsTag(self.finish_jsdoc_node(
                    node,
                    SyntaxKind::JSDocImplementsTag,
                    start,
                ))
            }
            "augments" | "extends" => {
                let class_name = self.parse_expression_with_type_arguments_for_augments();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                let node = JSDocAugmentsTag::new(tag_name, class_name, comment);
                tsr_ast::JSDocTag::JSDocAugmentsTag(self.finish_jsdoc_node(
                    node,
                    SyntaxKind::JSDocAugmentsTag,
                    start,
                ))
            }
            "public" => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocPublicTag(self.finish_jsdoc_node(
                    JSDocPublicTag::new(tag_name, comment),
                    SyntaxKind::JSDocPublicTag,
                    start,
                ))
            }
            "private" => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocPrivateTag(self.finish_jsdoc_node(
                    JSDocPrivateTag::new(tag_name, comment),
                    SyntaxKind::JSDocPrivateTag,
                    start,
                ))
            }
            "protected" => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocProtectedTag(self.finish_jsdoc_node(
                    JSDocProtectedTag::new(tag_name, comment),
                    SyntaxKind::JSDocProtectedTag,
                    start,
                ))
            }
            "readonly" => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocReadonlyTag(self.finish_jsdoc_node(
                    JSDocReadonlyTag::new(tag_name, comment),
                    SyntaxKind::JSDocReadonlyTag,
                    start,
                ))
            }
            "override" => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocOverrideTag(self.finish_jsdoc_node(
                    JSDocOverrideTag::new(tag_name, comment),
                    SyntaxKind::JSDocOverrideTag,
                    start,
                ))
            }
            "deprecated" => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocDeprecatedTag(self.finish_jsdoc_node(
                    JSDocDeprecatedTag::new(tag_name, comment),
                    SyntaxKind::JSDocDeprecatedTag,
                    start,
                ))
            }
            "this" => self.parse_this_tag(start, tag_name, margin, indent_text),
            "arg" | "argument" | "param" => self.parse_parameter_or_property_tag(
                start,
                tag_name,
                PropertyLike::PARAMETER,
                margin,
            ),
            "prop" | "property" => self.parse_parameter_or_property_tag(
                start,
                tag_name,
                PropertyLike::PROPERTY,
                margin,
            ),
            "return" | "returns" => {
                let type_expression = self.try_parse_type_expression();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocReturnTag(self.finish_jsdoc_node(
                    JSDocReturnTag::new(tag_name, type_expression, comment),
                    SyntaxKind::JSDocReturnTag,
                    start,
                ))
            }
            "template" => self.parse_template_tag(start, tag_name, margin, indent_text),
            "type" => self.parse_type_tag(start, tag_name, Some((margin, indent_text))),
            "satisfies" => {
                let type_expression = self.try_parse_type_expression();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocSatisfiesTag(self.finish_jsdoc_node(
                    JSDocSatisfiesTag::new(tag_name, type_expression, comment),
                    SyntaxKind::JSDocSatisfiesTag,
                    start,
                ))
            }
            // §269 — supersedes §218, whose "reparser first" arithmetic was
            // upstream's and not this port's. Upstream binds nothing until
            // `reparser.go` rewrites the tag as a synthetic
            // `JSImportDeclaration`; THIS port has no reparser at all and binds
            // JSDoc declarations directly (`bind_jsdoc_declarations`), so the
            // parse's consumer already exists — the same clause node kinds
            // classify (ALIAS, Locals) in the binder untouched. §218's real
            // finding stands: the parse ALONE converts nothing. It lands in one
            // commit with its binder and checker halves.
            "import" => self.parse_import_tag(start, tag_name, margin, indent_text),
            "typedef" => self.parse_typedef_tag(start, tag_name, margin, indent_text),
            "callback" => self.parse_callback_tag(start, tag_name, margin, indent_text),
            "overload" => {
                let type_expression = self.try_parse_type_expression();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocOverloadTag(self.finish_jsdoc_node(
                    JSDocOverloadTag::new(tag_name, type_expression, comment),
                    SyntaxKind::JSDocOverloadTag,
                    start,
                ))
            }
            "see" => {
                self.parse_see_tag_name_reference();
                let name_expression = None;
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocSeeTag(self.finish_jsdoc_node(
                    JSDocSeeTag::new(tag_name, name_expression, comment),
                    SyntaxKind::JSDocSeeTag,
                    start,
                ))
            }
            "exception" | "throws" => {
                let type_expression = self.try_parse_type_expression();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocThrowsTag(self.finish_jsdoc_node(
                    JSDocThrowsTag::new(tag_name, type_expression, comment),
                    SyntaxKind::JSDocThrowsTag,
                    start,
                ))
            }
            _ => {
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocUnknownTag(self.finish_jsdoc_node(
                    JSDocUnknownTag::new(tag_name, comment),
                    SyntaxKind::JSDocUnknownTag,
                    start,
                ))
            }
        }
    }

    /// `@param {T} name description` and `@property` in one shape —
    /// `parseParameterOrPropertyTag` (`parser/jsdoc.go:833`).
    ///
    /// Either the type or the name may come first — `@param {T} x` and
    /// `@param x {T}` are both accepted — and a bracketed name marks the
    /// parameter optional: `@param [x]`. An `Object`-typed tag takes the
    /// following `x.y` tags as its nested type literal.
    fn parse_parameter_or_property_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        target: PropertyLike,
        margin: u32,
    ) -> tsr_ast::JSDocTag<'a> {
        let mut type_expression = self.try_parse_type_expression();
        let mut is_name_first = type_expression.is_none();
        self.skip_whitespace_or_asterisk();

        let (name, is_bracketed) = self.parse_bracket_name_in_property_and_param_tag(target);
        let indent_text = self.skip_whitespace_or_asterisk();
        // `@param x {@link Y}` puts a link in the comment, not a type.
        if is_name_first
            && !self.look_ahead(|parser| {
                parser.skip_whitespace_or_asterisk();
                if !parser.at_jsdoc(SyntaxKind::OpenBraceToken) {
                    return false;
                }
                parser.next_jsdoc_token();
                parser.jsdoc_link_prefix().is_some()
            })
        {
            type_expression = self.try_parse_type_expression();
        }
        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        if let Some(nested) = self.parse_nested_type_literal(type_expression, name, target, margin)
        {
            type_expression = Some(nested);
            is_name_first = true;
        }

        let kind = if target == PropertyLike::PROPERTY {
            SyntaxKind::JSDocPropertyTag
        } else {
            SyntaxKind::JSDocParameterTag
        };
        // The node is shared between the two kinds, so it carries the kind it was
        // built as — upstream models this the same way.
        let kind_token = self.alloc_token(kind, Span::at(start));
        let node = JSDocParameterOrPropertyTag::new(
            kind_token,
            tag_name,
            name,
            is_bracketed,
            type_expression,
            is_name_first,
            comment,
        );
        let node = self.finish_jsdoc_node(node, kind, start);
        tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(node)
    }

    /// `@template T` and `@template {Constraint} T, U`.
    fn parse_template_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        margin: u32,
        indent_text: &'a str,
    ) -> tsr_ast::JSDocTag<'a> {
        // A `{…}` here is the constraint on every parameter in the list, not a
        // type for one of them.
        let constraint = if self.at_jsdoc(SyntaxKind::OpenBraceToken) {
            Some(Node::from(self.parse_jsdoc_type_expression(false)))
        } else {
            None
        };

        // `parseTemplateTagTypeParameters` (`jsdoc.go:1279`).
        let mut type_parameters: Vec<&'a TypeParameterDeclaration<'a>> = Vec::new();
        loop {
            self.skip_whitespace();
            if let Some(parameter) = self.parse_template_tag_type_parameter() {
                type_parameters.push(parameter);
            }
            self.skip_whitespace_or_asterisk();
            if !self.eat_jsdoc(SyntaxKind::CommaToken) {
                break;
            }
        }

        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        tsr_ast::JSDocTag::JSDocTemplateTag(self.finish_jsdoc_node(
            JSDocTemplateTag::new(tag_name, constraint, type_parameters, comment),
            SyntaxKind::JSDocTemplateTag,
            start,
        ))
    }

    /// `parseTemplateTagTypeParameter` (`jsdoc.go:1253`): `[`? modifiers name
    /// (`= default ]`)?. A default exists only inside brackets; a missing name
    /// yields no parameter.
    fn parse_template_tag_type_parameter(&mut self) -> Option<&'a TypeParameterDeclaration<'a>> {
        let parameter_start = self.pos();
        let is_bracketed = self.eat_jsdoc(SyntaxKind::OpenBracketToken);
        if is_bracketed {
            self.skip_whitespace();
        }
        let mut word_start = self.pos();
        let mut name = self.parse_jsdoc_identifier_name_with(
            &messages::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES,
        );
        // `parseModifiersEx` (`jsdoc.go:1260`): any modifier keyword followed
        // by a name on the line is a modifier — `const`, `in`, `out`, but also
        // `@template private T`, whose `private` the checker rejects (TS1273)
        // rather than taking as the parameter's name (`jsdocTemplateTag7`).
        // `default` is not one here: `nextTokenCanFollowDefaultKeyword` wants
        // a declaration keyword after it.
        let mut modifiers = Vec::new();
        while let Some(kind) = match name.text {
            "const" => Some(SyntaxKind::ConstKeyword),
            "in" => Some(SyntaxKind::InKeyword),
            "out" => Some(SyntaxKind::OutKeyword),
            "abstract" => Some(SyntaxKind::AbstractKeyword),
            "accessor" => Some(SyntaxKind::AccessorKeyword),
            "async" => Some(SyntaxKind::AsyncKeyword),
            "declare" => Some(SyntaxKind::DeclareKeyword),
            "export" => Some(SyntaxKind::ExportKeyword),
            "override" => Some(SyntaxKind::OverrideKeyword),
            "private" => Some(SyntaxKind::PrivateKeyword),
            "protected" => Some(SyntaxKind::ProtectedKeyword),
            "public" => Some(SyntaxKind::PublicKeyword),
            "readonly" => Some(SyntaxKind::ReadonlyKeyword),
            "static" => Some(SyntaxKind::StaticKeyword),
            _ => None,
        } {
            let modifier_end = self.pos();
            self.skip_whitespace();
            if !self.at_jsdoc_identifier() {
                break;
            }
            let token = self.alloc_token(kind, tsr_core::Span::new(word_start, modifier_end));
            modifiers.push(tsr_ast::ModifierLike::Token(token));
            word_start = self.pos();
            name = self.parse_jsdoc_identifier_name_with(
                &messages::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES,
            );
        }
        let modifiers = self.arena.alloc_slice(&modifiers);
        let default = if is_bracketed {
            self.skip_whitespace();
            self.expect_jsdoc(SyntaxKind::EqualsToken);
            let default = self.parse_jsdoc_type_from_token();
            self.expect_jsdoc(SyntaxKind::CloseBracketToken);
            Some(default)
        } else {
            None
        };
        if name.text.is_empty() {
            return None;
        }
        let parameter = TypeParameterDeclaration::new(modifiers, Some(name), None, None, default);
        Some(self.finish_jsdoc_node(parameter, SyntaxKind::TypeParameter, parameter_start))
    }

    /// `@import { Foo } from "./types"` — `parseImportTag`
    /// (`parser/jsdoc.go:940`). §269.
    ///
    /// The clause and specifier are ordinary import grammar, so this hands over
    /// to `module.rs`'s pieces the same way [`Parser::parse_jsdoc_type_expression`]
    /// hands over to the type grammar: rewind to the current JSDoc token's
    /// start, rescan under main rules with leading `*` suppressed (the clause
    /// may span continuation lines — `importTag18`–`20`), and rescan back to
    /// JSDoc tokens afterwards. Upstream reaches the same effect with
    /// `skipJSDocLeadingAsterisks = true` on `tryParseImportClause`.
    ///
    /// The phase modifier is `None` rather than a synthesised `type` token:
    /// upstream's reparser stamps `KindTypeKeyword` on the cloned clause so the
    /// EMITTER elides it, and this port neither reparses nor emits JS from
    /// JSDoc. Nothing downstream here reads type-onlyness off the clause.
    fn parse_import_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        margin: u32,
        indent_text: &'a str,
    ) -> tsr_ast::JSDocTag<'a> {
        let resume = self.token.span.start;
        let limit = self.scanner.limit();
        self.scanner.set_range(resume, limit);
        self.scanner.set_skip_jsdoc_leading_asterisks(true);
        self.next_token();

        let clause_start = self.pos();
        let default_name =
            if self.at_binding_identifier() { Some(self.parse_identifier()) } else { None };
        // `tryParseImportClause` (`parser.go:2332`): a clause, and the `from`
        // after it, only when a default name, `*` or `{` begins one. A bare
        // `@import "./m"` — or a bare `@import` — carries no clause and expects
        // no `from`.
        let clause = if default_name.is_some()
            || matches!(self.token.kind, SyntaxKind::AsteriskToken | SyntaxKind::OpenBraceToken)
        {
            let named_bindings = if default_name.is_none() || self.eat(SyntaxKind::CommaToken) {
                self.parse_named_import_bindings()
            } else {
                None
            };
            let clause = self.finish_node(
                tsr_ast::ImportClause::new(None, default_name, named_bindings),
                SyntaxKind::ImportClause,
                clause_start,
            );
            self.expect(SyntaxKind::FromKeyword);
            Some(clause)
        } else {
            None
        };
        let specifier = self.parse_module_specifier();
        let attributes = self.parse_import_attributes();

        self.scanner.set_skip_jsdoc_leading_asterisks(false);
        self.scanner_reset_to_token_start();
        self.next_jsdoc_token();
        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        tsr_ast::JSDocTag::JSDocImportTag(self.finish_jsdoc_node(
            tsr_ast::JSDocImportTag::new(tag_name, clause, Some(specifier), attributes, comment),
            SyntaxKind::JSDocImportTag,
            start,
        ))
    }

    /// `@typedef {T} Name`, and the nested form whose following `@property`
    /// tags (or one `@type` tag) supply the body — `parseTypedefTag`
    /// (`parser/jsdoc.go:1017`).
    ///
    /// Upstream keeps the children in a `JSDocTypeLiteral` and its reparser
    /// (`reparseJSDocTypeLiteral`, `parser/reparser.go:240`) turns that into
    /// the `TypeLiteralNode` its synthetic `JSTypeAliasDeclaration` carries.
    /// This port has no reparse list and binds the tag directly, so the
    /// parse stores the reparsed body as the tag's type expression: the
    /// property tags are consumed here and never reach the comment's flat
    /// tag list, exactly as upstream's `JSDoc.Tags` omits them.
    fn parse_typedef_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        margin: u32,
        indent_text: &'a str,
    ) -> tsr_ast::JSDocTag<'a> {
        let mut type_expression = self.try_parse_type_expression().map(Node::from);
        self.skip_whitespace_or_asterisk();
        let name = if self.at_jsdoc_identifier() {
            Some(tsr_ast::JSDocFullName::Identifier(self.parse_jsdoc_identifier_name()))
        } else {
            None
        };
        self.skip_whitespace();
        let mut comment = self.parse_tag_comments(margin);

        let mut end = None;
        let mut has_children = false;
        let written = type_expression.and_then(jsdoc_type_expression_type);
        if type_expression.is_none()
            || written.is_some_and(is_object_or_object_array_type_reference)
        {
            let mut child_type_tag: Option<&'a JSDocTypeTag<'a>> = None;
            let mut property_tags: Vec<&'a JSDocParameterOrPropertyTag<'a>> = Vec::new();
            while let Some(child) = self.try_parse(|parser| {
                parser.parse_child_parameter_or_property_tag(PropertyLike::PROPERTY, margin, None)
            }) {
                has_children = true;
                match child {
                    tsr_ast::JSDocTag::JSDocTemplateTag(template) => self.error_at(
                        &messages::A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG,
                        self.span_of(template.tag_name.node_id),
                    ),
                    tsr_ast::JSDocTag::JSDocTypeTag(type_tag) => {
                        if child_type_tag.is_none() {
                            child_type_tag = Some(type_tag);
                        } else {
                            self.error_at_current(
                                &messages::A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS,
                            );
                        }
                    }
                    tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(property) => {
                        property_tags.push(property);
                    }
                    _ => {}
                }
            }
            if has_children {
                let is_array_type = matches!(written, Some(TypeNode::ArrayTypeNode(_)));
                let child_type =
                    child_type_tag.and_then(|tag| tag.type_expression).filter(|expression| {
                        jsdoc_type_expression_type(*expression)
                            .is_some_and(|ty| !is_object_or_object_array_type_reference(ty))
                    });
                let body = if let Some(child_type) = child_type {
                    child_type
                } else {
                    // `!!! This differs from Strada but prevents a crash`:
                    // the literal starts at its first property, or at the tag.
                    let position =
                        property_tags.first().map_or(start, |tag| self.span_of(tag.node_id).start);
                    let end = self.pos();
                    Node::from(self.reparse_jsdoc_type_literal(
                        &property_tags,
                        is_array_type,
                        position,
                        end,
                    ))
                };
                end = body.node_id().map(|id| self.nodes.span(id).end);
                type_expression = Some(body);
            }
        }

        // Only the characters between the name and the next token count when a
        // comment was actually parsed out; otherwise they are just whitespace.
        let end = end.unwrap_or_else(|| {
            if !comment.is_empty() {
                self.pos()
            } else if let Some(name) = name {
                self.span_of(name.node_id()).end
            } else if let Some(expression) = type_expression {
                self.span_of(expression.node_id()).end
            } else {
                self.span_of(tag_name.node_id).end
            }
        });
        if comment.is_empty() {
            comment = self.parse_trailing_tag_comments_ending(start, end, margin, indent_text);
        }
        tsr_ast::JSDocTag::JSDocTypedefTag(self.finish_node_with_end(
            JSDocTypedefTag::new(tag_name, type_expression, name, comment),
            SyntaxKind::JSDocTypedefTag,
            start,
            end,
        ))
    }

    /// `@callback Name` and its following `@param`/`@return` tags —
    /// `parseCallbackTag` (`parser/jsdoc.go:1137`).
    ///
    /// The signature is stored as the function type upstream's reparser
    /// builds from it (`reparseJSDocSignature`'s `KindJSDocCallbackTag` arm,
    /// `parser/reparser.go:142`), which its `JSTypeAliasDeclaration` carries
    /// as `Type` — see [`Parser::parse_typedef_tag`] for why the parse holds
    /// the reparsed form. Template parameters stay on the comment: upstream
    /// gathers them onto the alias (`gatherTypeParameters`), not onto the
    /// function type.
    fn parse_callback_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        margin: u32,
        indent_text: &'a str,
    ) -> tsr_ast::JSDocTag<'a> {
        let name = if self.at_jsdoc_identifier() {
            Some(tsr_ast::JSDocFullName::Identifier(self.parse_jsdoc_identifier_name()))
        } else {
            self.error_at_current(&messages::IDENTIFIER_EXPECTED);
            None
        };
        self.skip_whitespace();
        let mut comment = self.parse_tag_comments(margin);
        let signature_start = self.pos();
        let signature = self.parse_jsdoc_signature(signature_start, margin);
        if comment.is_empty() {
            comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        }
        let end = if comment.is_empty() { self.span_of(signature.node_id).end } else { self.pos() };
        tsr_ast::JSDocTag::JSDocCallbackTag(self.finish_node_with_end(
            tsr_ast::JSDocCallbackTag::new(
                tag_name,
                Some(TypeNode::FunctionTypeNode(signature)),
                name,
                comment,
            ),
            SyntaxKind::JSDocCallbackTag,
            start,
            end,
        ))
    }

    /// `parseJSDocSignature` (`parser/jsdoc.go:1121`) followed by
    /// `reparseJSDocSignature`'s `KindJSDocCallbackTag` arm
    /// (`parser/reparser.go:142`): the callback parameters, then one
    /// optional `@return`, as a function type.
    ///
    /// `@overload` still parses flat: its reparse is an overload declaration
    /// of the host function, which needs the checker's overload-signature arm
    /// before the children can move.
    fn parse_jsdoc_signature(
        &mut self,
        start: u32,
        indent: u32,
    ) -> &'a tsr_ast::FunctionTypeNode<'a> {
        // parseCallbackTagParameters (`parser/jsdoc.go:1101`).
        let mut parameters: Vec<tsr_ast::JSDocTag<'a>> = Vec::new();
        while let Some(child) = self.try_parse(|parser| {
            parser.parse_child_parameter_or_property_tag(
                PropertyLike::CALLBACK_PARAMETER,
                indent,
                None,
            )
        }) {
            if let tsr_ast::JSDocTag::JSDocTemplateTag(template) = child {
                self.error_at(
                    &messages::A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG,
                    self.span_of(template.tag_name.node_id),
                );
            } else {
                parameters.push(child);
            }
        }
        let return_tag = self.try_parse(|parser| {
            if !parser.at_jsdoc(SyntaxKind::AtToken) {
                return None;
            }
            match parser.parse_tag(indent) {
                tsr_ast::JSDocTag::JSDocReturnTag(tag) => Some(tag),
                _ => None,
            }
        });
        let end = self.pos();

        let mut reparsed: Vec<&'a tsr_ast::ParameterDeclaration<'a>> = Vec::new();
        for (index, parameter) in parameters.iter().enumerate() {
            match parameter {
                tsr_ast::JSDocTag::JSDocThisTag(this_tag) => {
                    let span = self.span_of(this_tag.node_id);
                    let this = self.finish_node_with_end(
                        Identifier::new("this"),
                        SyntaxKind::Identifier,
                        span.start,
                        span.end,
                    );
                    let ty = this_tag
                        .type_expression
                        .and_then(|expression| jsdoc_type_expression_type(Node::from(expression)));
                    reparsed.push(self.finish_node_with_end(
                        tsr_ast::ParameterDeclaration::new(
                            &[],
                            None,
                            Some(tsr_ast::BindingName::Identifier(this)),
                            None,
                            ty,
                            None,
                        ),
                        SyntaxKind::Parameter,
                        span.start,
                        span.end,
                    ));
                }
                tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(tag) => {
                    // Sub-property parameters (`@param x.y`) describe a parent
                    // parameter, not a standalone one.
                    let Some(EntityName::Identifier(name)) = tag.name else { continue };
                    let span = self.span_of(tag.node_id);
                    let mut dot_dot_dot = None;
                    let mut ty = tag
                        .type_expression
                        .and_then(|expression| jsdoc_type_expression_type(Node::from(expression)));
                    if let Some(TypeNode::JSDocVariadicType(variadic)) = ty {
                        dot_dot_dot = Some(self.alloc_token(SyntaxKind::DotDotDotToken, span));
                        ty = variadic.r#type;
                    }
                    let name = self.reparsed_parameter_name(name, index);
                    let question = self.question_if_optional(tag);
                    reparsed.push(self.finish_node_with_end(
                        tsr_ast::ParameterDeclaration::new(
                            &[],
                            dot_dot_dot,
                            Some(tsr_ast::BindingName::Identifier(name)),
                            question,
                            ty,
                            None,
                        ),
                        SyntaxKind::Parameter,
                        span.start,
                        span.end,
                    ));
                }
                _ => {}
            }
        }
        let parameters = self.arena.alloc_slice(&reparsed);
        let return_type = return_tag
            .and_then(|tag| tag.type_expression)
            .and_then(|expression| jsdoc_type_expression_type(Node::from(expression)));
        // A callback's reparse starts from `NewFunctionTypeNode(nil, nil,
        // NewKeywordTypeNode(AnyKeyword))`.
        let return_type = return_type.unwrap_or_else(|| {
            TypeNode::KeywordTypeNode(self.finish_node_with_end(
                tsr_ast::KeywordTypeNode::new(SyntaxKind::AnyKeyword),
                SyntaxKind::AnyKeyword,
                end,
                end,
            ))
        });
        self.finish_node_with_end(
            tsr_ast::FunctionTypeNode::new(&[], parameters, Some(return_type), &[], None),
            SyntaxKind::FunctionType,
            start,
            end,
        )
    }

    /// `parseChildParameterOrPropertyTag` (`parser/jsdoc.go:1189`): scan to
    /// the next tag that may start a line, and parse it if it is a child of
    /// `name` (or of the enclosing typedef/callback when `name` is `None`).
    ///
    /// Callers wrap this in [`Parser::try_parse`], upstream's `mark`/`rewind`.
    fn parse_child_parameter_or_property_tag(
        &mut self,
        target: PropertyLike,
        indent: u32,
        name: Option<EntityName<'a>>,
    ) -> Option<tsr_ast::JSDocTag<'a>> {
        // Upstream's `parseTagComments` rewinds the scanner onto the `@` that
        // ended a tag, so the first `nextTokenJSDoc` here rescans it; this
        // port's leaves the scanner past it.
        if self.at_jsdoc(SyntaxKind::AtToken) {
            self.scanner_reset_to_token_start();
        }
        let mut can_parse_tag = true;
        let mut seen_asterisk = false;
        loop {
            self.next_jsdoc_token();
            match self.token.kind {
                SyntaxKind::AtToken => {
                    if can_parse_tag && self.scanner.can_follow_jsdoc_at() {
                        let child = self.try_parse_child_tag(target, indent);
                        if let (
                            Some(name),
                            Some(tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(child)),
                        ) = (name, child)
                        {
                            let nested = match child.name {
                                Some(EntityName::QualifiedName(qualified)) => {
                                    qualified.left.is_some_and(|left| texts_equal(name, left))
                                }
                                _ => false,
                            };
                            if !nested {
                                return None;
                            }
                        }
                        return child;
                    }
                    seen_asterisk = false;
                }
                SyntaxKind::NewLineTrivia => {
                    can_parse_tag = true;
                    seen_asterisk = false;
                }
                SyntaxKind::AsteriskToken => {
                    if seen_asterisk {
                        can_parse_tag = false;
                    }
                    seen_asterisk = true;
                }
                SyntaxKind::Identifier => can_parse_tag = false,
                SyntaxKind::EndOfFile => return None,
                _ => {}
            }
        }
    }

    /// `tryParseChildTag` (`parser/jsdoc.go:1221`), positioned on the `@`.
    fn try_parse_child_tag(
        &mut self,
        target: PropertyLike,
        indent: u32,
    ) -> Option<tsr_ast::JSDocTag<'a>> {
        debug_assert_eq!(self.token.kind, SyntaxKind::AtToken);
        let start = self.pos();
        self.next_jsdoc_token();
        let tag_name = self.parse_jsdoc_identifier_name();
        let indent_text = self.skip_whitespace_or_asterisk();
        let kind = match tag_name.text {
            "type" if target == PropertyLike::PROPERTY => {
                return Some(self.parse_type_tag(start, tag_name, None));
            }
            "prop" | "property" => PropertyLike::PROPERTY,
            "arg" | "argument" | "param" => {
                PropertyLike(PropertyLike::PARAMETER.0 | PropertyLike::CALLBACK_PARAMETER.0)
            }
            "template" => {
                return Some(self.parse_template_tag(start, tag_name, indent, indent_text));
            }
            "this" => return Some(self.parse_this_tag(start, tag_name, indent, indent_text)),
            _ => return None,
        };
        if !target.intersects(kind) {
            return None;
        }
        Some(self.parse_parameter_or_property_tag(start, tag_name, target, indent))
    }

    /// `parseNestedTypeLiteral` (`parser/jsdoc.go:858`): the `@param x.y` /
    /// `@property x.y` children of an `Object`-typed tag, as the reparsed
    /// type literal `reparseJSDocTypeLiteral` builds from them.
    fn parse_nested_type_literal(
        &mut self,
        type_expression: Option<TypeNode<'a>>,
        name: Option<EntityName<'a>>,
        target: PropertyLike,
        indent: u32,
    ) -> Option<TypeNode<'a>> {
        let written = type_expression.and_then(|ty| jsdoc_type_expression_type(Node::from(ty)))?;
        if !is_object_or_object_array_type_reference(written) {
            return None;
        }
        let position = self.pos();
        let mut children: Vec<&'a JSDocParameterOrPropertyTag<'a>> = Vec::new();
        while let Some(child) = self
            .try_parse(|parser| parser.parse_child_parameter_or_property_tag(target, indent, name))
        {
            match child {
                tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(child) => children.push(child),
                tsr_ast::JSDocTag::JSDocTemplateTag(template) => self.error_at(
                    &messages::A_JSDOC_TEMPLATE_TAG_MAY_NOT_FOLLOW_A_TYPEDEF_CALLBACK_OR_OVERLOAD_TAG,
                    self.span_of(template.tag_name.node_id),
                ),
                _ => {}
            }
        }
        if children.is_empty() {
            return None;
        }
        let end = self.pos();
        let is_array_type = matches!(written, TypeNode::ArrayTypeNode(_));
        let literal = self.reparse_jsdoc_type_literal(&children, is_array_type, position, end);
        let expression = self.finish_node_with_end(
            JSDocTypeExpression::new(Some(literal)),
            SyntaxKind::JSDocTypeExpression,
            position,
            end,
        );
        Some(TypeNode::JSDocTypeExpression(expression))
    }

    /// `reparseJSDocTypeLiteral` (`parser/reparser.go:240`): property tags as
    /// a `TypeLiteralNode`, wrapped in an array type for `Object[]`.
    fn reparse_jsdoc_type_literal(
        &mut self,
        tags: &[&'a JSDocParameterOrPropertyTag<'a>],
        is_array_type: bool,
        start: u32,
        end: u32,
    ) -> TypeNode<'a> {
        let mut members: Vec<tsr_ast::TypeElement<'a>> = Vec::with_capacity(tags.len());
        for tag in tags {
            let name = match tag.name {
                Some(EntityName::Identifier(name)) => name,
                Some(EntityName::QualifiedName(qualified)) => match qualified.right {
                    Some(right) => right,
                    None => continue,
                },
                None => continue,
            };
            let name = if is_valid_identifier(name.text) {
                tsr_ast::PropertyName::Identifier(name)
            } else {
                let span = self.span_of(name.node_id);
                tsr_ast::PropertyName::StringLiteral(self.finish_node_with_end(
                    tsr_ast::StringLiteral::new(name.text, tsr_ast::TokenFlags::empty()),
                    SyntaxKind::StringLiteral,
                    span.start,
                    span.end,
                ))
            };
            let question = self.question_if_optional(tag);
            let ty = tag
                .type_expression
                .and_then(|expression| jsdoc_type_expression_type(Node::from(expression)));
            let span = self.span_of(tag.node_id);
            let property = self.finish_node_with_end(
                tsr_ast::PropertySignatureDeclaration::new(&[], name, question, ty, None),
                SyntaxKind::PropertySignature,
                span.start,
                span.end,
            );
            members.push(tsr_ast::TypeElement::PropertySignatureDeclaration(property));
        }
        let members = self.arena.alloc_slice(&members);
        let literal = TypeNode::TypeLiteralNode(self.finish_node_with_end(
            tsr_ast::TypeLiteralNode::new(members),
            SyntaxKind::TypeLiteral,
            start,
            end,
        ));
        if !is_array_type {
            return literal;
        }
        TypeNode::ArrayTypeNode(self.finish_node_with_end(
            tsr_ast::ArrayTypeNode::new(Some(literal)),
            SyntaxKind::ArrayType,
            start,
            end,
        ))
    }

    /// `makeQuestionIfOptional` (`parser/reparser.go:611`): a bracketed name
    /// or a postfix `=` type makes the reparsed member optional.
    fn question_if_optional(
        &mut self,
        tag: &JSDocParameterOrPropertyTag<'a>,
    ) -> Option<&'a tsr_ast::Token<'a>> {
        let postfix_optional = tag.type_expression.is_some_and(|expression| {
            matches!(
                jsdoc_type_expression_type(Node::from(expression)),
                Some(TypeNode::JSDocOptionalType(_))
            )
        });
        (tag.is_bracketed || postfix_optional)
            .then(|| self.alloc_token(SyntaxKind::QuestionToken, self.span_of(tag.node_id)))
    }

    /// The parameter name `reparseJSDocSignature` writes: invalid identifier
    /// characters become `_`, and an empty result becomes `_<index>`.
    fn reparsed_parameter_name(
        &mut self,
        name: &'a Identifier<'a>,
        index: usize,
    ) -> &'a Identifier<'a> {
        if is_valid_identifier(name.text) {
            return name;
        }
        let mut text = String::with_capacity(name.text.len());
        for (position, ch) in name.text.chars().enumerate() {
            let valid = if position == 0 {
                tsr_scanner::is_identifier_start(ch)
            } else {
                tsr_scanner::is_identifier_part(ch)
            };
            text.push(if valid { ch } else { '_' });
        }
        if text.is_empty() {
            text.push('_');
            text.push_str(&index.to_string());
        }
        let text: &'a str = self.arena.alloc_str(&text);
        let span = self.span_of(name.node_id);
        self.finish_node_with_end(
            Identifier::new(text),
            SyntaxKind::Identifier,
            span.start,
            span.end,
        )
    }

    /// `parseTypeTag` (`parser/jsdoc.go:894`). `comments` is `None` for a
    /// `@type` nested in a typedef, which upstream signals with `indent = -1`.
    fn parse_type_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        comments: Option<(u32, &'a str)>,
    ) -> tsr_ast::JSDocTag<'a> {
        // `mayOmitBraces`: `@type object` is a type expression too.
        self.skip_whitespace_or_asterisk();
        let type_expression = Some(Node::from(self.parse_jsdoc_type_expression(true)));
        let comment = match comments {
            Some((margin, indent_text)) => {
                self.parse_trailing_tag_comments(start, margin, indent_text)
            }
            None => &[],
        };
        tsr_ast::JSDocTag::JSDocTypeTag(self.finish_jsdoc_node(
            JSDocTypeTag::new(tag_name, type_expression, comment),
            SyntaxKind::JSDocTypeTag,
            start,
        ))
    }

    /// `parseThisTag` (`parser/jsdoc.go:985`).
    fn parse_this_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        margin: u32,
        indent_text: &'a str,
    ) -> tsr_ast::JSDocTag<'a> {
        let type_expression = Some(self.parse_jsdoc_type_expression(false));
        self.skip_whitespace();
        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        tsr_ast::JSDocTag::JSDocThisTag(self.finish_jsdoc_node(
            JSDocThisTag::new(tag_name, type_expression, comment),
            SyntaxKind::JSDocThisTag,
            start,
        ))
    }

    fn span_of(&self, id: Option<tsr_ast::NodeId>) -> Span {
        id.map_or_else(|| Span::at(self.pos()), |id| self.nodes.span(id))
    }

    /// The `[name]` / `name` of a `@param` or `@property`.
    ///
    /// `parseBracketNameInPropertyAndParamTag` (`jsdoc.go:793`). A missing
    /// `@param` name is not reported (upstream passes no message for the
    /// parameter target); a missing `@property` name is.
    fn parse_bracket_name_in_property_and_param_tag(
        &mut self,
        target: PropertyLike,
    ) -> (Option<EntityName<'a>>, bool) {
        let is_bracketed = self.eat_jsdoc(SyntaxKind::OpenBracketToken);
        if is_bracketed {
            self.skip_whitespace();
        }
        // A markdown-quoted name: `arg` is not legal JSDoc, but occurs in the wild.
        let is_backquoted = self.eat_jsdoc(SyntaxKind::BacktickToken);
        let name = self.parse_jsdoc_entity_name(target == PropertyLike::PROPERTY);
        if is_backquoted {
            self.expect_jsdoc(SyntaxKind::BacktickToken);
        }
        if is_bracketed {
            self.skip_whitespace();
            // A default value: `@param [x=1]`. The initialiser is not modelled —
            // only the fact that the parameter is optional, which the bracket
            // already recorded — so it is skipped to the closing bracket.
            if self.at_jsdoc(SyntaxKind::EqualsToken) {
                while !matches!(
                    self.token.kind,
                    SyntaxKind::CloseBracketToken | SyntaxKind::EndOfFile
                ) {
                    self.next_jsdoc_token();
                }
            }
            self.expect_jsdoc(SyntaxKind::CloseBracketToken);
        }
        (Some(name), is_bracketed)
    }

    /// `@augments {Base}` / `@implements {I}`.
    fn parse_expression_with_type_arguments_for_augments(
        &mut self,
    ) -> Option<&'a tsr_ast::ExpressionWithTypeArguments<'a>> {
        // `usedBrace := p.parseOptional(OpenBrace)` scans on under the ordinary
        // rules, which skip the space in `@extends { A }`; so is it skipped here.
        let used_brace = self.eat_jsdoc(SyntaxKind::OpenBraceToken);
        if used_brace {
            self.skip_whitespace();
        }
        let start = self.pos();
        let expression = Some(self.parse_property_access_entity_name_expression()?);
        // `parseTypeArguments` with leading asterisks skipped, handed to the
        // ordinary type grammar the way `parse_jsdoc_type_expression` does.
        let type_arguments: &'a [tsr_ast::TypeNode<'a>] =
            if self.at_jsdoc(SyntaxKind::LessThanToken) {
                let resume = self.token.span.start;
                let limit = self.scanner.limit();
                self.scanner.set_range(resume, limit);
                self.scanner.set_skip_jsdoc_leading_asterisks(true);
                self.next_token();
                let arguments = self.parse_type_arguments();
                self.scanner.set_skip_jsdoc_leading_asterisks(false);
                self.scanner_reset_to_token_start();
                self.next_jsdoc_token();
                self.arena.alloc_slice(&arguments)
            } else {
                &[]
            };
        let node = tsr_ast::ExpressionWithTypeArguments::new(expression, type_arguments);
        let node = self.finish_jsdoc_node(node, SyntaxKind::ExpressionWithTypeArguments, start);
        if used_brace {
            self.skip_whitespace();
            self.expect_jsdoc(SyntaxKind::CloseBraceToken);
        }
        Some(node)
    }

    /// A dotted name as an expression: `a.b.c`.
    fn parse_property_access_entity_name_expression(&mut self) -> Option<Expression<'a>> {
        let start = self.pos();
        // `parsePropertyAccessEntityNameExpression` (`jsdoc.go:971`): the head
        // name is required, so a bare `@implements` reports `Identifier
        // expected` — but the missing name is not handed on, as no consumer
        // here distinguishes a missing name from an absent one.
        if !self.at_jsdoc_identifier() {
            self.error_at_current(&messages::IDENTIFIER_EXPECTED);
            return None;
        }
        let mut expression = Expression::Identifier(self.parse_jsdoc_identifier_name());
        while self.at_jsdoc(SyntaxKind::DotToken) {
            self.next_jsdoc_token();
            let name = self.parse_jsdoc_identifier_name();
            let node = PropertyAccessExpression::new(
                Some(expression),
                None,
                Some(tsr_ast::MemberName::Identifier(name)),
            );
            expression = Expression::PropertyAccessExpression(self.finish_jsdoc_node(
                node,
                SyntaxKind::PropertyAccessExpression,
                start,
            ));
        }
        Some(expression)
    }

    // ---- types and links -------------------------------------------------

    /// `parseJSDocLinkName` (`jsdoc.go:742`): `a.b`, `a.#b` and `a#b`. A
    /// private name after a dot leaves that dot's right side missing (no
    /// report), and each `#name` then qualifies the name in turn.
    fn parse_jsdoc_link_name(&mut self) -> Option<EntityName<'a>> {
        if !self.at_jsdoc_identifier() {
            return None;
        }
        let start = self.pos();
        let mut name = EntityName::Identifier(self.parse_jsdoc_identifier_name());
        let qualify = |p: &mut Self, left: EntityName<'a>, right: &'a Identifier<'a>| {
            let node = tsr_ast::QualifiedName::new(Some(left), Some(right));
            EntityName::QualifiedName(p.finish_jsdoc_node(node, SyntaxKind::QualifiedName, start))
        };
        while self.eat_jsdoc(SyntaxKind::DotToken) {
            let right = if self.at_jsdoc(SyntaxKind::HashToken) {
                self.missing_identifier()
            } else {
                self.parse_jsdoc_identifier_name()
            };
            name = qualify(self, name, right);
        }
        while self.eat_jsdoc(SyntaxKind::HashToken) {
            let right = self.parse_jsdoc_identifier_name();
            name = qualify(self, name, right);
        }
        Some(name)
    }

    /// `parseSeeTag`'s name reference (`jsdoc.go:907`, `parseJSDocNameReference`
    /// at `jsdoc.go:126`): `@see Name`, `@see {Name}` — read for its grammar
    /// only. The tree's `name_expression` slot is a type node and a
    /// `JSDocNameReference` is not one; no consumer here reads the name.
    fn parse_see_tag_name_reference(&mut self) {
        let has_name_reference = (self.at_jsdoc_identifier()
            && !self.source[self.token.span.end as usize..].starts_with("://"))
            || (self.at_jsdoc(SyntaxKind::OpenBraceToken)
                && self.look_ahead(|p| {
                    p.next_jsdoc_token();
                    p.at_jsdoc_identifier()
                }));
        if !has_name_reference {
            return;
        }
        let has_brace = self.eat_jsdoc(SyntaxKind::OpenBraceToken);
        self.skip_whitespace();
        self.parse_jsdoc_link_name();
        if has_brace {
            self.skip_whitespace();
            self.expect_jsdoc(SyntaxKind::CloseBraceToken);
        }
    }

    /// `parseNonArrayType`'s JSDoc arms (`parser.go:2763`): `*` is
    /// `JSDocAllType` (`*=` is `*` then a postfix `=`), `?T` is
    /// `JSDocNullableType` (`??T` is `?` then `?T`), `!T` is
    /// `JSDocNonNullableType`. Upstream parses them in every file; the checker
    /// owns the complaint outside a comment.
    pub(crate) fn parse_jsdoc_prefix_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        match self.token.kind {
            SyntaxKind::AsteriskToken | SyntaxKind::AsteriskEqualsToken => {
                // `ReScanAsteriskEqualsToken`: the `*` alone, then rescan from
                // just past it.
                let limit = self.scanner.limit();
                self.scanner.set_range(start + 1, limit);
                let node =
                    self.finish_node(tsr_ast::JSDocAllType::new(), SyntaxKind::JSDocAllType, start);
                self.next_token();
                TypeNode::JSDocAllType(node)
            }
            SyntaxKind::QuestionToken | SyntaxKind::QuestionQuestionToken => {
                // `ReScanQuestionToken` for `??`: the first `?` alone.
                let limit = self.scanner.limit();
                self.scanner.set_range(start + 1, limit);
                self.next_token();
                let inner = self.parse_type_operator_or_higher();
                TypeNode::JSDocNullableType(self.finish_node(
                    tsr_ast::JSDocNullableType::new(Some(inner)),
                    SyntaxKind::JSDocNullableType,
                    start,
                ))
            }
            _ => {
                self.next_token();
                let inner = self.parse_type_operator_or_higher();
                TypeNode::JSDocNonNullableType(self.finish_node(
                    tsr_ast::JSDocNonNullableType::new(Some(inner)),
                    SyntaxKind::JSDocNonNullableType,
                    start,
                ))
            }
        }
    }

    /// `parsePostfixTypeOrHigher`'s JSDoc arms (`parser.go:2719`): `T!`, and
    /// `T?` unless a type follows the `?` (then it opens a conditional type's
    /// true branch). `None` leaves the cursor alone.
    pub(crate) fn parse_jsdoc_postfix_type(
        &mut self,
        start: u32,
        type_node: TypeNode<'a>,
    ) -> Option<TypeNode<'a>> {
        match self.token.kind {
            SyntaxKind::ExclamationToken => {
                self.next_token();
                Some(TypeNode::JSDocNonNullableType(self.finish_node(
                    tsr_ast::JSDocNonNullableType::new(Some(type_node)),
                    SyntaxKind::JSDocNonNullableType,
                    start,
                )))
            }
            SyntaxKind::QuestionToken => {
                if self.look_ahead(|p| {
                    p.next_token();
                    p.is_start_of_type(false)
                }) {
                    return None;
                }
                self.next_token();
                Some(TypeNode::JSDocNullableType(self.finish_node(
                    tsr_ast::JSDocNullableType::new(Some(type_node)),
                    SyntaxKind::JSDocNullableType,
                    start,
                )))
            }
            _ => None,
        }
    }

    /// `{T}`, or a bare `T` when `may_omit_braces`.
    fn parse_jsdoc_type_expression(&mut self, may_omit_braces: bool) -> tsr_ast::TypeNode<'a> {
        let start = self.pos();
        let has_brace = self.at_jsdoc(SyntaxKind::OpenBraceToken);
        if !has_brace && !may_omit_braces {
            self.error_at_current(&messages::IDENTIFIER_EXPECTED);
        }

        // Hand over to the ordinary type grammar by rewinding to just past the
        // `{` and rescanning. Advancing with `eat_jsdoc` would not do: it also
        // scans the *following* token under JSDoc rules, and the type parser then
        // starts one token late — which is invisible in a `.ts` file, because
        // JSDoc diagnostics are discarded, and shows up only as a silently
        // mis-parsed type.
        let resume = if has_brace { self.token.span.end } else { self.token.span.start };
        let inner = self.parse_jsdoc_type_at(resume);
        if has_brace {
            self.expect_jsdoc(SyntaxKind::CloseBraceToken);
        }
        let node = JSDocTypeExpression::new(Some(inner));
        tsr_ast::TypeNode::JSDocTypeExpression(self.finish_jsdoc_node(
            node,
            SyntaxKind::JSDocTypeExpression,
            start,
        ))
    }

    /// `parseJSDocType` (`parser.go:2858`) from the current JSDoc token's start
    /// — the unbraced type of a `@template [T=default]`.
    fn parse_jsdoc_type_from_token(&mut self) -> tsr_ast::TypeNode<'a> {
        self.parse_jsdoc_type_at(self.token.span.start)
    }

    /// `parseJSDocType` (`parser.go:2858`) under the ordinary type grammar,
    /// rescanning from `resume` and handing back to JSDoc tokens at the token
    /// the type parser stopped on.
    fn parse_jsdoc_type_at(&mut self, resume: u32) -> tsr_ast::TypeNode<'a> {
        let limit = self.scanner.limit();
        self.scanner.set_range(resume, limit);

        // Leading `*` is suppressed inside the braces: a type may span
        // continuation lines, whose decoration is not part of it.
        self.scanner.set_skip_jsdoc_leading_asterisks(true);
        self.next_token();
        let type_start = self.pos();
        // parseJSDocType (parser.go): a leading `...` makes the parsed type a
        // JSDocVariadicType.
        let has_dot_dot_dot = self.eat(SyntaxKind::DotDotDotToken);
        let mut inner = self.parse_type_or_type_predicate();
        if has_dot_dot_dot {
            inner = tsr_ast::TypeNode::JSDocVariadicType(self.finish_node(
                tsr_ast::JSDocVariadicType::new(Some(inner)),
                SyntaxKind::JSDocVariadicType,
                type_start,
            ));
        }
        // parseJSDocType (parser.go): a suffix = wraps the complete type,
        // including arrays/unions, before the closing brace is consumed.
        if self.eat(SyntaxKind::EqualsToken) {
            inner = tsr_ast::TypeNode::JSDocOptionalType(self.finish_node(
                tsr_ast::JSDocOptionalType::new(Some(inner)),
                SyntaxKind::JSDocOptionalType,
                type_start,
            ));
        }
        self.scanner.set_skip_jsdoc_leading_asterisks(false);

        // Back to JSDoc tokens, rescanning the token the type parser stopped on.
        self.scanner_reset_to_token_start();
        self.next_jsdoc_token();
        inner
    }

    /// A `{T}` if one is present, otherwise nothing.
    fn try_parse_type_expression(&mut self) -> Option<tsr_ast::TypeNode<'a>> {
        self.skip_whitespace_or_asterisk();
        if self.at_jsdoc(SyntaxKind::OpenBraceToken) {
            Some(self.parse_jsdoc_type_expression(false))
        } else {
            None
        }
    }

    /// `{@link Target text}` and its `@linkcode` / `@linkplain` variants.
    ///
    /// Returns `None` when the brace does not in fact open a link, leaving the
    /// caller to treat it as prose. Positioned on the `{`.
    fn parse_jsdoc_link(&mut self, start: u32) -> Option<JSDocComment<'a>> {
        let saved = self.scanner.save();
        let saved_token = self.token;

        self.next_jsdoc_token();
        let Some(kind) = self.jsdoc_link_prefix() else {
            self.scanner.restore(saved);
            self.token = saved_token;
            return None;
        };

        // The whitespace after `@link` comes first: testing for the name before
        // skipping it puts the target in the link's display text instead.
        self.skip_whitespace();
        let name = self.parse_jsdoc_link_name();
        self.skip_whitespace();

        // Everything up to the closing brace is the link's display text.
        let mut text: Vec<&'a str> = Vec::new();
        while !matches!(self.token.kind, SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFile) {
            text.push(self.jsdoc_token_text());
            self.next_jsdoc_token();
        }
        self.eat_jsdoc(SyntaxKind::CloseBraceToken);

        let text = self.arena.alloc_slice(&text);
        Some(match kind {
            LinkKind::Link => JSDocComment::JSDocLink(self.finish_jsdoc_node(
                JSDocLink::new(name, text),
                SyntaxKind::JSDocLink,
                start,
            )),
            LinkKind::Code => JSDocComment::JSDocLinkCode(self.finish_jsdoc_node(
                JSDocLinkCode::new(name, text),
                SyntaxKind::JSDocLinkCode,
                start,
            )),
            LinkKind::Plain => JSDocComment::JSDocLinkPlain(self.finish_jsdoc_node(
                JSDocLinkPlain::new(name, text),
                SyntaxKind::JSDocLinkPlain,
                start,
            )),
        })
    }

    /// The `@link` / `@linkcode` / `@linkplain` that follows a `{`.
    fn jsdoc_link_prefix(&mut self) -> Option<LinkKind> {
        self.skip_whitespace_or_asterisk();
        if !self.at_jsdoc(SyntaxKind::AtToken) {
            return None;
        }
        self.next_jsdoc_token();
        let kind = match self.jsdoc_token_text() {
            "link" => LinkKind::Link,
            "linkcode" => LinkKind::Code,
            "linkplain" => LinkKind::Plain,
            _ => return None,
        };
        self.next_jsdoc_token();
        Some(kind)
    }

    /// A dotted name inside JSDoc: `a.b.c`, `a#b`.
    /// `parseJSDocEntityName(diagnosticMessage)` (`jsdoc.go:1321`): a missing
    /// head name reports `Identifier expected` only when `report` — upstream's
    /// non-nil message.
    fn parse_jsdoc_entity_name(&mut self, report: bool) -> EntityName<'a> {
        let start = self.pos();
        let head = if report || self.at_jsdoc_identifier() {
            self.parse_jsdoc_identifier_name()
        } else {
            self.missing_identifier()
        };
        let mut entity = EntityName::Identifier(head);
        while self.at_jsdoc(SyntaxKind::DotToken) || self.at_jsdoc(SyntaxKind::HashToken) {
            self.next_jsdoc_token();
            let right = self.parse_jsdoc_identifier_name();
            let node = tsr_ast::QualifiedName::new(Some(entity), Some(right));
            entity = EntityName::QualifiedName(self.finish_jsdoc_node(
                node,
                SyntaxKind::QualifiedName,
                start,
            ));
        }
        entity
    }

    /// An identifier, or a zero-width stand-in with a diagnostic.
    ///
    /// Every keyword is a valid JSDoc name — `@param {T} default` is ordinary —
    /// so the test is "does it look like a word", not "is it an identifier".
    fn parse_jsdoc_identifier_name(&mut self) -> &'a Identifier<'a> {
        self.parse_jsdoc_identifier_name_with(&messages::IDENTIFIER_EXPECTED)
    }

    /// `parseJSDocIdentifierName(diagnosticMessage)` (`jsdoc.go:1340`).
    fn parse_jsdoc_identifier_name_with(
        &mut self,
        message: &'static tsr_diagnostics::Message,
    ) -> &'a Identifier<'a> {
        if !self.at_jsdoc_identifier() {
            self.error_at_current(message);
            return self.missing_identifier();
        }
        let start = self.pos();
        let text = self.jsdoc_token_text();
        self.next_jsdoc_token();
        self.finish_jsdoc_node(Identifier::new(text), SyntaxKind::Identifier, start)
    }

    // ---- cursor ----------------------------------------------------------

    fn next_jsdoc_token(&mut self) {
        self.token = self.scanner.scan_jsdoc_token();
    }

    fn next_jsdoc_comment_text_token(&mut self, in_backticks: bool) {
        self.token = self.scanner.scan_jsdoc_comment_text_token(in_backticks);
    }

    /// Put the scanner back at the current token's start, so the next scan can be
    /// taken in a different mode.
    fn scanner_reset_to_token_start(&mut self) {
        let start = self.token.span.start;
        let end = self.scanner.limit();
        self.scanner.set_range(start, end);
    }

    fn at_jsdoc(&self, kind: SyntaxKind) -> bool {
        self.token.kind == kind
    }

    fn at_jsdoc_identifier(&self) -> bool {
        self.token.kind == SyntaxKind::Identifier || self.token.kind.is_keyword()
    }

    fn eat_jsdoc(&mut self, kind: SyntaxKind) -> bool {
        if self.at_jsdoc(kind) {
            self.next_jsdoc_token();
            true
        } else {
            false
        }
    }

    fn expect_jsdoc(&mut self, kind: SyntaxKind) -> bool {
        // `parseExpectedJSDoc` (`parser.go:959`): `'{0}' expected.`
        if self.eat_jsdoc(kind) {
            true
        } else {
            let text = match kind {
                SyntaxKind::BacktickToken => "`",
                kind => crate::parser::token_to_text(kind),
            };
            self.error_at_current_with(&messages::_0_EXPECTED, &[text]);
            false
        }
    }

    /// The current token's source text, borrowed from the file.
    fn jsdoc_token_text(&self) -> &'a str {
        &self.source[self.token.span.start as usize..self.token.span.end as usize]
    }

    /// Skip layout, but never the run immediately before the end of the comment:
    /// trailing whitespace belongs to no node's span.
    fn skip_whitespace(&mut self) {
        if self.at_layout() && self.only_layout_remains() {
            return;
        }
        while self.at_layout() {
            self.next_jsdoc_token();
        }
    }

    /// As [`Parser::skip_whitespace`], but also skips the `*` that decorates a
    /// continuation line, returning the indentation seen after the last newline.
    fn skip_whitespace_or_asterisk(&mut self) -> &'a str {
        if self.at_layout() && self.only_layout_remains() {
            return "";
        }
        let mut preceding_line_break = self.token.flags.contains(TokenFlags::PRECEDING_LINE_BREAK);
        let mut seen_line_break = false;
        let mut indent_start = self.pos();
        let mut indent_end = indent_start;

        while (preceding_line_break && self.at_jsdoc(SyntaxKind::AsteriskToken)) || self.at_layout()
        {
            indent_end = self.token.span.end;
            if self.at_jsdoc(SyntaxKind::NewLineTrivia) {
                preceding_line_break = true;
                seen_line_break = true;
                indent_start = self.token.span.end;
            } else if self.at_jsdoc(SyntaxKind::AsteriskToken) {
                preceding_line_break = false;
            }
            self.next_jsdoc_token();
        }
        if seen_line_break && indent_end > indent_start {
            &self.source[indent_start as usize..indent_end as usize]
        } else {
            ""
        }
    }

    fn at_layout(&self) -> bool {
        matches!(self.token.kind, SyntaxKind::WhitespaceTrivia | SyntaxKind::NewLineTrivia)
    }

    /// Whether only layout separates the cursor from the end of the comment.
    fn only_layout_remains(&mut self) -> bool {
        let saved = self.scanner.save();
        let saved_token = self.token;
        let result = loop {
            self.next_jsdoc_token();
            match self.token.kind {
                SyntaxKind::EndOfFile => break true,
                SyntaxKind::WhitespaceTrivia | SyntaxKind::NewLineTrivia => {}
                _ => break false,
            }
        };
        self.scanner.restore(saved);
        self.token = saved_token;
        result
    }

    /// The comment text that follows a tag on the same and continuation lines.
    fn parse_trailing_tag_comments(
        &mut self,
        start: u32,
        margin: u32,
        indent_text: &'a str,
    ) -> &'a [JSDocComment<'a>] {
        let end = self.pos();
        self.parse_trailing_tag_comments_ending(start, end, margin, indent_text)
    }

    /// `parseTrailingTagComments` with the tag's end supplied: a typedef
    /// measures from its name or body rather than from the cursor.
    fn parse_trailing_tag_comments_ending(
        &mut self,
        start: u32,
        end: u32,
        margin: u32,
        indent_text: &'a str,
    ) -> &'a [JSDocComment<'a>] {
        // With no indentation of its own, a continuation line's margin is measured
        // from where the tag itself ended.
        let margin =
            if indent_text.is_empty() { margin + end.saturating_sub(start) } else { margin };
        self.parse_tag_comments(margin)
    }

    /// Prose belonging to a tag, up to the next tag or the end of the comment.
    fn parse_tag_comments(&mut self, indent: u32) -> &'a [JSDocComment<'a>] {
        let mut comments: Vec<&'a str> = Vec::new();
        let mut parts: Vec<JSDocComment<'a>> = Vec::new();
        let mut state = State::BeginningOfLine;
        let mut margin: Option<u32> = None;
        let mut indent = indent;
        let comments_pos = self.pos();
        let mut link_end = comments_pos;

        loop {
            match self.token.kind {
                SyntaxKind::EndOfFile => break,
                // A tag ends where the next one starts.
                SyntaxKind::AtToken if self.scanner.can_follow_jsdoc_at() => break,

                SyntaxKind::NewLineTrivia => {
                    comments.push(self.jsdoc_token_text());
                    state = State::BeginningOfLine;
                    indent = 0;
                }
                SyntaxKind::AsteriskToken if state == State::BeginningOfLine => {
                    state = State::SawAsterisk;
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        indent += self.jsdoc_token_text().len() as u32;
                    }
                }
                SyntaxKind::WhitespaceTrivia => {
                    let whitespace = self.jsdoc_token_text();
                    #[allow(clippy::cast_possible_truncation)]
                    let width = whitespace.len() as u32;
                    if let Some(margin_value) = margin
                        && indent + width > margin_value
                    {
                        let existing = margin_value.saturating_sub(indent);
                        let keep_from = (existing as usize).min(whitespace.len());
                        comments.push(&whitespace[keep_from..]);
                    }
                    indent += width;
                }
                SyntaxKind::OpenBraceToken => {
                    let comment_end = self.scanner.full_start();
                    let link_start = self.token.span.end - 1;
                    if let Some(link) = self.parse_jsdoc_link(link_start) {
                        let text = self.finish_jsdoc_text(&comments, link_end, comment_end);
                        parts.push(JSDocComment::JSDocText(text));
                        parts.push(link);
                        comments.clear();
                        link_end = self.token.span.end;
                        state = State::SavingComments;
                        continue;
                    }
                    state = State::SavingComments;
                    let text = self.jsdoc_token_text();
                    push_comment(&mut comments, &mut margin, &mut indent, text);
                }
                _ => {
                    state = State::SavingComments;
                    let text = self.jsdoc_token_text();
                    push_comment(&mut comments, &mut margin, &mut indent, text);
                }
            }

            if state == State::SavingComments {
                self.next_jsdoc_comment_text_token(false);
            } else {
                self.next_jsdoc_token();
            }
        }

        trim_trailing_whitespace(&mut comments);
        if !comments.is_empty() {
            let end = self.pos();
            let text = self.finish_jsdoc_text(&comments, link_end, end);
            parts.push(JSDocComment::JSDocText(text));
        }
        self.arena.alloc_slice(&parts)
    }

    // ---- node construction ------------------------------------------------

    fn finish_jsdoc_text(
        &mut self,
        comments: &[&'a str],
        start: u32,
        end: u32,
    ) -> &'a JSDocText<'a> {
        let text = self.arena.alloc_slice(comments);
        self.finish_node_with_end(JSDocText::new(text), SyntaxKind::JSDocText, start, end)
    }

    /// Finish a node whose end is the current cursor.
    ///
    /// Distinct from [`Parser::finish_node`] because inside JSDoc the cursor is
    /// the token start, not the previous token's end: layout is tokenised, so
    /// there is no trivia to step back over.
    fn finish_jsdoc_node<T>(&mut self, node: T, kind: SyntaxKind, start: u32) -> &'a T
    where
        T: tsr_ast::HasNodeId,
        &'a T: Into<Node<'a>>,
    {
        let end = self.token.span.start.max(start);
        self.finish_node_with_end(node, kind, start, end)
    }
}

/// Which of the three link tags was written.
#[derive(Debug, Clone, Copy)]
enum LinkKind {
    Link,
    Code,
    Plain,
}

/// The written type inside a `{…}` type expression — upstream's
/// `typeExpression.Type()`. A bare type node (a reparsed body) is itself.
fn jsdoc_type_expression_type(node: Node<'_>) -> Option<TypeNode<'_>> {
    match node {
        Node::JSDocTypeExpression(expression) => expression.r#type,
        node => TypeNode::try_from(node).ok(),
    }
}

/// `isObjectOrObjectArrayTypeReference` (`parser/jsdoc.go:818`).
fn is_object_or_object_array_type_reference(node: TypeNode<'_>) -> bool {
    match node {
        TypeNode::KeywordTypeNode(keyword) => keyword.kind == SyntaxKind::ObjectKeyword,
        TypeNode::ArrayTypeNode(array) => {
            array.element_type.is_some_and(is_object_or_object_array_type_reference)
        }
        TypeNode::TypeReferenceNode(reference) => {
            matches!(reference.type_name, Some(EntityName::Identifier(name)) if name.text == "Object")
                && reference.type_arguments.is_empty()
        }
        _ => false,
    }
}

/// `textsEqual` (`parser/jsdoc.go:1173`): two entity names spell the same path.
fn texts_equal(mut a: EntityName<'_>, mut b: EntityName<'_>) -> bool {
    loop {
        match (a, b) {
            (EntityName::Identifier(a), EntityName::Identifier(b)) => return a.text == b.text,
            (EntityName::QualifiedName(qa), EntityName::QualifiedName(qb))
                if qa.right.map(|r| r.text) == qb.right.map(|r| r.text) =>
            {
                let (Some(left_a), Some(left_b)) = (qa.left, qb.left) else { return false };
                a = left_a;
                b = left_b;
            }
            _ => return false,
        }
    }
}

/// `scanner.IsValidIdentifier`: every character may appear in an identifier.
fn is_valid_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(tsr_scanner::is_identifier_start)
        && chars.all(tsr_scanner::is_identifier_part)
}

/// Record a piece of prose, and start the margin at the current indent if this is
/// the first piece on the line.
fn push_comment<'a>(
    comments: &mut Vec<&'a str>,
    margin: &mut Option<u32>,
    indent: &mut u32,
    text: &'a str,
) {
    if margin.is_none() {
        *margin = Some(*indent);
    }
    comments.push(text);
    #[allow(clippy::cast_possible_truncation)]
    {
        *indent += text.len() as u32;
    }
}

/// Drop trailing blank pieces, and trim the last non-blank one.
fn trim_trailing_whitespace(comments: &mut Vec<&str>) {
    while comments.last().is_some_and(|c| c.trim().is_empty()) {
        comments.pop();
    }
    if let Some(last) = comments.last_mut() {
        *last = last.trim_end();
    }
}

/// Drop leading pieces that are only line breaks.
fn remove_leading_newlines(comments: &mut Vec<&str>) {
    let keep_from = comments
        .iter()
        .position(|c| !c.trim_matches(['\r', '\n']).is_empty())
        .unwrap_or(comments.len());
    comments.drain(..keep_from);
}
