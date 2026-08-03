//! Generates `crates/tsr-diagnostics/src/generated/messages.rs`.
//!
//! Input is TypeScript's own `diagnosticMessages.json` (2,130 messages) plus
//! typescript-go's `extraDiagnosticMessages.json` (34 tsgo-specific ones). This is
//! Category A in `docs/adr/0007-generated-code-policy.md`: derived from external
//! data, so the generator is ported.
//!
//! Ported from typescript-go's `internal/diagnostics/generate.go`.
//!
//! # Names and keys are different things
//!
//! The **key** (`_0_expected_1005`) is load-bearing: it is the localization lookup
//! key and appears in upstream's data files, so it is reproduced byte-for-byte
//! using upstream's exact mangling rules.
//!
//! The **identifier** is ours. Go needs a capitalized name to export, hence
//! upstream's `X_` prefix hack; Rust does not, so the prefix is dropped and the
//! name upper-snake-cased.

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
};

use anyhow::{Result, bail};
use serde::Deserialize;

/// One entry in `diagnosticMessages.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct RawMessage {
    /// `Error`, `Message`, `Suggestion`, or `Warning`.
    pub category: String,
    /// The stable numeric code, e.g. 1002.
    pub code: u32,
    /// Reported as an unused/unnecessary hint rather than a hard error.
    #[serde(default)]
    pub reports_unnecessary: bool,
    /// Reported as a deprecation.
    #[serde(default)]
    pub reports_deprecated: bool,
    /// The misspelling is upstream's, inherited from the original TypeScript
    /// source; renaming it here would break parsing of the input file.
    #[serde(default, rename = "elidedInCompatabilityPyramid")]
    pub elided_in_compatibility_pyramid: bool,
}

/// A message ready to emit.
struct Message {
    ident: String,
    key: String,
    code: u32,
    category: String,
    text: String,
    reports_unnecessary: bool,
    reports_deprecated: bool,
    elided: bool,
}

/// Apply upstream's identifier mangling.
///
/// Mirrors `convertPropertyName` in `internal/diagnostics/generate.go`. Returns
/// the Go-style name, from which both the key and our Rust identifier derive.
fn mangle(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '*' => out.push_str("_Asterisk"),
            '/' => out.push_str("_Slash"),
            ':' => out.push_str("_Colon"),
            c if c.is_alphabetic() || c.is_numeric() => out.push(c),
            _ => out.push('_'),
        }
    }

    // Collapse runs of underscores.
    let mut collapsed = String::with_capacity(out.len());
    let mut last_underscore = false;
    for ch in out.chars() {
        if ch == '_' {
            if !last_underscore {
                collapsed.push(ch);
            }
            last_underscore = true;
        } else {
            collapsed.push(ch);
            last_underscore = false;
        }
    }

    // Upstream: `^_+(\D)` -> `$1`. A *leading* underscore is dropped only when
    // followed by a non-digit; `'{0}' expected.` therefore keeps its underscore
    // and stays `_0_expected`, which would otherwise not be a valid identifier.
    //
    // Critically this only ever *removes* an underscore. Text that already begins
    // with a digit (`"4, unless --singleThreaded is passed."`) has none to keep,
    // and must not gain one.
    let leading_kept = match collapsed.strip_prefix('_') {
        Some(rest) if !rest.starts_with(|c: char| c.is_ascii_digit()) => rest.to_string(),
        _ => collapsed,
    };

    leading_kept.trim_end_matches('_').to_string()
}

/// The localization key: the mangled name truncated to 100 chars, plus the code.
fn make_key(mangled: &str, code: u32) -> String {
    let truncated: String = mangled.chars().take(100).collect();
    format!("{truncated}_{code}")
}

/// Our Rust identifier: upper-snake, with Go's export hack removed.
///
/// The mangled name is the *key*'s basis and must not be altered; the identifier
/// is a separate concern. Where the two conflict, the identifier bends: a name
/// beginning with a digit (`4_unless_singleThreaded_is_passed`) is not a legal
/// Rust identifier, so it gains a leading underscore. Go solves the same problem
/// with its `X_` prefix, for the opposite reason — it needs a capital to export.
fn rust_ident(mangled: &str) -> String {
    let upper = mangled.to_uppercase();
    if upper.starts_with(|c: char| c.is_ascii_digit()) { format!("_{upper}") } else { upper }
}

