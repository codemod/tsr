//! Comment ranges in leading trivia.
//!
//! Ported from `internal/scanner/scanner.go` (`GetLeadingCommentRanges`,
//! `iterateCommentRanges`, `isShebangTrivia`, `scanShebangTrivia`) at the pinned
//! commit.
//!
//! # Why this is not the JSDoc scan
//!
//! [`crate::jsdoc_ranges_in`] finds `/** … */` comments attached to a *node*, for
//! documentation. This finds **every** comment in a file's leading trivia,
//! including `//` line comments, because that is where `/// <reference … />`
//! directives live — and those are program structure, not documentation: they
//! add files to the compilation.
//!
//! The two differ in more than filtering. This one starts at offset 0, skips a
//! `#!` shebang line, and stops at the first thing that is neither whitespace nor
//! a comment — so a file's directive preamble is exactly what it returns.

/// Which comment syntax produced a range (`ast.Kind`, narrowed to the two
/// trivia kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    /// `// …`, not including the line break that ends it.
    SingleLine,
    /// `/* … */`, including both delimiters.
    MultiLine,
}

/// A comment found in trivia (`ast.CommentRange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriviaComment {
    /// Which syntax it used.
    pub kind: CommentKind,
    /// Offset of the opening `/`.
    pub start: u32,
    /// Offset just past the comment.
    ///
    /// For a single-line comment that is the position of the line break, not
    /// past it; for a multi-line comment it is past the closing `/`.
    pub end: u32,
    /// Whether a line break follows.
    pub has_trailing_new_line: bool,
}

impl TriviaComment {
    /// The comment's text, given the source it came from.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }
}

/// Whether the file opens with a `#!` line (`scanner.isShebangTrivia`).
#[must_use]
pub fn is_shebang_trivia(text: &str) -> bool {
    text.as_bytes().starts_with(b"#!")
}

/// Where the shebang line ends (`scanner.scanShebangTrivia`).
#[must_use]
pub fn scan_shebang_trivia(text: &str) -> usize {
    text[2..].find(['\n', '\r', '\u{2028}', '\u{2029}']).map_or(text.len(), |index| index + 2)
}

/// Every comment in the leading trivia at `pos`
/// (`scanner.GetLeadingCommentRanges`).
///
/// Stops at the first character that is neither whitespace nor a comment, so for
/// `pos == 0` this is exactly the file's directive preamble.
#[must_use]
pub fn leading_comment_ranges(text: &str, pos: usize) -> Vec<TriviaComment> {
    let bytes = text.as_bytes();
    let mut pos = pos;
    let mut ranges = Vec::new();
    // `collecting` exists for the trailing-comment variant upstream shares this
    // loop with; for leading comments at offset 0 it is on from the start.
    let mut collecting = pos == 0;
    if pos == 0 && is_shebang_trivia(text) {
        pos = scan_shebang_trivia(text);
    }

    // The most recent comment is held back so a following line break can mark it
    // as having a trailing newline.
    let mut pending: Option<TriviaComment> = None;

    while pos < text.len() {
        let ch = text[pos..].chars().next().expect("pos is a char boundary");
        match ch {
            '\r' | '\n' => {
                if ch == '\r' && bytes.get(pos + 1) == Some(&b'\n') {
                    pos += 1;
                }
                pos += 1;
                collecting = true;
                if let Some(pending) = pending.as_mut() {
                    pending.has_trailing_new_line = true;
                }
            }
            '\t' | '\u{b}' | '\u{c}' | ' ' => pos += 1,
            '/' => {
                let next = bytes.get(pos + 1).copied();
                let Some(kind) = (match next {
                    Some(b'/') => Some(CommentKind::SingleLine),
                    Some(b'*') => Some(CommentKind::MultiLine),
                    _ => None,
                }) else {
                    break;
                };
                let start = pos;
                pos += 2;
                let mut has_trailing_new_line = false;
                if kind == CommentKind::SingleLine {
                    while pos < text.len() {
                        let c = text[pos..].chars().next().expect("char boundary");
                        if is_line_break(c) {
                            has_trailing_new_line = true;
                            break;
                        }
                        pos += c.len_utf8();
                    }
                } else if let Some(index) = text[pos..].find("*/") {
                    pos += index + 2;
                } else {
                    // An unterminated block comment runs to end of file rather
                    // than being discarded; the parser reports it separately.
                    pos = text.len();
                }
                if collecting {
                    if let Some(previous) = pending.take() {
                        ranges.push(previous);
                    }
                    pending = Some(TriviaComment {
                        kind,
                        start: u32::try_from(start).unwrap_or(u32::MAX),
                        end: u32::try_from(pos).unwrap_or(u32::MAX),
                        has_trailing_new_line,
                    });
                }
            }
            other if !other.is_ascii() && is_whitespace_like(other) => {
                if is_line_break(other)
                    && let Some(pending) = pending.as_mut()
                {
                    pending.has_trailing_new_line = true;
                }
                pos += other.len_utf8();
            }
            _ => break,
        }
    }

    ranges.extend(pending);
    ranges
}

