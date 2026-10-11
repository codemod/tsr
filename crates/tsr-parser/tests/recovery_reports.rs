//! Recovery reports whose position, count or order follows native's sink
//! (docs/parity/notes/r7-parser.md §6, §7).

use tsr_core::Arena;
use tsr_parser::ParseOptions;

/// `(start, end, code, args)` for each parse diagnostic of `source`.
fn reports(path: &str, source: &str) -> Vec<(u32, u32, u32, Vec<String>)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse_with_options(&arena, source, ParseOptions::for_file(path));
    parsed
        .diagnostics
        .iter()
        .chain(parsed.jsdoc.diagnostics())
        .map(|d| (d.span.start, d.span.end, d.message.code(), d.args.clone()))
        .collect()
}

#[test]
fn a_missing_colon_leaves_a_silent_missing_false_branch() {
    // `parseConditionalExpressionRest`: `':' expected` and nothing for the
    // absent `whenFalse`.
    assert_eq!(reports("a.ts", "a ? b"), vec![(5, 5, 1005, vec![":".to_string()])]);
}

#[test]
fn reports_at_one_span_fold_only_when_their_arguments_match() {
    // Each unclosed element reports `'</' expected` at end of file after its
    // TS17008; the conditional's `':' expected` there stands beside them.
    let found = reports("a.tsx", "<a>{<b>{x ? <c></c>");
    let at_end: Vec<_> = found.iter().filter(|r| r.0 == 19).map(|r| r.3[0].as_str()).collect();
    assert_eq!(at_end, [":", "</"], "{found:?}");
}

#[test]
fn a_conflict_marker_inside_jsx_text_is_one_trivia_token() {
    assert_eq!(
        reports("a.tsx", "const x = <div>\n<<<<<<< HEAD"),
        vec![(15, 28, 1005, vec!["</".to_string()]), (16, 23, 1185, vec![])]
    );
}

#[test]
fn a_list_in_a_jsdoc_type_aborts_at_a_token_it_cannot_use() {
    // `PCJSDocComment` makes every token some list's element, so the type
    // parameter list stops at `}` rather than skipping it into `x`.
    assert_eq!(
        reports("a.js", "/** @param {<} x */\nfunction f(x) {}\n"),
        vec![(13, 14, 1139, vec![])]
    );
}

#[test]
fn an_unterminated_template_is_reported_once() {
    // Only a template whose first scan found an invalid escape is re-scanned.
    assert_eq!(reports("a.ts", "f(`abc"), vec![(6, 6, 1160, vec![])]);
}
