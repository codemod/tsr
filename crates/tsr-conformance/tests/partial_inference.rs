//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines_for_source(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/partial_inference", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
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

#[test]
fn nested_index_constraints_keep_the_native_clean_mapped_assignment() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler");
    if !corpus.is_dir() {
        eprintln!("skipping nested mapped assignment fixture: TypeScript submodule is absent");
        return;
    }
    let source = std::fs::read_to_string(corpus.join("twiceNestedKeyofIndexInference.ts"))
        .expect("upstream fixture exists");
    let case = TestCase::parse("probe/nested-index-constraints", "fixture.ts", &source);
    let diagnostics = tsr_conformance::diagnostics_suite::reported_for(&case);
    assert!(diagnostics.is_empty(), "native has no diagnostics: {diagnostics:?}");
}

#[test]
fn required_nested_mapped_members_keep_concrete_outer_substitutions() {
    // These explicit arguments distinguish a lost mapped-body substitution
    // from the separate source-intersection relation prerequisite. In
    // particular, the written Required<{ [key in K1]: ... }> spelling does
    // not imply that its semantic members still contain T, K1, or K2.
    let lines = lines_for_source(
        r#"// @target: es2015
type Set1<T, K1 extends keyof T> = T extends any[] ? T : Pick<T, Exclude<keyof T, K1>> & {
    [SK1 in K1]-?: Required<Pick<T, SK1>>;
}[K1];
type Set2<T, K1 extends keyof T, K2 extends keyof T[K1]> = T extends any[] ? T : Pick<T, Exclude<keyof T, K1>> & {
    [SK1 in K1]-?: Required<{ [key in K1]: Set1<T[K1], K2> }>;
}[K1];
interface State { a: { b: string; c: number }; d: boolean }
declare const one: Set1<State["a"], "b">;
declare const required: Required<{ [key in "a"]: Set1<State["a"], "b"> }>;
declare const two: Set2<State, "a", "b">;
export const oneB = one.b;
export const oneC = one.c;
export const reqB = required.a.b;
export const reqC = required.a.c;
export const twoB = two.a.b;
export const twoC = two.a.c;
export const twoD = two.d;
"#,
    );
    for wanted in [
        "oneB : string",
        "oneC : number",
        "reqB : string",
        "reqC : number",
        "twoB : string",
        "twoC : number",
        "twoD : boolean",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn indexed_callbacks_retain_native_negative_diagnostics_and_concrete_values() {
    // Nil-node unknown recovery must not publish incomplete projections as
    // usable callback types: it previously erased both assignment errors.
    let source = r#"// @strict: true
declare function missing<T extends Record<string, unknown>>(value: T, consume: (item: T["missing"]) => void): T;
missing({ present: 1 }, missingValue => {
    const rejected: string = missingValue;
});
declare function known<T extends { present: unknown }>(value: T, consume: (item: T["present"]) => void): T;
known({ present: true }, knownValue => {
    const rejected: string = knownValue;
});
declare const ordinary: { present: number };
ordinary.absent;
declare const dynamic: any;
export const anyAccess = dynamic.absent;
"#;
    let case = TestCase::parse("probe/indexed-callback-diagnostics", "fixture.ts", source);
    let mut diagnostics: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    // TestCase removes the directive line; native sites are lines 4, 8, 11.
    assert_eq!(diagnostics, [(3, 11, 2322), (7, 11, 2322), (10, 10, 2339)]);
    let lines = lines_for_source(source);
    for wanted in ["knownValue : boolean", "anyAccess : any"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
