//! A lone surrogate prints as native's `\uXXXX` escape and stays its own
//! literal type (ADR-0051). Expectations from pinned native tsgo 5b1047d.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"const a = "\uD800";
const b = "\uDC00";
const c = "\uD83D\uDE00";
const d = { "\uD800": 1 };
"#;

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/lone_surrogate_literals", "a.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

#[test]
fn lone_surrogates_print_escaped_and_stay_distinct() {
    let lines = assertions(SOURCE);
    for wanted in
        [r#"a : "\uD800""#, r#"b : "\uDC00""#, "c : \"\u{1F600}\"", r#"d : { "\uD800": number; }"#]
    {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
