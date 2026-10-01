//! Reverse mapped controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn reverse_homomorphic_templates_infer_objects_arrays_and_tuples() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };
declare function unboxify<T>(obj: Boxified<T>): T;
const object = unboxify({ a: { value: 1 }, b: { value: "x" } });
const array = unboxify([{ value: 1 }, { value: 2 }]);
const tuple = unboxify([{ value: 1 }, { value: "x" }] as const);
declare function unwrap<T>(obj: { [P in keyof T]: { value: T[P] } }): T;
const anonymous = unwrap({ a: { value: true } });
declare function partial<T>(obj: Partial<T>): T;
const identity = partial({ x: 1 });
const explicitArray = unboxify<number[]>([{ value: 1 }, { value: 2 }]);
const readonlyObject = unboxify({ a: { value: 1 } } as const);
type Nested<T> = { [K in keyof T]: { inner: { value: T[K] } } };
declare function nested<R>(value: Nested<R>): R;
const deep = nested({ a: { inner: { value: "x" } } });
type Conditional<T extends unknown[]> = { [K in keyof T]: T[K] extends T[number] ? { primitive: T[K] } : never };
declare function extract<T extends unknown[]>(...args: Conditional<T>): T;
const conditional = extract({ primitive: "x" }, { primitive: 1 });
type PrimitiveBox<Primitive extends any> = { primitive: Primitive };
type Old<Tuple extends any[]> = { [Key in keyof Tuple]: Tuple[Key] extends Tuple[number] ? PrimitiveBox<Tuple[Key]> : never };
type ConcreteOld = Old<[string, number]>;
declare function extractOld<Tuple extends any[]>(...args: Old<Tuple>): Tuple;
const namedConditional: [string, number] = extractOld({ primitive: "" }, { primitive: 0 });
type New<Tuple extends any[]> = { [Key in keyof Tuple]: PrimitiveBox<Tuple[Key]> };
type ConcreteNew = New<[string, number]>;
declare function extractNew<Tuple extends any[]>(...args: New<Tuple>): Tuple;
const namedTemplate: [string, number] = extractNew({ primitive: "" }, { primitive: 0 });
"#;
    let case = TestCase::parse("probe/reverse-mapped", "reverse-mapped.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "object : { a: number; b: string; }",
        "array : [number, number]",
        "tuple : readonly [1, \"x\"]",
        "anonymous : { a: boolean; }",
        "identity : { x: number; }",
        "explicitArray : number[]",
        "readonlyObject : { readonly a: 1; }",
        "deep : { a: string; }",
        "conditional : [string, number]",
        "extractOld({ primitive: \"\" }, { primitive: 0 }) : [string, number]",
        "extractNew({ primitive: \"\" }, { primitive: 0 }) : [string, number]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
