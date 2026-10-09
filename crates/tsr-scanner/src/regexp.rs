//! The regular-expression validator: `ReScanSlashToken(true)`'s flag pass and
//! `regExpParser` (`internal/scanner/regexp.go`), at the pinned commit.
//!
//! Native validates a regular expression literal **from the checker**, not
//! from the parser: `checkGrammarRegularExpressionLiteral`
//! (`checker/grammarchecks.go:68`) re-scans the literal with a scanner of its
//! own and `ReScanSlashToken(true)` (`scanner.go:1067`), whose `true` turns on
//! the flag checks and runs `regExpParser.run` over the body. The parser's
//! rescan (`parser.go:2998`) passes no argument, so a parse reports only an
//! unterminated literal. [`scan_regular_expression_errors`] is that checker
//! re-scan: the caller decides when to ask (once per literal, in a file with
//! no parse diagnostics) and folds the errors the way the checker's callback
//! does. Nothing here runs while parsing, so the parse hot path is unchanged.
//!
//! # Why a scanner of its own
//!
//! `regExpParser` drives native's `Scanner` directly: it moves `s.pos`, narrows
//! `s.end` to the body, reads `s.char()` (one **byte**, `-1` past `end`), and
//! calls `scanEscapeSequence`, `scanUnicodeEscape`, `scanHexDigits` and
//! `scanIdentifier`. Every column it reports depends on those byte-level
//! moves. [`Scanner`](crate::Scanner) is shaped for tokenising (char-level
//! cursor, decoded values, a diagnostic list the parser rewinds), so this
//! module carries the handful of native scanner routines the validator needs,
//! transliterated over bytes, rather than bending the token scanner to them.
//! `scanEscapeSequence` here is the regular-expression use of that function
//! only: `Scanner::scan_escape_into` stays the string/template use.
//!
//! Native's routines return Go strings (character values that may be lone
//! surrogates or raw bytes). The validator reads those values only to ask
//! whether one is empty and, for a range `a-b`, whether each side is exactly
//! one code point and how they order. [`CharValue`] keeps exactly that.

use tsr_core::options::ScriptTarget;
use tsr_core::spelling::get_spelling_suggestion;
use tsr_diagnostics::Message;
use tsr_diagnostics::messages;

use crate::unicode_properties::{
    BINARY_UNICODE_PROPERTIES, BINARY_UNICODE_PROPERTIES_OF_STRINGS, GENERAL_CATEGORY_VALUES,
    NON_BINARY_UNICODE_PROPERTIES, SCRIPT_VALUES,
};
use crate::{is_identifier_part, is_identifier_start};

/// One error the validator reports, in report order: native's
/// `errorAt(message, start, length, args...)` callback arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegExpError {
    /// The diagnostic message.
    pub message: &'static Message,
    /// Byte offset of the span.
    pub start: u32,
    /// Byte length of the span.
    pub length: u32,
    /// Substitution arguments.
    pub args: Vec<String>,
}

/// `regularExpressionFlags` (`regexp.go:17`).
mod flags {
    pub(super) const HAS_INDICES: u32 = 1 << 0; // d
    pub(super) const GLOBAL: u32 = 1 << 1; // g
    pub(super) const IGNORE_CASE: u32 = 1 << 2; // i
    pub(super) const MULTILINE: u32 = 1 << 3; // m
    pub(super) const DOT_ALL: u32 = 1 << 4; // s
    pub(super) const UNICODE: u32 = 1 << 5; // u
    pub(super) const UNICODE_SETS: u32 = 1 << 6; // v
    pub(super) const STICKY: u32 = 1 << 7; // y
    pub(super) const ANY_UNICODE_MODE: u32 = UNICODE | UNICODE_SETS;
    pub(super) const MODIFIERS: u32 = IGNORE_CASE | MULTILINE | DOT_ALL;
}

/// `charCodeToRegExpFlag` (`regexp.go:33`).
fn char_code_to_regexp_flag(ch: i32) -> Option<u32> {
    Some(match u8::try_from(ch).ok()? {
        b'd' => flags::HAS_INDICES,
        b'g' => flags::GLOBAL,
        b'i' => flags::IGNORE_CASE,
        b'm' => flags::MULTILINE,
        b's' => flags::DOT_ALL,
        b'u' => flags::UNICODE,
        b'v' => flags::UNICODE_SETS,
        b'y' => flags::STICKY,
        _ => return None,
    })
}

/// `regExpFlagToFirstAvailableLanguageVersion` (`regexp.go:44`), with the
/// target's lower-cased name the message prints.
fn first_available_language_version(flag: u32) -> Option<(ScriptTarget, &'static str)> {
    match flag {
        flags::HAS_INDICES => Some((ScriptTarget::ES2022, "es2022")),
        flags::DOT_ALL => Some((ScriptTarget::ES2018, "es2018")),
        flags::UNICODE_SETS => Some((ScriptTarget::ES2024, "es2024")),
        _ => None,
    }
}

/// `EscapeSequenceScanningFlags` (`scanner.go:21`), the bits the regular
/// expression callers pass.
mod escape {
    pub(super) const STRING: u32 = 1 << 0;
    pub(super) const REPORT_ERRORS: u32 = 1 << 1;
    pub(super) const REGULAR_EXPRESSION: u32 = 1 << 2;
    pub(super) const ANNEX_B: u32 = 1 << 3;
    pub(super) const ANY_UNICODE_MODE: u32 = 1 << 4;
    pub(super) const ATOM_ESCAPE: u32 = 1 << 5;
    pub(super) const REPORT_INVALID_ESCAPE_ERRORS: u32 = REGULAR_EXPRESSION | REPORT_ERRORS;
    pub(super) const ALLOW_EXTENDED_UNICODE_ESCAPE: u32 = STRING | ANY_UNICODE_MODE;
}

/// What the validator reads of a native character-value string: `""`, exactly
/// one code point (`len(s) == size` after `DecodeJSStringRune`, a lone
/// surrogate's sentinel included), or anything longer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharValue {
    Empty,
    One(u32),
    Many,
}

impl CharValue {
    /// `string(rune(ch))` for a rune native holds as `int32`: a negative
    /// value or a non-scalar is `"�"`, one code point either way.
    fn rune(ch: i32) -> Self {
        Self::One(u32::try_from(ch).unwrap_or(0xFFFD))
    }
}

/// `classSetExpressionType` (`regexp.go:56`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassSetExpressionType {
    ClassIntersection,
    ClassSubtraction,
}

/// Go's `utf8.DecodeRuneInString` over bytes: `(RuneError, 0)` for an empty
/// slice, `(RuneError, 1)` for an invalid sequence, and the scalar and its
/// length otherwise (a literal U+FFFD decodes as `RuneError` with size 3, as
/// in Go).
fn decode_rune(bytes: &[u8]) -> (i32, usize) {
    const RUNE_ERROR: i32 = 0xFFFD;
    let Some(&first) = bytes.first() else {
        return (RUNE_ERROR, 0);
    };
    if first < 0x80 {
        return (i32::from(first), 1);
    }
    let width = match first {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return (RUNE_ERROR, 1),
    };
    match bytes.get(..width).and_then(|slice| std::str::from_utf8(slice).ok()) {
        Some(text) => {
            let ch = text.chars().next().expect("non-empty");
            (i32::try_from(u32::from(ch)).expect("scalar"), width)
        }
        None => (RUNE_ERROR, 1),
    }
}

/// `IsIdentifierStart` over a native `rune` (`-1` is none).
fn rune_is_identifier_start(ch: i32) -> bool {
    u32::try_from(ch).ok().and_then(char::from_u32).is_some_and(is_identifier_start)
}

/// `IsIdentifierPart` over a native `rune`.
fn rune_is_identifier_part(ch: i32) -> bool {
    u32::try_from(ch).ok().and_then(char::from_u32).is_some_and(is_identifier_part)
}

/// `string(ch)` of a native rune, for a message argument.
fn rune_string(ch: i32) -> String {
    u32::try_from(ch).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}').to_string()
}

fn is_digit(ch: i32) -> bool {
    (i32::from(b'0')..=i32::from(b'9')).contains(&ch)
}

fn is_octal_digit(ch: i32) -> bool {
    (i32::from(b'0')..=i32::from(b'7')).contains(&ch)
}

