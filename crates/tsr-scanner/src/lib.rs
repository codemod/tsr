//! The TypeScript scanner.
//!
//! Ported from typescript-go's `internal/scanner/scanner.go`.
//!
//! # Shape
//!
//! A [`Scanner`] walks source text producing one token at a time. It is a
//! pull-based cursor, not an iterator producing a token vector: the parser needs
//! to re-scan the same position under different rules (a `/` is a division
//! operator or the start of a regular expression depending on grammatical
//! context, and `>>` may need splitting into two `>` when closing type
//! arguments), which a materialised token stream cannot express.
//!
//! Positions are byte offsets into UTF-8 source, held as `u32` — see
//! [`tsr_core::Span`].

mod comments;
mod generated;
mod jsdoc;
mod token;

pub use comments::{
    CommentKind, TriviaComment, is_shebang_trivia, leading_comment_ranges, scan_shebang_trivia,
};
pub use generated::keywords::keyword_kind;
pub use jsdoc::{CommentRange, is_jsdoc_like_text, jsdoc_ranges_in};
pub use token::{Token, TokenFlags};

use tsr_ast::SyntaxKind;
use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};

use generated::unicode;

/// ASCII bytes that may start an identifier: `A-Z a-z $ _`.
///
/// A table rather than a chain of comparisons because this is consulted once per
/// token and, in the continuation form, once per character of every identifier in
/// the file. One indexed load beats three branches, and the table is built at
/// compile time so it costs nothing but 256 bytes of rodata.
#[allow(clippy::cast_possible_truncation)] // `i < 128`, so the cast is exact.
static ASCII_ID_START: [bool; 256] = {
    let mut table = [false; 256];
    let mut i = 0;
    while i < 128 {
        let b = i as u8;
        table[i] = b.is_ascii_alphabetic() || b == b'$' || b == b'_';
        i += 1;
    }
    table
};

/// ASCII bytes that may continue an identifier: [`ASCII_ID_START`] plus digits.
#[allow(clippy::cast_possible_truncation)] // `i < 128`, so the cast is exact.
static ASCII_ID_PART: [bool; 256] = {
    let mut table = [false; 256];
    let mut i = 0;
    while i < 128 {
        let b = i as u8;
        table[i] = b.is_ascii_alphanumeric() || b == b'$' || b == b'_';
        i += 1;
    }
    table
};

/// Whether a code point may begin an identifier.
///
/// `$` and `_` are permitted by ECMAScript in addition to `ID_Start`.
#[must_use]
pub fn is_identifier_start(cp: char) -> bool {
    let c = cp as u32;
    if c < 128 {
        return ASCII_ID_START[c as usize];
    }
    unicode::contains(unicode::ID_START, c)
}

/// Whether a code point may continue an identifier.
///
/// Adds `$`, `_`, and the zero-width joiner/non-joiner, which ECMAScript permits.
#[must_use]
pub fn is_identifier_part(cp: char) -> bool {
    let c = cp as u32;
    if c < 128 {
        return ASCII_ID_PART[c as usize];
    }
    // ZWNJ (200C) and ZWJ (200D) are valid identifier parts per ECMAScript.
    c == 0x200C || c == 0x200D || unicode::contains(unicode::ID_CONTINUE, c)
}

