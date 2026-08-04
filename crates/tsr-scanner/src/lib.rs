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
            diagnostics: Vec::new(),
        }
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
        self.diagnostics.truncate(state.diagnostic_count);
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
    fn skip_block_comment(&mut self) -> bool {
        let start = self.pos;
        self.bump(); // '/'
        self.bump(); // '*'
        let mut crossed_line = false;
        loop {
            let Some(ch) = self.bump() else {
                // Unterminated: report at the opening delimiter, which is where a
                // reader needs to look.
                self.error(&messages::ASTERISK_SLASH_EXPECTED, Span::new(start, self.pos));
                return crossed_line;
            };
            if is_line_break(ch) {
                crossed_line = true;
            }
            if ch == '*' && self.peek() == Some('/') {
                self.bump();
                return crossed_line;
            }
        }
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
            b'"' | b'\'' => self.scan_string(flags),
            b'`' => self.scan_template(flags),
            // `#x` is a private identifier: one token, not `#` then `x`.
            b'#' if self.peek_at(1).is_some_and(|c| is_identifier_start(c) || c == '\\') => {
                self.bump();
                self.scan_identifier_or_keyword(flags);
                SyntaxKind::PrivateIdentifier
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

        let mut decoded: Option<String> = None;

        while let Some(ch) = self.peek() {
            if ch == '\\' {
                // A unicode escape inside an identifier: `abc` is `abc`.
                let escape_start = self.pos;
                self.bump();
                if let Some(decoded_char) = self.scan_unicode_escape().and_then(char::from_u32) {
                    {
                        let is_valid = if self.pos == start + 1 {
                            is_identifier_start(decoded_char)
                        } else {
                            is_identifier_part(decoded_char)
                        };
                        if !is_valid {
                            self.error(
                                &messages::INVALID_CHARACTER,
                                Span::new(escape_start, self.pos),
                            );
                        }
                        let buffer = decoded.get_or_insert_with(|| {
                            self.source[start as usize..escape_start as usize].to_string()
                        });
                        buffer.push(decoded_char);
                        *flags |= TokenFlags::UNICODE_ESCAPE;
                    }
                } else {
                    self.error(
                        &messages::HEXADECIMAL_DIGIT_EXPECTED,
                        Span::new(escape_start, self.pos),
                    );
                    break;
                }
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

        let text = &self.source[start as usize..self.pos as usize];

        // An identifier written with escapes is never a keyword: `if` is an
        // identifier named `if`, not the `if` keyword.
        if decoded.is_none() {
            if let Some(kind) = keyword_kind(text) {
                return kind;
            }
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
        if !self.eat('u') {
            return None;
        }
        if self.eat('{') {
            let start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.bump();
            }
            let digits = &self.source[start as usize..self.pos as usize];
            if !self.eat('}') || digits.is_empty() {
                return None;
            }
            // Values above 0x10FFFF are out of range; upstream reports them
            // separately, and returning None here reports "digit expected", which
            // is close enough until the parser distinguishes the two.
            return u32::from_str_radix(digits, 16).ok().filter(|&v| v <= 0x10_FFFF);
        }
        let start = self.pos;
        for _ in 0..4 {
            if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            self.bump();
        }
        let digits = &self.source[start as usize..self.pos as usize];
        u32::from_str_radix(digits, 16).ok()
    }

    // ---- numbers --------------------------------------------------------

    fn scan_number(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        let start = self.pos;

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
                    self.error(&messages::HEXADECIMAL_DIGIT_EXPECTED, Span::new(start, self.pos));
                }
                if self.eat('n') {
                    return SyntaxKind::BigIntLiteral;
                }
                return SyntaxKind::NumericLiteral;
            }
        }

        self.scan_digits(10, flags);

        if self.eat('n') {
            return SyntaxKind::BigIntLiteral;
        }

        // Fractional part. `1.` is legal; `1.5` more so.
        if self.peek() == Some('.') {
            self.bump();
            self.scan_digits(10, flags);
        }

        // Exponent.
        if matches!(self.peek(), Some('e' | 'E')) {
            let exponent_start = self.pos;
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if self.scan_digits(10, flags) == 0 {
                self.error(&messages::DIGIT_EXPECTED, Span::new(exponent_start, self.pos));
            } else {
                *flags |= TokenFlags::SCIENTIFIC;
            }
        }

        SyntaxKind::NumericLiteral
    }

    /// Consume digits of the given radix, allowing `_` separators.
    ///
    /// Returns how many digits were consumed, so callers can report an empty
    /// literal like `0x`.
    fn scan_digits(&mut self, radix: u32, flags: &mut TokenFlags) -> usize {
        let mut count = 0;
        let mut last_was_separator = false;
        while let Some(ch) = self.peek() {
            if ch == '_' {
                if count == 0 || last_was_separator {
                    self.error(
                        &messages::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                        Span::new(self.pos, self.pos + 1),
                    );
                }
                *flags |= TokenFlags::CONTAINS_SEPARATOR;
                last_was_separator = true;
                self.bump();
                continue;
            }
            if !ch.is_digit(radix) {
                break;
            }
            last_was_separator = false;
            count += 1;
            self.bump();
        }
        if last_was_separator {
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
        let start = self.pos;
        let mut decoded: Option<String> = None;

        loop {
            let Some(ch) = self.peek() else {
                *flags |= TokenFlags::UNTERMINATED;
                self.error(
                    &messages::UNTERMINATED_STRING_LITERAL,
                    Span::new(self.token_start, self.pos),
                );
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
                self.error(
                    &messages::UNTERMINATED_STRING_LITERAL,
                    Span::new(self.token_start, self.pos),
                );
                break;
            }
            if ch == '\\' {
                let escape_start = self.pos;
                self.bump();
                let buffer = decoded.get_or_insert_with(|| {
                    self.source[start as usize..escape_start as usize].to_string()
                });
                self.scan_escape_into(buffer);
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
    fn scan_escape_into(&mut self, out: &mut String) {
        let Some(ch) = self.bump() else { return };
        match ch {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{C}'),
            'v' => out.push('\u{B}'),
            '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => out.push('\0'),
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
                    _ => self
                        .error(&messages::HEXADECIMAL_DIGIT_EXPECTED, Span::new(start, self.pos)),
                }
            }
            'u' => {
                // Back up so the shared escape scanner sees the `u`.
                self.pos -= 1;
                let start = self.pos;
                match self.scan_unicode_escape() {
                    Some(cp) => self.push_code_point(cp, out),
                    None => self
                        .error(&messages::HEXADECIMAL_DIGIT_EXPECTED, Span::new(start, self.pos)),
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
        self.scan_template_body(flags, true)
    }

    fn scan_template_body(&mut self, flags: &mut TokenFlags, is_head: bool) -> SyntaxKind {
        let start = self.pos;
        let mut decoded: Option<String> = None;

        loop {
            let Some(ch) = self.peek() else {
                *flags |= TokenFlags::UNTERMINATED;
                self.error(
                    &messages::UNTERMINATED_TEMPLATE_LITERAL,
                    Span::new(self.token_start, self.pos),
                );
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
                self.scan_escape_into(buffer);
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
    pub fn rescan_template_continuation(&mut self) -> Token {
        debug_assert_eq!(self.token.kind, SyntaxKind::CloseBraceToken);
        self.pos = self.token.span.start + 1;
        self.token_start = self.token.span.start;
        let mut flags = TokenFlags::empty();
        let kind = self.scan_template_body(&mut flags, false);
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
        self.pos = self.token.span.start;
        self.token_start = self.pos;
        self.bump(); // '/'

        let mut in_class = false;
        let mut terminated = false;
        while let Some(ch) = self.peek() {
            if is_line_break(ch) {
                break;
            }
            match ch {
                '\\' => {
                    self.bump();
                    self.bump();
                    continue;
                }
                '[' => in_class = true,
                ']' => in_class = false,
                '/' if !in_class => {
                    self.bump();
                    terminated = true;
                    break;
                }
                _ => {}
            }
            self.bump();
        }

        if terminated {
            // Flags are identifier parts: /a/gi
            while self.peek().is_some_and(is_identifier_part) {
                self.bump();
            }
        } else {
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
            if ch == '<' || ch == '{' {
                break;
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
        while let Some(ch) = self.peek() {
            if ch == '-' || is_identifier_part(ch) {
                self.bump();
                extended = true;
            } else {
                break;
            }
        }

        if extended {
            // The decoded value is the raw text: a JSX name with a dash cannot
            // also contain escapes worth decoding separately.
            self.value = None;
            self.token =
                Token::new(SyntaxKind::Identifier, Span::new(start, self.pos), self.token.flags);
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
                    self.error(
                        &messages::UNTERMINATED_STRING_LITERAL,
                        Span::new(self.token_start, self.pos),
                    );
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

    /// Re-scan the current token as a JSX attribute value.
    ///
    /// The parser holds one token of lookahead, so by the time it has consumed
    /// `=` the value has already been scanned under expression rules — where
    /// `"a\b"` reports a bad escape. This rewinds to that token's start and
    /// rescans it as a raw JSX string.
    ///
    /// Ported from typescript-go's `ReScanJsxAttributeValue`.
    pub fn rescan_jsx_attribute_value(&mut self) -> Token {
        self.pos = self.full_start;
        // Diagnostics from the discarded expression-rules scan would be
        // misattributed; drop anything reported at or after the rewind point.
        let from = self.full_start;
        self.diagnostics.retain(|d| d.span.start < from);
        self.scan_jsx_attribute_value()
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
                } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    // `.5` is a numeric literal, not a dot followed by 5.
                    self.pos = start;
                    let mut flags = TokenFlags::empty();
                    self.bump(); // '.'
                    self.scan_digits(10, &mut flags);
                    if matches!(self.peek(), Some('e' | 'E')) {
                        self.bump();
                        if matches!(self.peek(), Some('+' | '-')) {
                            self.bump();
                        }
                        self.scan_digits(10, &mut flags);
                    }
                    SyntaxKind::NumericLiteral
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
                if terminator.is_none_or(|&b| b.is_ascii_whitespace() || b == b'}' || b == b'*') {
                    return true;
                }
            }
        }
        i = after;
    }
    false
}
