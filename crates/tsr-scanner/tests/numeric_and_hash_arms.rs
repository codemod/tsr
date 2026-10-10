//! `Scan`'s numeric-radix, `.digit` and `#` arms (`scanner.go:604`, `:697-746`,
//! `:897-925`), checked against the pinned tsgo (r6-printer5 §1).

use tsr_ast::SyntaxKind;
use tsr_scanner::tokenize;

fn codes(source: &str) -> Vec<(u32, u32)> {
    let (_, diagnostics) = tokenize(source);
    diagnostics.iter().map(|d| (d.message.code(), d.span.start)).collect()
}

#[test]
fn an_empty_binary_or_octal_literal_names_its_own_radix() {
    assert_eq!(codes("0b"), vec![(1177, 2)]);
    assert_eq!(codes("0on"), vec![(1178, 2)]);
    assert_eq!(codes("0x;"), vec![(1125, 2)]);
}

#[test]
fn a_leading_dot_number_is_scan_number_so_a_bigint_suffix_is_ts1353() {
    assert_eq!(codes(".1n"), vec![(1353, 0)]);
    let (tokens, _) = tokenize(".5");
    assert_eq!(tokens[0].kind, SyntaxKind::NumericLiteral);
}

#[test]
fn a_hash_that_starts_no_identifier_is_an_invalid_character_private_name() {
    let (tokens, diagnostics) = tokenize("# ");
    assert_eq!(tokens[0].kind, SyntaxKind::PrivateIdentifier);
    assert_eq!(diagnostics.iter().map(|d| d.message.code()).collect::<Vec<_>>(), vec![1127]);
    // `#!` past the start of the file.
    assert_eq!(codes("x\n#!y"), vec![(18026, 2)]);
}
