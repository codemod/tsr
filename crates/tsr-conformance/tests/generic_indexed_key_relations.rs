//! Native 5b1047d keyof contravariance, generic access admission, and
//! getPropertyTypeForIndexType's string-signature fallback.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @target: es2020
export function inherited<T, U extends T>(value: U, key: keyof T) { return value[key]; }
export function partial<T, U extends T>(value: Partial<U>, key: keyof T) { return value[key]; }
export function readonly<T, U extends T, K extends keyof T>(value: Readonly<U>, key: K) { return value[key]; }
export function reverse<T, U extends T>(value: T, key: keyof U) { return value[key]; }
export function unrelated<T, U>(value: T, key: keyof U) { return value[key]; }
export function ordered<T, U extends T>(a: keyof T, b: keyof U) { b = a; a = b; }
export function circular<T extends { [K in keyof T]: string }, K extends keyof T>(value: T, key: K) { return value[key]; }
type Items = { [key: string]: { name: string; rank: number } };
export function member<T extends Items, K extends keyof T>(value: T, key: K) { return value[key].rank; }
export type SymbolFallback = Items[symbol];
export type InvalidKey = Items[boolean];
export type NullableKey = Items[undefined];
export type NumericPreferred = { [key: string]: string | boolean; [key: number]: boolean }[number];
export type SymbolPreferred = { [key: string]: string; [key: symbol]: number }[symbol];
type DropText<T> = { [K in keyof T as Exclude<K, "text">]: T[K] };
export function remapped<T>(value: DropText<T>, key: keyof T) { return value[key]; }
type Difference<T, U> = T extends U ? never : T;
interface A { a: "a" }
export interface B extends A { b: "b"; difference: Difference<keyof this, keyof A>; }
export function difference(value: B["difference"]) { return value; }
type Entries = { text: string; count: number; 1: boolean };
declare const entries: Entries & { extra: Date };
export const concrete = inherited<Entries, Entries & { extra: Date }>(entries, "count");
"#;

#[test]
fn generic_keys_keep_the_receiver_and_do_not_reverse_the_constraint() {
    assert_types(&[
        "inherited : <T, U extends T>(value: U, key: keyof T) => U[keyof T]",
        "partial : <T, U extends T>(value: Partial<U>, key: keyof T) => Partial<U>[keyof T]",
        "readonly : <T, U extends T, K extends keyof T>(value: Readonly<U>, key: K) => Readonly<U>[K]",
        "reverse : <T, U extends T>(value: T, key: keyof U) => any",
        "unrelated : <T, U>(value: T, key: keyof U) => any",
        "circular : <T extends { [K in keyof T]: string; }, K extends keyof T>(value: T, key: K) => T[K]",
        "remapped : <T>(value: DropText<T>, key: keyof T) => any",
        "concrete : string | number | boolean",
    ]);
}

#[test]
fn base_constraints_keep_index_signature_precedence_and_generic_conditionals() {
    assert_types(&[
        "member : <T extends Items, K extends keyof T>(value: T, key: K) => number",
        "NumericPreferred : boolean",
        "SymbolPreferred : number",
        "difference : Difference<keyof this, \"a\">",
        "difference : (value: B[\"difference\"]) => \"b\" | \"difference\"",
    ]);
}

fn assert_types(wanted: &[&str]) {
    let case = TestCase::parse("probe/generic-indexed-keys", "keys.ts", SOURCE);
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
