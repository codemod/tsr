//! `Parser.nextToken`'s escaped-keyword report (`parser.go:381`) and
//! `nextTokenWithoutCheck` at identifier creation (r6-printer5 §3). The
//! expectations are the pinned tsgo's for `scannerUnicodeEscapeInKeyword2`.

fn codes(source: &str) -> Vec<(u32, u32)> {
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    parsed.diagnostics.iter().map(|d| (d.message.code(), d.span.start)).collect()
}

#[test]
fn a_keyword_spelled_with_an_escape_is_reported_where_it_is_a_keyword() {
    assert_eq!(codes(r#"\u{0076}ar x = "hello";"#), vec![(1260, 0)]);
    assert_eq!(codes("typ\\u0065 notok = 0;"), vec![(1260, 0)]);
}

#[test]
fn an_escaped_keyword_used_as_a_name_is_not() {
    assert!(codes("var \\u0061wait = 12;").is_empty());
    assert!(codes("const a = {def\\u0061ult: 12};").is_empty());
    assert!(codes("type typ\\u0065 = 12;").is_empty());
}
