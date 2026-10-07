//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(fixture: &str, wanted: &[&str]) {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler");
    if !corpus.is_dir() {
        eprintln!("skipping fresh signature fixture: TypeScript submodule is absent");
        return;
    }
    let source = std::fs::read_to_string(corpus.join(fixture)).expect("upstream fixture exists");
    let case = TestCase::parse("probe/fresh_signatures", "probe.ts", &source);
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
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn generic_method_identity_preserves_sequential_callback_inference() {
    expect(
        "nonInferrableTypePropagation1.ts",
        &[
            "result1 : Thing<number>",
            "result2 : Thing<number>",
            "tap((v) => log(v)) : Op<Box<number>, Box<number>>",
        ],
    );
}

#[test]
fn canonical_signatures_allow_recursive_promise_unwrapping() {
    expect("recursiveConditionalTypes.ts", &["P0 : string | number | null | undefined"]);
}

#[test]
fn erased_generic_keys_use_any_index_recovery() {
    expect("inferenceErasedSignatures.ts", &["T1 : number", "T2 : number"]);
}

#[test]
fn delegated_yield_absence_does_not_infer_a_real_never_candidate() {
    expect(
        "asyncYieldStarContextualType.ts",
        &[
            r#"await authorPromise.then(mapper) : Result<Author, "NOT_FOUND_AUTHOR">"#,
            r#"authorPromise.then(mapper) : Promise<Result<Author, "NOT_FOUND_AUTHOR">>"#,
        ],
    );
}

#[test]
fn mapped_conditionals_keep_the_root_until_each_tuple_key_is_known() {
    expect(
        "recursiveTypeAliasWithSpreadConditionalReturnNotCircular.ts",
        &[
            "zipped1 : Option<[number, string, boolean]>",
            "zipped3 : Option<[number, string, boolean]>",
        ],
    );
}

#[test]
fn unresolved_alias_indices_do_not_become_any_recovery_values() {
    expect(
        "ramdaToolsNoInfinite2.ts",
        &["0 : DropForth<Tail<L>, Prev<N>>", "'->' : DropForth<L, N>"],
    );
}

#[test]
fn chained_calls_keep_each_generation_of_the_same_parameter_distinct() {
    expect("promiseChaining.ts", &["z : Chain<number>", "x => x.length : (x: string) => number"]);
}

#[test]
fn a_missing_delegated_return_context_leaves_the_inference_unknown() {
    expect("yieldStarContextualType.ts", &["g() : Generator<string, unknown, unknown>"]);
}
