//! `newFlowType` (pinned flow.go:28): an incomplete `never` from a loop's
//! provisional pass is `silentNeverType`, which a property access
//! (`isAnyLike`, checker.go:11266) and a call (`resolveCallExpression`,
//! :8512) answer without a report, so the else-branch assignment drops out
//! of the loop's join. Expected lines are pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb (`controlFlowWithIncompleteTypes.types`).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn incomplete_never_is_silent_in_the_loop_pass() {
    let source = r#"// @target: es2015
declare var cond: boolean;
function foo2() {
    let x: string | number | boolean = 0;
    while (cond) {
        if (typeof x === "number") {
            x = "abc";
        }
        else {
            x = x.slice();
        }
    }
}
"#;
    let case = TestCase::parse("probe/r7_flow_silent_never", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<String> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in [
        "x : string | number",
        "x = x.slice() : string",
        "x.slice() : string",
        "x : string",
        "slice : (start?: number, end?: number) => string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
