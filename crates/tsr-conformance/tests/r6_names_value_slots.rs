//! r6-names: which identifiers native `checkExpression` reaches
//! (`crates/tsr-checker/src/name_slots.rs`). Expectations are native tsgo
//! output at the pinned 5b1047d, `--jsx react --target es2015 --strict false`.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(file: &str, source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names", file, source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "false".into());
    case.options.insert("jsx".into(), "react".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

/// `checkWithStatement` checks the `with` expression, and a value JSX tag is
/// `checkExpression`ed at both its opening and closing tag; an intrinsic tag
/// is not.
#[test]
fn with_expressions_and_value_jsx_tags_are_value_references() {
    let source = "declare const React: any;
with (missingWith) { }
const a = <Missing x={1} />;
const b = <div />;
const c = <Open>text</Open>;
const d = <my-element />;";
    assert_eq!(
        diagnostics("slots.tsx", source),
        [(2, 1, 1101), (2, 1, 2410), (2, 7, 2304), (3, 12, 2304), (5, 12, 2552), (5, 23, 2552)],
    );
}
