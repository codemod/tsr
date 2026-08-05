//! Rendering a type as the string a `.types` baseline compares against.
//!
//! Ported from `Checker.typeToString` (`internal/checker/checker.go`), which
//! delegates to the node builder and then to the printer. Only the intrinsic and
//! literal cases are ported; everything else needs types that do not exist yet.
//!
//! # Why this is more delicate than it looks
//!
//! Every `.types` baseline compares **whole lines, verbatim**. So the rendering is
//! not a debugging convenience — it is the thing under test, and a difference of
//! one character fails a case just as surely as an outright wrong type. The two
//! places a port drifts here are string escaping and number formatting, and both
//! are handled explicitly below rather than left to `Display`.

use crate::types::{Type, TypeData};

/// Render a type.
///
/// Ported from `Checker.typeToString`.
#[must_use]
pub fn type_to_string(ty: &Type) -> String {
    match &ty.data {
        TypeData::Intrinsic { name } => (*name).to_string(),
        TypeData::StringLiteral(value) => quote(value),
        TypeData::NumberLiteral(text) => text.clone(),
        TypeData::BigIntLiteral(text) => format!("{text}n"),
        TypeData::BooleanLiteral(value) => value.to_string(),
    }
}

/// Render a string literal type's value the way TypeScript prints one.
///
/// Double quotes, with `\`, `"`, and the C0 controls escaped. TypeScript's own
/// `escapeString` is what this mirrors; the set here covers what appears in the
/// corpus and deliberately stops short of the full table — a character outside it
/// is emitted raw, which is visible as a baseline mismatch rather than as silent
/// corruption. `bd tsr-4sc.1`.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Normalise a numeric literal's source text to the form TypeScript prints.
///
/// TypeScript prints a numeric literal *type* as the ECMAScript `Number::toString`
/// of its value, not as it was written: `1.0`, `1e0` and `0x1` are all the type
/// `1`. So the source text cannot be used directly.
///
/// **What is ported:** decimal integers and fractions, hex/octal/binary literals,
/// numeric separators, and exponents whose result stays inside the range where
/// Rust's `f64` formatting and `Number::toString` agree.
///
/// **What is not:** the exponential-notation boundaries. `Number::toString`
/// switches to exponential at `1e21` and below `1e-6`, and Rust's `{}` does not,
/// so `1e21` prints `1000000000000000000000` here and `1e+21` upstream. Left
/// unported rather than half-ported, and tracked as `bd tsr-4sc.1`: the corpus
/// will show whether it matters before it is guessed at.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "ECMAScript numbers *are* f64, so narrowing an integer literal to f64 \
              is the specified behaviour rather than a defect: `0x20000000000001` is \
              genuinely the same number as `0x20000000000000` in TypeScript, and \
              upstream prints it that way."
)]
pub fn normalise_number(text: &str) -> String {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();

    let value = if let Some(rest) =
        cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X"))
    {
        u128::from_str_radix(rest, 16).ok().map(|v| v as f64)
    } else if let Some(rest) = cleaned.strip_prefix("0o").or_else(|| cleaned.strip_prefix("0O")) {
        u128::from_str_radix(rest, 8).ok().map(|v| v as f64)
    } else if let Some(rest) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B")) {
        u128::from_str_radix(rest, 2).ok().map(|v| v as f64)
    } else {
        cleaned.parse::<f64>().ok()
    };

    // An unparseable literal keeps its source text: the scanner already reported
    // it, and inventing a value here would turn a syntax error into a wrong type.
    let Some(value) = value else { return cleaned };

    // Exact integrality is the question, so an epsilon would be wrong: `1.0` must
    // print `1` and `1.0000000000000002` must not.
    #[allow(clippy::float_cmp, reason = "exact integrality is the intended test")]
    let is_integral = value == value.trunc();
    if is_integral && value.abs() < 1e21 {
        // Integral values print without a fractional part, which `{}` on f64 also
        // does — but only for values that fit; the guard above is what keeps this
        // inside the agreed range.
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_literal_type_is_double_quoted_and_escaped() {
        assert_eq!(quote("a"), "\"a\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn a_numeric_literal_type_prints_its_value_not_its_spelling() {
        // The whole reason source text cannot be used directly.
        assert_eq!(normalise_number("1"), "1");
        assert_eq!(normalise_number("1.0"), "1");
        assert_eq!(normalise_number("1e0"), "1");
        assert_eq!(normalise_number("0x1"), "1");
        assert_eq!(normalise_number("0b101"), "5");
        assert_eq!(normalise_number("0o17"), "15");
        assert_eq!(normalise_number("1_000"), "1000");
        assert_eq!(normalise_number("1.5"), "1.5");
    }

    #[test]
    fn an_unparseable_literal_keeps_its_text_rather_than_inventing_a_value() {
        assert_eq!(normalise_number("not-a-number"), "not-a-number");
    }
}
