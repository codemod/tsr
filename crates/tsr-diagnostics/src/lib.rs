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

mod compare;
pub mod format;
mod generated;

pub use compare::{
    compact_and_merge_related_infos, compare_diagnostics, equal_diagnostics,
    equal_diagnostics_no_related_info, sort_and_deduplicate_diagnostics,
    sort_and_deduplicate_located_diagnostics,
};

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

/// A diagnostic with ordered explanations and related locations.
///
/// Ported from typescript-go's `Diagnostic` (`internal/ast/diagnostic.go`).
/// Rare tree/location state is boxed so head-only diagnostics allocate no extra storage.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The message template.
    pub message: &'static Message,
    /// Where in the file it applies.
    pub span: Span,
    /// Arguments substituted into the template.
    pub args: Vec<String>,
    category: Category,
    reports_unnecessary: bool,
    reports_deprecated: bool,
    details: Option<Box<DiagnosticDetails>>,
}

#[derive(Debug, Clone, Default)]
struct DiagnosticDetails {
    file: Option<std::sync::Arc<DiagnosticFile>>,
    chain: Vec<Diagnostic>,
    related: std::sync::Arc<Vec<Diagnostic>>,
    skipped_on_no_emit: bool,
}

impl Diagnostic {
    /// Create a diagnostic with no arguments.
    #[must_use]
    pub fn new(message: &'static Message, span: Span) -> Self {
        Self::with_args(message, span, std::iter::empty())
    }

    /// Create a diagnostic with substitution arguments.
    #[must_use]
    pub fn with_args(
        message: &'static Message,
        span: Span,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            message,
            span,
            args: args.into_iter().collect(),
            category: message.category(),
            reports_unnecessary: message.flags().contains(MessageFlags::REPORTS_UNNECESSARY),
            reports_deprecated: message.flags().contains(MessageFlags::REPORTS_DEPRECATED),
            details: None,
        }
    }

    /// Ported from typescript-go's `NewDiagnosticChain` (`internal/ast/diagnostic.go`).
    /// The parent inherits the child's location and shares its related information.
    #[must_use]
    pub fn new_chain(
        child: Option<Self>,
        message: &'static Message,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut parent =
            Self::with_args(message, child.as_ref().map_or(Span::new(0, 0), |d| d.span), args);
        if let Some(child) = child {
            let details = parent.details.get_or_insert_with(Default::default);
            if let Some(child_details) = &child.details {
                details.file.clone_from(&child_details.file);
                details.related.clone_from(&child_details.related);
            }
            details.chain.push(child);
        }
        parent
    }

    /// Ordered child explanations (`Diagnostic.MessageChain`).
    #[must_use]
    pub fn message_chain(&self) -> &[Self] {
        self.details.as_ref().map_or(&[], |details| &details.chain)
    }

    /// Append one explanation (`Diagnostic.AddMessageChain`); absent children do nothing.
    pub fn add_message_chain(&mut self, child: Option<Self>) -> &mut Self {
        if let Some(child) = child {
            self.details.get_or_insert_with(Default::default).chain.push(child);
        }
        self
    }

    /// Replace explanations (`Diagnostic.SetMessageChain`).
    pub fn set_message_chain(&mut self, children: Vec<Self>) -> &mut Self {
        self.details.get_or_insert_with(Default::default).chain = children;
        self
    }

    /// Ordered related diagnostics (`Diagnostic.RelatedInformation`).
    #[must_use]
    pub fn related_information(&self) -> &[Self] {
        self.details.as_ref().map_or(&[], |details| &details.related)
    }

    /// Replace related information (`Diagnostic.SetRelatedInfo`).
    pub fn set_related_information(&mut self, related: std::sync::Arc<Vec<Self>>) -> &mut Self {
        self.details.get_or_insert_with(Default::default).related = related;
        self
    }

    /// Append related information (`Diagnostic.AddRelatedInfo`).
    pub fn add_related_information(&mut self, related: Option<Self>) -> &mut Self {
        if let Some(related) = related {
            let details = self.details.get_or_insert_with(Default::default);
            std::sync::Arc::make_mut(&mut details.related).push(related);
        }
        self
    }

    /// Runtime category (`Diagnostic.Category`), initialized by `NewDiagnostic`.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// Update runtime severity (`Diagnostic.SetCategory`).
    pub fn set_category(&mut self, category: Category) -> &mut Self {
        self.category = category;
        self
    }

    /// Runtime unused-code payload (`Diagnostic.ReportsUnnecessary`).
    #[must_use]
    pub const fn reports_unnecessary(&self) -> bool {
        self.reports_unnecessary
    }

    /// Set the explicit payload supplied by native diagnostic deserialization.
    pub fn set_reports_unnecessary(&mut self, reports_unnecessary: bool) -> &mut Self {
        self.reports_unnecessary = reports_unnecessary;
        self
    }

    /// Runtime deprecation payload (`Diagnostic.ReportsDeprecated`).
    #[must_use]
    pub const fn reports_deprecated(&self) -> bool {
        self.reports_deprecated
    }

    /// Set the explicit payload supplied by native diagnostic deserialization.
    pub fn set_reports_deprecated(&mut self, reports_deprecated: bool) -> &mut Self {
        self.reports_deprecated = reports_deprecated;
        self
    }

    /// Per-diagnostic emit policy (`Diagnostic.SkippedOnNoEmit`).
    /// This state is independent of the generated message's flags.
    #[must_use]
    pub fn skipped_on_no_emit(&self) -> bool {
        self.details.as_ref().is_some_and(|details| details.skipped_on_no_emit)
    }

    /// Mark this diagnostic as omitted from semantic diagnostics under noEmit.
    /// Ported from typescript-go's `Diagnostic.SetSkippedOnNoEmit`
    /// (`internal/ast/diagnostic.go`). Clone retains this flag; new chain parents
    /// do not inherit it from their child.
    pub fn set_skipped_on_no_emit(&mut self) -> &mut Self {
        self.details.get_or_insert_with(Default::default).skipped_on_no_emit = true;
        self
    }

    /// The source file for independently located related information.
    #[must_use]
    pub fn file(&self) -> Option<&DiagnosticFile> {
        self.details.as_ref().and_then(|details| details.file.as_deref())
    }

    /// Attach a Program-owned source image (`Diagnostic.SetFile`).
    pub fn set_file(&mut self, file: std::sync::Arc<DiagnosticFile>) -> &mut Self {
        self.details.get_or_insert_with(Default::default).file = Some(file);
        self
    }

    /// The localized head text, not the flattened tree (`Diagnostic.Localize`).
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