fn is_line_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Unicode whitespace, as the scanner defines it
/// (`stringutil.IsWhiteSpaceLike`).
fn is_whitespace_like(ch: char) -> bool {
    is_line_break(ch) || crate::is_whitespace_single_line(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<(CommentKind, &str)> {
        leading_comment_ranges(source, 0)
            .into_iter()
            .map(|range| (range.kind, range.text(source)))
            .collect()
    }

    #[test]
    fn the_preamble_is_every_comment_before_the_first_real_token() {
        let source = "// a\n/* b */\n/// <reference path=\"c.ts\" />\nconst x = 1; // not this\n";
        assert_eq!(
            kinds(source),
            [
                (CommentKind::SingleLine, "// a"),
                (CommentKind::MultiLine, "/* b */"),
                (CommentKind::SingleLine, "/// <reference path=\"c.ts\" />"),
            ]
        );
    }

    #[test]
    fn a_single_line_comment_stops_before_its_line_break() {
        // The range must exclude the newline: the pragma scanner indexes from
        // the end of the comment and would otherwise read past the directive.
        let ranges = leading_comment_ranges("// a\nx", 0);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end, 4);
        assert!(ranges[0].has_trailing_new_line);
    }

    #[test]
    fn a_shebang_is_skipped_and_is_not_a_comment() {
        let source = "#!/usr/bin/env node\n// a\nx";
        assert_eq!(kinds(source), [(CommentKind::SingleLine, "// a")]);
        // `#!` only counts at offset 0; anywhere else it is not trivia at all.
        assert!(!is_shebang_trivia("x#!"));
    }

    #[test]
    fn scanning_stops_at_the_first_non_trivia_character() {
        assert_eq!(kinds("const x = 1;\n// after"), []);
        assert_eq!(kinds("/* a */ const x = 1; /* b */"), [(CommentKind::MultiLine, "/* a */")]);
    }

    #[test]
    fn an_unterminated_block_comment_runs_to_end_of_file() {
        let source = "/* never closed";
        assert_eq!(kinds(source), [(CommentKind::MultiLine, "/* never closed")]);
    }

    #[test]
    fn unicode_whitespace_between_comments_is_trivia() {
        // U+00A0 is whitespace to the scanner; a naive ASCII-only loop stops
        // here and loses every directive after it.
        let source = "// a\n\u{a0}// b\nx";
        assert_eq!(
            kinds(source),
            [(CommentKind::SingleLine, "// a"), (CommentKind::SingleLine, "// b")]
        );
    }

    #[test]
    fn an_empty_file_has_no_comments() {
        assert_eq!(kinds(""), []);
        assert_eq!(kinds("\n\n"), []);
    }
}