/// Whether a code point terminates a line.
///
/// ECMAScript counts LS (2028) and PS (2029) as line terminators; most languages
/// do not, and forgetting them shifts every subsequent line number.
#[must_use]
pub fn is_line_break(cp: char) -> bool {
    matches!(cp, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Whether a code point is whitespace for ECMAScript's purposes.
///
/// Ported from typescript-go's `stringutil.IsWhiteSpaceSingleLine`. Two entries
/// are easy to miss and both appear in the corpus: **U+0085 (NEL)** is whitespace
/// but explicitly *not* a line break — it is in the Zs category yet outside
/// ECMAScript's line-terminator set — and **U+200B (zero-width space)** is
/// whitespace despite being invisible.
#[must_use]
pub fn is_whitespace_single_line(cp: char) -> bool {
    matches!(
        cp,
        ' ' | '\t'
            | '\u{000B}' // vertical tab
            | '\u{000C}' // form feed
            | '\u{0085}' // next line — whitespace, not a line break
            | '\u{00A0}' // non-breaking space
            | '\u{1680}' // ogham space mark
            | '\u{2000}'
            ..='\u{200A}'
            | '\u{200B}' // zero-width space
            | '\u{202F}' // narrow no-break space
            | '\u{205F}' // medium mathematical space
            | '\u{3000}' // ideographic space
            | '\u{FEFF}' // byte order mark
    )
}

/// A saved scanner position, produced by [`Scanner::save`].
///
/// Deliberately opaque and `Copy`: it is a bookmark, not a snapshot of the source.
#[derive(Debug, Clone, Copy)]
pub struct ScannerState {
    pos: u32,
    token_start: u32,
    full_start: u32,
    limit: u32,
    token: Token,
    diagnostic_count: usize,
}

/// A cursor over source text producing tokens.
pub struct Scanner<'a> {
    source: &'a str,
    /// Byte offset of the next character to read.
    pos: u32,
    /// Start of the current token, after leading trivia.
    token_start: u32,
    /// Start of the current token including leading trivia.
    full_start: u32,
    /// Byte offset at which scanning stops, as if the source ended there.
    ///
    /// Normally `source.len()`. JSDoc parsing narrows it to the comment body so
    /// the closing `*/` is out of reach: upstream achieves the same by slicing
    /// `sourceText` and calling `SetText`, which we cannot do without either
    /// re-borrowing or losing the `'a` lifetime that ties token text to the
    /// original source.
    limit: u32,
    /// Depth of nested "skip leading `*`" requests.
    ///
    /// A counter rather than a flag because JSDoc type parsing re-enters the
    /// normal scanner, and the innermost region must not clear the outer one's
    /// setting on exit. Upstream counts for the same reason.
    skip_jsdoc_leading_asterisks: u32,
    token: Token,
    /// Decoded value for identifiers, strings, and numbers.
    ///
    /// Only populated when the raw text differs from the value (escapes, numeric
    /// separators), so the common case allocates nothing.
    value: Option<String>,
    /// Whether a template continuation should report an invalid escape.
    ///
    /// Set by [`Scanner::rescan_template`] and read by the continuation rescan,
    /// which happens once per `}` and cannot be told the tag from its own
    /// arguments. Upstream passes `isTaggedTemplate` to
    /// `ReScanTemplateToken` at every call; this port carries it between them.
    /// §223.
    report_template_escapes: bool,
    /// `languageVariant == LanguageVariantJSX`: the one place ordinary
    /// scanning depends on it is `</`, which is a single token in JSX.
    jsx: bool,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Scanner<'a> {
    /// Create a scanner over `source`, positioned before the first token.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            token_start: 0,
            full_start: 0,
            // `source.len()` fits in `u32`: the parser rejects larger inputs.
            #[allow(clippy::cast_possible_truncation)]
            limit: source.len() as u32,
            skip_jsdoc_leading_asterisks: 0,
            token: Token::new(SyntaxKind::Unknown, Span::at(0), TokenFlags::empty()),
            value: None,
            report_template_escapes: true,
            jsx: false,
            diagnostics: Vec::new(),
        }
    }

    /// `SetLanguageVariant(core.LanguageVariantJSX)` (`scanner.go`), which
    /// the parser calls for `.tsx`/`.jsx` files before the first token.
    pub fn set_jsx_language_variant(&mut self, jsx: bool) {
        self.jsx = jsx;
    }

    /// The most recently scanned token.
    #[must_use]
    pub fn token(&self) -> Token {
        self.token
    }

    /// The raw source text of the current token.
    #[must_use]
    pub fn token_text(&self) -> &'a str {
        &self.source[self.token.span.start as usize..self.token.span.end as usize]
    }

    /// The decoded value of the current token.
    ///
    /// For identifiers this is the text with escapes resolved; for strings, the
    /// contents with escapes resolved; for numbers, the digits with separators
    /// removed. Equal to [`Scanner::token_text`] when no decoding was needed.
    #[must_use]
    pub fn token_value(&self) -> &str {
        self.value.as_deref().unwrap_or_else(|| self.token_text())
    }

    /// The decoded value, if the token needed decoding.
    ///
    /// `None` when the token's text is its value, which is the overwhelmingly
    /// common case. Distinct from [`Scanner::token_value`], which papers over the
    /// difference — callers that only want to know *whether* decoding happened
    /// should ask this and avoid materialising the text.
    #[must_use]
    pub fn decoded_value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Diagnostics produced so far.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Take ownership of the diagnostics, leaving the scanner's list empty.
    #[must_use]
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Byte offset of the current token including any leading trivia.
    #[must_use]
    pub fn full_start(&self) -> u32 {
        self.full_start
    }

    /// Capture the scanner's position so it can be rewound.
    ///
    /// The parser needs this for speculative parsing: TypeScript's grammar is not
    /// LL(k), so deciding between an arrow function and a parenthesised expression
    /// means trying one and backing out. Diagnostics emitted during a speculative
    /// scan are discarded by [`Scanner::restore`] — otherwise a path the parser
    /// abandoned would still report its errors.
    #[must_use]
    pub fn save(&self) -> ScannerState {
        ScannerState {
            pos: self.pos,
            token_start: self.token_start,
            full_start: self.full_start,
            limit: self.limit,
            token: self.token,
            diagnostic_count: self.diagnostics.len(),
        }
    }

    /// Rewind to a captured position, discarding diagnostics emitted since.
    pub fn restore(&mut self, state: ScannerState) {
        self.pos = state.pos;
        self.token_start = state.token_start;
        self.full_start = state.full_start;
        self.limit = state.limit;
        self.token = state.token;
        // Guarded: `Vec::truncate` to the current length still calls the
        // out-of-line `[Diagnostic]` drop glue on an empty tail, and almost
        // every rewind (a lookahead) emitted nothing (r5-binperf.md §5).
        if state.diagnostic_count < self.diagnostics.len() {
            self.diagnostics.truncate(state.diagnostic_count);
        }
        // The decoded value belongs to the token we just discarded.
        self.value = None;
    }

    // ---- character access ----------------------------------------------

    fn rest(&self) -> &'a str {
        // Everything downstream asks "is there a character here?", so confining
        // the window is enough to make `limit` behave exactly like end-of-file.
        &self.source[self.pos as usize..self.limit as usize]
    }

    fn peek(&self) -> Option<char> {
        let pos = self.pos as usize;
        let bytes = self.source.as_bytes();
        if pos >= self.limit as usize {
            return None;
        }
        let byte = bytes[pos];
        if byte < 0x80 {
            return Some(byte as char);
        }
        self.rest().chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        let pos = self.pos as usize + offset;
        let bytes = self.source.as_bytes();
        if pos < self.limit as usize && bytes[self.pos as usize..=pos].is_ascii() {
            return Some(bytes[pos] as char);
        }
        self.rest().chars().nth(offset)
    }

    fn bump(&mut self) -> Option<char> {
        let pos = self.pos as usize;
        let bytes = self.source.as_bytes();
        if pos >= self.limit as usize {
            return None;
        }
        let byte = bytes[pos];
        if byte < 0x80 {
            self.pos += 1;
            return Some(byte as char);
        }
        let ch = self.rest().chars().next()?;
        #[allow(clippy::cast_possible_truncation)]
        {
            self.pos += ch.len_utf8() as u32;
        }
        Some(ch)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            // `len_utf8` is 1..=4, so the cast cannot truncate.
            #[allow(clippy::cast_possible_truncation)]
            {
                self.pos += expected.len_utf8() as u32;
            }
            true
        } else {
            false
        }
    }

    /// Flags describing a block comment that was just skipped.
    ///
    /// JSDoc is recorded here rather than parsed: the parser only needs to know
    /// that a `/** … */` was in this token's trivia, and the two tag scans are
    /// substring searches that let it skip the JSDoc parse entirely for the
    /// overwhelming majority of nodes. See `docs/architecture/jsdoc.md`.
    fn classify_block_comment(&self, start: u32) -> TokenFlags {
        let text = &self.source[start as usize..self.pos as usize];
        if !is_jsdoc_like_text(text) {
            return TokenFlags::empty();
        }
        let mut flags = TokenFlags::PRECEDING_JSDOC_COMMENT;
        if mentions_tag(text, &["deprecated"]) {
            flags |= TokenFlags::PRECEDING_JSDOC_WITH_DEPRECATED;
        }
        if mentions_tag(text, &["see", "link", "linkcode", "linkplain"]) {
            flags |= TokenFlags::PRECEDING_JSDOC_WITH_SEE_OR_LINK;
        }
        flags
    }

    fn error(&mut self, message: &'static tsr_diagnostics::Message, span: Span) {
        self.diagnostics.push(Diagnostic::new(message, span));
    }

    /// `errorAt` with substitution arguments.
    fn error_with(
        &mut self,
        message: &'static tsr_diagnostics::Message,
        span: Span,
        args: &[&str],
    ) {
        self.diagnostics.push(Diagnostic::with_args(
            message,
            span,
            args.iter().map(|a| (*a).to_string()),
        ));
    }

    // ---- scanning ------------------------------------------------------

    /// Scan the next token and return it.
    pub fn scan(&mut self) -> Token {
        self.value = None;
        // A `#!` shebang is trivia, but only on the very first line of a file —
        // anywhere else `#` starts a private name.
        if self.pos == 0 && self.source.starts_with("#!") {
            while self.peek().is_some_and(|c| !is_line_break(c)) {
                self.bump();
            }
        }
        self.full_start = self.pos;
        let mut flags = TokenFlags::empty();

        // Skip trivia, remembering whether a line break was crossed. The parser
        // needs that for automatic semicolon insertion, so it rides on the token
        // rather than requiring a separate trivia scan.
        //
        // Driven by bytes rather than `char`s. Trivia is the single hottest loop
        // in the scanner — it runs between every pair of tokens, and indented
        // source is mostly spaces — and every case that matters is one ASCII
        // byte. Only the rare non-ASCII trivia (NBSP, U+2028, U+FEFF) needs a
        // decode, and it falls out of the loop to get one.
        let bytes = self.source.as_bytes();
        let limit = self.limit as usize;
        let mut i = self.pos as usize;
        'trivia: loop {
            while i < limit {
                match bytes[i] {
                    b' ' | b'\t' | 0x0b | 0x0c => i += 1,
                    b'\n' => {
                        flags |= TokenFlags::PRECEDING_LINE_BREAK;
                        i += 1;
                    }
                    b'\r' => {
                        flags |= TokenFlags::PRECEDING_LINE_BREAK;
                        i += 1;
                        // CRLF is one break.
                        if i < limit && bytes[i] == b'\n' {
                            i += 1;
                        }
                    }
                    b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                        i += 2;
                        while i < limit {
                            match bytes[i] {
                                b'\n' | b'\r' => break,
                                // U+2028 and U+2029 are line terminators in
                                // ECMAScript and end a line comment. They are the
                                // only non-ASCII bytes that mean anything here,
                                // so everything else is comment text and is
                                // skipped as bytes — breaking out on any
                                // non-ASCII byte would resume scanning *inside*
                                // the comment and read its contents as code.
                                0xE2 if i + 2 < limit
                                    && bytes[i + 1] == 0x80
                                    && matches!(bytes[i + 2], 0xA8 | 0xA9) =>
                                {
                                    break;
                                }
                                _ => i += 1,
                            }
                        }
                    }
                    b'/' if i + 1 < limit && bytes[i + 1] == b'*' => break,
                    // §269: a line-leading `*` inside a JSDoc bridge is the
                    // comment's decoration, not a token — upstream's
                    // `skipJSDocLeadingAsterisks` arm (`scanner.go:569`). One
                    // per token, exactly as upstream's
                    // `PrecedingJSDocLeadingAsterisks` guard enforces, and
                    // never when it would split `**` or `*=` (upstream tests
                    // those first because the arm lives in its punctuation
                    // scanner). This counter was WRITE-ONLY until now: the
                    // parser has set it around every bridged type expression
                    // since §110, and multi-line `{...}` types worked only
                    // when they avoided a continuation `*`.
                    b'*' if self.skip_jsdoc_leading_asterisks > 0
                        && flags.contains(TokenFlags::PRECEDING_LINE_BREAK)
                        && !flags.contains(TokenFlags::PRECEDING_JSDOC_LEADING_ASTERISKS)
                        && (i + 1 >= limit || !matches!(bytes[i + 1], b'*' | b'=')) =>
                    {
                        flags |= TokenFlags::PRECEDING_JSDOC_LEADING_ASTERISKS;
                        i += 1;
                    }
                    // §301: a MERGE CONFLICT MARKER at line start is trivia
                    // with an error — `isConflictMarkerTrivia` /
                    // `scanConflictMarkerTrivia` (`scanner.go:2409/2444`).
                    // Seven identical `<`/`>`/`=`/`|` at a line start (for
                    // `=` unconditionally, for the others followed by a
                    // space): `<`/`>` skip their line; `=`/`|` skip until the
                    // start of the next `=======` or `>>>>>>>` marker.
                    // Without this, `<<<<<<< HEAD` lexed as shift operators
                    // and the fixtures printed the marker as expressions
                    // (`conflictMarkerTrivia1/3`, `conflictMarkerDiff3Trivia1`).
                    b @ (b'<' | b'=' | b'>' | b'|')
                        if (flags.contains(TokenFlags::PRECEDING_LINE_BREAK) || i == 0)
                            && Self::is_conflict_marker(bytes, i, limit) =>
                    {
                        i = self.scan_conflict_marker_trivia(i, b);
                    }
                    // Anything else ASCII starts a token.
                    b if b < 0x80 => break 'trivia,
                    // Non-ASCII: might be trivia, might be an identifier.
                    _ => break,
                }
            }
            if i >= limit {
                break;
            }

            // Out of the byte loop: either non-ASCII, or a `/*` comment, or a
            // line comment that ran into a non-ASCII character. Fall back to the
            // character-level path for one step, then resume.
            #[allow(clippy::cast_possible_truncation)]
            {
                self.pos = i as u32;
            }
            let Some(ch) = self.peek() else { break };
            if ch == '/' {
                if self.peek_at(1) == Some('*') {
                    let comment_start = self.pos;
                    if self.skip_block_comment() {
                        flags |= TokenFlags::PRECEDING_LINE_BREAK;
                    }
                    flags |= self.classify_block_comment(comment_start);
                } else {
                    // A line comment stopped at a non-ASCII character.
                    self.skip_line_comment();
                }
            } else if is_line_break(ch) {
                flags |= TokenFlags::PRECEDING_LINE_BREAK;
                self.bump();
            } else if is_whitespace_single_line(ch) {
                self.bump();
            } else {
                break;
            }
            i = self.pos as usize;
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            self.pos = i.min(limit) as u32;
        }

        self.token_start = self.pos;
        let kind = self.scan_token(&mut flags);
        self.token = Token::new(kind, Span::new(self.token_start, self.pos), flags);
        self.token
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if is_line_break(ch) {
                break;
            }
            self.bump();
        }
    }

    /// Skip `/* … */`, returning whether it spanned a line break.
    ///
    /// Driven by bytes, like the trivia loop in [`Scanner::scan`]: comment text
    /// is most of the bytes in a commented declaration file (`lib.dom.d.ts`),
    /// and a `char` decode per byte made this the scanner's largest self cost
    /// (`r5-bind.md` §3). Only `*`, `/`, `\n`, `\r` and the UTF-8 lead of
    /// U+2028/U+2029 matter, all of which are bytes no UTF-8 continuation
    /// byte can equal, so every other byte is skipped without decoding.
    fn skip_block_comment(&mut self) -> bool {
        let start = self.pos;
        let bytes = self.source.as_bytes();
        let limit = self.limit as usize;
        // Past the opening `/*`.
        let mut i = start as usize + 2;
        let mut crossed_line = false;
        while i < limit {
            match bytes[i] {
                b'*' if i + 1 < limit && bytes[i + 1] == b'/' => {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        self.pos = (i + 2) as u32;
                    }
                    return crossed_line;
                }
                b'\n' | b'\r' => {
                    crossed_line = true;
                    break;
                }
                // U+2028 / U+2029 (`E2 80 A8` / `E2 80 A9`), the non-ASCII
                // line terminators `is_line_break` accepts.
                0xE2 if i + 2 < limit
                    && bytes[i + 1] == 0x80
                    && matches!(bytes[i + 2], 0xA8 | 0xA9) =>
                {
                    crossed_line = true;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        // Once a line break is seen only the terminator matters, and `*` is
        // found by the library's word-at-a-time `memchr` rather than per byte.
        // `i` sits on an ASCII byte or a U+2028/U+2029 lead, so the slice
        // starts on a character boundary.
        if crossed_line {
            let text = &self.source[..limit];
            while let Some(offset) = text[i..].find('*') {
                let star = i + offset;
                if star + 1 < limit && bytes[star + 1] == b'/' {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        self.pos = (star + 2) as u32;
                    }
                    return true;
                }
                i = star + 1;
            }
            i = limit;
        }
        debug_assert!(i >= limit);
        // Unterminated: `s.error(Asterisk_Slash_expected)` (`scanner.go:677`)
        // reports zero-width at `s.pos`, the end of the text, not at the
        // opening delimiter.
        self.pos = self.limit;
        self.error(&messages::ASTERISK_SLASH_EXPECTED, Span::at(self.pos));
        crossed_line
    }

    fn scan_token(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        let Some(ch) = self.peek() else {
            return SyntaxKind::EndOfFile;
        };

        // Dispatch on the byte, not the decoded character. Identifiers, digits,
        // quotes and punctuation are all ASCII, so only genuinely non-ASCII input
        // needs `ch` at all — and that is the rare case.
        let byte = self.source.as_bytes()[self.pos as usize];
        match byte {
            b'0'..=b'9' => self.scan_number(flags),
            // `case '.'` (`scanner.go`): a digit after the dot is `scanNumber`
            // from the dot, so `.1n` reaches its bigint-suffix report.
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.scan_number(flags),
            b'"' | b'\'' => self.scan_string(flags),
            b'`' => self.scan_template(flags),
            // `#x` is a private identifier: one token, not `#` then `x`.
            b'#' if self.peek_at(1).is_some_and(|c| is_identifier_start(c) || c == '\\') => {
                self.bump();
                self.scan_identifier_or_keyword(flags);
                SyntaxKind::PrivateIdentifier
            }
            // The rest of `case '#'` (`scanner.go:897`): a `#!` past the
            // file's first position is TS18026 over both characters and an
            // `Unknown` token of the `#` alone; any other `#` is an invalid
            // character, still scanned as a (nameless) private identifier.
            b'#' => {
                let start = self.pos;
                self.bump();
                if self.peek() == Some('!') {
                    self.error(
                        &messages::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE,
                        Span::new(start, start + 2),
                    );
                    SyntaxKind::Unknown
                } else {
                    self.error(&messages::INVALID_CHARACTER, Span::new(start, start + 1));
                    SyntaxKind::PrivateIdentifier
                }
            }
            b'\\' => self.scan_identifier_or_keyword(flags),
            b if ASCII_ID_START[b as usize] => self.scan_identifier_or_keyword(flags),
            b if b < 0x80 => self.scan_punctuation(),
            // Non-ASCII: only now is it worth decoding, to tell an identifier
            // start from a stray character.
            _ => {
                if is_identifier_start(ch) {
                    self.scan_identifier_or_keyword(flags)
                } else if ch == char::REPLACEMENT_CHARACTER {
                    // A binary file: report once at the start and abandon it.
                    //
                    // Ported from `scanner.Scan`'s default arm
                    // (`internal/scanner/scanner.go:936-941`): on `utf8.RuneError`
                    // upstream reports `File_appears_to_be_binary` at offset 0 with
                    // length 0, sets `pos` to the end of the text, and returns
                    // `KindNonTextFileMarkerTrivia`.
                    //
                    // Abandoning the file is the whole point. Without it every
                    // undecodable byte is its own `TS1127 Invalid character`:
                    // `compiler/TransportStream` alone produced 557 of them where
                    // upstream produces one, and `compiler/corrupted`'s baseline has
                    // exactly one error in total.
                    //
                    // Upstream tests `ch == utf8.RuneError` without checking the
                    // decoded size, so a file containing a *validly encoded* U+FFFD
                    // is also declared binary — and `corrupted.ts` is precisely
                    // that, three `EF BF BD` sequences. Matching on the replacement
                    // character reproduces that, including the quirk: by the time
                    // the scanner sees the text, an undecodable byte and a real
                    // U+FFFD are the same character, because `decode_bytes`
                    // substituted one for the other.
                    self.error(&messages::FILE_APPEARS_TO_BE_BINARY, Span::new(0, 0));
                    self.pos = self.limit;
                    SyntaxKind::NonTextFileMarkerTrivia
                } else {
                    self.scan_punctuation()
                }
            }
        }
    }

    // ---- identifiers ----------------------------------------------------

    fn scan_identifier_or_keyword(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        let start = self.pos;

        // Fast path: a run of ASCII identifier bytes. This is what essentially
        // every identifier in real source is, and it costs one table load and one
        // increment per byte instead of a UTF-8 decode, a `char` classification
        // and a second read to advance.
        let bytes = self.source.as_bytes();
        let limit = self.limit as usize;
        let mut i = self.pos as usize;
        while i < limit && ASCII_ID_PART[bytes[i] as usize] {
            i += 1;
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            self.pos = i as u32;
        }

        // Anything that could still continue the identifier — a `\u` escape or a
        // non-ASCII code point — drops into the general loop below, which handles
        // both. Otherwise the identifier is exactly the bytes just scanned.
        let needs_slow_path = i < limit && (bytes[i] == b'\\' || bytes[i] >= 0x80);
        if !needs_slow_path {
            let text = &self.source[start as usize..i];
            // A zero-length match means the caller dispatched here on a character
            // the fast loop rejects (a leading `\`), which the general loop below
            // is responsible for.
            if !text.is_empty() {
                return keyword_kind(text).unwrap_or(SyntaxKind::Identifier);
            }
        }

        let mut decoded: Option<String> = self.value.clone();

        while let Some(ch) = self.peek() {
            if ch == '\\' {
                // A unicode escape inside an identifier: `\u0061bc` is `abc`.
                //
                // **Peeked, not consumed.** `scanIdentifierParts`
                // (`scanner.go:1571-1579`) tests `peekUnicodeEscape()`, which
                // does not move the cursor, and `break`s with `pos` still on
                // the backslash when the escape is invalid *or* decodes to
                // something that cannot continue an identifier. This port
                // consumed the `\` and the `u` before finding out, so
                // `var arg\uxxxx` scanned as `arg\u` + `xxxx` where upstream
                // scans `arg` + an invalid-character `\` + `uxxxx`, and
                // `\u0031a` kept its backslash. The TYPES were right in both
                // cases; the assertion's source TEXT was not, which fails a
                // baseline line just the same. §208.
                //
                // Breaking rather than reporting here is also upstream's: the
                // main scanner's fallback arm sees the `\` next and reports
                // `Invalid character` once, at the backslash.
                let escape_start = self.pos;
                let peeked = self.peek_unicode_escape().and_then(char::from_u32);
                // Whether this escape spells the identifier's FIRST character
                // is decided by where the escape began, not by the cursor
                // after it (`\u0031a`: `1` cannot start an identifier;
                // `invalidUnicodeEscapeSequance4`).
                let valid = match peeked {
                    Some(decoded_char) if escape_start == start => {
                        is_identifier_start(decoded_char)
                    }
                    Some(decoded_char) => is_identifier_part(decoded_char),
                    None => false,
                };
                if !valid {
                    break;
                }
                self.bump();
                let Some(decoded_char) = self.scan_unicode_escape().and_then(char::from_u32) else {
                    break;
                };
                let buffer = decoded.get_or_insert_with(|| {
                    self.source[start as usize..escape_start as usize].to_string()
                });
                buffer.push(decoded_char);
                *flags |= TokenFlags::UNICODE_ESCAPE;
                continue;
            }

            if !is_identifier_part(ch) {
                break;
            }
            self.bump();
            if let Some(buffer) = decoded.as_mut() {
                buffer.push(ch);
            }
        }

        // **A LEADING escape that cannot start an identifier is not an
        // identifier at all.** Upstream tests it in `scan()` before ever
        // calling `scanIdentifier`, and answers `Invalid character` plus one
        // consumed backslash. This port dispatches here on the leading `\`,
        // so the same answer has to be produced here — and it has to consume
        // something, or the caller re-dispatches on the same backslash for
        // ever. §208's first build did exactly that and hung the corpus run.
        if self.pos == start {
            self.bump();
            self.error(&messages::INVALID_CHARACTER, Span::new(start, self.pos));
            return SyntaxKind::Unknown;
        }

        let text = &self.source[start as usize..self.pos as usize];

        // §302: the keyword table keys the DECODED text — upstream's
        // `getIdentifierToken` runs on `tokenValue` whatever spelled it, so
        // `default` IS the `default` keyword, carrying
        // `TokenFlagsUnicodeEscape` for the parser's "keyword must not
        // contain escaped characters" report
        // (`switchStatementsWithMultipleDefaults` parses it as the clause).
        // The rule this replaces ("an identifier written with escapes is
        // never a keyword") was exactly backwards.
        let lookup = decoded.as_deref().unwrap_or(text);
        if let Some(kind) = keyword_kind(lookup) {
            self.value = decoded;
            return kind;
        }
        self.value = decoded;
        SyntaxKind::Identifier
    }

    /// Scan `u{XXXX}` or `uXXXX` after the backslash has been consumed.
    ///
    /// Returns the raw code point rather than a `char`, because JavaScript strings
    /// are UTF-16 and may contain lone surrogates (`"\u{D800}"` is legal, and the
    /// corpus tests it). A `char` cannot represent one, so the caller decides how
    /// to encode it.
    fn scan_unicode_escape(&mut self) -> Option<u32> {
        self.scan_unicode_escape_ex(true)
    }

    /// `peekUnicodeEscape` (`scanner.go:1571`): decode the escape the cursor is
    /// sitting on **without moving** and without reporting.
    ///
    /// The cursor is on the backslash, which this steps over and then restores.
    /// Silent by construction — a caller that peeks and declines must leave the
    /// diagnostic to whoever consumes the character, or the same backslash is
    /// reported twice.
    fn peek_unicode_escape(&mut self) -> Option<u32> {
        let saved = self.pos;
        self.bump();
        let decoded = self.scan_unicode_escape_ex(false);
        self.pos = saved;
        decoded
    }

    /// `scanUnicodeEscape(shouldEmitInvalidEscapeError)` (`scanner.go:1854`).
    ///
    /// The flag is `false` inside a **tagged** template, where the ES2018
    /// revision permits an invalid escape: the cooked value is `undefined` and
    /// the tag receives the raw text. §223.
    fn scan_unicode_escape_ex(&mut self, report: bool) -> Option<u32> {
        if !self.eat('u') {
            return None;
        }
        // `start` is the position **after** `\u`, as upstream's is
        // (`scanner.go:1856`, `s.pos += 2; start := s.pos`).
        let start = self.pos;
        if self.eat('{') {
            let digits_start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.bump();
            }
            let digits = &self.source[digits_start as usize..self.pos as usize];
            if digits.is_empty() {
                if report {
                    self.error(
                        &messages::HEXADECIMAL_DIGIT_EXPECTED,
                        Span::new(self.pos, self.pos),
                    );
                }
                return None;
            }
            // `hexValue > 0x10FFFF` (`scanner.go:1877`) is **its own message**
            // and its own span — `errorAt(msg, start+1, s.pos-start-1)`, the
            // digits themselves. This function used to fold it into the caller's
            // `Hexadecimal digit expected`, with a comment calling that "close
            // enough until the parser distinguishes the two". §222 measured it:
            // `templateLiteralEscapeSequence` invents 22 lines, and every one is
            // this fold or the position it reports at.
            let value = u32::from_str_radix(digits, 16).ok();
            let out_of_range = value.is_none_or(|v| v > 0x10_FFFF);
            if out_of_range && report {
                self.error(
                    &messages::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                    Span::new(start + 1, self.pos),
                );
            }
            // The closing `}` is checked **after** the range, so
            // `\u{ffffff}` reports the range and not the terminator.
            if self.peek().is_none() {
                if report {
                    self.error(&messages::UNEXPECTED_END_OF_TEXT, Span::new(self.pos, self.pos));
                }
                return None;
            }
            if !self.eat('}') {
                if report {
                    self.error(
                        &messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                        Span::new(self.pos, self.pos),
                    );
                }
                return None;
            }
            if out_of_range {
                return None;
            }
            return value;
        }
        let digits_start = self.pos;
        for _ in 0..4 {
            if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                if report {
                    self.error(
                        &messages::HEXADECIMAL_DIGIT_EXPECTED,
                        Span::new(self.pos, self.pos),
                    );
                }
                return None;
            }
            self.bump();
        }
        let digits = &self.source[digits_start as usize..self.pos as usize];
        u32::from_str_radix(digits, 16).ok()
    }

    // ---- numbers --------------------------------------------------------

    fn scan_number(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        if self.peek() == Some('0') {
            let radix = match self.peek_at(1) {
                Some('x' | 'X') => Some((16, TokenFlags::HEX_SPECIFIER)),
                Some('b' | 'B') => Some((2, TokenFlags::BINARY_SPECIFIER)),
                Some('o' | 'O') => Some((8, TokenFlags::OCTAL_SPECIFIER)),
                _ => None,
            };
            if let Some((radix, flag)) = radix {
                self.bump();
                self.bump();
                *flags |= flag;
                let digits = self.scan_digits(radix, flags);
                if digits == 0 {
                    // `scanner.go:703/728/740`: one message per radix.
                    let message = match radix {
                        16 => &messages::HEXADECIMAL_DIGIT_EXPECTED,
                        2 => &messages::BINARY_DIGIT_EXPECTED,
                        _ => &messages::OCTAL_DIGIT_EXPECTED,
                    };
                    self.error(message, Span::new(self.pos, self.pos));
                }
                if self.eat('n') {
                    return SyntaxKind::BigIntLiteral;
                }
                return SyntaxKind::NumericLiteral;
            }
        }

        // typescript-go's `Scanner.scanNumber` (`scanner.go`), decimal arm.
        //
        // After a `0`, upstream splits three ways: nothing follows (plain
        // zero), the run contains an 8 or a 9 (a decimal with a leading zero,
        // `Decimals with leading zeros are not allowed`), or every digit is
        // octal — the pre-ES5 literal, which is `Octal literals are not
        // allowed`.
        //
        // `withMinus` reads the *previous* token, which `self.token` still
        // holds while the next one is scanned: after a `-` the suggested
        // spelling gains the sign and the span widens one character left,
        // onto whatever precedes the digits (`-01` → `-0o1` over `-01`).
        let start = self.pos;
        let mut leading_zero = false;
        if self.peek() == Some('0') {
            self.bump();
            if self.peek() == Some('_') {
                *flags |= TokenFlags::CONTAINS_SEPARATOR;
                self.error(
                    &messages::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                    Span::new(self.pos, self.pos + 1),
                );
                self.pos = start;
                self.scan_digits(10, flags);
            } else {
                // `scanDigits`: plain digits, no separators.
                let digits_start = self.pos;
                let mut is_octal = true;
                while let Some(ch) = self.peek().filter(char::is_ascii_digit) {
                    is_octal &= ch < '8';
                    self.bump();
                }
                if self.pos > digits_start {
                    if is_octal {
                        let digits = &self.source[digits_start as usize..self.pos as usize];
                        let value = u64::from_str_radix(digits, 8).unwrap_or(0);
                        let with_minus = self.token.kind == SyntaxKind::MinusToken;
                        let sign = if with_minus { "-" } else { "" };
                        self.error_with(
                            &messages::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX_0,
                            Span::new(start - u32::from(with_minus), self.pos),
                            &[&format!("{sign}0o{value:o}")],
                        );
                        return SyntaxKind::NumericLiteral;
                    }
                    leading_zero = true;
                }
            }
        } else {
            self.scan_digits(10, flags);
        }
        let fixed_part_end = self.pos;

        // Fractional part. `1.` is legal; `1.5` more so.
        if self.peek() == Some('.') {
            self.bump();
            self.scan_digits(10, flags);
        }
        let mut end = self.pos;

        // Exponent.
        let mut scientific = false;
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            scientific = true;
            *flags |= TokenFlags::SCIENTIFIC;
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if self.scan_digits(10, flags) == 0 {
                self.error(&messages::DIGIT_EXPECTED, Span::new(self.pos, self.pos));
            } else {
                end = self.pos;
            }
        }

        if leading_zero {
            self.error(
                &messages::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED,
                Span::new(start, self.pos),
            );
            return SyntaxKind::NumericLiteral;
        }

        let result = if fixed_part_end == self.pos && self.eat('n') {
            SyntaxKind::BigIntLiteral
        } else {
            SyntaxKind::NumericLiteral
        };

        // An identifier glued to the literal: `3a`, `1n` after a fraction.
        if self.peek().is_some_and(is_identifier_start) {
            let id_start = self.pos;
            while self.peek().is_some_and(is_identifier_part) {
                self.bump();
            }
            if result != SyntaxKind::BigIntLiteral
                && self.pos == id_start + 1
                && self.source.as_bytes()[id_start as usize] == b'n'
            {
                // The `n` stays part of the literal, as upstream's token
                // value (`text[start:end]`) leaves it out.
                self.value = Some(self.source[start as usize..end as usize].to_string());
                if scientific {
                    self.error(
                        &messages::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION,
                        Span::new(start, self.pos),
                    );
                    return result;
                }
                if fixed_part_end < id_start {
                    self.error(
                        &messages::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER,
                        Span::new(start, self.pos),
                    );
                    return result;
                }
            }
            self.error(
                &messages::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                Span::new(id_start, self.pos),
            );
            self.value = None;
            self.pos = id_start;
        }

        result
    }

    /// Consume digits of the given radix, allowing `_` separators.
    ///
    /// Returns how many digits were consumed, so callers can report an empty
    /// literal like `0x`.
    fn scan_digits(&mut self, radix: u32, flags: &mut TokenFlags) -> usize {
        // `scanNumberFragment` / `scanHexDigits` / `scanBinaryOrOctalDigits`
        // (`scanner.go`): a separator is allowed only right after a digit; one
        // right after another separator is `Multiple consecutive numeric
        // separators`, any other misplaced one `not allowed here`.
        let mut count = 0;
        let mut allow_separator = false;
        let mut is_previous_separator = false;
        while let Some(ch) = self.peek() {
            if ch == '_' {
                *flags |= TokenFlags::CONTAINS_SEPARATOR;
                if allow_separator {
                    allow_separator = false;
                    is_previous_separator = true;
                } else if is_previous_separator {
                    self.error(
                        &messages::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED,
                        Span::new(self.pos, self.pos + 1),
                    );
                } else {
                    self.error(
                        &messages::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                        Span::new(self.pos, self.pos + 1),
                    );
                }
                self.bump();
                continue;
            }
            if !ch.is_digit(radix) {
                break;
            }
            allow_separator = true;
            is_previous_separator = false;
            count += 1;
            self.bump();
        }
        if is_previous_separator {
            self.error(
                &messages::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                Span::new(self.pos - 1, self.pos),
            );
        }
        count
    }

    // ---- strings and templates ------------------------------------------

    fn scan_string(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        let quote = self.bump().expect("caller checked");
        if quote == '\'' {
            *flags |= TokenFlags::SINGLE_QUOTE;
        }
        let start = self.pos;
        let mut decoded: Option<String> = self.value.clone();

        loop {
            let Some(ch) = self.peek() else {
                *flags |= TokenFlags::UNTERMINATED;
                // `s.error` reports at `s.pos` with length zero
                // (`scanner.go:413`) — the same correction the template arm
                // needed, at the two sites `scanner.go:1613` and `:1630`.
                self.error(&messages::UNTERMINATED_STRING_LITERAL, Span::new(self.pos, self.pos));
                break;
            };
            if ch == quote {
                self.bump();
                break;
            }
            // Only CR and LF terminate a string. U+2028/U+2029 are line
            // terminators *for the grammar* but ES2019 explicitly permits them
            // unescaped inside string literals (the JSON-superset proposal), so
            // `is_line_break` is deliberately not used here.
            if ch == '\n' || ch == '\r' {
                *flags |= TokenFlags::UNTERMINATED;
                // `s.error` reports at `s.pos` with length zero
                // (`scanner.go:413`) — the same correction the template arm
                // needed, at the two sites `scanner.go:1613` and `:1630`.
                self.error(&messages::UNTERMINATED_STRING_LITERAL, Span::new(self.pos, self.pos));
                break;
            }
            if ch == '\\' {
                let escape_start = self.pos;
                self.bump();
                let buffer = decoded.get_or_insert_with(|| {
                    self.source[start as usize..escape_start as usize].to_string()
                });
                self.scan_escape_into(buffer, flags, true);
                continue;
            }
            self.bump();
            if let Some(buffer) = decoded.as_mut() {
                buffer.push(ch);
            }
        }

        self.value = Some(decoded.unwrap_or_else(|| {
            let end = self.pos.saturating_sub(1).max(start);
            self.source[start as usize..end as usize].to_string()
        }));
        SyntaxKind::StringLiteral
    }

    /// Decode one escape sequence, appending to `out`. The backslash is consumed.
    fn scan_escape_into(&mut self, out: &mut String, flags: &mut TokenFlags, report: bool) {
        // The backslash's position, for the octal/decimal spans below —
        // `scanEscapeSequence`'s `start` (`scanner.go:1691`) is taken BEFORE
        // the first character is consumed.
        let backslash = self.pos - 1;
        // `scanEscapeSequence` (`scanner.go:1694`): a backslash at end of
        // text is TS1126 at that end, whatever `flags` says.
        let Some(ch) = self.bump() else {
            self.error(&messages::UNEXPECTED_END_OF_TEXT, Span::new(self.pos, self.pos));
            return;
        };
        match ch {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{C}'),
            'v' => out.push('\u{B}'),
            '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => out.push('\0'),
            // §147 (`checker-notes-narrow.md`), from `scanEscapeSequence`
            // (`scanner.go:1700-1743`): a legacy octal escape COOKS to its
            // character when invalid escapes are reported (TS1487), and keeps
            // its RAW text when they are not (a template's initial scan —
            // the parser rescans untagged forms with reporting on). The
            // cascade: '0' followed by a digit falls through, '1'-'3' take up
            // to two more octal digits, '4'-'7' up to one; `\08` is NUL then
            // a plain '8', which the '0' arm above already answered.
            '0'..='7' => {
                let mut digits = String::from(ch);
                if matches!(ch, '0'..='3') && self.peek().is_some_and(|d| ('0'..='7').contains(&d))
                {
                    digits.push(self.bump().unwrap());
                }
                if self.peek().is_some_and(|d| ('0'..='7').contains(&d)) {
                    digits.push(self.bump().unwrap());
                }
                *flags |= TokenFlags::CONTAINS_INVALID_ESCAPE;
                if report {
                    let code = u32::from_str_radix(&digits, 8).unwrap();
                    self.error_with(
                        &messages::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX_0,
                        Span::new(backslash, self.pos),
                        &[&format!("\\x{code:02x}")],
                    );
                    out.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
                } else {
                    out.push('\\');
                    out.push_str(&digits);
                }
            }
            // `\8` and `\9`, the invalid decimal escapes (`scanner.go:1732`):
            // reported, they cook to the bare digit (TS1488); unreported,
            // the raw text survives.
            '8' | '9' => {
                *flags |= TokenFlags::CONTAINS_INVALID_ESCAPE;
                if report {
                    self.error_with(
                        &messages::ESCAPE_SEQUENCE_0_IS_NOT_ALLOWED,
                        Span::new(backslash, self.pos),
                        &[&format!("\\{ch}")],
                    );
                    out.push(ch);
                } else {
                    out.push('\\');
                    out.push(ch);
                }
            }
            'x' => {
                let start = self.pos;
                for _ in 0..2 {
                    if self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                        self.bump();
                    }
                }
                let digits = &self.source[start as usize..self.pos as usize];
                match u32::from_str_radix(digits, 16).ok().and_then(char::from_u32) {
                    Some(c) if digits.len() == 2 => out.push(c),
                    // §147: an invalid `\x` escape keeps its RAW text as the
                    // value (`scanner.go:1819`) — reported or not — and marks
                    // the token.
                    _ => {
                        *flags |= TokenFlags::CONTAINS_INVALID_ESCAPE;
                        if report {
                            self.error(
                                &messages::HEXADECIMAL_DIGIT_EXPECTED,
                                Span::new(self.pos, self.pos),
                            );
                        }
                        let end = self.pos as usize;
                        out.push_str(&self.source[backslash as usize..end]);
                    }
                }
            }
            'u' => {
                // Back up so the shared escape scanner sees the `u`.
                self.pos -= 1;
                if let Some(cp) = self.scan_unicode_escape_ex(report) {
                    self.push_code_point(cp, out);
                } else {
                    // §147: an invalid `\u` escape keeps its RAW text as the
                    // value (`scanner.go:1773`/`:1786`) and marks the token.
                    *flags |= TokenFlags::CONTAINS_INVALID_ESCAPE;
                    let end = self.pos as usize;
                    out.push_str(&self.source[backslash as usize..end]);
                }
            }
            c if is_line_break(c) => {
                // A line continuation contributes nothing to the value.
                if c == '\r' {
                    self.eat('\n');
                }
            }
            c => out.push(c),
        }
    }

    /// Append a decoded code point, pairing surrogates as JavaScript does.
    ///
    /// `"\uD83D\uDCA9"` is one astral character written as a UTF-16 pair, so a
    /// leading surrogate looks ahead for its trailing partner. A surrogate with no
    /// partner is legal JavaScript but unrepresentable in a Rust `String`; it
    /// becomes U+FFFD, and notably does **not** produce a diagnostic — TypeScript
    /// accepts it, and reporting one would be a false positive on real code.
    fn push_code_point(&mut self, cp: u32, out: &mut String) {
        const LEAD: std::ops::Range<u32> = 0xD800..0xDC00;
        const TRAIL: std::ops::Range<u32> = 0xDC00..0xE000;

        if LEAD.contains(&cp) {
            // Look for an immediately following `\uXXXX` trailing surrogate.
            let saved = self.pos;
            if self.peek() == Some('\\') && self.peek_at(1) == Some('u') {
                self.bump();
                if let Some(low) = self.scan_unicode_escape() {
                    if TRAIL.contains(&low) {
                        let combined = 0x1_0000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                        if let Some(ch) = char::from_u32(combined) {
                            out.push(ch);
                            return;
                        }
                    }
                }
                self.pos = saved;
            }
        }

        out.push(char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER));
    }

    /// Scan a template literal starting at a backtick.
    ///
    /// Produces `NoSubstitutionTemplateLiteral` when there is no `${`, otherwise
    /// `TemplateHead`; the parser drives the middle and tail via
    /// [`Scanner::rescan_template_continuation`].
    fn scan_template(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        self.bump(); // '`'
        // `s.scanTemplateAndSetTokenValue(false)` (`scanner.go:522`) — the
        // **initial** scan never reports an invalid escape. Upstream emits them
        // only from `ReScanTemplateToken(!isTaggedTemplate)`, because a tagged
        // template is allowed to contain them (the ES2018 revision: the cooked
        // value is `undefined` and the raw text is what the tag receives).
        // §223.
        self.scan_template_body(flags, true, false)
    }

    fn scan_template_body(
        &mut self,
        flags: &mut TokenFlags,
        is_head: bool,
        report_escapes: bool,
    ) -> SyntaxKind {
        let start = self.pos;
        let mut decoded: Option<String> = self.value.clone();

        loop {
            let Some(ch) = self.peek() else {
                *flags |= TokenFlags::UNTERMINATED;
                // `s.error` (`scanner.go:413`) reports at **`s.pos` with length
                // zero** — the position the scanner has reached, not the token's
                // start. `templateStringUnterminated1.ts` is a file containing
                // one backtick and upstream's caret is at column **2**.
                // Reporting from `token_start` put it at column 1 in every
                // unterminated-template case in the corpus: 12 cases, and TS1160
                // was the *only* difference in all twelve.
                self.error(&messages::UNTERMINATED_TEMPLATE_LITERAL, Span::new(self.pos, self.pos));
                self.value =
                    Some(decoded.unwrap_or_else(|| {
                        self.source[start as usize..self.pos as usize].to_string()
                    }));
                return if is_head {
                    SyntaxKind::NoSubstitutionTemplateLiteral
                } else {
                    SyntaxKind::TemplateTail
                };
            };

            if ch == '`' {
                let end = self.pos;
                self.bump();
                self.value = Some(
                    decoded
                        .unwrap_or_else(|| self.source[start as usize..end as usize].to_string()),
                );
                return if is_head {
                    SyntaxKind::NoSubstitutionTemplateLiteral
                } else {
                    SyntaxKind::TemplateTail
                };
            }

            if ch == '$' && self.peek_at(1) == Some('{') {
                let end = self.pos;
                self.bump();
                self.bump();
                self.value = Some(
                    decoded
                        .unwrap_or_else(|| self.source[start as usize..end as usize].to_string()),
                );
                return if is_head { SyntaxKind::TemplateHead } else { SyntaxKind::TemplateMiddle };
            }

            if ch == '\\' {
                let escape_start = self.pos;
                self.bump();
                let buffer = decoded.get_or_insert_with(|| {
                    self.source[start as usize..escape_start as usize].to_string()
                });
                self.scan_escape_into(buffer, flags, report_escapes);
                continue;
            }

            self.bump();
            if let Some(buffer) = decoded.as_mut() {
                buffer.push(ch);
            }
        }
    }

    /// Re-scan from a `}` as the continuation of a template literal.
    ///
    /// The parser calls this after consuming a substitution expression: `}` is
    /// otherwise a close-brace, and only grammatical context distinguishes them.
    /// `ReScanTemplateToken(isTaggedTemplate)` (`scanner.go:1052`) for the
    /// **head** of a template.
    ///
    /// The initial scan is silent (`scanner.go:522`), so every invalid-escape
    /// diagnostic in a template comes from here — and none does when the
    /// template is tagged, which is the ES2018 revision this port did not have.
    /// §223.
    pub fn rescan_template(&mut self, is_tagged: bool) -> Token {
        debug_assert!(matches!(
            self.token.kind,
            SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead
        ));
        self.report_template_escapes = !is_tagged;
        self.pos = self.token.span.start;
        let mut flags = TokenFlags::empty();
        self.bump(); // '`'
        let kind = self.scan_template_body(&mut flags, true, !is_tagged);
        self.token = Token::new(kind, Span::new(self.token.span.start, self.pos), flags);
        self.token
    }

    /// `ReScanTemplateToken` for a template **continuation** — the `}` that
    /// resumes the literal after a substitution.
    ///
    /// Whether invalid escapes are reported is carried on the scanner rather
    /// than passed, because this is called once per `}` from a site that does
    /// not know the tag. [`Scanner::rescan_template`] sets it. §223.
    pub fn rescan_template_continuation(&mut self) -> Token {
        debug_assert_eq!(self.token.kind, SyntaxKind::CloseBraceToken);
        self.pos = self.token.span.start + 1;
        self.token_start = self.token.span.start;
        let mut flags = TokenFlags::empty();
        let kind = self.scan_template_body(&mut flags, false, self.report_template_escapes);
        self.token = Token::new(kind, Span::new(self.token_start, self.pos), flags);
        self.token
    }

    /// Re-scan a `/` or `/=` token as a regular expression literal.
    ///
    /// Whether `/` begins a regex is not decidable lexically — `a / b` and
    /// `a(/b/)` differ only grammatically — so the parser asks for a re-scan when
    /// its context permits one.
    pub fn rescan_as_regular_expression(&mut self) -> Token {
        debug_assert!(matches!(
            self.token.kind,
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken
        ));
        self.token_start = self.token.span.start;
        // `ReScanSlashToken` (`scanner.go:1067`). The first pass walks
        // **bytes**, as upstream's `rune(s.text[p])` does: only `\n` and `\r`
        // end the body there, and an escape skips one byte.
        let bytes = self.source.as_bytes();
        let end = self.limit as usize;
        let start_of_body = self.token_start as usize + 1;
        let mut p = start_of_body;
        let mut in_escape = false;
        let mut in_class = false;
        let mut terminated = false;
        while p < end {
            let ch = bytes[p];
            if ch == b'\n' || ch == b'\r' {
                break;
            }
            if in_escape {
                in_escape = false;
            } else if ch == b'/' && !in_class {
                terminated = true;
                break;
            } else if ch == b'[' {
                in_class = true;
            } else if ch == b'\\' {
                in_escape = true;
            } else if ch == b']' {
                in_class = false;
            }
            p += 1;
        }

        if terminated {
            // Consume the slash; flags are identifier parts: /a/gi
            #[allow(clippy::cast_possible_truncation)]
            {
                self.pos = (p + 1) as u32;
            }
            while self.peek().is_some_and(is_identifier_part) {
                self.bump();
            }
        } else {
            // Recovery: the nearest unbalanced bracket outside a character
            // class ends the body, then trailing whitespace and `;` are
            // dropped; the token ends there too.
            let end_of_body = p;
            p = start_of_body;
            let mut in_escape = false;
            let mut class_depth = 0u32;
            let mut in_decimal_quantifier = false;
            let mut group_depth = 0u32;
            while p < end_of_body {
                let ch = bytes[p];
                if in_escape {
                    in_escape = false;
                } else if ch == b'\\' {
                    in_escape = true;
                } else if ch == b'[' {
                    class_depth += 1;
                } else if ch == b']' && class_depth != 0 {
                    class_depth -= 1;
                } else if class_depth == 0 {
                    if ch == b'{' {
                        in_decimal_quantifier = true;
                    } else if ch == b'}' && in_decimal_quantifier {
                        in_decimal_quantifier = false;
                    } else if !in_decimal_quantifier {
                        if ch == b'(' {
                            group_depth += 1;
                        } else if ch == b')' && group_depth != 0 {
                            group_depth -= 1;
                        } else if matches!(ch, b')' | b']' | b'}') {
                            break;
                        }
                    }
                }
                p += 1;
            }
            while p > start_of_body {
                let Some(ch) = self.source[..p].chars().next_back() else { break };
                if is_whitespace_single_line(ch) || is_line_break(ch) || ch == ';' {
                    p -= ch.len_utf8();
                } else {
                    break;
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            {
                self.pos = p as u32;
            }
            self.error(
                &messages::UNTERMINATED_REGULAR_EXPRESSION_LITERAL,
                Span::new(self.token_start, self.pos),
            );
        }

        self.token = Token::new(
            SyntaxKind::RegularExpressionLiteral,
            Span::new(self.token_start, self.pos),
            TokenFlags::empty(),
        );
        self.token
    }

    // ---- JSX -------------------------------------------------------------

    /// Scan the next token as JSX child content.
    ///
    /// JSX children obey different rules from expressions: everything up to the
    /// next `<` or `{` is literal text, so `&nbsp;`, backslashes, and quotes carry
    /// no special meaning. Only the parser knows when it is inside a JSX element,
    /// which is why this is a separate entry point rather than a scanner mode flag.
    ///
    /// Ported from typescript-go's `ScanJsxTokenEx`.
    pub fn scan_jsx_token(&mut self) -> Token {
        self.value = None;
        self.full_start = self.pos;
        self.token_start = self.pos;

        let Some(ch) = self.peek() else {
            self.token = Token::new(SyntaxKind::EndOfFile, Span::at(self.pos), TokenFlags::empty());
            return self.token;
        };

        let kind = match ch {
            '<' => {
                self.bump();
                if self.eat('/') {
                    SyntaxKind::LessThanSlashToken
                } else {
                    SyntaxKind::LessThanToken
                }
            }
            '{' => {
                self.bump();
                SyntaxKind::OpenBraceToken
            }
            _ => self.scan_jsx_text(),
        };

        self.token = Token::new(kind, Span::new(self.token_start, self.pos), TokenFlags::empty());
        self.token
    }

    /// Consume literal JSX text, up to the next `<` or `{`.
    ///
    /// Distinguishes `JsxText` from `JsxTextAllWhiteSpaces`: whitespace that
    /// begins with a line break is layout, not content, and is dropped from the
    /// emitted children. `<div>\n  </div>` has no text child; `<div>  </div>` does.
    fn scan_jsx_text(&mut self) -> SyntaxKind {
        // `None` until a non-whitespace character is seen; `Some(false)` once a
        // line break has been seen with only whitespace before it.
        let mut saw_content = false;
        let mut leading_line_break = false;

        while let Some(ch) = self.peek() {
            // `ScanJsxTokenEx` (`scanner.go:1281`): a `<` that opens a merge
            // conflict marker at a line start ends the text as one
            // `ConflictMarkerTrivia` token.
            if ch == '<' {
                let bytes = self.source.as_bytes();
                let i = self.pos as usize;
                if (i == 0 || bytes[i - 1] == b'\n' || bytes[i - 1] == b'\r')
                    && Self::is_conflict_marker(bytes, i, self.limit as usize)
                {
                    let end = self.scan_conflict_marker_trivia(i, b'<');
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        self.pos = end as u32;
                    }
                    return SyntaxKind::ConflictMarkerTrivia;
                }
            }
            if ch == '<' || ch == '{' {
                break;
            }
            // `scanJsxTokenEx` (`scanner.go`): a bare `>` or `}` in JSX text
            // is reported and kept as text.
            if ch == '>' {
                self.error(
                    &tsr_diagnostics::messages::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                    Span::new(self.pos, self.pos + 1),
                );
            } else if ch == '}' {
                self.error(
                    &tsr_diagnostics::messages::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                    Span::new(self.pos, self.pos + 1),
                );
            }
            if is_line_break(ch) && !saw_content {
                leading_line_break = true;
            } else if !is_whitespace_single_line(ch) && !is_line_break(ch) {
                saw_content = true;
            }
            self.bump();
        }

        if saw_content || !leading_line_break {
            SyntaxKind::JsxText
        } else {
            SyntaxKind::JsxTextAllWhiteSpaces
        }
    }

    /// Re-scan the current token as JSX child content.
    ///
    /// The parser reaches a child position holding a token scanned under
    /// expression rules; this rewinds to that token's start and rescans.
    pub fn rescan_jsx_token(&mut self) -> Token {
        self.pos = self.full_start;
        self.scan_jsx_token()
    }

    /// Extend the current identifier with JSX's extra characters.
    ///
    /// JSX names may contain `-`, which is not an identifier character anywhere
    /// else: `<my-element data-foo="1" />`. Upstream describes this as *mutating*
    /// the current token rather than producing a new one, and so does this.
    ///
    /// Ported from typescript-go's `ScanJsxIdentifier`.
    pub fn scan_jsx_identifier(&mut self) -> Token {
        if self.token.kind != SyntaxKind::Identifier && !self.token.kind.is_keyword() {
            return self.token;
        }

        let start = self.token.span.start;
        let mut extended = false;
        // Upstream reuses `scanIdentifierParts` here precisely "so unicode
        // escapes are handled" (`scanner.go:1338`): `data-\u0076ideo` is ONE
        // attribute named `data-video`, and stopping at the backslash split it
        // into two. The decoded value is rebuilt only when an escape appears —
        // or already appeared: upstream appends to the decoded `tokenValue`, so
        // `<\u0061-b>` is named `a-b` and matches `</a-b>`.
        let mut decoded: Option<String> = self.value.clone();
        while let Some(ch) = self.peek() {
            if ch == '-' || is_identifier_part(ch) {
                if let Some(value) = decoded.as_mut() {
                    value.push(ch);
                }
                self.bump();
                extended = true;
            } else if ch == '\\' && self.peek_at(1) == Some('u') {
                let escape_start = self.pos;
                self.bump();
                let Some(code_point) = self.scan_unicode_escape() else {
                    self.pos = escape_start;
                    break;
                };
                let Some(character) = char::from_u32(code_point) else {
                    self.pos = escape_start;
                    break;
                };
                if decoded.is_none() {
                    decoded = Some(
                        self.source[self.token.span.start as usize..escape_start as usize]
                            .to_string(),
                    );
                }
                decoded.as_mut().expect("just seeded").push(character);
                extended = true;
            } else {
                break;
            }
        }

        if extended {
            // `scanIdentifierParts` records an escape in the token flags, which
            // `parseIdentifierNameErrorOnUnicodeEscapeSequence` reads: `<a-\u0063>`
            // reports TS17021 like `<\u0061>` does.
            let mut flags = self.token.flags;
            if decoded.is_some() {
                flags |= TokenFlags::UNICODE_ESCAPE;
            }
            self.value = decoded;
            self.token = Token::new(SyntaxKind::Identifier, Span::new(start, self.pos), flags);
        }
        self.token
    }

    /// Scan a JSX attribute value.
    ///
    /// A quoted attribute value is a raw string: `class="a\b"` contains a
    /// backslash, not an escape. Anything else falls back to ordinary scanning so
    /// `{expr}` works.
    ///
    /// Ported from typescript-go's `ScanJsxAttributeValue`.
    pub fn scan_jsx_attribute_value(&mut self) -> Token {
        self.value = None;
        self.full_start = self.pos;

        while self.peek().is_some_and(|c| is_whitespace_single_line(c) || is_line_break(c)) {
            self.bump();
        }
        self.token_start = self.pos;

        match self.peek() {
            Some(quote @ ('"' | '\'')) => {
                self.bump();
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if ch == quote {
                        break;
                    }
                    self.bump();
                }
                let end = self.pos;
                let unterminated = self.peek().is_none();
                if !unterminated {
                    self.bump();
                }
                self.value = Some(self.source[start as usize..end as usize].to_string());
                let mut flags = TokenFlags::empty();
                if unterminated {
                    flags |= TokenFlags::UNTERMINATED;
                    // `scanString` reports with `s.error`: zero-width at the
                    // end of input, not over the token.
                    self.error(&messages::UNTERMINATED_STRING_LITERAL, Span::at(self.pos));
                }
                self.token = Token::new(
                    SyntaxKind::StringLiteral,
                    Span::new(self.token_start, self.pos),
                    flags,
                );
                self.token
            }
            // Not a quoted value: `{expr}` and error recovery both want ordinary
            // scanning from here.
            _ => self.scan(),
        }
    }

    /// Re-scan a compound `<` token as a single `<`.
    ///
    /// `<<T>() => void>x` lexes the opening as a shift operator; JSX and type
    /// assertions both need it split.
    pub fn rescan_less_than(&mut self) -> Token {
        if self.token.kind == SyntaxKind::LessThanLessThanToken {
            self.pos = self.token.span.start + 1;
            self.token = Token::new(
                SyntaxKind::LessThanToken,
                Span::new(self.token.span.start, self.pos),
                self.token.flags,
            );
        }
        self.token
    }

    /// Re-scan a compound `>` token as a single `>`.
    ///
    /// `List<List<T>>` lexes the trailing `>>` as a shift operator; the parser
    /// splits it when closing type arguments.
    pub fn rescan_greater_than(&mut self) -> Token {
        if matches!(
            self.token.kind,
            SyntaxKind::GreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
        ) {
            self.pos = self.token.span.start + 1;
            self.token = Token::new(
                SyntaxKind::GreaterThanToken,
                Span::new(self.token.span.start, self.pos),
                self.token.flags,
            );
        }
        self.token
    }

    /// `scanConflictMarkerTrivia` (`scanner.go:2444`) from the marker at `i`
    /// (its byte `b`): TS1185 over the seven marker characters, then `<`/`>`
    /// skip their line and `=`/`|` skip to the next `=======` or `>>>>>>>`
    /// marker at a line start. Returns the position after the trivia.
    fn scan_conflict_marker_trivia(&mut self, mut i: usize, b: u8) -> usize {
        let bytes = self.source.as_bytes();
        let limit = self.limit as usize;
        #[allow(clippy::cast_possible_truncation)]
        self.error(&messages::MERGE_CONFLICT_MARKER_ENCOUNTERED, Span::new(i as u32, i as u32 + 7));
        if b == b'<' || b == b'>' {
            while i < limit && bytes[i] != b'\n' && bytes[i] != b'\r' {
                i += 1;
            }
        } else {
            i += 7;
            while i < limit {
                let current = bytes[i];
                if (current == b'=' || current == b'>')
                    && current != b
                    && (i == 0 || bytes[i - 1] == b'\n' || bytes[i - 1] == b'\r')
                    && Self::is_conflict_marker(bytes, i, limit)
                {
                    break;
                }
                i += 1;
            }
        }
        i
    }

    /// §301: `isConflictMarkerTrivia` (`scanner.go:2409`) — seven identical
    /// marker bytes; `=` needs nothing after, the others a following space.
    /// Line-start is the CALLER's test (the trivia loop knows it crossed a
    /// newline).
    fn is_conflict_marker(bytes: &[u8], pos: usize, limit: usize) -> bool {
        if pos + 7 > limit {
            return false;
        }
        let ch = bytes[pos];
        if bytes[pos..pos + 7].iter().any(|&b| b != ch) {
            return false;
        }
        ch == b'=' || (pos + 7 < limit && bytes[pos + 7] == b' ')
    }

    // ---- punctuation ----------------------------------------------------

    /// Maximal-munch punctuation scanning.
    ///
    /// Ordering within each arm matters: the longest operator must be tested
    /// first, or `>>>=` lexes as `>>` followed by `>=`.
    fn scan_punctuation(&mut self) -> SyntaxKind {
        let start = self.pos;
        let ch = self.bump().expect("caller checked");

        match ch {
            '{' => SyntaxKind::OpenBraceToken,
            '}' => SyntaxKind::CloseBraceToken,
            '(' => SyntaxKind::OpenParenToken,
            ')' => SyntaxKind::CloseParenToken,
            '[' => SyntaxKind::OpenBracketToken,
            ']' => SyntaxKind::CloseBracketToken,
            ';' => SyntaxKind::SemicolonToken,
            ',' => SyntaxKind::CommaToken,
            '~' => SyntaxKind::TildeToken,
            '@' => SyntaxKind::AtToken,
            '#' => SyntaxKind::HashToken,
            ':' => SyntaxKind::ColonToken,

            '.' => {
                if self.peek() == Some('.') && self.peek_at(1) == Some('.') {
                    self.bump();
                    self.bump();
                    SyntaxKind::DotDotDotToken
                } else {
                    SyntaxKind::DotToken
                }
            }

            '?' => {
                if self.eat('?') {
                    if self.eat('=') {
                        SyntaxKind::QuestionQuestionEqualsToken
                    } else {
                        SyntaxKind::QuestionQuestionToken
                    }
                } else if self.peek() == Some('.')
                    // `a?.5:b` is a conditional, not optional chaining.
                    && !self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
                {
                    self.bump();
                    SyntaxKind::QuestionDotToken
                } else {
                    SyntaxKind::QuestionToken
                }
            }

            '+' => {
                if self.eat('+') {
                    SyntaxKind::PlusPlusToken
                } else if self.eat('=') {
                    SyntaxKind::PlusEqualsToken
                } else {
                    SyntaxKind::PlusToken
                }
            }
            '-' => {
                if self.eat('-') {
                    SyntaxKind::MinusMinusToken
                } else if self.eat('=') {
                    SyntaxKind::MinusEqualsToken
                } else {
                    SyntaxKind::MinusToken
                }
            }
            '*' => {
                if self.eat('*') {
                    if self.eat('=') {
                        SyntaxKind::AsteriskAsteriskEqualsToken
                    } else {
                        SyntaxKind::AsteriskAsteriskToken
                    }
                } else if self.eat('=') {
                    SyntaxKind::AsteriskEqualsToken
                } else {
                    SyntaxKind::AsteriskToken
                }
            }
            '/' => {
                if self.eat('=') {
                    SyntaxKind::SlashEqualsToken
                } else {
                    SyntaxKind::SlashToken
                }
            }
            '%' => {
                if self.eat('=') {
                    SyntaxKind::PercentEqualsToken
                } else {
                    SyntaxKind::PercentToken
                }
            }
            '^' => {
                if self.eat('=') {
                    SyntaxKind::CaretEqualsToken
                } else {
                    SyntaxKind::CaretToken
                }
            }
            '!' => {
                if self.eat('=') {
                    if self.eat('=') {
                        SyntaxKind::ExclamationEqualsEqualsToken
                    } else {
                        SyntaxKind::ExclamationEqualsToken
                    }
                } else {
                    SyntaxKind::ExclamationToken
                }
            }
            '=' => {
                if self.eat('=') {
                    if self.eat('=') {
                        SyntaxKind::EqualsEqualsEqualsToken
                    } else {
                        SyntaxKind::EqualsEqualsToken
                    }
                } else if self.eat('>') {
                    SyntaxKind::EqualsGreaterThanToken
                } else {
                    SyntaxKind::EqualsToken
                }
            }
            '&' => {
                if self.eat('&') {
                    if self.eat('=') {
                        SyntaxKind::AmpersandAmpersandEqualsToken
                    } else {
                        SyntaxKind::AmpersandAmpersandToken
                    }
                } else if self.eat('=') {
                    SyntaxKind::AmpersandEqualsToken
                } else {
                    SyntaxKind::AmpersandToken
                }
            }
            '|' => {
                if self.eat('|') {
                    if self.eat('=') {
                        SyntaxKind::BarBarEqualsToken
                    } else {
                        SyntaxKind::BarBarToken
                    }
                } else if self.eat('=') {
                    SyntaxKind::BarEqualsToken
                } else {
                    SyntaxKind::BarToken
                }
            }
            '<' => {
                if self.eat('<') {
                    if self.eat('=') {
                        SyntaxKind::LessThanLessThanEqualsToken
                    } else {
                        SyntaxKind::LessThanLessThanToken
                    }
                } else if self.eat('=') {
                    SyntaxKind::LessThanEqualsToken
                } else if self.jsx && self.peek() == Some('/') && self.peek_at(1) != Some('*') {
                    // `scanner.go:778`: in JSX, `</` (but not `</*`) is one token.
                    self.bump();
                    SyntaxKind::LessThanSlashToken
                } else {
                    SyntaxKind::LessThanToken
                }
            }
            // `>` is scanned at its shortest useful length here and split further
            // by `rescan_greater_than`; scanning the full `>>>=` keeps arithmetic
            // correct, and type-argument lists ask for a re-scan.
            '>' => {
                if self.eat('>') {
                    if self.eat('>') {
                        if self.eat('=') {
                            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                        } else {
                            SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                        }
                    } else if self.eat('=') {
                        SyntaxKind::GreaterThanGreaterThanEqualsToken
                    } else {
                        SyntaxKind::GreaterThanGreaterThanToken
                    }
                } else if self.eat('=') {
                    SyntaxKind::GreaterThanEqualsToken
                } else {
                    SyntaxKind::GreaterThanToken
                }
            }

            _ => {
                self.error(&messages::INVALID_CHARACTER, Span::new(start, self.pos));
                SyntaxKind::Unknown
            }
        }
    }
}

