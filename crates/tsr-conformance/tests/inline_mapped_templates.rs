//! Inline mapped template controls verified against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn inline_mapped_function_templates() {
    let source = r#"// @strict: true
// @target: es2020
declare function onlyCallbacks<T>(value: { [K in keyof T]: (value: T[K]) => void }): T;
export const callbackOnly = onlyCallbacks({ a: value => {} });
declare function read<T>(value: { [K in keyof T]: () => T[K] }): T;
export const result = read({ n: () => 1, s: () => "ok" });
declare const concrete: { [K in "a" | "b"]: (value: K) => K };
export const a = concrete.a("a");
export const b = concrete.b("b");

export function arrayGuard(value: string | number | ReadonlyArray<string | number>) {
    if (Array.isArray(value)) {
        const reversed = value.slice().reverse();
        return reversed;
    }
}
"#;
    let case = TestCase::parse("probe/inline-mapped", "inference.ts", source);
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
    for wanted in [
        "callbackOnly : unknown",
        "reversed : any[]",
        "result : { n: number; s: string; }",
        r#"a : "a""#,
        r#"b : "b""#,
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
