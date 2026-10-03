//! A minimal JSON reader for `package.json`.
//!
//! Stands in for upstream's `internal/json` at the pinned commit, which is a
//! vendored fork of Go's `encoding/json/v2`.
//!
//! # Why not a JSON crate
//!
//! Two requirements that most JSON crates do not meet together:
//!
//! 1. **Object key order is behaviour.** `exports` conditions are matched in
//!    declaration order and the resolution trace records each one as it is tried
//!    (`Saw non-matching condition '0'`), so a hash-ordered object would produce
//!    a different, equally-plausible trace and fail every baseline.
//! 2. **Duplicate keys are tolerated, last one wins.** Upstream passes
//!    `json.AllowDuplicateNames(true)` explicitly; a strict parser would reject
//!    package files the compiler accepts.
//!
//! `serde_json` needs the `preserve_order` feature (pulling in `indexmap`) for
//! the first and still errors on nothing for the second. The parser below is
//! about 150 lines, and the object representation it produces is the one
//! [`crate::package_json`] wants anyway.
//!
//! # What it does not do
//!
//! Numbers are kept as their source text rather than parsed. Nothing in module
//! resolution reads a number's *value* — `JSONValue` only ever asks whether a
//! field is a number, for the "expected type" trace line — so parsing one would
//! be inventing a precision question nobody asked.

/// A parsed JSON value (`packagejson.JSONValue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, kept as written.
    Number(String),
    /// A string, with escapes resolved.
    String(String),
    /// An array.
    Array(Vec<Json>),
    /// An object, in declaration order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// The JSON type name, as the `Expected type of '0' field...` trace line
    /// spells it (`packagejson.JSONValueType.String`).
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }

    /// The value for `key`, if this is an object.
    ///
    /// Later duplicates win, matching `AllowDuplicateNames`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(entries) => {
                entries.iter().rev().find(|(k, _)| k == key).map(|(_, value)| value)
            }
            _ => None,
        }
    }

    /// This value as a string, if it is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }
}

