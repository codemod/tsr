//! Declaration outcomes verified against the pinned native tsgo executable.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/spread-inference", "spread.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

#[test]
fn generic_rest_spreads_preserve_effective_tuple_elements() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
export declare function tuple<T extends unknown[]>(...args: T): T;
export declare function literal<T extends (string | number | boolean)[]>(...args: T): T;
export declare function frozen<const T extends readonly unknown[]>(...args: T): T;
declare const pair: [number, string];
declare const readonlyPair: readonly [1, "x"];
declare const optionalPair: [number, string?];
declare const tail: [number, ...string[]];
declare const numbers: number[];
declare const strings: readonly string[];
declare const iterator: Set<number>;
export const a = tuple(...pair);
export const b = tuple(true, ...pair);
export const c = tuple(...pair, false);
export const d = tuple(...readonlyPair);
export const e = tuple(...optionalPair);
export const f = tuple(...tail);
export const g = tuple(...numbers);
export const h = tuple(...strings);
export const i = tuple(...iterator);
export const j = tuple(1, ...numbers, "x");
export const k = literal(1, ...pair);
export const l = frozen(1, ...readonlyPair);
export const m = tuple(...numbers, ...strings);
export function identity<U extends string[]>(u: U) { return tuple(...u); }
export function readonlyIdentity<U extends readonly string[]>(u: U) { return tuple(...u); }
export function prefix<U extends string[]>(u: U) { return tuple(1, ...u); }
"#,
    );
    for wanted in [
        "a : [number, string]",
        "b : [boolean, number, string]",
        "c : [number, string, boolean]",
        "d : [1, \"x\"]",
        "e : [number, string | undefined]",
        "f : [number, ...string[]]",
        "g : number[]",
        "h : string[]",
        "i : number[]",
        "j : [number, ...number[], string]",
        "k : [1, number, string]",
        "l : readonly [1, 1, \"x\"]",
        "m : (string | number)[]",
        "identity : <U extends string[]>(u: U) => U",
        "readonlyIdentity : <U extends readonly string[]>(u: U) => [...U]",
        "prefix : <U extends string[]>(u: U) => [number, ...U]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn spread_inference_preserves_labels_unions_and_iterables() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
export declare function tuple<T extends unknown[]>(...args:T):T;
export declare function pair<X,T extends unknown[]>(x:X,...args:T):[X,T];
declare let anyValue:any;
declare let union: readonly [1] | readonly [2, 3];
declare let opt: [first:number,last?:string];
declare let variadic: [first:number,...rest:string[]];
export const anySpread=tuple(...anyValue);
export const anyMixed=tuple(1,...anyValue);
export const unionSpread=tuple(...union);
export const optSpread=tuple(...opt);
export const mixed=pair(1,"x",...variadic);
export const named=tuple(...variadic,true);
export const primitive=tuple(..."hi");
"#,
    );
    for wanted in [
        "anySpread : any",
        "anyMixed : [number, ...any[]]",
        "unionSpread : [1] | [2, 3]",
        "optSpread : [first: number, last: string | undefined]",
        "mixed : [number, [string, first: number, ...rest: string[]]]",
        "named : [first: number, ...rest: string[], boolean]",
        "primitive : string[]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn primitive_and_literal_rest_constraints_preserve_literals() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
export declare const key: unique symbol;
export declare function literals<T extends ("x" | "y")[]>(...args:T):T;
export declare function symbols<T extends symbol[]>(...args:T):T;
export declare function booleans<T extends boolean[]>(...args:T):T;
export const a = literals("x");
export const b = symbols(key);
export const c = booleans(true, false);
export declare function plain<T extends unknown[]>(...args:T):T;
export const widened = plain("x", true, 1);
"#,
    );
    for wanted in ["a : [\"x\"]", "c : [true, false]", "widened : [string, boolean, number]"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn discriminated_mapped_properties_use_instantiated_contexts() {
    // Reduced from the pinned correlatedUnions.types baseline: these property
    // assertions are widened even though their template uses TypeMap[P].
    let lines = assertions(
        r#"// @strict: true
    type TypeMap = { foo: string, bar: number };
    type DataEntry<K extends keyof TypeMap = keyof TypeMap> = {
        [P in K]: { type: P, data: TypeMap[P] }
    }[K];
    const entries: DataEntry[] = [
        { type: "foo", data: "abc" },
        { type: "foo", data: "def" },
        { type: "bar", data: 42 },
    ];
"#,
    );
    assert_eq!(
        lines.iter().filter(|line| line.as_str() == "data : string").count(),
        2,
        "{lines:?}"
    );
    assert_eq!(
        lines.iter().filter(|line| line.as_str() == "data : number").count(),
        1,
        "{lines:?}"
    );
}

#[test]
fn exact_optional_tuple_spreads_still_supply_undefined() {
    let lines = assertions(
        r"// @strict: true
// @exactOptionalPropertyTypes: true
declare function tuple<T extends unknown[]>(...args:T):T;
declare const optional: [number, string?];
const result = tuple(...optional);
",
    );
    assert!(lines.iter().any(|line| line == "result : [number, string | undefined]"), "{lines:?}");
}
