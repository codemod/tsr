//! Mapped sequence controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn homomorphic_maps_transform_sequences_and_preserve_modifiers() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };
declare let array: Boxified<string[]>;
export const arrayResult = array;
declare let tuple: Boxified<[first:number, second?:string, ...tail:boolean[]]>;
export const tupleResult = tuple;
declare let frozen: Boxified<readonly [number,string]>;
export const frozenResult = frozen;
type OptionalBoxes<T> = { [P in keyof T]?: Box<T[P]> };
declare let optional: OptionalBoxes<[number,string]>;
export const optionalResult = optional;
type MutableBoxes<T> = { -readonly [P in keyof T]: Box<T[P]> };
declare let mutable: MutableBoxes<readonly [number,string]>;
export const mutableResult = mutable;
declare let primitive: Boxified<string>;
export const primitiveResult = primitive;
type Prefix<T extends unknown[]> = Boxified<[number,...T,string]>;
declare let concrete: Prefix<[boolean]>;
export const concreteResult = concrete;
declare function unbox<T extends unknown[]>(value: Boxified<T>): T;
export const inferred = unbox([{value:1},{value:"s"}]);
type ArrayBoxes<T extends readonly unknown[]> = { [P in keyof T]: Box<T[P]> };
declare let anyArray: ArrayBoxes<any>;
export const anyArrayResult = anyArray;
declare let intersection: Boxified<number[] & string[]>;
export const intersectionResult = intersection;
export function genericPop<T extends string[]>(value: Boxified<T>) { return value.pop(); }
export function genericReadonly<T extends string[]>(value: Readonly<Boxified<T>>) { return value[0]; }
"#;
    let case = TestCase::parse("probe/mapped-sequence", "mapped-sequence.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "arrayResult : Box<string>[]",
        "tupleResult : [first: Box<number>, second?: Box<string | undefined> | undefined, ...tail: Box<boolean>[]]",
        "frozenResult : readonly [Box<number>, Box<string>]",
        "optionalResult : [(Box<number> | undefined)?, (Box<string> | undefined)?]",
        "mutableResult : [Box<number>, Box<string>]",
        "primitiveResult : string",
        "concreteResult : [Box<number>, Box<boolean>, Box<string>]",
        "inferred : [number, string]",
        "anyArrayResult : Box<any>[]",
        "intersectionResult : Box<number>[] & Box<string>[]",
        "genericPop : <T extends string[]>(value: Boxified<T>) => Box<string> | undefined",
        "genericReadonly : <T extends string[]>(value: Readonly<Boxified<T>>) => Box<string>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