fn is_hex_digit(ch: i32) -> bool {
    u8::try_from(ch).is_ok_and(|b| b.is_ascii_hexdigit())
}

fn is_ascii_letter(ch: i32) -> bool {
    u8::try_from(ch).is_ok_and(|b| b.is_ascii_alphabetic())
}

/// `isWordCharacter` (`scanner.go:2237`).
fn is_word_character(ch: i32) -> bool {
    is_ascii_letter(ch) || is_digit(ch) || ch == i32::from(b'_')
}

fn is_high_surrogate(cp: u32) -> bool {
    (0xD800..0xDC00).contains(&cp)
}

fn is_low_surrogate(cp: u32) -> bool {
    (0xDC00..0xE000).contains(&cp)
}

fn surrogate_pair_to_code_point(high: u32, low: u32) -> u32 {
    0x1_0000 + ((high - 0xD800) << 10) + (low - 0xDC00)
}

/// `compareDecimalStrings` (`regexp.go:129`).
fn compare_decimal_strings(a: &str, b: &str) -> std::cmp::Ordering {
    let a = match a.trim_start_matches('0') {
        "" => "0",
        rest => rest,
    };
    let b = match b.trim_start_matches('0') {
        "" => "0",
        rest => rest,
    };
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// The errors `ReScanSlashToken(true)` reports for the regular expression
/// literal whose `/` is at `token_start`: the flag checks, then
/// `regExpParser.run` over the body. Ported from `scanner.go:1067-1222` (the
/// `shouldReportErrors` arms) and `regexp.go`.
///
/// `language_version` is the checker's `languageVersion`; `ScriptTarget::None`
/// is `ScriptTargetLatest`, as `Scanner.languageVersion` reads it.
///
/// An unterminated literal reports `Unterminated regular expression literal`
/// over native's recovery span and validates nothing more, as native does.
#[must_use]
pub fn scan_regular_expression_errors(
    text: &str,
    token_start: u32,
    language_version: ScriptTarget,
) -> Vec<RegExpError> {
    let mut scanner = RegExpScanner {
        text: text.as_bytes(),
        pos: 0,
        end: text.len(),
        token_value: String::new(),
        token_start: 0,
        language_version: if language_version == ScriptTarget::None {
            ScriptTarget::ESNext
        } else {
            language_version
        },
        errors: Vec::new(),
    };
    scanner.rescan_slash_token(token_start as usize);
    scanner.errors
}

/// The native scanner state `regExpParser` drives.
struct RegExpScanner<'a> {
    text: &'a [u8],
    pos: usize,
    end: usize,
    token_value: String,
    token_start: usize,
    language_version: ScriptTarget,
    errors: Vec<RegExpError>,
}

