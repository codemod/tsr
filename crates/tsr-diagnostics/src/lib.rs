//! TypeScript diagnostic messages.
//!
//! The message catalogue in [`messages`] is generated from TypeScript's own
//! `diagnosticMessages.json`; see `docs/adr/0007-generated-code-policy.md`.
//!
//! ```
//! use tsr_diagnostics::{messages, Category};
//!
//! let m = &messages::UNTERMINATED_STRING_LITERAL;
//! assert_eq!(m.code(), 1002);
//! assert_eq!(m.category(), Category::Error);
//! assert_eq!(m.format(&[]), "Unterminated string literal.");
//!
//! // `{0}`-style placeholders are substituted positionally.
//! assert_eq!(messages::_0_EXPECTED.format(&[";"]), "';' expected.");
//! ```

pub mod format;
mod generated;

pub use format::{
    DiagnosticFile, FormattingOptions, LocatedDiagnostic, format_diagnostics,
    write_error_summary_text, write_format_diagnostics,
};
pub use generated::messages;
use tsr_core::Span;

/// How severe a diagnostic is.
///
/// Corresponds to typescript-go's `diagnostics.Category`. Discriminants match
/// upstream's `iota` ordering, which the API surface serialises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Category {
    /// A warning.
    Warning = 0,
    /// An error.
    Error = 1,
    /// A suggestion, shown only in editors.
    Suggestion = 2,
    /// Informational text, used for CLI help and trace output.
    Message = 3,
}

impl Category {
    /// The lowercase name used in rendered output and baselines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Suggestion => "suggestion",
            Self::Message => "message",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

bitflags::bitflags! {
    /// Rarely-set properties of a message.
    ///
    /// Upstream stores these as three separate booleans; folding them into flags
    /// keeps [`Message`] small, and only 15 of 2,162 messages set any of them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct MessageFlags: u8 {
        /// Render as an unused-code hint rather than a hard error.
        const REPORTS_UNNECESSARY = 1 << 0;
        /// Render as a deprecation.
        const REPORTS_DEPRECATED = 1 << 1;
        /// Excluded from the compatibility pyramid.
        ///
        /// The misspelling in upstream's JSON key (`elidedInCompatabilityPyramid`)
        /// is inherited from TypeScript and preserved by the input parser.
        const ELIDED_IN_COMPATIBILITY_PYRAMID = 1 << 2;
    }
}

/// A diagnostic message template.
///
/// Corresponds to typescript-go's `diagnostics.Message`. Instances are `static`
/// and generated; construct diagnostics by pairing one with a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message {
    code: u32,
    category: Category,
    key: &'static str,
    text: &'static str,
    flags: MessageFlags,
}

impl Message {
    /// Construct a message. Called only by generated code.
    #[must_use]
    pub const fn new(
        code: u32,
        category: Category,
        key: &'static str,
        text: &'static str,
        flags: MessageFlags,
    ) -> Self {
        Self { code, category, key, text, flags }
    }

    /// The stable numeric code, e.g. 1002. This is what `TS1002` in output refers
    /// to, and what baselines record.
    #[must_use]
    pub const fn code(&self) -> u32 {
        self.code
    }

    /// Severity.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// The localization key, e.g. `Unterminated_string_literal_1002`.
    ///
    /// Load-bearing: it is how translated strings are looked up, and it is
    /// reproduced byte-for-byte from upstream's mangling rules.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// The untranslated English template, with `{0}`-style placeholders intact.
    #[must_use]
    pub const fn text(&self) -> &'static str {
        self.text
    }

    /// Rarely-set properties.
    #[must_use]
    pub const fn flags(&self) -> MessageFlags {
        self.flags
    }

    /// Substitute positional arguments into the template.
    ///
    /// `{0}` is replaced by `args[0]`, and so on. A placeholder with no
    /// corresponding argument is left as written rather than panicking: a
    /// half-rendered message is far more useful in a bug report than a crash in
    /// the diagnostic path.
    #[must_use]
    pub fn format(&self, args: &[&str]) -> String {
        format_template(self.text, args)
    }
}

