//! getTypeAtFlowArrayMutation (pinned flow.go:1393) evolves only the array
//! the mutation targets. Expected texts are pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn each_evolving_array_takes_only_its_own_mutations() {
    let source = r#"const a = [];
const b = [];
a.push(1);
b.push("s");
b[0] = "t";
export const x: string = a;
export const y: number = b;
"#;
    let mut case = TestCase::parse("probe/array-mutation-target", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let mut reported: Vec<_> = diagnostics_suite::rendered_for(&case)
        .into_iter()
        .map(|((_, line, column, code), message)| (line, column, code, message))
        .collect();
    reported.sort();
    assert_eq!(
        reported,
        [
            (6, 14, 2322, "Type 'number[]' is not assignable to type 'string'.".to_string()),
            (7, 14, 2322, "Type 'string[]' is not assignable to type 'number'.".to_string()),
        ]
    );
}