impl RegExpScanner<'_> {
    /// `errorAt`.
    #[allow(clippy::cast_possible_truncation)]
    fn error_at(
        &mut self,
        message: &'static Message,
        pos: usize,
        length: usize,
        args: Vec<String>,
    ) {
        self.errors.push(RegExpError { message, start: pos as u32, length: length as u32, args });
    }

    /// `error`: at `s.pos`, length zero.
    fn error(&mut self, message: &'static Message) {
        self.error_at(message, self.pos, 0, Vec::new());
    }

    /// `char`: the byte at `pos` as a rune, `-1` at or past `end`.
    fn char(&self) -> i32 {
        if self.pos < self.end { i32::from(self.text[self.pos]) } else { -1 }
    }

    /// `charAt(offset)`: the byte at `pos + offset`, `-1` at or past `end`.
    fn char_at(&self, offset: usize) -> i32 {
        let at = self.pos + offset;
        if at < self.end { i32::from(self.text[at]) } else { -1 }
    }

    /// `charAndSize`: decodes past `end` too, as native's slow path does.
    fn char_and_size(&self) -> (i32, usize) {
        if self.pos < self.end && self.text[self.pos] < 0x80 {
            return (i32::from(self.text[self.pos]), 1);
        }
        decode_rune(self.text.get(self.pos..).unwrap_or_default())
    }

    /// `checkRegularExpressionFlagAvailability` (`regexp.go:50`).
    fn check_regular_expression_flag_availability(&mut self, flag: u32, pos: usize, size: usize) {
        if let Some((available_from, name)) = first_available_language_version(flag)
            && self.language_version < available_from
        {
            self.error_at(
                &messages::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_0_OR_LATER,
                pos,
                size,
                vec![name.to_string()],
            );
        }
    }

    /// `ReScanSlashToken(true)` (`scanner.go:1067`) from the `/` at
    /// `token_start`.
    fn rescan_slash_token(&mut self, token_start: usize) {
        let start_of_body = token_start + 1;
        let mut p = start_of_body;
        let mut in_escape = false;
        let mut named_capture_groups = false;
        let mut in_character_class = false;
        let mut unterminated = false;
        loop {
            if p >= self.end {
                unterminated = true;
                break;
            }
            let ch = self.text[p];
            if ch == b'\n' || ch == b'\r' {
                unterminated = true;
                break;
            } else if in_escape {
                in_escape = false;
            } else if ch == b'/' && !in_character_class {
                break;
            } else if ch == b'[' {
                in_character_class = true;
            } else if ch == b'\\' {
                in_escape = true;
            } else if ch == b']' {
                in_character_class = false;
            } else if !in_character_class
                && ch == b'('
                && self.text.get(p + 1) == Some(&b'?')
                && self.text.get(p + 2) == Some(&b'<')
                && !matches!(self.text.get(p + 3), Some(b'=' | b'!'))
            {
                named_capture_groups = true;
            }
            p += 1;
        }
        let end_of_body = p;
        if unterminated {
            self.report_unterminated(token_start, start_of_body, end_of_body);
            return;
        }
        // Consume the slash character.
        p += 1;
        let mut regexp_flags = 0;
        while p < self.end {
            let (ch, size) = decode_rune(&self.text[p..]);
            if ch == 0xFFFD || !rune_is_identifier_part(ch) {
                break;
            }
            match char_code_to_regexp_flag(ch) {
                None => {
                    self.error_at(&messages::UNKNOWN_REGULAR_EXPRESSION_FLAG, p, size, Vec::new());
                }
                Some(flag) if regexp_flags & flag != 0 => {
                    self.error_at(
                        &messages::DUPLICATE_REGULAR_EXPRESSION_FLAG,
                        p,
                        size,
                        Vec::new(),
                    );
                }
                Some(flag)
                    if (regexp_flags | flag) & flags::ANY_UNICODE_MODE
                        == flags::ANY_UNICODE_MODE =>
                {
                    self.error_at(
                        &messages::THE_UNICODE_U_FLAG_AND_THE_UNICODE_SETS_V_FLAG_CANNOT_BE_SET_SIMULTANEOUSLY,
                        p,
                        size,
                        Vec::new(),
                    );
                }
                Some(flag) => {
                    regexp_flags |= flag;
                    self.check_regular_expression_flag_availability(flag, p, size);
                }
            }
            p += size;
        }
        self.pos = start_of_body;
        let saved_end = self.end;
        self.end = end_of_body;
        let any_unicode_mode = regexp_flags & flags::ANY_UNICODE_MODE != 0;
        let mut parser = RegExpParser {
            scanner: self,
            end: end_of_body,
            any_unicode_mode,
            unicode_sets_mode: regexp_flags & flags::UNICODE_SETS != 0,
            annex_b: true,
            any_unicode_mode_or_non_annex_b: false,
            named_capture_groups,
            may_contain_strings: false,
            number_of_capturing_groups: 0,
            group_specifiers: Vec::new(),
            group_name_references: Vec::new(),
            decimal_escapes: Vec::new(),
            named_capturing_groups: Vec::new(),
            pending_low_surrogate: 0,
        };
        parser.run();
        self.end = saved_end;
        self.pos = p;
    }

    /// The unterminated arm of `ReScanSlashToken` (`scanner.go:1124-1172`):
    /// the nearest unbalanced bracket ends the guess, then trailing white
    /// space and semicolons are trimmed.
    fn report_unterminated(
        &mut self,
        token_start: usize,
        start_of_body: usize,
        end_of_body: usize,
    ) {
        let mut p = start_of_body;
        let mut in_escape = false;
        let mut character_class_depth = 0usize;
        let mut in_decimal_quantifier = false;
        let mut group_depth = 0usize;
        while p < end_of_body {
            let ch = self.text[p];
            if in_escape {
                in_escape = false;
            } else if ch == b'\\' {
                in_escape = true;
            } else if ch == b'[' {
                character_class_depth += 1;
            } else if ch == b']' && character_class_depth != 0 {
                character_class_depth -= 1;
            } else if character_class_depth == 0 {
                if ch == b'{' {
                    in_decimal_quantifier = true;
                } else if ch == b'}' && in_decimal_quantifier {
                    in_decimal_quantifier = false;
                } else if !in_decimal_quantifier {
                    if ch == b'(' {
                        group_depth += 1;
                    } else if ch == b')' && group_depth != 0 {
                        group_depth -= 1;
                    } else if ch == b')' || ch == b']' || ch == b'}' {
                        break;
                    }
                }
            }
            p += 1;
        }
        let p = unterminated_regular_expression_end(self.text, start_of_body, p);
        self.error_at(
            &messages::UNTERMINATED_REGULAR_EXPRESSION_LITERAL,
            token_start,
            p - token_start,
            Vec::new(),
        );
    }

    /// `scanHexDigits(minCount, scanAsManyAsPossible, false)`
    /// (`scanner.go:2102`): the digits, or `""` when fewer than `min_count`
    /// were read (the cursor still moves past them).
    fn scan_hex_digits(&mut self, min_count: usize, scan_as_many_as_possible: bool) -> &str {
        let start = self.pos;
        let mut count = 0;
        while count < min_count || scan_as_many_as_possible {
            if !is_hex_digit(self.char()) {
                break;
            }
            count += 1;
            self.pos += 1;
        }
        if count < min_count {
            return "";
        }
        std::str::from_utf8(&self.text[start..self.pos]).expect("ASCII hex digits")
    }

    /// `scanUnicodeEscape` (`scanner.go:1855`), known to be at `\u`; `-1`
    /// for an invalid escape.
    fn scan_unicode_escape(&mut self, should_emit_invalid_escape_error: bool) -> i64 {
        self.pos += 2;
        let start = self.pos;
        let extended = self.char() == i32::from(b'{');
        let hex_digits = if extended {
            self.pos += 1;
            self.scan_hex_digits(1, true)
        } else {
            self.scan_hex_digits(4, false)
        };
        if hex_digits.is_empty() {
            if should_emit_invalid_escape_error {
                self.error(&messages::HEXADECIMAL_DIGIT_EXPECTED);
            }
            return -1;
        }
        // `strconv.ParseInt(hexDigits, 16, 32)` saturates; every saturated
        // value is past 0x10FFFF, which is all the caller asks.
        let hex_value = i64::from_str_radix(hex_digits, 16)
            .map_or(i64::from(i32::MAX), |v| v.min(i64::from(i32::MAX)));
        if extended {
            let mut is_invalid_extended_escape = false;
            if hex_value > 0x10_FFFF {
                if should_emit_invalid_escape_error {
                    self.error_at(
                        &messages::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                        start + 1,
                        self.pos - start - 1,
                        Vec::new(),
                    );
                }
                is_invalid_extended_escape = true;
            }
            if self.pos >= self.end {
                if should_emit_invalid_escape_error {
                    self.error(&messages::UNEXPECTED_END_OF_TEXT);
                }
                is_invalid_extended_escape = true;
            } else if self.char() == i32::from(b'}') {
                self.pos += 1;
            } else {
                if should_emit_invalid_escape_error {
                    self.error(&messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE);
                }
                is_invalid_extended_escape = true;
            }
            if is_invalid_extended_escape {
                return -1;
            }
        }
        hex_value
    }

    /// `peekUnicodeEscape` (`scanner.go:1932`).
    fn peek_unicode_escape(&mut self) -> i64 {
        if self.char_at(1) == i32::from(b'u') {
            let saved = self.pos;
            let code_point = self.scan_unicode_escape(false);
            self.pos = saved;
            return code_point;
        }
        -1
    }

    /// `scanEscapeSequence(flags)` (`scanner.go:1690`) for its regular
    /// expression callers: `flags` always carries `RegularExpression`, so
    /// every invalid escape is reported.
    #[allow(clippy::too_many_lines)]
    fn scan_escape_sequence(&mut self, flags: u32) -> CharValue {
        let report = flags & escape::REPORT_INVALID_ESCAPE_ERRORS != 0;
        let start = self.pos;
        self.pos += 1;
        let ch = self.char();
        if ch < 0 {
            self.error(&messages::UNEXPECTED_END_OF_TEXT);
            return CharValue::Empty;
        }
        self.pos += 1;
        let Ok(byte) = u8::try_from(ch) else { unreachable!("a byte") };
        match byte {
            b'0'..=b'7' => {
                // '\0' not followed by a digit is NUL; '\08' is '\0' + '8'.
                if byte == b'0' && !is_digit(self.char()) {
                    return CharValue::One(0);
                }
                if byte <= b'3' && is_octal_digit(self.char()) {
                    self.pos += 1;
                }
                if is_octal_digit(self.char()) {
                    self.pos += 1;
                }
                if report {
                    let digits =
                        std::str::from_utf8(&self.text[start + 1..self.pos]).expect("ASCII");
                    let code = u32::from_str_radix(digits, 8).expect("octal digits");
                    let suggestion = format!("\\x{code:02x}");
                    if flags & escape::REGULAR_EXPRESSION != 0
                        && flags & escape::ATOM_ESCAPE == 0
                        && byte != b'0'
                    {
                        self.error_at(
                            &messages::OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_IF_THIS_WAS_INTENDED_AS_AN_ESCAPE_SEQUENCE_USE_THE_SYNTAX_0_INSTEAD,
                            start,
                            self.pos - start,
                            vec![suggestion],
                        );
                    } else {
                        self.error_at(
                            &messages::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX_0,
                            start,
                            self.pos - start,
                            vec![suggestion],
                        );
                    }
                    return CharValue::One(code);
                }
                CharValue::Many
            }
            b'8' | b'9' => {
                if report {
                    if flags & escape::REGULAR_EXPRESSION != 0 && flags & escape::ATOM_ESCAPE == 0 {
                        self.error_at(
                            &messages::DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS,
                            start,
                            self.pos - start,
                            Vec::new(),
                        );
                    } else {
                        let text =
                            String::from_utf8_lossy(&self.text[start..self.pos]).into_owned();
                        self.error_at(
                            &messages::ESCAPE_SEQUENCE_0_IS_NOT_ALLOWED,
                            start,
                            self.pos - start,
                            vec![text],
                        );
                    }
                    return CharValue::One(u32::from(byte));
                }
                CharValue::Many
            }
            b'b' => CharValue::One(0x08),
            b't' => CharValue::One(u32::from(b'\t')),
            b'n' => CharValue::One(u32::from(b'\n')),
            b'v' => CharValue::One(0x0B),
            b'f' => CharValue::One(0x0C),
            b'r' => CharValue::One(u32::from(b'\r')),
            b'\'' | b'"' => CharValue::One(u32::from(byte)),
            b'u' => {
                let extended = self.char() == i32::from(b'{');
                self.pos -= 2;
                let code_point = self.scan_unicode_escape(report);
                if extended {
                    if flags & escape::ALLOW_EXTENDED_UNICODE_ESCAPE == 0 && report {
                        self.error_at(
                            &messages::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICODE_SETS_V_FLAG_IS_SET,
                            start,
                            self.pos - start,
                            Vec::new(),
                        );
                    }
                    // A string's `\u{High}` + low-surrogate pairing does not
                    // apply: these callers are regular expressions.
                    return match u32::try_from(code_point) {
                        Ok(cp) => CharValue::One(cp),
                        Err(_) => CharValue::Many,
                    };
                }
                let Ok(cp) = u32::try_from(code_point) else {
                    return CharValue::Many;
                };
                if is_high_surrogate(cp)
                    && flags & escape::ANY_UNICODE_MODE != 0
                    && self.char() == i32::from(b'\\')
                    && self.char_at(1) == i32::from(b'u')
                    && self.char_at(2) != i32::from(b'{')
                {
                    // In regex AnyUnicodeMode, `\uHigh\uLow` is one code point.
                    let saved = self.pos;
                    let next = self.scan_unicode_escape(report);
                    if let Ok(low) = u32::try_from(next)
                        && is_low_surrogate(low)
                    {
                        return CharValue::One(surrogate_pair_to_code_point(cp, low));
                    }
                    self.pos = saved;
                }
                // A lone surrogate is one code unit (native's CESU-8 sentinel).
                CharValue::One(cp)
            }
            b'x' => {
                while self.pos < start + 4 {
                    if !is_hex_digit(self.char()) {
                        if report {
                            self.error(&messages::HEXADECIMAL_DIGIT_EXPECTED);
                        }
                        return CharValue::Many;
                    }
                    self.pos += 1;
                }
                let digits = std::str::from_utf8(&self.text[start + 2..self.pos]).expect("ASCII");
                CharValue::One(u32::from_str_radix(digits, 16).expect("hex digits"))
            }
            b'\r' => {
                if self.char() == i32::from(b'\n') {
                    self.pos += 1;
                }
                CharValue::Empty
            }
            b'\n' => CharValue::Empty,
            _ => {
                let mut ch = ch;
                if byte >= 0x80 {
                    self.pos -= 1;
                    let (decoded, size) = decode_rune(&self.text[self.pos..]);
                    ch = decoded;
                    self.pos += size;
                }
                if ch == 0x2028 || ch == 0x2029 {
                    return CharValue::Empty;
                }
                if flags & escape::ANY_UNICODE_MODE != 0
                    || flags & escape::REGULAR_EXPRESSION != 0
                        && flags & escape::ANNEX_B == 0
                        && rune_is_identifier_part(ch)
                {
                    self.error_at(
                        &messages::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
                        start,
                        self.pos - start,
                        Vec::new(),
                    );
                }
                CharValue::rune(ch)
            }
        }
    }

    /// `scanIdentifier(0)` (`scanner.go:1527`) for a group name; sets
    /// `token_value`.
    fn scan_identifier(&mut self) -> bool {
        let start = self.pos;
        let ch = self.char();
        if is_ascii_letter(ch) || ch == i32::from(b'_') || ch == i32::from(b'$') {
            self.pos += 1;
            while self.pos < self.end
                && matches!(self.text[self.pos], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$')
            {
                self.pos += 1;
            }
            let ch = self.char();
            if ch < 0x80 && ch != i32::from(b'\\') {
                self.token_value =
                    String::from_utf8_lossy(&self.text[start..self.pos]).into_owned();
                return true;
            }
            self.pos = start;
        }
        let (mut ch, mut size) = self.char_and_size();
        if rune_is_identifier_start(ch) {
            loop {
                self.pos += size;
                (ch, size) = self.char_and_size();
                if !rune_is_identifier_part(ch) {
                    break;
                }
            }
            self.token_value = String::from_utf8_lossy(&self.text[start..self.pos]).into_owned();
            if ch == i32::from(b'\\') {
                let parts = self.scan_identifier_parts();
                self.token_value.push_str(&parts);
            }
            return true;
        }
        false
    }

    /// `scanIdentifierParts` (`scanner.go:1562`).
    fn scan_identifier_parts(&mut self) -> String {
        let mut out = String::new();
        let mut start = self.pos;
        loop {
            let (ch, size) = self.char_and_size();
            if rune_is_identifier_part(ch) {
                self.pos += size;
                continue;
            }
            if ch == i32::from(b'\\') {
                let escaped = self.peek_unicode_escape();
                if escaped >= 0 && i32::try_from(escaped).is_ok_and(rune_is_identifier_part) {
                    out.push_str(&String::from_utf8_lossy(&self.text[start..self.pos]));
                    let cp = self.scan_unicode_escape(true);
                    let cp = u32::try_from(cp).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}');
                    out.push(cp);
                    start = self.pos;
                    continue;
                }
            }
            break;
        }
        out.push_str(&String::from_utf8_lossy(&self.text[start..self.pos]));
        out
    }
}

