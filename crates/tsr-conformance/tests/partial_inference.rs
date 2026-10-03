//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines_for_source(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/partial_inference", "probe.ts", source);
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
    let lines = lines_for_source(&source);
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

#[test]
fn exact_optional_reverse_inference_removes_missing_but_keeps_explicit_undefined() {
    let lines = lines_for_source(
        r"// @strict: true
// @exactOptionalPropertyTypes: true
type OptionalImage<T> = { [K in keyof T]?: T[K] };
declare function reverse<T>(value: OptionalImage<T>): T;
declare const absentOnly: { p?: string };
declare const explicitUndefined: { p?: string | undefined };
declare const requiredExplicit: { p: string | undefined };
export const inferredAbsent = reverse(absentOnly);
export const inferredExplicit = reverse(explicitUndefined);
export const inferredRequired = reverse(requiredExplicit);
",
    );
    for wanted in [
        "inferredAbsent : { p: string; }",
        "inferredExplicit : { p: string | undefined; }",
        "inferredRequired : { p: string | undefined; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn ordinary_optional_inference_still_treats_missing_as_undefined() {
    let lines = lines_for_source(
        r"// @strict: true
type OptionalImage<T> = { [K in keyof T]?: T[K] };
declare function reverse<T>(value: OptionalImage<T>): T;
declare const absentOnly: { p?: string };
declare const explicitUndefined: { p?: string | undefined };
declare const requiredExplicit: { p: string | undefined };
export const inferredAbsent = reverse(absentOnly);
export const inferredExplicit = reverse(explicitUndefined);
export const inferredRequired = reverse(requiredExplicit);
",
    );
    for wanted in [
        "inferredAbsent : { p: string; }",
        "inferredExplicit : { p: string; }",
        "inferredRequired : { p: string; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
