//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/spread_origins", "probe.ts", source);
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
fn spread_order_follows_surviving_declarations() {
    expect(
        r"// @strict: true
declare let source: { a: number; b: number };
declare let optional: { a: number; b?: number };
export let requiredOverride = { b: 1, ...source };
export let optionalOverride = { b: 1, ...optional };
export let afterSpread = { ...source, b: 1 };
export let again = { ...requiredOverride, c: true };
declare let flag: boolean;
export let partial = { b: 1, ...(flag ? source : {}) };
export let constCopy = { b: 1, ...source } as const;
export let plain = { z: 1, a: 2 };
",
        &[
            "requiredOverride : { a: number; b: number; }",
            "optionalOverride : { a: number; b: number; }",
            "afterSpread : { a: number; b: number; }",
            "again : { a: number; b: number; c: boolean; }",
            "partial : { a?: number | undefined; b: number; }",
            "constCopy : { readonly a: number; readonly b: number; }",
            "plain : { z: number; a: number; }",
        ],
    );
}

#[test]
fn mapped_and_shorthand_members_keep_the_native_declaration_origin() {
    expect(
        r#"// @strict: true
type Select<T, K extends keyof T> = { [P in K]: T[P] };
type Props = { foo: string; bar: string };
declare let defaults: Select<Props, "foo">;
declare let input: { foo?: string; bar: string };
export let merged = { ...defaults, ...input };
export let selectedOnly = { ...defaults };
type Rename<T> = { [K in keyof T as `pre${K & string}`]: T[K] };
declare let renamed: Rename<{ z: number; a: string }>;
export let remapped = { ...renamed };
declare let y: string;
declare let x: number;
export let shorthand = { x, y };
"#,
        &[
            "merged : { foo: string; bar: string; }",
            "selectedOnly : { foo: string; }",
            "remapped : { prea: string; prez: number; }",
            "shorthand : { x: number; y: string; }",
        ],
    );
}