/// The recovery end of an unterminated literal: trailing white space and
/// semicolons are not likely part of it (`scanner.go:1162-1169`).
fn unterminated_regular_expression_end(text: &[u8], start_of_body: usize, mut p: usize) -> usize {
    while p > start_of_body {
        let Ok(prefix) = std::str::from_utf8(&text[start_of_body..p]) else {
            break;
        };
        let Some(ch) = prefix.chars().next_back() else { break };
        // `IsWhiteSpaceLike`: single-line white space or a line break.
        if crate::is_whitespace_single_line(ch) || crate::is_line_break(ch) || ch == ';' {
            p -= ch.len_utf8();
        } else {
            break;
        }
    }
    p
}

struct GroupNameReference {
    pos: usize,
    end: usize,
    name: String,
}

struct DecimalEscapeValue {
    pos: usize,
    end: usize,
    value: usize,
}

/// `regExpParser` (`regexp.go:77`); its flags are native's fields.
#[allow(clippy::struct_excessive_bools)]
struct RegExpParser<'s, 'a> {
    scanner: &'s mut RegExpScanner<'a>,
    end: usize,
    any_unicode_mode: bool,
    unicode_sets_mode: bool,
    annex_b: bool,
    any_unicode_mode_or_non_annex_b: bool,
    named_capture_groups: bool,
    /// See `scan_class_set_expression`.
    may_contain_strings: bool,
    /// The number of all (named and unnamed) capturing groups.
    number_of_capturing_groups: usize,
    /// All named capturing groups defined in the regex, in definition order
    /// (native's map; only membership and the spelling candidates are read).
    group_specifiers: Vec<String>,
    group_name_references: Vec<GroupNameReference>,
    decimal_escapes: Vec<DecimalEscapeValue>,
    /// A stack of scopes for named capturing groups. See `scan_group_name`.
    named_capturing_groups: Vec<Vec<String>>,
    /// The low surrogate a non-BMP source character still owes in a
    /// non-unicode pattern (`regexp.go:104`).
    pending_low_surrogate: u32,
}

impl RegExpParser<'_, '_> {
    fn pos(&self) -> usize {
        self.scanner.pos
    }

    fn inc_pos(&mut self, n: usize) {
        self.scanner.pos += n;
    }

    fn dec_pos(&mut self, n: usize) {
        self.scanner.pos -= n;
    }

    fn char(&self) -> i32 {
        self.scanner.char()
    }

    /// `charAt(pos)` over an absolute position.
    fn char_at(&self, pos: usize) -> i32 {
        self.scanner.char_at(pos - self.pos())
    }

    fn text(&self) -> &[u8] {
        self.scanner.text
    }

    fn error(&mut self, message: &'static Message, pos: usize, length: usize) {
        self.scanner.error_at(message, pos, length, Vec::new());
    }

    fn error_with(&mut self, message: &'static Message, pos: usize, length: usize, arg: String) {
        self.scanner.error_at(message, pos, length, vec![arg]);
    }

    fn is(&self, ch: u8) -> bool {
        self.char() == i32::from(ch)
    }

    /// The two bytes at `pos`, when both are inside the body.
    fn two_chars(&self) -> Option<[u8; 2]> {
        let pos = self.pos();
        (pos + 1 < self.end).then(|| [self.text()[pos], self.text()[pos + 1]])
    }

    /// `run` (`regexp.go:1043`).
    fn run(&mut self) {
        // Checked more strictly in 'u' or 'v' mode, or when not using Annex
        // B's looser syntax.
        self.any_unicode_mode_or_non_annex_b = self.any_unicode_mode || !self.annex_b;
        self.scan_disjunction(false);

        let references = std::mem::take(&mut self.group_name_references);
        for reference in &references {
            if !self.group_specifiers.contains(&reference.name) {
                self.error_with(
                    &messages::THERE_IS_NO_CAPTURING_GROUP_NAMED_0_IN_THIS_REGULAR_EXPRESSION,
                    reference.pos,
                    reference.end - reference.pos,
                    reference.name.clone(),
                );
                if !self.group_specifiers.is_empty()
                    && let Some(suggestion) = get_spelling_suggestion(
                        &reference.name,
                        &self.group_specifiers,
                        String::as_str,
                        std::cmp::Ord::cmp,
                    )
                {
                    let suggestion = suggestion.clone();
                    self.error_with(
                        &messages::DID_YOU_MEAN_0,
                        reference.pos,
                        reference.end - reference.pos,
                        suggestion,
                    );
                }
            }
        }
        let escapes = std::mem::take(&mut self.decimal_escapes);
        for escape in &escapes {
            // Annex B reads a backreference past the group count as a legacy
            // octal or identity escape, but it is reported anyway.
            if escape.value > self.number_of_capturing_groups {
                if self.number_of_capturing_groups > 0 {
                    self.error_with(
                        &messages::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_ONLY_0_CAPTURING_GROUPS_IN_THIS_REGULAR_EXPRESSION,
                        escape.pos,
                        escape.end - escape.pos,
                        self.number_of_capturing_groups.to_string(),
                    );
                } else {
                    self.error(
                        &messages::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING_GROUPS_IN_THIS_REGULAR_EXPRESSION,
                        escape.pos,
                        escape.end - escape.pos,
                    );
                }
            }
        }
    }

