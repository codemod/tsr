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

/// `(code, start, args)` of each related report on each parse diagnostic.
fn related(path: &str, source: &str) -> Vec<Vec<(u32, u32, Vec<String>)>> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse_with_options(&arena, source, ParseOptions::for_file(path));
    parsed
        .diagnostics
        .iter()
        .map(|d| {
            d.related_information()
                .iter()
                .map(|r| (r.message.code(), r.span.start, r.args.clone()))
                .collect()
        })
        .collect()
}

#[test]
fn a_missing_closing_bracket_points_back_at_its_opening_one() {
    // `parseExpectedMatchingBrackets`: TS1007 at the `{`, `(` or `[`.
    let pair = |open: &str, close: &str| vec![open.to_string(), close.to_string()];
    assert_eq!(related("a.ts", "function f() {\n  x;\n"), vec![vec![(1007, 13, pair("{", "}"))]]);
    assert_eq!(related("a.ts", "if (a b;"), vec![vec![(1007, 3, pair("(", ")"))]]);
    assert_eq!(related("a.ts", "x = [1, 2"), vec![vec![(1007, 4, pair("[", "]"))]]);
    assert_eq!(related("a.ts", "x = { a: 1"), vec![vec![(1007, 4, pair("{", "}"))]]);
}

#[test]
fn a_report_the_guard_dropped_gets_no_related_report() {
    // `if (` at end of file: `Expression expected` takes the position, and
    // `')' expected` there is dropped, so nothing carries TS1007.
    assert!(related("a.ts", "if (").iter().all(Vec::is_empty));
}

#[test]
fn a_nameless_typedef_with_a_type_reports_in_the_file_of_a_javascript_file() {
    // `reparseUnhosted`'s `checkNonIdentifierName`: on the character before
    // the missing name, in the file's diagnostics; the comment's own
    // `Identifier expected` stays with the comment's.
    let arena = Arena::new();
    let source = "/** @typedef {{a: 1}} */\nvar x;\n";
    let parsed = tsr_parser::parse_with_options(&arena, source, ParseOptions::for_file("a.js"));
    let file: Vec<_> =
        parsed.diagnostics.iter().map(|d| (d.span.start, d.span.end, d.message.code())).collect();
    assert_eq!(file, [(20, 21, 1003)]);
    assert_eq!(parsed.jsdoc.diagnostics().len(), 1);
    let parsed = tsr_parser::parse_with_options(&arena, source, ParseOptions::for_file("a.ts"));
    assert!(parsed.diagnostics.is_empty());
}
