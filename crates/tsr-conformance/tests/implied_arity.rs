//! Adjacent variadic tuple controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn supplied_rest_arity_splits_adjacent_variadic_parameters() {
    let source = r#"// @strict: true
// @target: es2015
declare function bind<A extends unknown[], B extends unknown[], R>(f: (...args: [...A, ...B]) => R, ...args: A): (...args: B) => R;
declare function work(a: number, b: string, c: boolean): Date;
const zero = bind(work);
const one = bind(work, 1);
const two = bind(work, 1, "x");
const all = bind(work, 1, "x", true);
const optional = bind((a: number, b?: string) => a, 1);
"#;
    let case = TestCase::parse("probe/implied-arity", "implied-arity.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "zero : (a: number, b: string, c: boolean) => Date",
        "one : (b: string, c: boolean) => Date",
        "two : (c: boolean) => Date",
        "all : () => Date",
        "optional : (b?: string | undefined) => number",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
