//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(fixture: &str, wanted: &[&str]) {
    expect_counts(fixture, &wanted.iter().map(|line| (*line, 1)).collect::<Vec<_>>());
}

fn expect_counts(fixture: &str, wanted: &[(&str, usize)]) {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler");
    if !corpus.is_dir() {
        eprintln!("skipping partial inference fixture: TypeScript submodule is absent");
        return;
    }
    let source = std::fs::read_to_string(corpus.join(fixture)).expect("upstream fixture exists");
    let case = TestCase::parse("probe/partial_inference", "probe.ts", &source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for (line, expected) in wanted {
        let actual = lines.iter().filter(|actual| *actual == line).count();
        assert!(
            actual >= *expected,
            "expected {expected} occurrences of {line}, got {actual}: {lines:?}"
        );
    }
}

#[test]
fn object_methods_keep_inferable_sibling_data() {
    expect(
        "reverseMappedPartiallyInferableTypes.ts",
        &["contains : (k: string) => boolean", "k : string"],
    );
}

#[test]
fn tuple_callbacks_keep_inferable_sibling_elements() {
    expect_counts("reverseMappedPartiallyInferableTypes.ts", &[("arg : { key: number; }", 4)]);
}

#[test]
fn array_subtype_reduction_retains_the_inferable_object() {
    expect(
        "subtypeReductionWithAnyFunctionType.ts",
        &["_ : Fooer[]", "foo : (v: string) => string"],
    );
}

#[test]
fn nested_noncontextual_methods_contribute_before_callback_fixing() {
    expect(
        "thislessFunctionsNotContextSensitive1.ts",
        &["inc : (state: { bar2: number; }) => number"],
    );
}
