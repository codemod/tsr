//! Scanner behaviour.

use tsr_ast::SyntaxKind::{self, *};
use tsr_scanner::{Scanner, TokenFlags, tokenize};

/// Kinds of every token in `source`, excluding the trailing end-of-file.
fn kinds(source: &str) -> Vec<SyntaxKind> {
    let (tokens, _) = tokenize(source);
    tokens[..tokens.len() - 1].iter().map(|t| t.kind).collect()
}

/// Scan and return `(kinds, diagnostic codes)`.
fn scan_with_errors(source: &str) -> (Vec<SyntaxKind>, Vec<u32>) {
    let (tokens, diagnostics) = tokenize(source);
    (
        tokens[..tokens.len() - 1].iter().map(|t| t.kind).collect(),
        diagnostics.iter().map(|d| d.message.code()).collect(),
    )
}

#[test]
fn empty_source_yields_only_end_of_file() {
    let (tokens, diagnostics) = tokenize("");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, EndOfFile);
    assert!(diagnostics.is_empty());
}

#[test]
fn identifiers_and_keywords_are_distinguished() {
    assert_eq!(kinds("foo if bar"), vec![Identifier, IfKeyword, Identifier]);
    assert_eq!(kinds("$_ _x $1"), vec![Identifier, Identifier, Identifier]);
}

#[test]
fn keywords_written_with_escapes_are_identifiers() {
    // `\u0069f` spells "if", but an identifier written with an escape is never a
    // keyword — it is an ordinary identifier that happens to be named `if`.
    let source = r"\u0069f";
    let (tokens, _) = tokenize(source);
    assert_eq!(tokens[0].kind, Identifier);
    assert!(tokens[0].flags.contains(TokenFlags::UNICODE_ESCAPE));

    let mut scanner = Scanner::new(source);
    scanner.scan();
    assert_eq!(scanner.token_value(), "if");

    // Without the escape it is the keyword.
    assert_eq!(kinds("if"), vec![IfKeyword]);
}

#[test]
fn unicode_identifiers_are_accepted() {
    // Non-ASCII identifier characters exercise the generated tables rather than
    // the ASCII fast path.
    assert_eq!(kinds("café"), vec![Identifier]);
    assert_eq!(kinds("日本語"), vec![Identifier]);
    assert_eq!(kinds("π"), vec![Identifier]);
}

#[test]
fn numeric_literal_forms() {
    assert_eq!(kinds("0"), vec![NumericLiteral]);
    assert_eq!(kinds("1.5"), vec![NumericLiteral]);
    assert_eq!(kinds(".5"), vec![NumericLiteral], "a leading dot starts a number");
    assert_eq!(kinds("1e10"), vec![NumericLiteral]);
    assert_eq!(kinds("1E-10"), vec![NumericLiteral]);
    assert_eq!(kinds("0x1F"), vec![NumericLiteral]);
    assert_eq!(kinds("0b1010"), vec![NumericLiteral]);
    assert_eq!(kinds("0o777"), vec![NumericLiteral]);
    assert_eq!(kinds("1_000_000"), vec![NumericLiteral]);
    assert_eq!(kinds("123n"), vec![BigIntLiteral]);
    assert_eq!(kinds("0xFFn"), vec![BigIntLiteral]);
}

#[test]
fn numeric_flags_record_how_the_literal_was_written() {
    let (tokens, _) = tokenize("0x1F");
    assert!(tokens[0].flags.contains(TokenFlags::HEX_SPECIFIER));

    let (tokens, _) = tokenize("1_000");
    assert!(tokens[0].flags.contains(TokenFlags::CONTAINS_SEPARATOR));

    let (tokens, _) = tokenize("1e5");
    assert!(tokens[0].flags.contains(TokenFlags::SCIENTIFIC));
}

#[test]
fn malformed_numbers_report_diagnostics_but_still_produce_a_token() {
    // Error recovery: the parser must keep going, so a bad literal is still a
    // literal.
    let (kinds, codes) = scan_with_errors("0x");
    assert_eq!(kinds, vec![NumericLiteral]);
    assert!(!codes.is_empty(), "0x should report a missing digit");

    let (kinds, codes) = scan_with_errors("1e");
    assert_eq!(kinds, vec![NumericLiteral]);
    assert!(!codes.is_empty(), "1e should report a missing exponent");
}