/// Scan `source` to completion, returning every token and any diagnostics.
///
/// Convenience for tests and tooling; the parser drives [`Scanner`] directly
/// because it needs re-scanning.
#[must_use]
pub fn tokenize(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut scanner = Scanner::new(source);
    let mut tokens = Vec::new();
    loop {
        let token = scanner.scan();
        let done = token.kind == SyntaxKind::EndOfFile;
        tokens.push(token);
        if done {
            break;
        }
    }
    let diagnostics = scanner.take_diagnostics();
    (tokens, diagnostics)
}

/// Whether a JSDoc comment mentions any of `tags` as `@tag`.
///
/// Deliberately approximate — it does not know about backticks, nesting, or code
/// fences, so it over-reports. That is the safe direction: the flag only causes
/// the comment to be parsed, and the parse settles the question.
fn mentions_tag(text: &str, tags: &[&str]) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(offset) = text[i..].find('@') {
        let after = i + offset + 1;
        for tag in tags {
            if text[after..].starts_with(tag) {
                let terminator = bytes.get(after + tag.len());
                // A tag name ends at whitespace, `}` (from `{@link x}`), `*`, or
                // the end of the comment. Without this check `@seeder` matches
                // `@see`.
                // `scanner.hasJSDocTag` at 5b1047d: this is an explicit byte
                // set, not ASCII whitespace (which would also admit form feed).
                if terminator
                    .is_none_or(|&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'}' | b'*'))
                {
                    return true;
                }
            }
        }
        i = after;
    }
    false
}
