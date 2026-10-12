//! A conditional's true branch that is its check type reads as a
//! substitution type (getConditionalFlowTypeOfType, pinned checker.go:24952),
//! so `Extract<T, string | undefined>` has the base constraint
//! `string | undefined` and getNarrowableTypeForReference (:31491)
//! substitutes it in a constraint position.
//!
//! Expected lines are pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//! (`controlFlowGenericTypes.types` for `f2`); the probe has no errors there.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r7_flow_conditional_substitution", "probe.ts", source);
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
fn extract_of_unconstrained_parameter_narrows_through_its_substitution_constraint() {
    let lines = lines(
        r#"// @strict: true
type Pick1<T, U> = T extends U ? T : never;
function f2<T>(x: Extract<T, string | undefined> | null): string {
    if (x) {
        x;
        x.length;
        return x;
    }
    return "hello";
}
function f3<T>(x: Pick1<T, unknown> | null) {
    if (x) {
        x;
    }
}
"#,
    );
    let at = |text: &str| lines.iter().filter(|line| *line == text).count();
    assert_eq!(at("x : NonNullable<Extract<T, string | undefined>>"), 1, "{lines:?}");
    assert_eq!(at("x : string"), 2, "{lines:?}");
    assert_eq!(at("x.length : number"), 1, "{lines:?}");
    // An unknown extends type keeps the bare check type (no substitution).
    assert_eq!(at("x : NonNullable<Pick1<T, unknown>>"), 1, "{lines:?}");
}
