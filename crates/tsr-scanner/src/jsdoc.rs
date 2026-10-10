//! Scanning inside a JSDoc comment.
//!
//! JSDoc is not part of the token stream. The main scanner treats `/** … */` as
//! trivia and only records that it was there; the parser later points the scanner
//! back at the comment's byte range and drives it with the entry points here.
//!
//! Inside a comment the lexical rules are different enough to need their own
//! tokenizer:
//!
//! - Whitespace and line breaks are **tokens**, not trivia. JSDoc is
//!   layout-sensitive — a tag ends where the next `@` at the start of a line
//!   begins, and comment text keeps its own indentation-relative margin — so the
//!   parser has to see the gaps.
//! - Most punctuation is a single character. `=>` is `=` then `>`, because
//!   `{function(): void=}` needs the `=` on its own.
//! - Identifiers may contain `-`, so `@my-tag` is one name.
//! - Everything that is not punctuation is prose, returned in runs as
//!   [`SyntaxKind::JSDocCommentTextToken`].
//!
//! Ported from `internal/scanner/scanner.go` (`ScanJSDocToken`,
//! `ScanJSDocCommentTextToken`, `CanFollowJSDocAt`) at the pinned commit.

use tsr_ast::SyntaxKind;
use tsr_core::Span;

use crate::{
    Scanner, Token, TokenFlags, is_identifier_part, is_identifier_start, is_line_break,
    is_whitespace_single_line, keyword_kind,
};

/// A `/** … */` comment found in trivia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentRange {
    /// Offset of the opening `/`.
    pub start: u32,
    /// Offset just past the closing `/`.
    pub end: u32,
    /// Whether a line break precedes the comment.
    pub preceded_by_line_break: bool,
}

