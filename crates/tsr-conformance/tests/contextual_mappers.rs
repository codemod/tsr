//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(fixture: &str, wanted: &[&str]) {
    expect_counts(fixture, &wanted.iter().map(|line| (*line, 1)).collect::<Vec<_>>());
}

fn expect_counts(fixture: &str, wanted: &[(&str, usize)]) {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler");
    if !corpus.is_dir() {
        eprintln!("skipping contextual mapper fixture: TypeScript submodule is absent");
        return;
    }
    let source = std::fs::read_to_string(corpus.join(fixture)).expect("upstream fixture exists");
    let case = TestCase::parse("probe/contextual_mappers", "probe.ts", &source);
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
fn dependent_constraints_see_all_argument_candidates() {
    expect("typeInferenceCacheInvalidation.ts", &["foo : number", "bar : string"]);
    expect(
        "contextualParameterAndSelfReferentialConstraint1.ts",
        &["until : (x: boolean) => boolean"],
    );
}

#[test]
fn contextual_initializers_use_instantiated_constraints_and_defaults() {
    expect(
        "contextuallyTypedParametersWithInitializers2.ts",
        &[
            "checkLimit : (ctx: { count: number; }, max?: number) => void",
            "hasAccess : (ctx: { count: number; }, user: { name: string; }) => void",
        ],
    );
    expect(
        "contextuallyTypedParametersWithInitializers4.ts",
        &["checkLimit : (ctx: { count: number; }, max?: number) => void"],
    );
    expect("genericInferenceDefaultTypeParameter.ts", &["event => { } : (event: string) => void"]);
}

#[test]
fn generic_mapped_contexts_keep_key_templates_until_member_lookup() {
    expect_counts(
        "mappedTypeContextualTypesApplied.ts",
        &[("{foo: s => 42} : { foo: (s: string) => number; }", 7)],
    );
    expect(
        "contextualTypeFunctionObjectPropertyIntersection.ts",
        &["f : (a: string) => void", "a : string"],
    );
}

#[test]
fn an_unfixed_outer_return_parameter_keeps_its_identity() {
    expect(
        "simpleArrowFunctionParameterReferencedInObjectLiteral1.ts",
        &["p => ({ X: p }) : (p: never) => { X: never; }"],
    );
}

#[test]
fn skipped_callback_signatures_do_not_infer_an_empty_rest_tuple() {
    expect(
        "nonInferrableTypePropagation3.ts",
        &["usersOverAge : (age: number) => { id: string; age: number; }[]"],
    );
}

#[test]
fn callback_arity_failure_recovers_with_no_contextual_return_candidate() {
    expect("promiseTry.ts", &["Promise.try((foo) => \"Async result\") : Promise<unknown>"]);
}

#[test]
fn contextual_this_uses_the_same_mapper_as_ordinary_parameters() {
    expect("instantiateContextuallyTypedGenericThis.ts", &["this : string", "dit : string"]);
}

#[test]
fn excluded_mapped_keys_do_not_supply_callback_context() {
    expect_counts(
        "contextualPropertyOfGenericFilteringMappedType.ts",
        &[("value : any", 1), ("key : any", 1)],
    );
}

#[test]
fn const_inference_queries_keep_uninstantiated_context() {
    expect(
        "../conformance/types/typeParameters/typeParameterLists/typeParameterConstModifiersReturnsAndYields.ts",
        &[r#"middleware : () => "someValue""#],
    );
}
