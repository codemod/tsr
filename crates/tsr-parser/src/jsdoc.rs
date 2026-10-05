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
    JSDocTypedefTag, JSDocUnknownTag, Node, PropertyAccessExpression, SyntaxKind,
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

/// Which of the two shapes `parse_parameter_or_property_tag` is parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyLike {
    Parameter,
    Property,
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

        // JSDoc diagnostics are dropped rather than reported. In a `.ts` file the
        // comment is not part of the program's syntax, so complaining about its
        // contents would turn a malformed comment into a compile error. Upstream
        // routes them to a separate `jsdocDiagnostics` list used only for `.js`;
        // we have no consumer for that list yet — see the bd issue in
        // docs/architecture/jsdoc.md.
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
            "this" => {
                let type_expression = Some(self.parse_jsdoc_type_expression(false));
                self.skip_whitespace();
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocThisTag(self.finish_jsdoc_node(
                    JSDocThisTag::new(tag_name, type_expression, comment),
                    SyntaxKind::JSDocThisTag,
                    start,
                ))
            }
            "arg" | "argument" | "param" => self.parse_parameter_or_property_tag(
                start,
                tag_name,
                PropertyLike::Parameter,
                margin,
            ),
            "prop" | "property" => self.parse_parameter_or_property_tag(
                start,
                tag_name,
                PropertyLike::Property,
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
            "type" => {
                // `parseTypeTag` (`parser/jsdoc.go:894`) parses with
                // `mayOmitBraces`: `@type object` is a type expression too.
                self.skip_whitespace_or_asterisk();
                let type_expression = Some(Node::from(self.parse_jsdoc_type_expression(true)));
                let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
                tsr_ast::JSDocTag::JSDocTypeTag(self.finish_jsdoc_node(
                    JSDocTypeTag::new(tag_name, type_expression, comment),
                    SyntaxKind::JSDocTypeTag,
                    start,
                ))
            }
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
                let name_expression = if self.at_jsdoc(SyntaxKind::AtToken) {
                    None
                } else {
                    self.try_parse_type_expression()
                };
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

    /// `@param {T} name description` and `@property` in one shape.
    ///
    /// Either the type or the name may come first — `@param {T} x` and
    /// `@param x {T}` are both accepted — and a bracketed name marks the
    /// parameter optional: `@param [x]`.
    fn parse_parameter_or_property_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        target: PropertyLike,
        margin: u32,
    ) -> tsr_ast::JSDocTag<'a> {
        let mut type_expression = self.try_parse_type_expression();
        let is_name_first = type_expression.is_none();
        self.skip_whitespace_or_asterisk();

        let (name, is_bracketed) = self.parse_bracket_name_in_property_and_param_tag();
        let indent_text = self.skip_whitespace_or_asterisk();
        if is_name_first {
            type_expression = self.try_parse_type_expression();
        }
        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);

        let kind = match target {
            PropertyLike::Parameter => SyntaxKind::JSDocParameterTag,
            PropertyLike::Property => SyntaxKind::JSDocPropertyTag,
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

        let mut type_parameters: Vec<&'a TypeParameterDeclaration<'a>> = Vec::new();
        loop {
            self.skip_whitespace_or_asterisk();
            let parameter_start = self.pos();
            let mut word_start = parameter_start;
            let mut name = self.parse_jsdoc_identifier_name();
            // `parseTemplateTagTypeParameter` delegates to `parseModifiersEx`
            // (`parser/jsdoc.go:1259`), so `const`, `in`, `out`, and `in out`
            // precede the actual parameter name. Parsing only `const` made
            // `@template out T` declare a parameter literally named `out`
            // and discard T (`jsdocTemplateTag8`).
            let mut modifiers = Vec::new();
            while let Some(kind) = match name.text {
                "const" => Some(SyntaxKind::ConstKeyword),
                "in" => Some(SyntaxKind::InKeyword),
                "out" => Some(SyntaxKind::OutKeyword),
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
                name = self.parse_jsdoc_identifier_name();
            }
            let modifiers = self.arena.alloc_slice(&modifiers);
            let default = if self.at_jsdoc(SyntaxKind::EqualsToken) {
                self.next_jsdoc_token();
                Some(self.parse_jsdoc_type_expression(true))
            } else {
                None
            };
            let parameter =
                TypeParameterDeclaration::new(modifiers, Some(name), None, None, default);
            type_parameters.push(self.finish_jsdoc_node(
                parameter,
                SyntaxKind::TypeParameter,
                parameter_start,
            ));
            self.skip_whitespace();
            if !self.at_jsdoc(SyntaxKind::CommaToken) {
                break;
            }
            self.next_jsdoc_token();
        }

        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        tsr_ast::JSDocTag::JSDocTemplateTag(self.finish_jsdoc_node(
            JSDocTemplateTag::new(tag_name, constraint, type_parameters, comment),
            SyntaxKind::JSDocTemplateTag,
            start,
        ))
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
        let named_bindings = if default_name.is_none() || self.eat(SyntaxKind::CommaToken) {
            self.parse_named_import_bindings()
        } else {
            None
        };
        // A bare `@import "./m"` carries no clause, as upstream's
        // `tryParseImportClause` answers nil there.
        let clause = (default_name.is_some() || named_bindings.is_some()).then(|| {
            self.finish_node(
                tsr_ast::ImportClause::new(None, default_name, named_bindings),
                SyntaxKind::ImportClause,
                clause_start,
            )
        });
        self.expect(SyntaxKind::FromKeyword);
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

    /// `@typedef {T} Name`.
    ///
    /// Only the simple form is built: the nested-property form, where following
    /// `@property` tags become a synthesised type literal, needs the reparser and
    /// is not ported. See docs/architecture/jsdoc.md.
    fn parse_typedef_tag(
        &mut self,
        start: u32,
        tag_name: &'a Identifier<'a>,
        margin: u32,
        indent_text: &'a str,
    ) -> tsr_ast::JSDocTag<'a> {
        let type_expression = self.try_parse_type_expression().map(Node::from);
        self.skip_whitespace_or_asterisk();
        let name = if self.at_jsdoc_identifier() {
            Some(tsr_ast::JSDocFullName::Identifier(self.parse_jsdoc_identifier_name()))
        } else {
            None
        };
        let comment = self.parse_trailing_tag_comments(start, margin, indent_text);
        tsr_ast::JSDocTag::JSDocTypedefTag(self.finish_jsdoc_node(
            JSDocTypedefTag::new(tag_name, type_expression, name, comment),
            SyntaxKind::JSDocTypedefTag,
            start,
        ))
    }

    /// The `[name]` / `name` of a `@param` or `@property`.
    fn parse_bracket_name_in_property_and_param_tag(&mut self) -> (Option<EntityName<'a>>, bool) {
        let is_bracketed = self.at_jsdoc(SyntaxKind::OpenBracketToken);
        if is_bracketed {
            self.next_jsdoc_token();
            self.skip_whitespace();
        }
        let name = self.parse_jsdoc_entity_name();
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
        let used_brace = self.eat_jsdoc(SyntaxKind::OpenBraceToken);
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
            self.expect_jsdoc(SyntaxKind::CloseBraceToken);
        }
        Some(node)
    }

    /// A dotted name as an expression: `a.b.c`.
    fn parse_property_access_entity_name_expression(&mut self) -> Option<Expression<'a>> {
        let start = self.pos();
        if !self.at_jsdoc_identifier() {
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
        let limit = self.scanner.limit();
        self.scanner.set_range(resume, limit);

        // Leading `*` is suppressed inside the braces: a type may span
        // continuation lines, whose decoration is not part of it.
        self.scanner.set_skip_jsdoc_leading_asterisks(true);
        self.next_token();
        let type_start = self.pos();
        let mut inner = self.parse_type();
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
        let name =
            if self.at_jsdoc_identifier() { Some(self.parse_jsdoc_entity_name()) } else { None };
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
    fn parse_jsdoc_entity_name(&mut self) -> EntityName<'a> {
        let start = self.pos();
        let mut entity = EntityName::Identifier(self.parse_jsdoc_identifier_name());
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
        if !self.at_jsdoc_identifier() {
            self.error_at_current(&messages::IDENTIFIER_EXPECTED);
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
        if self.eat_jsdoc(kind) {
            true
        } else {
            self.error_at_current(&messages::IDENTIFIER_EXPECTED);
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