/// Substitute `{n}` placeholders positionally.
fn format_template(template: &str, args: &[&str]) -> String {
    // Fast path: most messages have no placeholders at all.
    if !template.contains('{') {
        return template.to_string();
    }

    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // Unmatched brace; the rest is literal.
            out.push('{');
            rest = after;
            break;
        };
        let body = &after[..close];
        match body.parse::<usize>() {
            Ok(index) if index < args.len() => out.push_str(args[index]),
            // Unknown index or non-numeric body: emit verbatim.
            _ => {
                out.push('{');
                out.push_str(body);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Look up a message by its numeric code.
///
/// **Codes are not unique.** Eight codes are shared by two messages each (5074,
/// 5090, 6048, 6353, 6401, 6420, 8030, 9019); this returns one of them, chosen
/// deterministically by key order. Identify a message by [`Message::key`] when it
/// matters. Binary search over [`messages::ALL`], which the generator emits sorted
/// by code.
#[must_use]
pub fn by_code(code: u32) -> Option<&'static Message> {
    let index = messages::ALL.partition_point(|m| m.code() < code);
    messages::ALL.get(index).copied().filter(|m| m.code() == code)
}

/// Look up a message by its localization key.
///
/// Linear; used only by tooling that reads keys back from data files, never on a
/// hot path.
#[must_use]
pub fn by_key(key: &str) -> Option<&'static Message> {
    messages::ALL.iter().copied().find(|m| m.key() == key)
}

/// A diagnostic: a message, its location, and its arguments.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The message template.
    pub message: &'static Message,
    /// Where in the file it applies.
    pub span: Span,
    /// Arguments substituted into the template.
    pub args: Vec<String>,
}

impl Diagnostic {
    /// Create a diagnostic with no arguments.
    #[must_use]
    pub fn new(message: &'static Message, span: Span) -> Self {
        Self { message, span, args: Vec::new() }
    }

    /// Create a diagnostic with substitution arguments.
    #[must_use]
    pub fn with_args(
        message: &'static Message,
        span: Span,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        Self { message, span, args: args.into_iter().collect() }
    }

    /// The rendered message text.
    #[must_use]
    pub fn text(&self) -> String {
        let refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
        self.message.format(&refs)
    }

    /// The code as it appears in output, e.g. `TS1002`.
    #[must_use]
    pub fn code(&self) -> String {
        format!("TS{}", self.message.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_substituted_positionally() {
        assert_eq!(format_template("'{0}' expected.", &[";"]), "';' expected.");
        assert_eq!(
            format_template(
                "The parser expected to find a '{1}' to match the '{0}' token here.",
                &["(", ")"]
            ),
            "The parser expected to find a ')' to match the '(' token here."
        );
    }

    #[test]
    fn a_missing_argument_leaves_the_placeholder_rather_than_panicking() {
        // A crash in the diagnostic path would replace a useful error with a
        // useless one.
        assert_eq!(format_template("'{0}' expected.", &[]), "'{0}' expected.");
        assert_eq!(format_template("{0} and {1}", &["a"]), "a and {1}");
    }

    #[test]
    fn text_without_placeholders_is_returned_verbatim() {
        assert_eq!(
            format_template("Unterminated string literal.", &[]),
            "Unterminated string literal."
        );
        assert_eq!(
            format_template("Unterminated string literal.", &["x"]),
            "Unterminated string literal."
        );
    }

    #[test]
    fn malformed_braces_do_not_panic() {
        assert_eq!(format_template("a { b", &[]), "a { b");
        assert_eq!(format_template("a {notanumber} b", &["x"]), "a {notanumber} b");
        assert_eq!(format_template("{", &[]), "{");
    }

    #[test]
    fn lookup_by_code_and_key_agree() {
        let m = by_code(1002).expect("1002 exists");
        assert_eq!(m.key(), "Unterminated_string_literal_1002");
        assert_eq!(by_key("Unterminated_string_literal_1002").map(Message::code), Some(1002));
        assert!(by_code(999_999).is_none());
    }

    #[test]
    fn all_is_sorted_by_code_so_binary_search_is_valid() {
        let codes: Vec<u32> = messages::ALL.iter().map(|m| m.code()).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "messages::ALL must be sorted by code");
    }

    #[test]
    fn keys_are_unique_even_though_codes_are_not() {
        // Codes are shared by 8 pairs of messages; keys are the real identity.
        let mut keys: Vec<&str> = messages::ALL.iter().map(|m| m.key()).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "diagnostic keys must be unique");
    }

    #[test]
    fn by_code_finds_the_first_message_for_a_shared_code() {
        // 9019 is shared; the lookup must return one of the two, not None.
        let m = by_code(9019).expect("9019 exists");
        assert_eq!(m.code(), 9019);
    }

    #[test]
    fn diagnostic_renders_code_and_text() {
        let d = Diagnostic::with_args(&messages::_0_EXPECTED, Span::new(0, 1), [";".to_string()]);
        assert_eq!(d.code(), "TS1005");
        assert_eq!(d.text(), "';' expected.");
    }
}