impl Scanner<'_> {
    /// Point the scanner at `[start, end)` and treat everything outside as absent.
    ///
    /// The caller is responsible for restoring the previous state; [`Scanner::save`]
    /// captures the window along with the position, so [`Scanner::restore`] undoes
    /// this.
    pub fn set_range(&mut self, start: u32, end: u32) {
        debug_assert!(start <= end && end as usize <= self.source.len());
        self.pos = start;
        self.limit = end;
        self.token_start = start;
        self.full_start = start;
    }

    /// The offset at which scanning currently stops.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Whether leading `*` on JSDoc continuation lines should be skipped.
    ///
    /// Nests: two `true`s need two `false`s. JSDoc type expressions re-enter the
    /// ordinary scanner, and an inner region must not switch the outer one off.
    pub fn set_skip_jsdoc_leading_asterisks(&mut self, skip: bool) {
        if skip {
            self.skip_jsdoc_leading_asterisks += 1;
        } else {
            self.skip_jsdoc_leading_asterisks = self.skip_jsdoc_leading_asterisks.saturating_sub(1);
        }
    }

    /// Scan one token under JSDoc rules.
    pub fn scan_jsdoc_token(&mut self) -> Token {
        self.value = None;
        self.full_start = self.pos;
        self.token_start = self.pos;
        let mut flags = TokenFlags::empty();

        let Some(ch) = self.bump() else {
            self.token = Token::new(SyntaxKind::EndOfFile, Span::at(self.pos), flags);
            return self.token;
        };

        let kind = match ch {
            // Runs of horizontal whitespace collapse into one token: the parser
            // measures the margin from its width, not from its contents.
            c if is_whitespace_single_line(c) => {
                while self.peek().is_some_and(is_whitespace_single_line) {
                    self.bump();
                }
                SyntaxKind::WhitespaceTrivia
            }
            '\r' | '\n' => {
                if ch == '\r' {
                    self.eat('\n');
                }
                flags |= TokenFlags::PRECEDING_LINE_BREAK;
                SyntaxKind::NewLineTrivia
            }
            '@' => SyntaxKind::AtToken,
            '*' => SyntaxKind::AsteriskToken,
            '{' => SyntaxKind::OpenBraceToken,
            '}' => SyntaxKind::CloseBraceToken,
            '[' => SyntaxKind::OpenBracketToken,
            ']' => SyntaxKind::CloseBracketToken,
            '(' => SyntaxKind::OpenParenToken,
            ')' => SyntaxKind::CloseParenToken,
            '<' => SyntaxKind::LessThanToken,
            '>' => SyntaxKind::GreaterThanToken,
            '=' => SyntaxKind::EqualsToken,
            ',' => SyntaxKind::CommaToken,
            '.' => SyntaxKind::DotToken,
            '`' => SyntaxKind::BacktickToken,
            '#' => SyntaxKind::HashToken,
            // `ScanJSDocToken`'s backslash arm (`scanner.go:1490`): a unicode
            // escape that can start an identifier begins one, decoded into
            // the token's value (`@param {number} \u0061` names `a`).
            '\\' => {
                self.pos = self.token_start;
                match self.peek_unicode_escape().and_then(char::from_u32) {
                    Some(decoded) if is_identifier_start(decoded) => {
                        self.bump();
                        let mut value = String::new();
                        if let Some(first) = self.scan_unicode_escape().and_then(char::from_u32) {
                            value.push(first);
                        }
                        self.scan_jsdoc_identifier_parts(&mut value);
                        let kind = keyword_kind(&value).unwrap_or(SyntaxKind::Identifier);
                        self.value = Some(value);
                        kind
                    }
                    _ => {
                        self.bump();
                        SyntaxKind::Unknown
                    }
                }
            }
            c if is_identifier_start(c) => {
                // `-` is an identifier part here so that `@my-custom-tag` is one
                // name rather than a subtraction.
                while self.peek().is_some_and(|c| is_identifier_part(c) || c == '-') {
                    self.bump();
                }
                let end = self.pos;
                // `if char == '\\' { s.tokenValue += s.scanIdentifierParts() }`
                // (`scanner.go:1515`): an escape continues the name.
                if self.peek() == Some('\\') {
                    let mut value =
                        self.source[self.token_start as usize..end as usize].to_string();
                    self.scan_jsdoc_identifier_parts(&mut value);
                    if self.pos != end {
                        let kind = keyword_kind(&value).unwrap_or(SyntaxKind::Identifier);
                        self.value = Some(value);
                        return self.finish_jsdoc_token(kind, flags);
                    }
                }
                let text = &self.source[self.token_start as usize..end as usize];
                keyword_kind(text).unwrap_or(SyntaxKind::Identifier)
            }
            _ => SyntaxKind::Unknown,
        };

        self.finish_jsdoc_token(kind, flags)
    }

    fn finish_jsdoc_token(&mut self, kind: SyntaxKind, flags: TokenFlags) -> Token {
        self.token = Token::new(kind, Span::new(self.token_start, self.pos), flags);
        self.token
    }

    /// `scanIdentifierParts` (`scanner.go:1562`): identifier parts and the
    /// unicode escapes that spell one, decoded onto `value`. Stops, without
    /// consuming it, at a backslash whose escape does not continue a name.
    fn scan_jsdoc_identifier_parts(&mut self, value: &mut String) {
        while let Some(ch) = self.peek() {
            if is_identifier_part(ch) {
                self.bump();
                value.push(ch);
                continue;
            }
            if ch == '\\'
                && self
                    .peek_unicode_escape()
                    .and_then(char::from_u32)
                    .is_some_and(is_identifier_part)
            {
                self.bump();
                if let Some(decoded) = self.scan_unicode_escape().and_then(char::from_u32) {
                    value.push(decoded);
                }
                continue;
            }
            break;
        }
    }

    /// Scan a run of comment prose, or fall back to [`Scanner::scan_jsdoc_token`].
    ///
    /// A run ends at a line break, a backtick, and — outside backticks — at `{`
    /// (a possible `{@link}`) or at an `@` that begins a tag. The `@` test needs
    /// both neighbours: preceded by horizontal whitespace and followed by an
    /// identifier start, so that `foo@bar.com` and `@` alone stay prose.
    pub fn scan_jsdoc_comment_text_token(&mut self, in_backticks: bool) -> Token {
        self.value = None;
        self.full_start = self.pos;
        self.token_start = self.pos;

        while let Some(ch) = self.peek() {
            if is_line_break(ch) || ch == '`' {
                break;
            }
            if !in_backticks {
                if ch == '{' {
                    break;
                }
                if ch == '@' && self.at_tag_start() {
                    break;
                }
            }
            self.bump();
        }

        if self.pos == self.token_start {
            return self.scan_jsdoc_token();
        }
        self.token = Token::new(
            SyntaxKind::JSDocCommentTextToken,
            Span::new(self.token_start, self.pos),
            TokenFlags::empty(),
        );
        self.token
    }

    /// Whether the `@` at the current position begins a tag.
    fn at_tag_start(&self) -> bool {
        let before = self.source[..self.pos as usize].chars().next_back();
        if !before.is_some_and(is_whitespace_single_line) {
            return false;
        }
        let mut after = self.rest().chars();
        after.next();
        after.next().is_some_and(is_identifier_start)
    }

    /// Whether a tag name can follow the `@` just consumed.
    ///
    /// Whitespace, a line break, and end-of-comment all count, so that an
    /// unfinished `@` still parses as a tag — the language service completes
    /// tag names at exactly that position.
    #[must_use]
    pub fn can_follow_jsdoc_at(&self) -> bool {
        match self.peek() {
            None => true,
            Some(ch) => {
                is_identifier_start(ch) || is_whitespace_single_line(ch) || is_line_break(ch)
            }
        }
    }
}

