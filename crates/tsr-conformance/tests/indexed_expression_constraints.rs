//! Expression indexes eagerly read base constraints after generic deferral.
//! Native: getIndexedAccessTypeOrUndefined / shouldDeferIndexedAccessType
//! (checker.go:26935, :27370), including `AccessFlagsNoIndexSignatures` on writes.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn concrete_expression_indexes_read_array_and_dictionary_constraints() {
    assert_types(
        r"// @strict: true
export function array<T extends string[]>(value: T, index: number) { return value[index]; }
export function dictionary<T extends { [key: string]: number }>(value: T, key: string) { return value[key]; }
export function chain<T extends { [key: number]: boolean }, U extends T>(value: U, key: number) { return value[key]; }
",
        &[
            "array : <T extends string[]>(value: T, index: number) => string",
            "dictionary : <T extends { [key: string]: number; }>(value: T, key: string) => number",
            "chain : <T extends { [key: number]: boolean; }, U extends T>(value: U, key: number) => boolean",
        ],
    );
}

#[test]
fn unchecked_expression_reads_include_undefined_but_numeric_writes_do_not() {
    assert_types(
        r#"// @strict: true
// @noUncheckedIndexedAccess: true
export function array<T extends string[]>(value: T, index: number) { return value[index]; }
export function dictionary<T extends { [key: string]: number }>(value: T, key: string) { return value[key]; }
export function write<T extends string[]>(value: T, index: number) { value[index] = "ok"; }
export function compound<T extends number[]>(value: T, index: number) { value[index] += 1; }
"#,
        &[
            "array : <T extends string[]>(value: T, index: number) => string | undefined",
            "dictionary : <T extends { [key: string]: number; }>(value: T, key: string) => number | undefined",
            "value[index] : string",
            "value[index] : number | undefined",
        ],
    );
}

#[test]
fn concrete_union_keys_distribute_over_generic_receiver_constraints() {
    assert_types(
        r#"// @strict: true
export function union<T extends { a: string; b: number }>(value: T, key: "a" | "b") { return value[key]; }
export function missing<T extends { a: string; b: number }>(value: T, key: "a" | "z") { return value[key]; }
export function unionReceiver<T extends { a: string; b: boolean }>(value: T | { a: number; b: boolean }, key: "a" | "b") { return value[key]; }
"#,
        &[
            "union : <T extends { a: string; b: number; }>(value: T, key: \"a\" | \"b\") => string | number",
            "missing : <T extends { a: string; b: number; }>(value: T, key: \"a\" | \"z\") => any",
            "unionReceiver : <T extends { a: string; b: boolean; }>(value: T | { a: number; b: boolean; }, key: \"a\" | \"b\") => string | number | boolean",
        ],
    );
}

#[test]
fn generic_key_deferral_and_generic_string_index_write_rejection_are_preserved() {
    assert_types(
        r"// @strict: true
export function symbolic<T, K extends keyof T>(value: T, key: K) { return value[key]; }
export function unrelated<T extends { [key: string]: number }, U, K extends keyof U>(value: T, key: K) { return value[key]; }
export function write<T extends { [key: string]: number }>(map: T, key: string) { map[key] = 42; }
export function unconstrained<T>(value: T, key: number) { return value[key]; }
export class Dict { [key: string]: unknown; set(key: string) { this[key] = 1; } }
",
        &[
            "symbolic : <T, K extends keyof T>(value: T, key: K) => T[K]",
            "unrelated : <T extends { [key: string]: number; }, U, K extends keyof U>(value: T, key: K) => any",
            "map[key] : error",
            "unconstrained : <T>(value: T, key: number) => any",
            "this[key] : unknown",
        ],
    );
}

fn assert_types(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/indexed-expression-constraints", "constraints.ts", source);
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
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
