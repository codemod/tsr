//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(fixture: &str, wanted: &[&str]) {
    expect_counts(fixture, &wanted.iter().map(|line| (*line, 1)).collect::<Vec<_>>());
}

fn expect_counts(fixture: &str, wanted: &[(&str, usize)]) {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/typescript-go/_submodules/TypeScript/tests/cases");
    if !corpus.is_dir() {
        eprintln!("skipping live inference fixture: TypeScript submodule is absent");
        return;
    }
    let source = std::fs::read_to_string(corpus.join(fixture)).expect("upstream fixture exists");
    expect_source(&source, wanted);
}

fn expect_source(source: &str, wanted: &[(&str, usize)]) {
    let case = TestCase::parse("probe/live_inference", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
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

const INTRA: &str =
    "conformance/types/typeRelationships/typeInference/intraExpressionInferences.ts";

#[test]
fn completed_tuple_callback_contributes_before_the_next_parameter_is_fixed() {
    expect(
        INTRA,
        &["[_a => 0, n => n.toFixed()] : [(_a: number) => number, (n: number) => string]"],
    );
}

#[test]
fn nested_object_callbacks_contribute_at_their_completion() {
    expect(INTRA, &["consume : (arg: number[]) => string", "arg.join(\",\") : string"]);
}

#[test]
fn deeply_nested_callbacks_fix_from_completed_siblings() {
    expect(INTRA, &["c : (arg: string) => boolean", "d : (arg: number) => boolean"]);
}

#[test]
fn unconstrained_context_has_no_call_signature() {
    expect("compiler/promisePermutations.ts", &["r12 : IPromise<(x: any) => any>"]);
}

#[test]
fn method_source_keeps_method_syntax_during_inference() {
    expect(INTRA, &["action : (a: { foo(): void; }) => void"]);
}

#[test]
fn a_consumer_before_its_producer_fixes_unknown() {
    expect_source(
        r"// @strict: true
        declare function ordered<T>(arg: { consume: (x: T) => void; produce: (n: number) => T }): T;
        const result = ordered({consume: x => {}, produce: n => [n]});",
        &[("result : unknown", 1), ("consume : (x: unknown) => void", 1)],
    );
}

#[test]
fn written_annotations_contribute_before_contextual_parameters_are_fixed() {
    expect(
        "conformance/types/contextualTypes/partiallyAnnotatedFunction/partiallyAnnotatedFunctionInferenceWithTypeParameter.ts",
        &["(t1: D, t2) => { t2.test2 } : (t1: D, t2: D) => void"],
    );
    expect(
        "compiler/inferFromAnnotatedReturn1.ts",
        &["(arg): number => 'foo' : (arg: number) => number"],
    );
}

#[test]
fn generic_rest_arity_failure_recovers_the_call_result() {
    expect(
        "compiler/promiseTry.ts",
        &[r#"Promise.try((foo) => "Async result") : Promise<unknown>"#],
    );
}

#[test]
fn contextual_generator_method_checks_its_returned_callback() {
    expect(
        "conformance/es6/yieldExpressions/generatorTypeCheck46.ts",
        &["x => x.length : (x: string) => number"],
    );
}

#[test]
fn async_empty_returns_use_flow_reachability_without_rechecking_recursive_calls() {
    expect(
        "compiler/asyncFunctionsAcrossFiles.ts",
        &["a : { f: () => Promise<void>; }", "b : { f: () => Promise<void>; }"],
    );
    expect(
        "conformance/parser/ecmascript5/ArrowFunctionExpressions/parserArrowFunctionExpression7.ts",
        &["m : () => Promise<never>"],
    );
}

#[test]
fn contextual_array_elements_use_the_iterable_element_type() {
    expect("compiler/mapConstructor.ts", &["['1', 1] : [string, number]"]);
}