/// Generate the messages module.
pub fn generate(
    base: &BTreeMap<String, RawMessage>,
    extra: &BTreeMap<String, RawMessage>,
) -> Result<String> {
    // Extra messages override base ones on collision, matching upstream's merge.
    let mut merged: BTreeMap<&String, &RawMessage> = BTreeMap::new();
    for (text, msg) in base.iter().chain(extra.iter()) {
        merged.insert(text, msg);
    }

    let mut messages: Vec<Message> = Vec::with_capacity(merged.len());
    for (text, raw) in merged {
        let mangled = mangle(text);
        if mangled.is_empty() {
            bail!("message {text:?} mangles to an empty identifier");
        }
        messages.push(Message {
            ident: rust_ident(&mangled),
            key: make_key(&mangled, raw.code),
            code: raw.code,
            category: raw.category.clone(),
            text: text.clone(),
            reports_unnecessary: raw.reports_unnecessary,
            reports_deprecated: raw.reports_deprecated,
            elided: raw.elided_in_compatibility_pyramid,
        });
    }

    // Diagnostic codes are NOT unique: 8 codes are shared by two messages each
    // (5074, 5090, 6048, 6353, 6401, 6420, 8030, 9019).
    //
    // Upstream collapses those, because `readRawMessages` builds a
    // `map[int]*diagnosticMessage` keyed by code — and since Go map iteration is
    // randomised, *which* of the pair survives is nondeterministic across
    // regenerations. We keep both instead: no information is lost, the output is
    // deterministic, and messages are properly identified by key rather than by
    // code anyway. Our catalogue is therefore a strict superset of upstream's,
    // which the conformance test asserts.
    let mut shared_codes: HashMap<u32, usize> = HashMap::new();
    for m in &messages {
        *shared_codes.entry(m.code).or_default() += 1;
    }
    let shared = shared_codes.values().filter(|&&n| n > 1).count();
    if shared > 0 {
        println!("  {shared} diagnostic code(s) shared by more than one message; keeping all");
    }

    resolve_ident_collisions(&mut messages)?;

    // Sorted by code, ties broken by key, so `by_code` can binary search and the
    // output is byte-stable across runs.
    messages.sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.key.cmp(&b.key)));

    let mut out = String::with_capacity(512 * 1024);
    out.push_str(
        "//! TypeScript diagnostic messages.\n\
         //!\n\
         //! @generated by `cargo xtask codegen` from TypeScript's\n\
         //! `diagnosticMessages.json` and typescript-go's\n\
         //! `extraDiagnosticMessages.json`. Do not edit by hand.\n\
         //!\n\
         //! Corresponds to typescript-go's\n\
         //! `internal/diagnostics/diagnostics_generated.go`.\n\n\
         #![allow(clippy::doc_markdown, clippy::unreadable_literal)]\n\n\
         use crate::{Category, Message, MessageFlags};\n\n",
    );

    for m in &messages {
        let mut flags = Vec::new();
        if m.reports_unnecessary {
            flags.push("MessageFlags::REPORTS_UNNECESSARY");
        }
        if m.reports_deprecated {
            flags.push("MessageFlags::REPORTS_DEPRECATED");
        }
        if m.elided {
            flags.push("MessageFlags::ELIDED_IN_COMPATIBILITY_PYRAMID");
        }
        let flags = if flags.is_empty() {
            "MessageFlags::empty()".to_string()
        } else {
            flags.join(".union(") + &")".repeat(flags.len() - 1)
        };

        writeln!(
            out,
            "/// `{}`\n\
             pub static {}: Message = Message::new({}, Category::{}, {:?}, {:?}, {});",
            escape_doc(&m.text),
            m.ident,
            m.code,
            m.category,
            m.key,
            m.text,
            flags,
        )?;
        out.push('\n');
    }

    // A sorted table backs both lookups: binary search rather than a 2,162-arm
    // match, which keeps compile time sane and the generated file readable.
    writeln!(
        out,
        "/// Every diagnostic message, sorted by code.\n\
         ///\n\
         /// Sorted so [`crate::by_code`] can binary search; the ordering is asserted\n\
         /// in a test rather than merely assumed.\n\
         pub static ALL: &[&Message] = &["
    )?;
    for m in &messages {
        writeln!(out, "    &{},", m.ident)?;
    }
    out.push_str("];\n");

    Ok(out)
}