    /// `scanDisjunction` (`regexp.go:149`).
    fn scan_disjunction(&mut self, is_in_group: bool) {
        loop {
            self.named_capturing_groups.push(Vec::new());
            self.scan_alternative(is_in_group);
            self.named_capturing_groups.pop();
            if !self.is(b'|') {
                return;
            }
            self.inc_pos(1);
        }
    }

    /// `scanAlternative` (`regexp.go:200`).
    #[allow(clippy::too_many_lines)]
    fn scan_alternative(&mut self, is_in_group: bool) {
        let mut is_previous_term_quantifiable = false;
        while self.pos() < self.end {
            let start = self.pos();
            let ch = self.char();
            let Ok(byte) = u8::try_from(ch) else {
                self.scan_source_character();
                is_previous_term_quantifiable = true;
                continue;
            };
            // `{` falls through to the quantifier arm when it is a complete
            // (or, outside Annex B, a repaired) `{n,m}`.
            let mut quantifier = false;
            match byte {
                b'^' | b'$' => {
                    self.inc_pos(1);
                    is_previous_term_quantifiable = false;
                }
                b'\\' => {
                    self.inc_pos(1);
                    if self.is(b'b') || self.is(b'B') {
                        self.inc_pos(1);
                        is_previous_term_quantifiable = false;
                    } else {
                        self.scan_atom_escape();
                        is_previous_term_quantifiable = true;
                    }
                }
                b'(' => {
                    self.inc_pos(1);
                    if self.is(b'?') {
                        self.inc_pos(1);
                        if self.is(b'=') || self.is(b'!') {
                            self.inc_pos(1);
                            // In Annex B, `(?=…)` and `(?!…)` are quantifiable.
                            is_previous_term_quantifiable = !self.any_unicode_mode_or_non_annex_b;
                        } else if self.is(b'<') {
                            let group_name_start = self.pos();
                            self.inc_pos(1);
                            if self.is(b'=') || self.is(b'!') {
                                self.inc_pos(1);
                                is_previous_term_quantifiable = false;
                            } else {
                                self.scan_group_name(false);
                                self.scan_expected_char(b'>');
                                if self.scanner.language_version < ScriptTarget::ES2018 {
                                    self.error(
                                        &messages::NAMED_CAPTURING_GROUPS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ES2018_OR_LATER,
                                        group_name_start,
                                        self.pos() - group_name_start,
                                    );
                                }
                                self.number_of_capturing_groups += 1;
                                is_previous_term_quantifiable = true;
                            }
                        } else {
                            let flags_start = self.pos();
                            let set_flags = self.scan_pattern_modifiers(0);
                            if self.is(b'-') {
                                self.inc_pos(1);
                                self.scan_pattern_modifiers(set_flags);
                                if self.pos() == flags_start + 1 {
                                    self.error(
                                        &messages::SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN,
                                        flags_start,
                                        self.pos() - flags_start,
                                    );
                                }
                            }
                            self.scan_expected_char(b':');
                            is_previous_term_quantifiable = true;
                        }
                    } else {
                        self.number_of_capturing_groups += 1;
                        is_previous_term_quantifiable = true;
                    }
                    self.scan_disjunction(true);
                    self.scan_expected_char(b')');
                }
                b'{' => {
                    self.inc_pos(1);
                    let digits_start = self.pos();
                    self.scan_digits();
                    let min = std::mem::take(&mut self.scanner.token_value);
                    if !self.any_unicode_mode_or_non_annex_b && min.is_empty() {
                        is_previous_term_quantifiable = true;
                        continue;
                    }
                    if self.is(b',') {
                        self.inc_pos(1);
                        self.scan_digits();
                        let max = std::mem::take(&mut self.scanner.token_value);
                        if min.is_empty() {
                            if !max.is_empty() || self.is(b'}') {
                                self.error(
                                    &messages::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
                                    digits_start,
                                    0,
                                );
                            } else {
                                self.error_with(
                                    &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                    start,
                                    1,
                                    rune_string(ch),
                                );
                                is_previous_term_quantifiable = true;
                                continue;
                            }
                        } else if !max.is_empty()
                            && compare_decimal_strings(&min, &max).is_gt()
                            && (self.any_unicode_mode_or_non_annex_b || self.is(b'}'))
                        {
                            self.error(
                                &messages::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
                                digits_start,
                                self.pos() - digits_start,
                            );
                        }
                    } else if min.is_empty() {
                        if self.any_unicode_mode_or_non_annex_b {
                            self.error_with(
                                &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                start,
                                1,
                                rune_string(ch),
                            );
                        }
                        is_previous_term_quantifiable = true;
                        continue;
                    }
                    if !self.is(b'}') {
                        if self.any_unicode_mode_or_non_annex_b {
                            self.error_with(&messages::_0_EXPECTED, self.pos(), 0, "}".to_string());
                            self.dec_pos(1);
                        } else {
                            is_previous_term_quantifiable = true;
                            continue;
                        }
                    }
                    quantifier = true;
                }
                b'*' | b'+' | b'?' => quantifier = true,
                b'.' => {
                    self.inc_pos(1);
                    is_previous_term_quantifiable = true;
                }
                b'[' => {
                    self.inc_pos(1);
                    if self.unicode_sets_mode {
                        self.scan_class_set_expression();
                    } else {
                        self.scan_class_ranges();
                        self.pending_low_surrogate = 0;
                    }
                    self.scan_expected_char(b']');
                    is_previous_term_quantifiable = true;
                }
                b')' if is_in_group => return,
                b')' | b']' | b'}' => {
                    if self.any_unicode_mode_or_non_annex_b || byte == b')' {
                        self.error_with(
                            &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                            self.pos(),
                            1,
                            rune_string(ch),
                        );
                    }
                    self.inc_pos(1);
                    is_previous_term_quantifiable = true;
                }
                b'/' | b'|' => return,
                _ => {
                    self.scan_source_character();
                    is_previous_term_quantifiable = true;
                }
            }
            if quantifier {
                self.inc_pos(1);
                if self.is(b'?') {
                    // Non-greedy.
                    self.inc_pos(1);
                }
                if !is_previous_term_quantifiable {
                    self.error(
                        &messages::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                        start,
                        self.pos() - start,
                    );
                }
                is_previous_term_quantifiable = false;
            }
        }
    }

    /// `scanPatternModifiers` (`regexp.go:357`).
    fn scan_pattern_modifiers(&mut self, mut current_flags: u32) -> u32 {
        while self.pos() < self.end {
            let (ch, size) = decode_rune(&self.text()[self.pos()..]);
            if ch == 0xFFFD || !rune_is_identifier_part(ch) {
                break;
            }
            let pos = self.pos();
            match char_code_to_regexp_flag(ch) {
                None => self.error(&messages::UNKNOWN_REGULAR_EXPRESSION_FLAG, pos, size),
                Some(flag) if current_flags & flag != 0 => {
                    self.error(&messages::DUPLICATE_REGULAR_EXPRESSION_FLAG, pos, size);
                }
                Some(flag) if flag & flags::MODIFIERS == 0 => self.error(
                    &messages::THIS_REGULAR_EXPRESSION_FLAG_CANNOT_BE_TOGGLED_WITHIN_A_SUBPATTERN,
                    pos,
                    size,
                ),
                Some(flag) => {
                    current_flags |= flag;
                    self.scanner.check_regular_expression_flag_availability(flag, pos, size);
                }
            }
            self.inc_pos(size);
        }
        current_flags
    }

