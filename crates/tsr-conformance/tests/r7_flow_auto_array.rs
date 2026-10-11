//! The auto-array track: an element-less evolving array finalizes to
//! `autoArrayType` (`createFinalArrayType`, pinned flow.go:1582), and a branch
//! junction unions the finalized types with the junction's subtype reduction
//! (`getUnionOrEvolvingArrayType`, flow.go:1314).
//!
//! Expected lines are pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//! (`controlFlowArrayErrors.types` for `f2`; the junction probe prints `r :
//! any[]` with TS7034/TS7005 `any[]`).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r7_flow_auto_array", "probe.ts", source);
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
fn evolving_array_finalizes_and_joins_by_subtype_reduction() {
    let lines = lines(
        r"// @strict: true
// @noImplicitAny: true
declare const c: boolean;
function junction() {
    let r;
    if (c) r ??= []; else r = [1];
    r;
}
function empty() {
    let x;
    x = [];
    let y = x;
}
",
    );
    assert!(lines.iter().any(|line| line == "y : any[]"), "{lines:?}");
    assert!(!lines.iter().any(|line| line == "r : any[] | number[]"), "{lines:?}");
    assert_eq!(lines.iter().filter(|line| *line == "r : any[]").count(), 1, "{lines:?}");
}
