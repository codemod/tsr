//! r6-names: only native's six `isPrimitiveTypeName` spellings are TS2693 in a
//! value position; `symbol`, `bigint` and `object` take the ordinary cascade.
//! Expectations are native tsgo output at the pinned 5b1047d.

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
fn non_primitive_keyword_spellings_take_the_suggestion_cascade() {
    let source = "let a = symbol;
let b = bigint;
let c = object;
let d = string;
class E extends symbol {}";
    assert_eq!(
        diagnostics("k.ts", source),
        [(1, 9, 2552), (2, 9, 2304), (3, 9, 2552), (4, 9, 2693), (5, 17, 2552)],
    );
}
