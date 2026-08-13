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

use crate::{
    flags::TypeFlags,
    types::{Type, TypeData},
};

/// Render a type.
///
/// Ported from `Checker.typeToString`.
#[must_use]
pub fn type_to_string(ty: &Type) -> String {
    match &ty.data {
        TypeData::Intrinsic { name } => (*name).to_string(),
        TypeData::StringLiteral(value) => quote(value),
        TypeData::BigIntLiteral(text) => format!("{text}n"),
        TypeData::BooleanLiteral(value) => value.to_string(),
        // One body, three unrelated reasons — kept as one arm because clippy's
        // `match_same_arms` is a workspace gate and splitting them to hold three
        // comments would fail it:
        //
        // - a **number** prints its normalised text, since `1.0` and `0x1` are
        //   both the type `1` and source text cannot be used;
        // - a **named** type prints the form computed when it was created — see
        //   `TypeData::Named` for why that is a renderer divergence rather than a
        //   data-model one;
        // - a **union** prints a form computed when it was *built*, because that
        //   form depends on the constituents and this function takes a single
        //   `Type` with no way back to the store. See `TypeData::Union::text`.
        TypeData::NumberLiteral(text)
        | TypeData::Named { text, .. }
        // - an **anonymous** object type prints `typeof C` or its call
        //   signature, computed at creation for the same reason: rendering a
        //   signature needs the parameter and return *types*, and this function
        //   has only a `Type`.
        | TypeData::Union { text, .. }
        | TypeData::Intersection { text, .. }
        | TypeData::Anonymous { text, .. } => text.clone(),
    }
}

/// Whether a type prints as a **single token** rather than as its own
/// structure — a name, or a keyword.
///
/// The question every parenthesiser here is really asking. Upstream never asks
/// it, because it decides on the *node kind* the builder emitted
/// (`GetTypeNodePrecedence`, `ast/precedence.go:655`) and a named union is a
/// `TypeReferenceNode` while an anonymous one is a `UnionTypeNode` — two kinds,
/// two precedences, no predicate needed. This port computes text at creation and
/// has only the type, so the same distinction has to be recovered from what the
/// type carries.
///
/// Two ways a `Union` or `Intersection` prints as one token:
///
/// - **A type alias names it.** `type Tagged = A & B` prints `Tagged`, which the
///   node builder emits as a `TypeReferenceNode` — `NonArray`, the highest
///   precedence, never parenthesised.
/// - **It is `boolean`.** `booleanType` is the union `false | true`
///   ([`crate::unions::create_boolean_type`]) and prints as the keyword, which
///   the builder emits as `KindBooleanKeyword`.
///
/// Both were found by parenthesising without them: the first cost 19 lines
/// across three cases (`(TaggedString1) | (TaggedString2)` for
/// `TaggedString1 | TaggedString2`), the second is pinned by a test rather than
/// by the corpus.
pub(crate) fn prints_as_a_single_token(ty: &Type) -> bool {
    match &ty.data {
        TypeData::Union { symbol, .. } | TypeData::Intersection { symbol, .. } => {
            symbol.is_some() || ty.flags.contains(TypeFlags::BOOLEAN)
        }
        _ => false,
    }
}