    /// `scanAtomEscape` (`regexp.go:385`).
    fn scan_atom_escape(&mut self) {
        if self.is(b'k') {
            self.inc_pos(1);
            if self.is(b'<') {
                self.inc_pos(1);
                self.scan_group_name(true);
                self.scan_expected_char(b'>');
            } else if self.any_unicode_mode_or_non_annex_b || self.named_capture_groups {
                self.error(
                    &messages::K_MUST_BE_FOLLOWED_BY_A_CAPTURING_GROUP_NAME_ENCLOSED_IN_ANGLE_BRACKETS,
                    self.pos() - 2,
                    2,
                );
            }
            return;
        }
        if self.is(b'q') && self.unicode_sets_mode {
            self.inc_pos(1);
            self.error(&messages::Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS, self.pos() - 2, 2);
            return;
        }
        if !self.scan_character_class_escape() && !self.scan_decimal_escape() {
            // Regex literals cannot contain line breaks here, so a character
            // escape consumes something (native asserts it).
            let value = self.scan_character_escape(true);
            debug_assert_ne!(value, CharValue::Empty);
        }
    }

    /// `scanDecimalEscape` (`regexp.go:415`).
    fn scan_decimal_escape(&mut self) -> bool {
        let ch = self.char();
        if (i32::from(b'1')..=i32::from(b'9')).contains(&ch) {
            let start = self.pos();
            self.scan_digits();
            // `strconv.Atoi` overflow is `math.MaxInt`.
            let value = self.scanner.token_value.parse::<usize>().unwrap_or(usize::MAX);
            self.decimal_escapes.push(DecimalEscapeValue { pos: start, end: self.pos(), value });
            return true;
        }
        false
    }

    /// `scanCharacterEscape` (`regexp.go:441`).
    fn scan_character_escape(&mut self, atom_escape: bool) -> CharValue {
        let ch = self.char();
        if ch == -1 {
            self.error(&messages::UNDETERMINED_CHARACTER_ESCAPE, self.pos() - 1, 1);
            return CharValue::One(u32::from(b'\\'));
        }
        match u8::try_from(ch) {
            Ok(b'c') => {
                self.inc_pos(1);
                let ch = self.char();
                if is_ascii_letter(ch) {
                    self.inc_pos(1);
                    return CharValue::rune(ch & 0x1f);
                }
                if self.any_unicode_mode_or_non_annex_b {
                    self.error(&messages::C_MUST_BE_FOLLOWED_BY_AN_ASCII_LETTER, self.pos() - 2, 2);
                } else if atom_escape {
                    self.dec_pos(1);
                    return CharValue::One(u32::from(b'\\'));
                }
                CharValue::rune(ch)
            }
            Ok(
                b'^' | b'$' | b'/' | b'\\' | b'.' | b'*' | b'+' | b'?' | b'(' | b')' | b'[' | b']'
                | b'{' | b'}' | b'|',
            ) => {
                self.inc_pos(1);
                CharValue::rune(ch)
            }
            _ => {
                // Back up to include the backslash for scanEscapeSequence.
                self.dec_pos(1);
                let mut flags = escape::REGULAR_EXPRESSION;
                if self.annex_b {
                    flags |= escape::ANNEX_B;
                }
                if self.any_unicode_mode {
                    flags |= escape::ANY_UNICODE_MODE;
                }
                if atom_escape {
                    flags |= escape::ATOM_ESCAPE;
                }
                self.scanner.scan_escape_sequence(flags)
            }
        }
    }

    /// `scanGroupName` (`regexp.go:483`).
    fn scan_group_name(&mut self, is_reference: bool) {
        self.scanner.token_start = self.pos();
        self.scanner.scan_identifier();
        let token_start = self.scanner.token_start;
        if self.pos() == token_start {
            self.error(&messages::EXPECTED_A_CAPTURING_GROUP_NAME, self.pos(), 0);
        } else if is_reference {
            let name = self.scanner.token_value.clone();
            self.group_name_references.push(GroupNameReference {
                pos: token_start,
                end: self.pos(),
                name,
            });
        } else if self.named_capturing_groups_contains(&self.scanner.token_value) {
            self.error(
                &messages::NAMED_CAPTURING_GROUPS_WITH_THE_SAME_NAME_MUST_BE_MUTUALLY_EXCLUSIVE_TO_EACH_OTHER,
                token_start,
                self.pos() - token_start,
            );
        } else {
            let name = self.scanner.token_value.clone();
            if let Some(scope) = self.named_capturing_groups.last_mut()
                && !scope.contains(&name)
            {
                scope.push(name.clone());
            }
            if !self.group_specifiers.contains(&name) {
                self.group_specifiers.push(name);
            }
        }
    }

    fn named_capturing_groups_contains(&self, name: &str) -> bool {
        self.named_capturing_groups.iter().any(|scope| scope.iter().any(|n| n == name))
    }

    /// `isClassContentExit` (`regexp.go:513`).
    fn is_class_content_exit(&self, ch: i32) -> bool {
        ch == i32::from(b']') || self.pos() >= self.end
    }

    /// `scanClassRanges` (`regexp.go:518`).
    fn scan_class_ranges(&mut self) {
        self.pending_low_surrogate = 0;
        if self.is(b'^') {
            self.inc_pos(1);
        }
        while self.pos() < self.end {
            if self.is_class_content_exit(self.char()) {
                return;
            }
            let min_start = self.pos();
            let min_character = self.scan_class_atom();
            if self.is(b'-') {
                self.inc_pos(1);
                if self.is_class_content_exit(self.char()) {
                    return;
                }
                if min_character == CharValue::Empty && self.any_unicode_mode_or_non_annex_b {
                    self.error(
                        &messages::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
                        min_start,
                        self.pos() - 1 - min_start,
                    );
                }
                let max_start = self.pos();
                let max_character = self.scan_class_atom();
                if max_character == CharValue::Empty && self.any_unicode_mode_or_non_annex_b {
                    self.error(
                        &messages::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
                        max_start,
                        self.pos() - max_start,
                    );
                    continue;
                }
                if min_character == CharValue::Empty {
                    continue;
                }
                if let (CharValue::One(min), CharValue::One(max)) = (min_character, max_character)
                    && min > max
                {
                    self.error(
                        &messages::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
                        min_start,
                        self.pos() - min_start,
                    );
                }
            }
        }
    }