#[test]
fn dot_is_a_number_only_when_followed_by_a_digit() {
    assert_eq!(kinds("a.b"), vec![Identifier, DotToken, Identifier]);
    assert_eq!(kinds("..."), vec![DotDotDotToken]);
    assert_eq!(kinds(".5"), vec![NumericLiteral]);
}

#[test]
fn strings_decode_escapes() {
    let mut scanner = Scanner::new(r#""a\nb""#);
    assert_eq!(scanner.scan().kind, StringLiteral);
    assert_eq!(scanner.token_value(), "a\nb");

    let mut scanner = Scanner::new(r#""A\x42""#);
    scanner.scan();
    assert_eq!(scanner.token_value(), "AB");

    let mut scanner = Scanner::new(r#""\u{1F600}""#);
    scanner.scan();
    assert_eq!(scanner.token_value(), "\u{1F600}");
}

#[test]
fn plain_strings_need_no_decoding() {
    let mut scanner = Scanner::new(r"'hello'");
    scanner.scan();
    assert_eq!(scanner.token_value(), "hello");
    assert_eq!(scanner.token_text(), "'hello'", "text keeps the quotes; value does not");
}

#[test]
fn unterminated_strings_are_flagged_and_reported() {
    let (tokens, diagnostics) = tokenize("\"abc");
    assert_eq!(tokens[0].kind, StringLiteral);
    assert!(tokens[0].is_unterminated());
    assert_eq!(diagnostics[0].message.code(), 1002);

    // A newline also terminates the literal.
    let (tokens, diagnostics) = tokenize("\"abc\ndef");
    assert!(tokens[0].is_unterminated());
    assert!(!diagnostics.is_empty());
}

#[test]
fn template_literals_split_at_substitutions() {
    assert_eq!(kinds("`plain`"), vec![NoSubstitutionTemplateLiteral]);

    // `a${x}b` scans as head, expression, then the parser asks for the tail.
    let mut scanner = Scanner::new("`a${x}b`");
    assert_eq!(scanner.scan().kind, TemplateHead);
    assert_eq!(scanner.scan().kind, Identifier);
    assert_eq!(scanner.scan().kind, CloseBraceToken);
    assert_eq!(scanner.rescan_template_continuation().kind, TemplateTail);
}

#[test]
fn template_middle_is_reachable_for_multiple_substitutions() {
    let mut scanner = Scanner::new("`a${x}b${y}c`");
    assert_eq!(scanner.scan().kind, TemplateHead);
    assert_eq!(scanner.scan().kind, Identifier);
    assert_eq!(scanner.scan().kind, CloseBraceToken);
    assert_eq!(scanner.rescan_template_continuation().kind, TemplateMiddle);
    assert_eq!(scanner.scan().kind, Identifier);
    assert_eq!(scanner.scan().kind, CloseBraceToken);
    assert_eq!(scanner.rescan_template_continuation().kind, TemplateTail);
}

#[test]
fn punctuation_uses_maximal_munch() {
    assert_eq!(kinds(">>>="), vec![GreaterThanGreaterThanGreaterThanEqualsToken]);
    assert_eq!(kinds(">>>"), vec![GreaterThanGreaterThanGreaterThanToken]);
    assert_eq!(kinds(">>"), vec![GreaterThanGreaterThanToken]);
    assert_eq!(kinds("==="), vec![EqualsEqualsEqualsToken]);
    assert_eq!(kinds("=="), vec![EqualsEqualsToken]);
    assert_eq!(kinds("=>"), vec![EqualsGreaterThanToken]);
    assert_eq!(kinds("??="), vec![QuestionQuestionEqualsToken]);
    assert_eq!(kinds("**="), vec![AsteriskAsteriskEqualsToken]);
    assert_eq!(kinds("&&="), vec![AmpersandAmpersandEqualsToken]);
}

#[test]
fn optional_chaining_versus_conditional() {
    assert_eq!(kinds("a?.b"), vec![Identifier, QuestionDotToken, Identifier]);
    // `a?.5:b` is a conditional expression, not optional chaining.
    assert_eq!(
        kinds("a?.5:b"),
        vec![Identifier, QuestionToken, NumericLiteral, ColonToken, Identifier]
    );
}

#[test]
fn greater_than_can_be_split_for_type_arguments() {
    // `List<List<T>>` lexes `>>` as a shift; the parser splits it.
    let mut scanner = Scanner::new(">>");
    assert_eq!(scanner.scan().kind, GreaterThanGreaterThanToken);
    let split = scanner.rescan_greater_than();
    assert_eq!(split.kind, GreaterThanToken);
    assert_eq!(scanner.scan().kind, GreaterThanToken);
}

#[test]
fn comments_are_trivia() {
    assert_eq!(kinds("a // comment\nb"), vec![Identifier, Identifier]);
    assert_eq!(kinds("a /* c */ b"), vec![Identifier, Identifier]);
    assert_eq!(kinds("/* only a comment */"), vec![]);
}

#[test]
fn unterminated_block_comment_is_reported() {
    let (_, diagnostics) = tokenize("/* never closed");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message.code(), 1010, "expected '*/' expected");
}

#[test]
fn line_breaks_are_recorded_for_semicolon_insertion() {
    let (tokens, _) = tokenize("a\nb");
    assert!(!tokens[0].has_preceding_line_break());
    assert!(tokens[1].has_preceding_line_break());

    // A block comment spanning lines also counts.
    let (tokens, _) = tokenize("a /*\n*/ b");
    assert!(tokens[1].has_preceding_line_break());

    // One on the same line does not.
    let (tokens, _) = tokenize("a /* x */ b");
    assert!(!tokens[1].has_preceding_line_break());
}

#[test]
fn unusual_line_terminators_count() {
    // LS and PS are line terminators in ECMAScript but not in most languages.
    let (tokens, _) = tokenize("a\u{2028}b");
    assert!(tokens[1].has_preceding_line_break());
    let (tokens, _) = tokenize("a\u{2029}b");
    assert!(tokens[1].has_preceding_line_break());
}

#[test]
fn crlf_counts_as_one_break() {
    let (tokens, _) = tokenize("a\r\nb");
    assert_eq!(tokens.len(), 3, "a, b, EOF");
    assert!(tokens[1].has_preceding_line_break());
}

#[test]
fn regular_expressions_are_scanned_on_request() {
    let mut scanner = Scanner::new("/ab+c/gi");
    assert_eq!(scanner.scan().kind, SlashToken, "a slash is division until asked otherwise");
    let re = scanner.rescan_as_regular_expression();
    assert_eq!(re.kind, RegularExpressionLiteral);
    assert_eq!(scanner.token_text(), "/ab+c/gi");
}

#[test]
fn regex_character_class_may_contain_a_slash() {
    let mut scanner = Scanner::new("/[/]/");
    scanner.scan();
    let re = scanner.rescan_as_regular_expression();
    assert_eq!(re.kind, RegularExpressionLiteral);
    assert_eq!(scanner.token_text(), "/[/]/");
}

#[test]
fn unterminated_regex_is_reported() {
    let mut scanner = Scanner::new("/abc");
    scanner.scan();
    scanner.rescan_as_regular_expression();
    assert!(!scanner.diagnostics().is_empty());
}

#[test]
fn spans_cover_the_token_and_exclude_leading_trivia() {
    let (tokens, _) = tokenize("  foo  bar");
    assert_eq!(tokens[0].span.start, 2);
    assert_eq!(tokens[0].span.end, 5);
    assert_eq!(tokens[1].span.start, 7);
    assert_eq!(tokens[1].span.end, 10);
}

#[test]
fn spans_are_byte_offsets_not_character_counts() {
    // "é" is two bytes; a char-based offset would put the next token at 2.
    let source = "é x";
    let (tokens, _) = tokenize(source);
    assert_eq!(tokens[0].span, tsr_core::Span::new(0, 2));
    assert_eq!(tokens[1].span, tsr_core::Span::new(3, 4));
    assert_eq!(&source[3..4], "x");
}

#[test]
fn invalid_characters_produce_a_token_and_a_diagnostic() {
    // The scanner must not stall: an unknown character yields Unknown and
    // advances, or the parser loops forever.
    let (tokens, diagnostics) = tokenize("a \u{7} b");
    assert!(tokens.iter().any(|t| t.kind == Unknown));
    assert!(!diagnostics.is_empty());
    assert_eq!(tokens.last().expect("eof").kind, EndOfFile);
}

#[test]
fn a_realistic_snippet_scans_to_the_expected_shape() {
    let source = "export const x: number = 1;";
    assert_eq!(
        kinds(source),
        vec![
            ExportKeyword,
            ConstKeyword,
            Identifier,
            ColonToken,
            NumberKeyword,
            EqualsToken,
            NumericLiteral,
            SemicolonToken,
        ]
    );
}

#[test]
fn line_and_paragraph_separators_are_legal_inside_string_literals() {
    // ES2019's JSON-superset proposal permits U+2028/U+2029 unescaped in string
    // literals, even though they are line terminators elsewhere in the grammar.
    // Corpus case: conformance/allowUnescapedParagraphAndLineSeparatorsInStringLiteral.
    let (tokens, diagnostics) = tokenize("\"a\u{2028}b\"");
    assert_eq!(tokens[0].kind, StringLiteral);
    assert!(!tokens[0].is_unterminated());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (tokens, diagnostics) = tokenize("'a\u{2029}b'");
    assert!(!tokens[0].is_unterminated());
    assert!(diagnostics.is_empty());

    // A real newline still terminates.
    let (tokens, _) = tokenize("\"a\nb\"");
    assert!(tokens[0].is_unterminated());
}

#[test]
fn save_and_restore_rewinds_position_and_discards_speculative_diagnostics() {
    // The parser backtracks constantly; errors from an abandoned path must not
    // survive the rewind.
    let mut scanner = Scanner::new("/ not a regex");
    assert_eq!(scanner.scan().kind, SlashToken);

    let saved = scanner.save();
    scanner.rescan_as_regular_expression();
    assert!(!scanner.diagnostics().is_empty(), "unterminated regex should report");

    scanner.restore(saved);
    assert!(scanner.diagnostics().is_empty(), "restore must discard them");
    assert_eq!(scanner.token().kind, SlashToken);
    assert_eq!(scanner.scan().kind, Identifier, "and resume from the saved position");
}

#[test]
fn next_line_is_whitespace_not_a_line_break() {
    // U+0085 is in Zs but outside ECMAScript's line-terminator set. Corpus case:
    // compiler/fileWithNextLine2.ts, whose own comment says "treated like a space".
    let (tokens, diagnostics) = tokenize("a\u{85}b");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(tokens.len(), 3, "two identifiers and EOF");
    assert!(!tokens[1].has_preceding_line_break(), "NEL is not a line break");
}

#[test]
fn zero_width_space_is_whitespace() {
    let (tokens, diagnostics) = tokenize("a\u{200B}b");
    assert!(diagnostics.is_empty());
    assert_eq!(tokens.len(), 3);
}

#[test]
fn surrogate_pairs_combine_into_one_character() {
    let mut scanner = Scanner::new(r#""💩""#);
    scanner.scan();
    assert_eq!(scanner.token_value(), "\u{1F4A9}");
}

#[test]
fn lone_surrogates_are_accepted_without_a_diagnostic() {
    // JavaScript strings are UTF-16 and may hold a lone surrogate; Rust `String`
    // cannot, so the value degrades to U+FFFD. What matters is that TypeScript
    // accepts the source, so we must not report an error. Corpus cases:
    // unicodeExtendedEscapesInStrings10/11.
    let (tokens, diagnostics) = tokenize(r#""\u{D800}""#);
    assert_eq!(tokens[0].kind, StringLiteral);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (_, diagnostics) = tokenize(r#""\uD800""#);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn out_of_range_code_points_are_still_rejected() {
    // Above U+10FFFF is genuinely invalid, unlike a lone surrogate.
    let (_, diagnostics) = tokenize(r#""\u{110000}""#);
    assert!(!diagnostics.is_empty());
}
