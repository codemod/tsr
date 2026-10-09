//! A lone surrogate in a string value is a two-character sentinel (ADR-0051).

use tsr_ast::SyntaxKind::StringLiteral;
use tsr_scanner::{Scanner, decode_lone_surrogate_sentinel, push_js_string_code_point};

fn value(source: &str) -> String {
    let mut scanner = Scanner::new(source);
    assert_eq!(scanner.scan().kind, StringLiteral);
    scanner.token_value().to_string()
}

#[test]
fn the_sentinel_round_trips_every_surrogate() {
    for cp in 0xD800..0xE000 {
        let mut encoded = String::new();
        push_js_string_code_point(cp, &mut encoded);
        assert_eq!(decode_lone_surrogate_sentinel(&encoded), Some((cp, encoded.len())));
    }
}

#[test]
fn a_real_maximum_code_point_is_not_a_sentinel() {
    // `unicodeExtendedEscapesInStrings06`: `"\u{10FFFF}"` is that character.
    let max = value(r#""\u{10FFFF}""#);
    assert_eq!(max, "\u{10FFFF}");
    assert_eq!(decode_lone_surrogate_sentinel(&max), None);
    assert_eq!(decode_lone_surrogate_sentinel("A"), None);
}

#[test]
fn lone_surrogates_keep_their_code_unit_and_pairs_combine() {
    let lead = value(r#""\uD800""#);
    let trail = value(r#""\u{DC00}""#);
    assert_ne!(lead, trail);
    assert_eq!(decode_lone_surrogate_sentinel(&lead).map(|(cp, _)| cp), Some(0xD800));
    assert_eq!(decode_lone_surrogate_sentinel(&trail).map(|(cp, _)| cp), Some(0xDC00));
    assert_eq!(value(r#""\uD83D\uDE00""#), "\u{1F600}");
}
