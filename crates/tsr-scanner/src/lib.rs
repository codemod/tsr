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

mod generated;
mod token;

pub use token::{Token, TokenFlags};

use tsr_ast::SyntaxKind;
use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};

use generated::unicode;

/// Whether a code point may begin an identifier.
///
/// `$` and `_` are permitted by ECMAScript in addition to `ID_Start`.
#[must_use]
pub fn is_identifier_start(cp: char) -> bool {
    let c = cp as u32;
    if c < 128 {
        return cp.is_ascii_alphabetic() || cp == '$' || cp == '_';
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
        return cp.is_ascii_alphanumeric() || cp == '$' || cp == '_';
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

/// Look up a reserved word or contextual keyword.
///
/// The table is derived from [`SyntaxKind`] itself rather than duplicated: every
/// keyword kind is named `<Word>Keyword`, and the source text is its lowercase
/// form (`KeyOfKeyword` → `keyof`, `InstanceOfKeyword` → `instanceof`). That the
/// derivation matches upstream's hand-written map exactly is asserted in
/// `tests/keyword_conformance.rs` rather than assumed.
#[must_use]
pub fn keyword_kind(text: &str) -> Option<SyntaxKind> {
    // Cheap rejection: no keyword is shorter than 2 or longer than 11 chars, and
    // all are pure lowercase ASCII.
    if text.len() < 2 || text.len() > 11 || !text.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    let first = SyntaxKind::FIRST_KEYWORD as u16;
    let last = SyntaxKind::LAST_KEYWORD as u16;
    (first..=last).filter_map(SyntaxKind::from_u16).find(|kind| {
        kind.name().strip_suffix("Keyword").is_some_and(|word| word.eq_ignore_ascii_case(text))
    })
}

/// A saved scanner position, produced by [`Scanner::save`].
///
/// Deliberately opaque and `Copy`: it is a bookmark, not a snapshot of the source.
#[derive(Debug, Clone, Copy)]
pub struct ScannerState {
    pos: u32,
    token_start: u32,
    full_start: u32,
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
            token: self.token,
            diagnostic_count: self.diagnostics.len(),
        }
    }

    /// Rewind to a captured position, discarding diagnostics emitted since.
    pub fn restore(&mut self, state: ScannerState) {
        self.pos = state.pos;
        self.token_start = state.token_start;
        self.full_start = state.full_start;
        self.token = state.token;
        self.diagnostics.truncate(state.diagnostic_count);
        // The decoded value belongs to the token we just discarded.
        self.value = None;
    }

    // ---- character access ----------------------------------------------

    fn rest(&self) -> &'a str {
        &self.source[self.pos as usize..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.rest().chars().nth(offset)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        // `len_utf8` is 1..=4, so the cast cannot truncate.
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
        while let Some(ch) = self.peek() {
            if is_line_break(ch) {
                flags |= TokenFlags::PRECEDING_LINE_BREAK;
                self.bump();
                // Treat CRLF as one break.
                if ch == '\r' {
                    self.eat('\n');
                }
                continue;
            }
            if is_whitespace_single_line(ch) {
                self.bump();
                continue;
            }
            if ch == '/' {
                match self.peek_at(1) {
                    Some('/') => {
                        self.skip_line_comment();
                        continue;
                    }
                    Some('*') => {
                        if self.skip_block_comment() {
                            flags |= TokenFlags::PRECEDING_LINE_BREAK;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            break;
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

        match ch {
            '0'..='9' => self.scan_number(flags),
            '"' | '\'' => self.scan_string(flags),
            '`' => self.scan_template(flags),
            // `#x` is a private identifier: one token, not `#` then `x`.
            '#' if self.peek_at(1).is_some_and(|c| is_identifier_start(c) || c == '\\') => {
                self.bump();
                self.scan_identifier_or_keyword(flags);
                SyntaxKind::PrivateIdentifier
            }
            _ if is_identifier_start(ch) || ch == '\\' => self.scan_identifier_or_keyword(flags),
            _ => self.scan_punctuation(),
        }
    }

    // ---- identifiers ----------------------------------------------------

    fn scan_identifier_or_keyword(&mut self, flags: &mut TokenFlags) -> SyntaxKind {
        let start = self.pos;
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