    /// `scanClassSetExpression` (`regexp.go:577`).
    #[allow(clippy::too_many_lines)]
    fn scan_class_set_expression(&mut self) {
        let mut is_character_complement = false;
        if self.is(b'^') {
            self.inc_pos(1);
            is_character_complement = true;
        }
        let mut expression_may_contain_strings = false;
        let mut ch = self.char();
        if self.is_class_content_exit(ch) {
            return;
        }
        let mut start = self.pos();
        let mut operand;
        match self.two_chars() {
            Some(pair) if &pair == b"--" || &pair == b"&&" => {
                self.error(&messages::EXPECTED_A_CLASS_SET_OPERAND, self.pos(), 0);
                self.may_contain_strings = false;
                operand = CharValue::Empty;
            }
            _ => operand = self.scan_class_set_operand(),
        }
        let next = self.char();
        if next == i32::from(b'-') {
            if self.pos() + 1 < self.end && self.char_at(self.pos() + 1) == i32::from(b'-') {
                if is_character_complement && self.may_contain_strings {
                    self.error(
                        &messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSIDE_A_NEGATED_CHARACTER_CLASS,
                        start,
                        self.pos() - start,
                    );
                }
                expression_may_contain_strings = self.may_contain_strings;
                self.scan_class_set_sub_expression(ClassSetExpressionType::ClassSubtraction);
                self.may_contain_strings =
                    !is_character_complement && expression_may_contain_strings;
                return;
            }
        } else if next == i32::from(b'&') {
            if self.pos() + 1 < self.end && self.char_at(self.pos() + 1) == i32::from(b'&') {
                self.scan_class_set_sub_expression(ClassSetExpressionType::ClassIntersection);
                if is_character_complement && self.may_contain_strings {
                    self.error(
                        &messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSIDE_A_NEGATED_CHARACTER_CLASS,
                        start,
                        self.pos() - start,
                    );
                }
                expression_may_contain_strings = self.may_contain_strings;
                self.may_contain_strings =
                    !is_character_complement && expression_may_contain_strings;
                return;
            }
            // Native passes the expression's FIRST character here, the `ch`
            // read before the operand, not the `&`.
            self.error_with(
                &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                self.pos(),
                1,
                rune_string(ch),
            );
        } else {
            if is_character_complement && self.may_contain_strings {
                self.error(
                    &messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSIDE_A_NEGATED_CHARACTER_CLASS,
                    start,
                    self.pos() - start,
                );
            }
            expression_may_contain_strings = self.may_contain_strings;
        }
        while self.pos() < self.end {
            ch = self.char();
            if ch == i32::from(b'-') {
                self.inc_pos(1);
                ch = self.char();
                if self.is_class_content_exit(ch) {
                    self.may_contain_strings =
                        !is_character_complement && expression_may_contain_strings;
                    return;
                }
                if ch == i32::from(b'-') {
                    self.inc_pos(1);
                    self.error(
                        &messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_INSTEAD,
                        self.pos() - 2,
                        2,
                    );
                    start = self.pos() - 2;
                    operand = CharValue::Many;
                    continue;
                }
                if operand == CharValue::Empty {
                    self.error(
                        &messages::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
                        start,
                        self.pos() - 1 - start,
                    );
                }
                let second_start = self.pos();
                let second_operand = self.scan_class_set_operand();
                if is_character_complement && self.may_contain_strings {
                    self.error(
                        &messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSIDE_A_NEGATED_CHARACTER_CLASS,
                        second_start,
                        self.pos() - second_start,
                    );
                }
                expression_may_contain_strings =
                    expression_may_contain_strings || self.may_contain_strings;
                if second_operand == CharValue::Empty {
                    self.error(
                        &messages::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
                        second_start,
                        self.pos() - second_start,
                    );
                } else if let (CharValue::One(min), CharValue::One(max)) = (operand, second_operand)
                    && min > max
                {
                    self.error(
                        &messages::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
                        start,
                        self.pos() - start,
                    );
                }
            } else if ch == i32::from(b'&') {
                start = self.pos();
                self.inc_pos(1);
                if self.is(b'&') {
                    self.inc_pos(1);
                    self.error(
                        &messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_INSTEAD,
                        self.pos() - 2,
                        2,
                    );
                    if self.is(b'&') {
                        self.error_with(
                            &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                            self.pos(),
                            1,
                            rune_string(ch),
                        );
                        self.inc_pos(1);
                    }
                } else {
                    self.error_with(
                        &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                        self.pos() - 1,
                        1,
                        rune_string(ch),
                    );
                }
                operand = if self.pos() - start == 1 {
                    CharValue::One(u32::from(b'&'))
                } else {
                    CharValue::Many
                };
                continue;
            }
            if self.is_class_content_exit(self.char()) {
                break;
            }
            start = self.pos();
            match self.two_chars() {
                Some(pair) if &pair == b"--" || &pair == b"&&" => {
                    self.error(
                        &messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_INSTEAD,
                        self.pos(),
                        2,
                    );
                    self.inc_pos(2);
                    operand = CharValue::Many;
                }
                _ => operand = self.scan_class_set_operand(),
            }
        }
        self.may_contain_strings = !is_character_complement && expression_may_contain_strings;
    }

    /// `scanClassSetSubExpression` (`regexp.go:711`).
    fn scan_class_set_sub_expression(&mut self, expression_type: ClassSetExpressionType) {
        let mut expression_may_contain_strings = self.may_contain_strings;
        while self.pos() < self.end {
            let ch = self.char();
            if self.is_class_content_exit(ch) {
                break;
            }
            if ch == i32::from(b'-') {
                self.inc_pos(1);
                if self.is(b'-') {
                    self.inc_pos(1);
                    if expression_type != ClassSetExpressionType::ClassSubtraction {
                        self.error(
                            &messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_INSTEAD,
                            self.pos() - 2,
                            2,
                        );
                    }
                } else {
                    self.error(
                        &messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_INSTEAD,
                        self.pos() - 1,
                        1,
                    );
                }
            } else if ch == i32::from(b'&') {
                self.inc_pos(1);
                if self.is(b'&') {
                    self.inc_pos(1);
                    if expression_type != ClassSetExpressionType::ClassIntersection {
                        self.error(
                            &messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_INSTEAD,
                            self.pos() - 2,
                            2,
                        );
                    }
                    if self.is(b'&') {
                        self.error_with(
                            &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                            self.pos(),
                            1,
                            rune_string(ch),
                        );
                        self.inc_pos(1);
                    }
                } else {
                    self.error_with(
                        &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                        self.pos() - 1,
                        1,
                        rune_string(ch),
                    );
                }
            } else {
                let expected = match expression_type {
                    ClassSetExpressionType::ClassSubtraction => "--",
                    ClassSetExpressionType::ClassIntersection => "&&",
                };
                self.error_with(&messages::_0_EXPECTED, self.pos(), 0, expected.to_string());
            }
            if self.is_class_content_exit(self.char()) {
                self.error(&messages::EXPECTED_A_CLASS_SET_OPERAND, self.pos(), 0);
                break;
            }
            self.scan_class_set_operand();
            if expression_type == ClassSetExpressionType::ClassIntersection {
                expression_may_contain_strings =
                    expression_may_contain_strings && self.may_contain_strings;
            }
        }
        self.may_contain_strings = expression_may_contain_strings;
    }

    /// `scanClassSetOperand` (`regexp.go:777`).
    fn scan_class_set_operand(&mut self) -> CharValue {
        self.may_contain_strings = false;
        if self.is(b'[') {
            self.inc_pos(1);
            self.scan_class_set_expression();
            self.scan_expected_char(b']');
            return CharValue::Empty;
        }
        if self.is(b'\\') {
            self.inc_pos(1);
            if self.scan_character_class_escape() {
                return CharValue::Empty;
            }
            if self.is(b'q') {
                self.inc_pos(1);
                if self.is(b'{') {
                    self.inc_pos(1);
                    self.scan_class_string_disjunction_contents();
                    self.scan_expected_char(b'}');
                    return CharValue::Empty;
                }
                self.error(
                    &messages::Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES,
                    self.pos() - 2,
                    2,
                );
                return CharValue::One(u32::from(b'q'));
            }
            self.dec_pos(1);
        }
        self.scan_class_set_character()
    }

    /// `scanClassStringDisjunctionContents` (`regexp.go:809`).
    fn scan_class_string_disjunction_contents(&mut self) {
        let mut character_count = 0;
        while self.pos() < self.end {
            if self.is(b'}') {
                if character_count != 1 {
                    self.may_contain_strings = true;
                }
                return;
            }
            if self.is(b'|') {
                if character_count != 1 {
                    self.may_contain_strings = true;
                }
                self.inc_pos(1);
                character_count = 0;
            } else {
                self.scan_class_set_character();
                character_count += 1;
            }
        }
    }

    /// `scanClassSetCharacter` (`regexp.go:837`).
    fn scan_class_set_character(&mut self) -> CharValue {
        let ch = self.char();
        if ch == i32::from(b'\\') {
            self.inc_pos(1);
            let inner = self.char();
            return match u8::try_from(inner) {
                Ok(b'b') => {
                    self.inc_pos(1);
                    CharValue::One(0x08)
                }
                Ok(
                    b'&' | b'-' | b'!' | b'#' | b'%' | b',' | b':' | b';' | b'<' | b'=' | b'>'
                    | b'@' | b'`' | b'~',
                ) => {
                    self.inc_pos(1);
                    CharValue::rune(inner)
                }
                _ => self.scan_character_escape(false),
            };
        }
        if self.pos() + 1 < self.end
            && ch == self.char_at(self.pos() + 1)
            && matches!(
                u8::try_from(ch),
                Ok(b'&'
                    | b'!'
                    | b'#'
                    | b'%'
                    | b'*'
                    | b'+'
                    | b','
                    | b'.'
                    | b':'
                    | b';'
                    | b'<'
                    | b'='
                    | b'>'
                    | b'?'
                    | b'@'
                    | b'`'
                    | b'~')
            )
        {
            self.error(
                &messages::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                self.pos(),
                2,
            );
            self.inc_pos(2);
            return CharValue::Many;
        }
        if matches!(
            u8::try_from(ch),
            Ok(b'/' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'-' | b'|')
        ) {
            self.error_with(
                &messages::UNEXPECTED_0_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                self.pos(),
                1,
                rune_string(ch),
            );
            self.inc_pos(1);
            return CharValue::rune(ch);
        }
        self.scan_source_character()
    }

