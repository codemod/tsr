//! Native getContextualType's transparent non-null assertion parent, and
//! getNarrowableTypeForReference's concrete versus generic context boundary.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r"// @strict: true
// @target: es2020
export function concrete<K extends string>(source: { [P in K]: number | undefined }, key: K) {
    const accepted: number = source[key]!;
    return accepted;
}
export function parenthesized<K extends string>(source: { [P in K]: string | undefined }, key: K) {
    const accepted: string = (((source[key])!));
    return accepted;
}
export function nullableContext<K extends string>(source: { [P in K]: boolean | undefined }, key: K) {
    const accepted: boolean | undefined = source[key]!;
    return accepted;
}
export function free<K extends string>(source: { [P in K]: number | undefined }, key: K) {
    const held = source[key];
    return held;
}
export function freeAssertion<K extends string>(source: { [P in K]: string | undefined }, key: K) {
    const held = source[key]!;
    return held;
}
export function generic<T, K extends keyof T>(source: { [P in keyof T]: T[P] | undefined }, key: K) {
    const held: T[K] = source[key]!;
    return held;
}
export function aliased<T extends number | undefined>(source: T) {
    const accepted: number = source!;
    return accepted;
}
export function plain(source: number | undefined) {
    const accepted: number = source!;
    return accepted;
}
export function invalid<K extends string>(source: { [P in K]: number | undefined }, key: K) {
    const rejected: string = source[key]!;
    return rejected;
}
export function assertedReturn<K extends string>(source: { [P in K]: boolean | undefined }, key: K): boolean {
    return source[key]!;
}
";

#[test]
fn concrete_context_reaches_the_operand_without_removing_its_undefined() {
    assert_types(&[
        "source[key]! : number",
        "source[key] : number | undefined",
        "(((source[key])!)) : string",
        "(source[key]) : string | undefined",
        "source! : number",
        "source : number | undefined",
        "source[key]! : boolean",
        "source[key] : boolean | undefined",
        "nullableContext : <K extends string>(source: { [P in K]: boolean | undefined; }, key: K) => boolean",
    ]);
}

#[test]
fn free_and_generic_contexts_keep_the_indexed_receiver_identity() {
    assert_types(&[
        "free : <K extends string>(source: { [P in K]: number | undefined; }, key: K) => { [P in K]: number | undefined; }[K]",
        "freeAssertion : <K extends string>(source: { [P in K]: string | undefined; }, key: K) => NonNullable<{ [P in K]: string | undefined; }[K]>",
        "source[key]! : NonNullable<{ [P in keyof T]: T[P] | undefined; }[K]>",
        "source[key] : { [P in keyof T]: T[P] | undefined; }[K]",
    ]);
}

#[test]
fn incompatible_concrete_context_is_not_used_as_the_operand_type() {
    let case = TestCase::parse("probe/non-null-context", "non-null-context.ts", SOURCE);
    let actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    // Native rejects only `const rejected: string = source[key]!`.
    assert_eq!(actual, vec![(34, 11, 2322)]);
}

fn assert_types(wanted: &[&str]) {
    let case = TestCase::parse("probe/non-null-context", "non-null-context.ts", SOURCE);
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
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