/// Render a string literal type's value the way TypeScript prints one.
///
/// Double quotes, with `\`, `"`, and the C0 controls escaped. TypeScript's own
/// `escapeString` is what this mirrors; the set here covers what appears in the
/// corpus and deliberately stops short of the full table — a character outside it
/// is emitted raw, which is visible as a baseline mismatch rather than as silent
/// corruption. `bd tsr-4sc.1`.
///
/// `pub(crate)` so an object literal's non-identifier property name — `{ "a-b": 1 }`
/// printing `{ "a-b": number; }` — quotes through *this* table rather than a second
/// copy of it. That matters precisely **because** the table is incomplete: two
/// copies would both have to be corrected when `bd tsr-4sc.1` lands, and nothing
/// would fail if only one were. Same argument `render_object_type` records for
/// keeping one object renderer — separate ones are how a port ends up printing
/// `{ a: string }` in one position and `{ a: string; }` in another.
pub(crate) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // The rest of `escapedCharsMap` (`printer/utilities.go:41`) plus
            // the NUL rule (`:150`): `\0` prints `\0` unless a digit follows —
            // then `\x00`, so the result cannot re-parse as an octal. The
            // corpus pinned these as the `templateString*Escapes` W2 rows
            // (`"\t\n\v\f\r"` wanted where `` printed).
            '\u{0B}' => out.push_str("\\v"),
            '\u{0C}' => out.push_str("\\f"),
            '\u{08}' => out.push_str("\\b"),
            '\0' => {
                if chars.peek().is_some_and(char::is_ascii_digit) {
                    out.push_str("\\x00");
                } else {
                    out.push_str("\\0");
                }
            }
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

    // §276: a radix prefix with NO digits — `0x`, `0b`, `0o` — is upstream's
    // scanner-recovery ZERO: `scanNumber` reports "Hexadecimal digit
    // expected" and keeps value 0, so `0x` records `>0x : 0`
    // (`scannerS7.8.3_A6.1_T1`). An empty digit run parses as 0 rather than
    // falling to the keep-the-source arm below, which exists for literals
    // whose VALUE is unrepresentable, not unreadable.
    let radix_value = |rest: &str, radix: u32| {
        if rest.is_empty() {
            Some(0.0)
        } else {
            u128::from_str_radix(rest, radix).ok().map(|v| v as f64)
        }
    };
    let value = if let Some(rest) =
        cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X"))
    {
        radix_value(rest, 16)
    } else if let Some(rest) = cleaned.strip_prefix("0o").or_else(|| cleaned.strip_prefix("0O")) {
        radix_value(rest, 8)
    } else if let Some(rest) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B")) {
        radix_value(rest, 2)
    } else if cleaned.len() > 1
        && cleaned.starts_with('0')
        && cleaned.bytes().all(|b| b.is_ascii_digit())
        && !cleaned.contains(['8', '9'])
    {
        // §150 rider (`checker-notes-narrow.md`): the LEGACY octal literal —
        // leading zero, every digit octal — reads base 8 (`055` is `45`),
        // the same branch `tsr_core::jsnum::numeric_value` already carries.
        // A non-octal digit (`08`, `09`) or a `.` falls through to decimal.
        u128::from_str_radix(&cleaned[1..], 8).ok().map(|v| v as f64)
    } else {
        // §295, §276's exponent sibling: a DANGLING exponent — `1e`, `1e+`,
        // `1.0e_` (the separator strips to `1.0e`) — is upstream's scanner
        // recovery keeping the mantissa: "Digit expected" reports and the
        // value is the mantissa's (`scannerES3NumericLiteral4/6`,
        // `parser.numericSeparators.decmialNegative` 49/50 all record `1`).
        let trimmed = cleaned
            .strip_suffix(['+', '-'])
            .unwrap_or(&cleaned)
            .strip_suffix(['e', 'E'])
            .map(str::to_string);
        match trimmed {
            Some(mantissa) if !mantissa.is_empty() => mantissa.parse::<f64>().ok(),
            _ => cleaned.parse::<f64>().ok(),
        }
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
        assert_eq!(normalise_number("055"), "45");
        assert_eq!(normalise_number("08"), "8");
        assert_eq!(normalise_number("0.5"), "0.5");
        assert_eq!(normalise_number("1_000"), "1000");
        assert_eq!(normalise_number("1.5"), "1.5");
    }

    #[test]
    fn an_unparseable_literal_keeps_its_text_rather_than_inventing_a_value() {
        assert_eq!(normalise_number("not-a-number"), "not-a-number");
    }
}
