//! JavaScript number semantics for source-spelled literals.
//!
//! Ported from typescript-go's `internal/jsnum` as far as this port consumes
//! it: upstream's scanner canonicalises every numeric token value through
//! `jsnum.FromString(s).String()` (`internal/scanner/scanner.go:2194`), so a
//! member named `0b11` binds as `"3"` and `1.0` as `"1"`. This port keeps the
//! *source* spelling on the node — the printer round-trips it — so the
//! canonical value is computed where upstream would have read `Text()`: the
//! binder's member naming and the declaration emitter's literal re-spelling.

/// The numeric value of a source-spelled literal — `jsnum.FromString`.
///
/// Handles the four radix prefixes, legacy octal (`0644`), and numeric
/// separators. An unparseable spelling is `NaN`, exactly as JavaScript's
/// `Number("bogus")` is.
#[must_use]
pub fn numeric_value(text: &str) -> f64 {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let lower = cleaned.to_ascii_lowercase();
    #[allow(clippy::cast_precision_loss)]
    let radix_parse = |digits: &str, radix: u32| -> f64 {
        u128::from_str_radix(digits, radix).map_or(f64::NAN, |value| value as f64)
    };
    if let Some(digits) = lower.strip_prefix("0x") {
        return radix_parse(digits, 16);
    }
    if let Some(digits) = lower.strip_prefix("0o") {
        return radix_parse(digits, 8);
    }
    if let Some(digits) = lower.strip_prefix("0b") {
        return radix_parse(digits, 2);
    }
    if cleaned.len() > 1
        && cleaned.starts_with('0')
        && cleaned.bytes().all(|b| b.is_ascii_digit())
        && !cleaned.contains(['8', '9'])
    {
        return radix_parse(&cleaned[1..], 8);
    }
    cleaned.parse::<f64>().unwrap_or(f64::NAN)
}

/// Format a number the way JavaScript's `Number.prototype.toString` does —
/// `jsnum.Number.String`, which is what upstream's token values and `.d.ts`
/// literal re-spellings both carry.
#[must_use]
pub fn format_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    let magnitude = value.abs();
    if magnitude >= 1e21 || (magnitude != 0.0 && magnitude < 1e-6) {
        let scientific = format!("{value:e}");
        let (mantissa, exponent) = scientific.split_once('e').expect("Rust scientific notation");
        let exponent: i32 = exponent.parse().expect("Rust scientific exponent");
        return if exponent >= 0 {
            format!("{mantissa}e+{exponent}")
        } else {
            format!("{mantissa}e{exponent}")
        };
    }
    #[allow(clippy::float_cmp, clippy::cast_possible_truncation)]
    if value.fract() == 0.0 {
        // `{}` on an integral f64 prints a trailing `.0`; JavaScript never does.
        format!("{}", value as i128)
    } else {
        format!("{value}")
    }
}

/// The canonical spelling of a source-written numeric literal — what upstream's
/// `Text()` holds for the node.
#[must_use]
pub fn canonical_numeric_text(text: &str) -> String {
    format_number(numeric_value(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radix_spellings_reach_their_decimal_value() {
        assert_eq!(canonical_numeric_text("0b11"), "3");
        assert_eq!(canonical_numeric_text("0x10"), "16");
        assert_eq!(canonical_numeric_text("0o17"), "15");
        assert_eq!(canonical_numeric_text("017"), "15");
        assert_eq!(canonical_numeric_text("1_000"), "1000");
        assert_eq!(canonical_numeric_text("1.0"), "1");
        assert_eq!(canonical_numeric_text("1"), "1");
        assert_eq!(canonical_numeric_text("1.5"), "1.5");
    }
}