/// Upper-snake-casing can merge two upstream names that differed only in case.
///
/// At the pinned commit exactly one group collides: `file` (6025) and `FILE`
/// (6035). Every member of a colliding group gets a `_<code>` suffix — suffixing
/// only the later one would make which name is "plain" depend on iteration order.
fn resolve_ident_collisions(messages: &mut [Message]) -> Result<()> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in messages.iter() {
        *counts.entry(m.ident.clone()).or_default() += 1;
    }

    let colliding: Vec<String> =
        counts.iter().filter(|&(_, &c)| c > 1).map(|(name, _)| name.clone()).collect();

    for m in messages.iter_mut() {
        if colliding.contains(&m.ident) {
            m.ident = format!("{}_{}", m.ident, m.code);
        }
    }

    // Suffixing must actually have resolved it.
    let mut seen: HashMap<&str, u32> = HashMap::new();
    for m in messages.iter() {
        if let Some(previous) = seen.insert(&m.ident, m.code) {
            bail!(
                "identifier {} still collides after disambiguation (codes {} and {})",
                m.ident,
                previous,
                m.code
            );
        }
    }

    if !colliding.is_empty() {
        let mut names = colliding;
        names.sort();
        println!("  disambiguated {} colliding identifier(s): {names:?}", names.len());
    }
    Ok(())
}

/// Make message text safe inside a `///` doc comment.
fn escape_doc(text: &str) -> String {
    text.replace('\n', " ").replace('\r', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangling_matches_upstream_examples() {
        // Taken from `internal/diagnostics/diagnostics_generated.go`.
        assert_eq!(mangle("Unterminated string literal."), "Unterminated_string_literal");
        assert_eq!(mangle("'{0}' expected."), "_0_expected");
        assert_eq!(mangle("'*/' expected."), "Asterisk_Slash_expected");
    }

    #[test]
    fn text_beginning_with_a_digit_gains_no_leading_underscore() {
        // Regression: an earlier version added one, producing
        // `_4_unless_singleThreaded_is_passed` where upstream has
        // `4_unless_singleThreaded_is_passed`. Caught by conformance, not by
        // inspection — the two forms look equally plausible.
        assert_eq!(
            mangle("4, unless --singleThreaded is passed."),
            "4_unless_singleThreaded_is_passed"
        );
        assert_eq!(mangle("1 reference"), "1_reference");
    }

    #[test]
    fn identifiers_beginning_with_a_digit_are_made_legal_without_touching_the_key() {
        let mangled = mangle("4, unless --singleThreaded is passed.");
        assert_eq!(make_key(&mangled, 100_004), "4_unless_singleThreaded_is_passed_100004");
        assert_eq!(rust_ident(&mangled), "_4_UNLESS_SINGLETHREADED_IS_PASSED");
    }

    #[test]
    fn a_leading_underscore_is_dropped_before_a_letter() {
        // `^_+(\D)` strips it; only a following digit preserves it.
        assert_eq!(mangle("'foo' expected."), "foo_expected");
    }

    #[test]
    fn keys_match_upstream() {
        assert_eq!(make_key(&mangle("'{0}' expected."), 1005), "_0_expected_1005");
        assert_eq!(
            make_key(&mangle("Unterminated string literal."), 1002),
            "Unterminated_string_literal_1002"
        );
    }

    #[test]
    fn long_keys_truncate_the_name_but_keep_the_code() {
        let long = "a".repeat(150);
        let key = make_key(&long, 42);
        assert_eq!(key, format!("{}_42", "a".repeat(100)));
    }
}
