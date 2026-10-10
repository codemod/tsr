//! `ReScanSlashToken(true)` + `regExpParser` (r6-printer5 §2). Every
//! expectation is the pinned tsgo's output for the same literal
//! (`--target es2015`), as `(code, start column within the line)`.

use tsr_core::options::ScriptTarget;
use tsr_scanner::scan_regular_expression_errors;

fn errors(literal: &str, target: ScriptTarget) -> Vec<(u32, u32, u32)> {
    scan_regular_expression_errors(literal, 0, target)
        .iter()
        .map(|e| (e.message.code(), e.start, e.length))
        .collect()
}

#[test]
fn group_names_flags_ranges_and_quantifiers() {
    let es2015 = ScriptTarget::ES2015;
    // `regularExpressionGroupNameSuggestions`: TS1503 over `<foo>`, TS1532,
    // then its suggestion TS1369 at the same span (a Message the checker
    // folds into related information). `fob` is too far from `foo`.
    assert_eq!(
        errors(r"/(?<foo>)\k<Foo>/", es2015),
        vec![(1503, 3, 5), (1532, 12, 3), (1369, 12, 3)]
    );
    assert_eq!(errors(r"/(?<foo>a)\k<fob>/", es2015), vec![(1503, 3, 5), (1532, 13, 3)]);
    assert_eq!(errors(r"/\p{Scrpt=Latn}/u", es2015)[0], (1524, 4, 5));
    assert_eq!(errors("/[z-a]/", es2015), vec![(1517, 2, 3)]);
    assert_eq!(errors("/a{3,1}/u", es2015), vec![(1506, 3, 3)]);
    assert_eq!(errors("/x/gg", es2015), vec![(1500, 4, 1)]);
    assert_eq!(errors(r"/\u{1F600}/", es2015), vec![(1538, 1, 9)]);
    assert_eq!(errors("/x/q", ScriptTarget::ESNext), vec![(1499, 3, 1)]);
}

#[test]
fn a_flag_newer_than_the_target_names_the_target() {
    let found = scan_regular_expression_errors("/x/d", 0, ScriptTarget::ES2015);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].message.code(), 1501);
    assert_eq!(found[0].args, vec!["es2022".to_string()]);
}

#[test]
fn a_valid_literal_reports_nothing() {
    assert!(errors(r"/^(?<y>\d{4})-[a-z\-]+\k<y>$/giu", ScriptTarget::ESNext).is_empty());
}
