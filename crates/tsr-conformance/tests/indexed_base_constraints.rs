//! Base-constraint and flow controls checked against the pinned tsgo revision.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn indexed_base_constraints_and_contextual_narrowing() {
    let source = r"// @strict: true
// @target: es2020
type Numbers = { a: 1; b: 2 };
type OptionalNumbers = { [K in keyof Partial<Numbers>]: Numbers[K] };
export function numberMethod<K extends keyof Numbers>(value: Numbers[K]) { return value.toFixed(); }
export function optionalMethod<K extends keyof Numbers>(value: OptionalNumbers[K]) { return value?.toFixed(); }
export function indexed<K extends keyof Numbers>(value: Numbers, key: K) { return value[key].toFixed(); }
export function chain<A extends number, B extends A, C extends B>(value: C) { return value.toFixed(); }
export function unionConstraint<T extends string | undefined>(value: T) { return value?.length; }
export function symbolic<T, K extends keyof T>(value: T, key: K) { return value[key]; }
export function invalid<T, U extends T, K extends keyof U>(value: T, key: K) { return value[key]; }
declare const numeric: { [K in 1 | 2]?: string };
export function numericOptional<K extends 1 | 2>(key: K) { return numeric[key]?.length; }
type OptionalBox<T> = { [K in keyof T]?: { item: T[K] } };
export function nested<T, K extends keyof T>(value: OptionalBox<T>[K]) { return value?.item; }

type OptionalNumeric = { [K in keyof Partial<{ 1: 1; 2: 2 }>]: K };
export function numericInherited<K extends 1 | 2>(value: OptionalNumeric[K]) { return value?.toFixed(); }
export function concreteContext<T extends string | undefined>(value: T) { if (value) { const result: string = value; return result; } }
export function genericContext<T extends string | undefined>(value: T, other: NonNullable<T>) { other = value; return value; }
interface GenericBase<T> { set<K extends keyof this>(key: K, value: this[K]): this[K]; use(value: T): void; }
interface GenericStructure { set<K extends keyof this>(key: K, value: this[K]): this[K]; use(value: number): void; }
type ExtractBase<T> = T extends GenericBase<infer U> ? U : never;
export type Extracted = ExtractBase<GenericStructure>;

";
    assert_types(
        source,
        &[
            "numberMethod : <K extends keyof Numbers>(value: Numbers[K]) => string",
            "optionalMethod : <K extends keyof Numbers>(value: OptionalNumbers[K]) => string | undefined",
            "indexed : <K extends keyof Numbers>(value: Numbers, key: K) => string",
            "chain : <A extends number, B extends A, C extends B>(value: C) => string",
            "unionConstraint : <T extends string | undefined>(value: T) => number | undefined",
            "symbolic : <T, K extends keyof T>(value: T, key: K) => T[K]",
            "invalid : <T, U extends T, K extends keyof U>(value: T, key: K) => any",
            "numericOptional : <K extends 1 | 2>(key: K) => number | undefined",
            "nested : <T, K extends keyof T>(value: OptionalBox<T>[K]) => T[K] | undefined",
            "numericInherited : <K extends 1 | 2>(value: OptionalNumeric[K]) => string | undefined",
            "concreteContext : <T extends string | undefined>(value: T) => string | undefined",
            "genericContext : <T extends string | undefined>(value: T, other: NonNullable<T>) => T",
            "other = value : T",
            "Extracted : number",
        ],
    );
}

#[test]
fn tuple_members_keep_the_tuple_this_argument() {
    let source = r"// @strict: true
// @target: es2015
interface Array<T> { tupleSelf(): this; }
function tupleThis() { const value: [number, string] = [42, 'hello']; return value.tupleSelf(); }
function tupleArray() { const value: [number, string] = [42, 'hello']; return value.slice(1); }
";
    assert_types(
        source,
        &["tupleThis : () => [number, string]", "tupleArray : () => (string | number)[]"],
    );
}

fn assert_types(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/indexed-base-constraints", "constraints.ts", source);
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
