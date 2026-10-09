//! r6-names: `typeof null` names an identifier natively too, so an unresolved
//! one is TS2304; `class C extends null` stays silent. Expectations are native
//! tsgo output at the pinned 5b1047d.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(file: &str, source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names", file, source);
    case.options.insert("target".into(), "es2015".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn a_type_query_reports_an_unresolved_null() {
    let source = "var x6: typeof null;
class C extends null {}
var x7: typeof null.a;";
    assert_eq!(diagnostics("n.ts", source), [(1, 16, 2304), (3, 16, 2304)]);
}
