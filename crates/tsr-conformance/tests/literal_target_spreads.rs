//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/literal_target_spreads", "probe.ts", source);
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
fn literal_target_requires_actual_properties_even_with_an_index() {
    expect(
        r#"// @strict: true
export let single = [{["a" as string]:1},{b:2}][0];
export let same = [{["a" as string]:1},{b:1}][0];
export let annotated = [{["a" as string]:1} as {[x:string]:number},{b:2}][0];
"#,
        &[
            "single : { [x: string]: number; b?: undefined; } | { b: number; }",
            "same : { [x: string]: number; b?: undefined; } | { b: number; }",
            "annotated : { [x: string]: number; }",
        ],
    );
}

const SPREAD_SOURCE: &str = r#"
declare let required: { a: string };
declare let optional: { a?: string };
declare let explicit: { a?: string | undefined };
declare let leftOptional: { a?: number };
declare let maybe: boolean;
export let one = { a: 123, ...optional };
export let two = { a: 123, ...explicit };
export let three = { a: 123, ...(maybe ? required : {}) };
export let four = { a: 123, ...(maybe ? explicit : {}) };
export let five = { ...leftOptional, ...optional };
export let six = { ...optional, ...required };
export let seven = { a: 123, ...(maybe ? {b: "text"} : {}) };
export let eight = { a: 123, ...{ a: undefined } };
export let nine = { ...optional, ...optional };
export let ten = { a: 123, ...(maybe && required) };
export let readOne = one.a;
export let readFive = five.a;
export let readNine = nine.a;
"#;

#[test]
fn optional_spread_preserves_left_presence_and_merges_present_values() {
    expect(
        &format!("// @strict: true\n{SPREAD_SOURCE}"),
        &[
            "one : { a: string | number; }",
            "two : { a: string | number; }",
            "three : { a: string | number; }",
            "four : { a: string | number; }",
            "five : { a?: string | number | undefined; }",
            "six : { a: string; }",
            "seven : { a: number; b?: string | undefined; }",
            "eight : { a: undefined; }",
            "ten : { a: string | number; }",
            "readOne : string | number",
            "readFive : string | number | undefined",
            "readNine : string | undefined",
        ],
    );
}

#[test]
fn exact_optional_spread_keeps_explicit_undefined() {
    expect(
        &format!("// @strict: true\n// @exactOptionalPropertyTypes: true\n{SPREAD_SOURCE}"),
        &[
            "one : { a: string | number; }",
            "two : { a: string | number | undefined; }",
            "three : { a: string | number; }",
            "four : { a: string | number | undefined; }",
            "five : { a?: string | number; }",
            "six : { a: string; }",
            "seven : { a: number; b?: string; }",
            "eight : { a: undefined; }",
            "nine : { a?: string; }",
            "ten : { a: string | number; }",
            "readOne : string | number",
            "readFive : string | number | undefined",
            "readNine : string | undefined",
        ],
    );
}

#[test]
fn invalid_spread_unions_do_not_become_partial_objects() {
    expect(
        r"// @strict: true
declare let numberUnion: { a: number } | number;
declare let stringUnion: { a: number } | string;
declare let nullUnion: null | undefined;
export let badNumber = { ...numberUnion };
export let badString = { ...stringUnion };
export let badNull = { ...nullUnion };
",
        // The producer exposes errorType as `error`; native emission renders
        // that recovery type as `any` (and reports TS2698 for all three).
        &["badNumber : error", "badString : error", "badNull : error"],
    );
}