/// Whether `text` opens with `/**` but is not the empty `/**/`.
#[must_use]
pub fn is_jsdoc_like_text(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 4 && bytes[1] == b'*' && bytes[2] == b'*' && bytes[3] != b'/'
}

/// The `/** … */` comments in the trivia region `[start, end)`.
///
/// `start` is a construct's full start — where its leading trivia begins — and
/// `end` is where its first real token begins. Scanning *forward* over exactly
/// that region is what keeps this linear: sweeping backward from the file start
/// for every node would be quadratic in file size.
///
/// This mirrors `GetJSDocCommentRanges` (`internal/parser/utilities.go`), which
/// gets the same property for free because a typescript-go node's `Pos()` is
/// already its full start. Ours is the token start, so the caller passes both.
#[must_use]
pub fn jsdoc_ranges_in(text: &str, start: u32, end: u32) -> Vec<CommentRange> {
    let mut ranges = Vec::new();
    let bytes = text.as_bytes();
    let mut i = start as usize;
    let end = end as usize;
    let mut saw_line_break = false;

    while i < end {
        match bytes[i] {
            b'\n' => {
                saw_line_break = true;
                i += 1;
            }
            b'\r' => {
                saw_line_break = true;
                i += 1;
                if i < end && bytes[i] == b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                while i < end && !matches!(bytes[i], b'\n' | b'\r') {
                    i += 1;
                }
            }
            b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                let comment_start = i;
                i += 2;
                while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if matches!(bytes[i], b'\n' | b'\r') {
                        saw_line_break = true;
                    }
                    i += 1;
                }
                i = (i + 2).min(end);
                if is_jsdoc_like_text(&text[comment_start..]) {
                    // `u32` is safe: the parser rejects sources above `u32::MAX`.
                    #[allow(clippy::cast_possible_truncation)]
                    ranges.push(CommentRange {
                        start: comment_start as u32,
                        end: i as u32,
                        preceded_by_line_break: saw_line_break,
                    });
                }
                saw_line_break = false;
            }
            // Whitespace, and anything the trivia scanner accepted that we do not
            // model (a shebang, a stray byte in malformed input).
            _ => i += 1,
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jsdoc_kinds(source: &str) -> Vec<SyntaxKind> {
        let mut scanner = Scanner::new(source);
        // Skip the leading `/**`; stop before the trailing `*/`.
        #[allow(clippy::cast_possible_truncation)]
        scanner.set_range(3, source.len() as u32 - 2);
        let mut kinds = Vec::new();
        loop {
            let token = scanner.scan_jsdoc_token();
            if token.kind == SyntaxKind::EndOfFile {
                break;
            }
            kinds.push(token.kind);
        }
        kinds
    }

    #[test]
    fn the_closing_delimiter_is_out_of_reach() {
        // The window, not the text, is what ends the scan.
        let kinds = jsdoc_kinds("/** a */");
        assert_eq!(
            kinds,
            [SyntaxKind::WhitespaceTrivia, SyntaxKind::Identifier, SyntaxKind::WhitespaceTrivia]
        );
    }

    #[test]
    fn layout_is_tokenised_rather_than_skipped() {
        let kinds = jsdoc_kinds("/**\n * @a\n */");
        assert_eq!(
            kinds,
            [
                SyntaxKind::NewLineTrivia,
                SyntaxKind::WhitespaceTrivia,
                SyntaxKind::AsteriskToken,
                SyntaxKind::WhitespaceTrivia,
                SyntaxKind::AtToken,
                SyntaxKind::Identifier,
                SyntaxKind::NewLineTrivia,
                SyntaxKind::WhitespaceTrivia,
            ]
        );
    }

    #[test]
    fn hyphens_belong_to_the_identifier() {
        let mut scanner = Scanner::new("my-custom-tag ");
        scanner.set_range(0, 13);
        let token = scanner.scan_jsdoc_token();
        assert_eq!(token.kind, SyntaxKind::Identifier);
        assert_eq!(scanner.token_text(), "my-custom-tag");
    }

    #[test]
    fn prose_stops_only_at_a_real_tag() {
        let source = "write to foo@bar.com or {@link x} @see y";
        let mut scanner = Scanner::new(source);
        #[allow(clippy::cast_possible_truncation)]
        scanner.set_range(0, source.len() as u32);
        // `foo@bar.com` has no whitespace before the `@`, so it stays prose.
        let first = scanner.scan_jsdoc_comment_text_token(false);
        assert_eq!(first.kind, SyntaxKind::JSDocCommentTextToken);
        assert_eq!(scanner.token_text(), "write to foo@bar.com or ");
    }

    #[test]
    fn backticks_suppress_tag_and_brace_detection() {
        let source = "a @see {x} b";
        let mut scanner = Scanner::new(source);
        #[allow(clippy::cast_possible_truncation)]
        scanner.set_range(0, source.len() as u32);
        let token = scanner.scan_jsdoc_comment_text_token(true);
        assert_eq!(token.kind, SyntaxKind::JSDocCommentTextToken);
        assert_eq!(scanner.token_text(), source);
    }

    #[test]
    fn empty_and_degenerate_comments_are_not_jsdoc() {
        assert!(!is_jsdoc_like_text("/**/"));
        assert!(!is_jsdoc_like_text("/* x */"));
        assert!(is_jsdoc_like_text("/** x */"));
        assert!(is_jsdoc_like_text("/***/"));
    }

    #[allow(clippy::cast_possible_truncation)]
    fn ranges_before<'t>(source: &'t str, marker: &str) -> Vec<&'t str> {
        let end = source.find(marker).unwrap() as u32;
        // The trivia region starts after the previous construct; for these tests
        // the previous `;` or the file start is close enough.
        let start = source[..end as usize].rfind(';').map_or(0, |i| i as u32 + 1);
        jsdoc_ranges_in(source, start, end)
            .iter()
            .map(|r| &source[r.start as usize..r.end as usize])
            .collect()
    }

    #[test]
    fn every_jsdoc_comment_in_the_region_attaches() {
        // Two comments before one node both attach, in source order.
        let source = "/** a */\nlet x;\n/** b */\n/** c */\nlet y;";
        assert_eq!(ranges_before(source, "let y"), ["/** b */", "/** c */"]);
        assert_eq!(ranges_before(source, "let x"), ["/** a */"]);
    }

    #[test]
    fn line_comments_and_plain_blocks_do_not_attach() {
        let source = "let z;// a\n/* b */\n/** c */\nlet y;";
        assert_eq!(ranges_before(source, "let y"), ["/** c */"]);
    }

    #[test]
    fn an_unterminated_comment_does_not_run_past_the_region() {
        // Malformed input must not produce a range extending beyond `end`, which
        // would slice into the following construct.
        let source = "/** a\nlet y;";
        #[allow(clippy::cast_possible_truncation)]
        let end = source.find("let y").unwrap() as u32;
        for range in jsdoc_ranges_in(source, 0, end) {
            assert!(range.end <= end, "range {range:?} escaped the trivia region");
        }
    }
}
