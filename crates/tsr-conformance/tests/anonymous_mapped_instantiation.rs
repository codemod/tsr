//! Anonymous mapped instantiation controls verified against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn anonymous_mapped_shapes_and_callback_binding() {
    let source = r#"// @strict: true
// @target: es2020
declare function box<T>(value: T): { readonly [K in keyof T]: { value: T[K] } };
export const object = box({ n: 1, s: "s" });
export const tuple = box([1, "s"] as const);
export const array = box([1, 2]);
declare const maybe: { a: number } | undefined;
export const union = box(maybe);
export const primitive = box(1);
declare function required<T>(value: T): { -readonly [K in keyof T]-?: T[K] };
declare const optional: { readonly a?: number; b: string };
export const complete = required(optional);

declare function pair<T, U>(value: T, extra: U): { [K in keyof T]: [T[K], U] };
export const paired = pair({ x: 1 }, true);
declare function keyD(): "d";
declare function keyCallbacks<T>(value: { [K in keyof T]: (arg: { key: K }) => void }): void;
keyCallbacks({ [keyD()]: ({ key }) => {} });
"#;
    let case = TestCase::parse("probe/anonymous-mapped", "inference.ts", source);
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
    for wanted in [
        "object : { readonly n: { value: number; }; readonly s: { value: string; }; }",
        r#"tuple : readonly [{ value: 1; }, { value: "s"; }]"#,
        "array : readonly { value: number; }[]",
        "union : { readonly a: { value: number; }; } | undefined",
        "primitive : number",
        "complete : { a: number; b: string; }",
        "paired : { x: [number, boolean]; }",
        r#"key : "d""#,
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
