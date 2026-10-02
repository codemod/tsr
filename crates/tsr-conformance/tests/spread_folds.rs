//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/spread_folds", "probe.ts", source);
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
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn generic_spreads_fold_ordinary_batches_and_preserve_mapped_types() {
    expect(
        r#"// @strict: true
export function identity<T>(value: T) { return { ...value }; }
export function before<T>(value: T) { return { first: 1, ...value }; }
export function after<T>(value: T) { return { ...value, first: 1 }; }
export function repeated<T>(value: T) { return { ...value, first: 1, ...{ last: true } }; }
export function overwrite<T>(value: T) { return { ...value, first: 1, ...{ first: "ok", last: true } }; }
export function mapped<T>(value: Partial<T>) { return { ...value }; }
"#,
        &[
            "identity : <T>(value: T) => T",
            "before : <T>(value: T) => { first: number; } & T",
            "after : <T>(value: T) => T & { first: number; }",
            "repeated : <T>(value: T) => T & { first: number; last: boolean; }",
            "overwrite : <T>(value: T) => T & { first: string; last: boolean; }",
            "mapped : <T>(value: Partial<T>) => Partial<T>",
        ],
    );
}

#[test]
fn unions_distribute_through_repeated_spreads_and_trailing_properties() {
    expect(
        r"// @strict: true
declare let choice: { a: number } | { b: string };
export let distributed = { ...choice, tail: true };
declare let other: { c: number } | { d: string };
export let product = { ...choice, ...other };
declare let anything: any;
export let anyCopy = { first: 1, ...anything, last: true };
declare let maybe: { a: number } | undefined;
export let partial = { ...maybe, tail: true };
declare let opt: { a?: number };
export let constMerge = { a: 1, ...opt } as const;
",
        &[
            "distributed : { a: number; tail: boolean; } | { b: string; tail: boolean; }",
            "product : { a: number; c: number; } | { a: number; d: string; } | { b: string; c: number; } | { b: string; d: string; }",
            "anyCopy : any",
            "partial : { a?: number | undefined; tail: boolean; }",
            "constMerge : { a: number; }",
        ],
    );
}