    /// `scanClassAtom` (`regexp.go:877`).
    fn scan_class_atom(&mut self) -> CharValue {
        if self.is(b'\\') {
            self.inc_pos(1);
            let ch = self.char();
            if ch == i32::from(b'b') {
                self.inc_pos(1);
                return CharValue::One(0x08);
            }
            if ch == i32::from(b'-') {
                self.inc_pos(1);
                return CharValue::rune(ch);
            }
            if self.scan_character_class_escape() {
                return CharValue::Empty;
            }
            return self.scan_character_escape(false);
        }
        self.scan_source_character()
    }

    /// `scanCharacterClassEscape` (`regexp.go:903`).
    #[allow(clippy::too_many_lines)]
    fn scan_character_class_escape(&mut self) -> bool {
        let start = self.pos() - 1;
        let ch = self.char();
        let is_character_complement = match u8::try_from(ch) {
            Ok(b'd' | b'D' | b's' | b'S' | b'w' | b'W') => {
                self.inc_pos(1);
                return true;
            }
            Ok(b'P') => true,
            Ok(b'p') => false,
            _ => return false,
        };
        self.inc_pos(1);
        if self.is(b'{') {
            self.inc_pos(1);
            let name_or_value_start = self.pos();
            let name_or_value = self.scan_word_characters();
            if self.is(b'=') {
                let property_name = NON_BINARY_UNICODE_PROPERTIES
                    .iter()
                    .find(|(alias, _)| *alias == name_or_value)
                    .map(|(_, canonical)| *canonical);
                if self.pos() == name_or_value_start {
                    self.error(&messages::EXPECTED_A_UNICODE_PROPERTY_NAME, self.pos(), 0);
                } else if property_name.is_none() {
                    let length = self.pos() - name_or_value_start;
                    self.error(
                        &messages::UNKNOWN_UNICODE_PROPERTY_NAME,
                        name_or_value_start,
                        length,
                    );
                    let candidates = NON_BINARY_UNICODE_PROPERTIES.iter().map(|(alias, _)| *alias);
                    if let Some(suggestion) = spelling_suggestion(&name_or_value, candidates) {
                        self.error_with(
                            &messages::DID_YOU_MEAN_0,
                            name_or_value_start,
                            length,
                            suggestion,
                        );
                    }
                }
                self.inc_pos(1);
                let value_start = self.pos();
                let value = self.scan_word_characters();
                if self.pos() == value_start {
                    self.error(&messages::EXPECTED_A_UNICODE_PROPERTY_VALUE, self.pos(), 0);
                } else if let Some(property_name) = property_name {
                    let values = values_of_non_binary_unicode_property(property_name);
                    if !values.contains(&value.as_str()) {
                        let length = self.pos() - value_start;
                        self.error(&messages::UNKNOWN_UNICODE_PROPERTY_VALUE, value_start, length);
                        if let Some(suggestion) =
                            spelling_suggestion(&value, values.iter().copied())
                        {
                            self.error_with(
                                &messages::DID_YOU_MEAN_0,
                                value_start,
                                length,
                                suggestion,
                            );
                        }
                    }
                }
            } else if self.pos() == name_or_value_start {
                self.error(&messages::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE, self.pos(), 0);
            } else if BINARY_UNICODE_PROPERTIES_OF_STRINGS.contains(&name_or_value.as_str()) {
                let length = self.pos() - name_or_value_start;
                if !self.unicode_sets_mode {
                    self.error(
                        &messages::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_ONLY_AVAILABLE_WHEN_THE_UNICODE_SETS_V_FLAG_IS_SET,
                        name_or_value_start,
                        length,
                    );
                } else if is_character_complement {
                    self.error(
                        &messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSIDE_A_NEGATED_CHARACTER_CLASS,
                        name_or_value_start,
                        length,
                    );
                } else {
                    self.may_contain_strings = true;
                }
            } else if !GENERAL_CATEGORY_VALUES.contains(&name_or_value.as_str())
                && !BINARY_UNICODE_PROPERTIES.contains(&name_or_value.as_str())
            {
                let length = self.pos() - name_or_value_start;
                self.error(
                    &messages::UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE,
                    name_or_value_start,
                    length,
                );
                let candidates = GENERAL_CATEGORY_VALUES
                    .iter()
                    .chain(BINARY_UNICODE_PROPERTIES)
                    .chain(BINARY_UNICODE_PROPERTIES_OF_STRINGS)
                    .copied();
                if let Some(suggestion) = spelling_suggestion(&name_or_value, candidates) {
                    self.error_with(
                        &messages::DID_YOU_MEAN_0,
                        name_or_value_start,
                        length,
                        suggestion,
                    );
                }
            }
            self.scan_expected_char(b'}');
            if !self.any_unicode_mode {
                self.error(
                    &messages::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICODE_SETS_V_FLAG_IS_SET,
                    start,
                    self.pos() - start,
                );
            }
        } else if self.any_unicode_mode_or_non_annex_b {
            self.error_with(
                &messages::_0_MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES,
                self.pos() - 2,
                2,
                rune_string(ch),
            );
        } else {
            self.dec_pos(1);
            return false;
        }
        true
    }

    /// `scanWordCharacters` (`regexp.go:1003`).
    fn scan_word_characters(&mut self) -> String {
        let start = self.pos();
        while self.pos() < self.end && is_word_character(self.char()) {
            self.inc_pos(1);
        }
        String::from_utf8_lossy(&self.text()[start..self.pos()]).into_owned()
    }

    /// `scanSourceCharacter` (`regexp.go:1014`).
    fn scan_source_character(&mut self) -> CharValue {
        if self.pos() >= self.end {
            return CharValue::Empty;
        }
        let (ch, size) = decode_rune(&self.text()[self.pos()..]);
        if !self.any_unicode_mode {
            if self.pending_low_surrogate != 0 {
                // The second code unit of a non-BMP character; now advance
                // past the whole UTF-8 sequence.
                self.inc_pos(size);
                let low = self.pending_low_surrogate;
                self.pending_low_surrogate = 0;
                return CharValue::One(low);
            }
            if ch == 0xFFFD || size == 0 {
                // Not a valid rune: consume one raw byte.
                let byte = self.text()[self.pos()];
                self.inc_pos(1);
                return CharValue::One(u32::from(byte));
            }
            let cp = u32::try_from(ch).expect("scalar");
            if cp >= 0x1_0000 {
                // Emit the high surrogate first WITHOUT advancing; the low
                // one comes on the next call, which also advances.
                let offset = cp - 0x1_0000;
                self.pending_low_surrogate = 0xDC00 + (offset & 0x3FF);
                return CharValue::One(0xD800 + (offset >> 10));
            }
            self.inc_pos(size);
            return CharValue::One(cp);
        }
        if size == 0 {
            return CharValue::Empty;
        }
        if ch == 0xFFFD {
            // Invalid UTF-8; consume to avoid looping.
            self.inc_pos(size);
            return CharValue::Empty;
        }
        self.inc_pos(size);
        CharValue::rune(ch)
    }

    /// `scanExpectedChar` (`regexp.go:1036`).
    fn scan_expected_char(&mut self, ch: u8) {
        if self.is(ch) {
            self.inc_pos(1);
        } else {
            self.error_with(&messages::_0_EXPECTED, self.pos(), 0, char::from(ch).to_string());
        }
    }

    /// `scanDigits` (`regexp.go:1044`): sets `token_value`.
    fn scan_digits(&mut self) {
        let start = self.pos();
        while self.pos() < self.end && is_digit(self.char()) {
            self.inc_pos(1);
        }
        self.scanner.token_value =
            String::from_utf8_lossy(&self.text()[start..self.pos()]).into_owned();
    }
}

/// `values` of `valuesOfNonBinaryUnicodeProperties[name]` (`unicodeproperties.go:139`).
fn values_of_non_binary_unicode_property(canonical: &str) -> &'static [&'static str] {
    match canonical {
        "General_Category" => GENERAL_CATEGORY_VALUES,
        _ => SCRIPT_VALUES,
    }
}

/// `core.GetSpellingSuggestionForStrings`: the closest candidate, ties broken
/// lexically (native walks a map, in no order; the answer does not depend on
/// it).
fn spelling_suggestion<'c>(
    name: &str,
    candidates: impl Iterator<Item = &'c str>,
) -> Option<String> {
    let candidates: Vec<&str> = candidates.collect();
    get_spelling_suggestion(name, &candidates, |c| c, std::cmp::Ord::cmp).map(|s| (*s).to_string())
}