/// Parse a complete JSON document.
///
/// Returns `None` on any syntax error. Upstream keeps a `Parseable` flag on the
/// package rather than discarding it — a malformed `package.json` still *exists*,
/// and resolution behaves differently for "no package.json" and "a broken one" —
/// so the caller decides what an error means.
#[must_use]
pub fn parse(text: &str) -> Option<Json> {
    let mut parser = Parser { text, bytes: text.as_bytes(), position: 0 };
    parser.skip_whitespace();
    let value = parser.value()?;
    parser.skip_whitespace();
    parser.at_end().then_some(value)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn at_end(&self) -> bool {
        self.position >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Option<()> {
        (self.peek() == Some(byte)).then(|| self.position += 1)
    }

    fn literal(&mut self, text: &str) -> Option<()> {
        if self.bytes[self.position..].starts_with(text.as_bytes()) {
            self.position += text.len();
            Some(())
        } else {
            None
        }
    }

    fn value(&mut self) -> Option<Json> {
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Json::String),
            b't' => self.literal("true").map(|()| Json::Bool(true)),
            b'f' => self.literal("false").map(|()| Json::Bool(false)),
            b'n' => self.literal("null").map(|()| Json::Null),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Option<Json> {
        self.expect(b'{')?;
        let mut entries: Vec<(String, Json)> = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Some(Json::Object(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value()?;
            // Last one wins, and it keeps the *later* position — which is what
            // Go's decoder does for a repeated key.
            if let Some(existing) = entries.iter().position(|(k, _)| *k == key) {
                entries.remove(existing);
            }
            entries.push((key, value));
            self.skip_whitespace();
            match self.peek()? {
                b',' => self.position += 1,
                b'}' => {
                    self.position += 1;
                    return Some(Json::Object(entries));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self) -> Option<Json> {
        self.expect(b'[')?;
        let mut elements = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Some(Json::Array(elements));
        }
        loop {
            self.skip_whitespace();
            elements.push(self.value()?);
            self.skip_whitespace();
            match self.peek()? {
                b',' => self.position += 1,
                b']' => {
                    self.position += 1;
                    return Some(Json::Array(elements));
                }
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        self.expect(b'"')?;
        let mut result = String::new();
        loop {
            match self.peek()? {
                b'"' => {
                    self.position += 1;
                    return Some(result);
                }
                b'\\' => {
                    self.position += 1;
                    let escape = self.peek()?;
                    self.position += 1;
                    match escape {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'b' => result.push('\u{8}'),
                        b'f' => result.push('\u{c}'),
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'u' => result.push(self.unicode_escape()?),
                        _ => return None,
                    }
                }
                _ => {
                    // The input is already valid UTF-8. Revalidating the
                    // remaining document at every character is quadratic.
                    // Safe slicing also rejects a cursor inside a code point.
                    let rest = self.text.get(self.position..)?;
                    let character = rest.chars().next()?;
                    self.position += character.len_utf8();
                    result.push(character);
                }
            }
        }
    }

    /// A `\uXXXX` escape, including a surrogate pair.
    fn unicode_escape(&mut self) -> Option<char> {
        let high = self.hex4()?;
        if !(0xD800..0xDC00).contains(&high) {
            return char::from_u32(u32::from(high));
        }
        self.expect(b'\\')?;
        self.expect(b'u')?;
        let low = self.hex4()?;
        let combined = 0x1_0000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
        char::from_u32(combined)
    }

    fn hex4(&mut self) -> Option<u16> {
        let digits = self.bytes.get(self.position..self.position + 4)?;
        let text = std::str::from_utf8(digits).ok()?;
        self.position += 4;
        u16::from_str_radix(text, 16).ok()
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        let digits_start = self.position;
        while matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')) {
            self.position += 1;
        }
        if self.position == digits_start {
            return None;
        }
        Some(Json::Number(String::from_utf8(self.bytes[start..self.position].to_vec()).ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_key_order_is_preserved() {
        // Not a nicety: `exports` conditions are matched in this order and the
        // trace records each attempt.
        let Some(Json::Object(entries)) = parse(r#"{"types":1,"import":2,"default":3}"#) else {
            panic!("expected an object")
        };
        let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["types", "import", "default"]);
    }

    #[test]
    fn a_duplicate_key_keeps_the_last_value_at_the_last_position() {
        let Some(Json::Object(entries)) = parse(r#"{"a":1,"b":2,"a":3}"#) else {
            panic!("expected an object")
        };
        let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "a"]);
        assert_eq!(parse(r#"{"a":1,"a":3}"#).unwrap().get("a"), Some(&Json::Number("3".into())));
    }

    #[test]
    fn the_shapes_package_json_actually_uses() {
        let value = parse(
            r#"{"name":"x","version":"1.0.0","exports":{".":{"types":"./i.d.ts"}},
                "typesVersions":{">=3.1.0-0":{"*":["ts3.1/*"]}},"typings":null}"#,
        )
        .expect("parses");
        assert_eq!(value.get("name").and_then(Json::as_str), Some("x"));
        assert_eq!(value.get("typings"), Some(&Json::Null));
        assert_eq!(
            value.get("exports").and_then(|e| e.get(".")).and_then(|d| d.get("types")),
            Some(&Json::String("./i.d.ts".to_string()))
        );
    }

    #[test]
    fn a_malformed_document_is_rejected_rather_than_truncated() {
        // A broken `package.json` is a real state the resolver distinguishes from
        // a missing one, so this must fail rather than half-succeed.
        assert!(parse(r#"{"a":}"#).is_none());
        assert!(parse(r#"{"a":1"#).is_none());
        assert!(parse(r#"{"a":1} trailing"#).is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn strings_handle_escapes_and_non_ascii() {
        assert_eq!(parse(r#""a\/b\n""#), Some(Json::String("a/b\n".to_string())));
        assert_eq!(parse(r#""é""#), Some(Json::String("é".to_string())));
        // A surrogate pair, which a naive `\uXXXX` reader turns into mojibake.
        assert_eq!(parse(r#""😀""#), Some(Json::String("😀".to_string())));
        assert_eq!(parse(r#""café""#), Some(Json::String("café".to_string())));
    }

    #[test]
    fn strings_mix_all_utf8_widths_with_escaped_delimiters() {
        let value = parse(r#"{"é漢😀":"asciié漢😀\"\\\/\u0061\uD83D\uDE00tail"}"#)
            .expect("valid UTF-8 and escapes");
        assert_eq!(value.get("é漢😀").and_then(Json::as_str), Some("asciié漢😀\"\\/a😀tail"));
    }

    #[test]
    fn malformed_strings_after_multibyte_characters_are_rejected() {
        for text in [r#""é漢😀"#, r#""é\q""#, r#""漢\u00""#, r#""😀\uD83Dtail""#] {
            assert!(parse(text).is_none(), "malformed string accepted: {text}");
        }
    }
}
