//! Mapped tuple inference with the bundled libraries used by the corpus.

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn reflect_apply_rechecks_an_empty_array_under_the_inferred_tuple_context() {
    let source = "// @target: es2015\n
        const noOp = () => {};
        const observed = Reflect.apply(noOp, this, []);";
    let case = TestCase::parse("probe/mapped-tuple", "mapped-tuple.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    // The pinned doYouNeedToChangeYourTargetLibraryES2015 baseline records
    // both the return and the array's contextual tuple type.
    assert!(lines.iter().any(|line| line == "Reflect.apply(noOp, this, []) : void"), "{lines:?}");
    assert!(lines.iter().any(|line| line == "[] : []"), "{lines:?}");
}
