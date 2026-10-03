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
        | TypeData::EnumLiteral { text, .. }
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

/// `escapeNonAsciiString` (internal/printer/utilities.go) for synthesized
/// property names. Supplementary characters use UTF-16 surrogate escapes.
pub(crate) fn quote_ascii(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for character in quote(value).chars() {
        if character.is_ascii() {
            out.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                let _ = write!(out, "\\u{unit:04X}");
            }
        }
    }
    out
}

/// Normalise a numeric literal's source text to the form TypeScript prints.
///
/// TypeScript prints a numeric literal *type* as the ECMAScript `Number::toString`
/// of its value, not as it was written: `1.0`, `1e0` and `0x1` are all the type
/// `1`. So the source text cannot be used directly.
///
/// Decimal integers and fractions, radix literals, numeric separators, and the
/// ECMAScript exponential-notation boundaries are ported. Rust and ECMAScript
/// use the same shortest round-tripping digits but different notation cutoffs,
/// so [`number_to_ecmascript_string`] adjusts only that presentation choice.
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
        number_to_ecmascript_string(value)
    }
}

/// Apply ECMAScript `Number::toString`'s notation boundaries to Rust's shortest
/// round-tripping decimal digits.
fn number_to_ecmascript_string(value: f64) -> String {
    let raw = value.to_string();
    let absolute = value.abs();
    if !value.is_finite() || absolute == 0.0 || (1e-6..1e21).contains(&absolute) {
        return raw;
    }

    let (sign, magnitude) = raw.strip_prefix('-').map_or(("", raw.as_str()), |rest| ("-", rest));
    let Some((first, exponent, tail)) = scientific_parts(magnitude) else { return raw };
    let tail = tail.trim_end_matches('0');
    let mantissa = if tail.is_empty() { first.to_string() } else { format!("{first}.{tail}") };
    let exponent_sign = if exponent >= 0 { "+" } else { "" };
    format!("{sign}{mantissa}e{exponent_sign}{exponent}")
}

/// Split a non-zero, non-exponential decimal into scientific components.
fn scientific_parts(decimal: &str) -> Option<(char, isize, String)> {
    if let Some(fraction) = decimal.strip_prefix("0.") {
        let first_index = fraction.find(|character| character != '0')?;
        let exponent = -isize::try_from(first_index).ok()? - 1;
        let mut digits = fraction[first_index..].chars();
        Some((digits.next()?, exponent, digits.collect()))
    } else {
        let digits: String = decimal.chars().filter(|character| *character != '.').collect();
        let mut digits = digits.chars();
        let first = digits.next()?;
        let tail: String = digits.collect();
        let exponent = isize::try_from(tail.len()).ok()?;
        Some((first, exponent, tail))
    }
}

/// Convert a bigint literal spelling to the decimal value identity TypeScript
/// stores in a bigint literal type.
///
/// This ports `jsnum.ParsePseudoBigInt` (`internal/jsnum/pseudobigint.go`) rather
/// than parsing through a machine integer. Decimal multiply/add makes the result
/// arbitrary-precision without adding a checker-wide bigint dependency.
#[must_use]
pub fn normalise_bigint(text: &str) -> String {
    let (negative, text) = text.strip_prefix('-').map_or((false, text), |rest| (true, rest));
    let cleaned: String = text
        .strip_suffix('n')
        .unwrap_or(text)
        .chars()
        .filter(|character| *character != '_')
        .collect();
    let (radix, digits) = if let Some(digits) =
        cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B"))
    {
        (2, digits)
    } else if let Some(digits) = cleaned.strip_prefix("0o").or_else(|| cleaned.strip_prefix("0O")) {
        (8, digits)
    } else if let Some(digits) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
        (16, digits)
    } else {
        (10, cleaned.as_str())
    };

    // Little-endian decimal digits. Each source digit multiplies the current
    // value by at most 16, so every intermediate fits comfortably in u16.
    let mut decimal = vec![0_u8];
    for character in digits.chars() {
        let Some(value) = character.to_digit(radix) else {
            // Scanner recovery can leave malformed source text. Preserve the
            // old spelling in that case instead of manufacturing a value.
            return text.strip_suffix('n').unwrap_or(text).to_string();
        };
        let mut carry = value;
        for digit in &mut decimal {
            let next = u32::from(*digit) * radix + carry;
            *digit = (next % 10) as u8;
            carry = next / 10;
        }
        while carry != 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    while decimal.len() > 1 && decimal.last() == Some(&0) {
        decimal.pop();
    }
    let value: String = decimal.iter().rev().map(|digit| char::from(b'0' + digit)).collect();
    if negative && value != "0" { format!("-{value}") } else { value }
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
    fn number_notation_uses_ecmascript_boundaries() {
        assert_eq!(normalise_number("1e20"), "100000000000000000000");
        assert_eq!(normalise_number("1e21"), "1e+21");
        assert_eq!(normalise_number("1.2e35"), "1.2e+35");
        assert_eq!(normalise_number("0.000001"), "0.000001");
        assert_eq!(normalise_number("0.0000001"), "1e-7");
        assert_eq!(normalise_number("-0.00000012"), "-1.2e-7");
    }

    #[test]
    fn bigint_normalisation_is_radix_independent_and_arbitrary_precision() {
        assert_eq!(normalise_bigint("0xC0Bn"), "3083");
        assert_eq!(normalise_bigint("0b010_10_1n"), "21");
        assert_eq!(normalise_bigint("0o1234_567n"), "342391");
        assert_eq!(normalise_bigint("123_456_789n"), "123456789");
        assert_eq!(normalise_bigint("-0x000n"), "0");
        assert_eq!(normalise_bigint("0xn"), "0");
        // One followed by 32 hexadecimal zeroes is 16^32 = 2^128, one
        // greater than u128::MAX. The decimal expectation is derived from
        // that boundary rather than from the implementation under test.
        assert_eq!(
            normalise_bigint("0x100000000000000000000000000000000n"),
            "340282366920938463463374607431768211456"
        );
    }

    #[test]
    fn an_unparseable_literal_keeps_its_text_rather_than_inventing_a_value() {
        assert_eq!(normalise_number("not-a-number"), "not-a-number");
    }
}
