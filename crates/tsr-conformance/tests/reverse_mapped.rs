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

/// Recursive reverse mappings, optional templates and the node builder's
/// nested-placeholder rule, each answer checked against pinned tsgo output.
#[test]
fn recursive_reverse_mappings_resolve_members_on_demand() {
    let source = r#"// @strict: true
// @target: es2015
type Func<T> = (...args: any[]) => T;
type Spec<T> = { [P in keyof T]: Func<T[P]> | Spec<T[P]> };
declare function applySpec<T>(obj: Spec<T>): (...args: any[]) => T;
export const spec = applySpec({ sum: (a: any) => 3, nested: { mul: (b: any) => "n" } });
export const deeper = applySpec({ foo: { bar: { baz: (x: any) => true } } });
declare function validate<T>(obj: { [P in keyof T]?: T[P] }): T;
type Foo = { a?: number; readonly b: string };
declare const foo: Foo;
export const validated = validate(foo);
interface Link { next: Link }
type Deep<T> = { [K in keyof T]: Deep<T[K]> };
declare function undeep<T>(deep: Deep<T>): T;
declare const link: Link;
export const cyclic = undeep(link);
export const cyclicNext = cyclic.next;
export const cyclicDeeper = cyclic.next.next.next;
type Reducer<S> = (state: S) => S;
declare function combine<S>(reducers: { [K in keyof S]: Reducer<S[K]> }): Reducer<S>;
declare const count: Reducer<number>;
export const combined = combine({ inner: combine({ count }) });
"#;
    let case = TestCase::parse("probe/reverse-recursive", "reverse-recursive.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "spec : (...args: any[]) => { sum: number; nested: { mul: string; }; }",
        "deeper : (...args: any[]) => { foo: { bar: { baz: boolean; }; }; }",
        "validated : { a: number; readonly b: string; }",
        "cyclic : { next: { next: any; }; }",
        "cyclicNext : { next: { next: any; }; }",
        "cyclicDeeper : { next: { next: any; }; }",
        "combined : Reducer<{ inner: { count: number; }; }>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// `getLiteralTypeFromProperty` (`inference.go`) retains the written-name
/// provenance of reverse mapped properties: numeric syntax is a string key in
/// `keyof` inference, distinct from the ordinary `keyof { 1: T }` number key.
#[test]
fn reverse_mapped_key_inference_preserves_numeric_name_origins() {
    let source = r#"// @strict: true
type BoxPick<T, K extends keyof T> = { [P in K]: { value: T[P] } };
declare function numeric<T, K extends keyof T>(value: BoxPick<T, K>): [T, K];
export const written = numeric({ 1: { value: 1 }, "2": { value: "x" } });
"#;
    let case = TestCase::parse("probe/reverse-numeric-keys", "reverse-numeric-keys.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    let wanted = "written : [{ 1: number; \"2\": string; }, \"1\" | \"2\"]";
    assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
}

/// Reverse index display is a placeholder, not the semantic value type.
/// Recursive index reads must resolve pending members instead of bottoming out
/// at the expanding-stack guard during the creation of the outer object.
#[test]
fn reverse_mapped_indexes_elide_display_but_preserve_typed_reads() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T> = { value: T };
type Image<T> = { [K in keyof T]: Box<T[K]> };
declare function unbox<T>(value: Image<T>): T;
declare const numbers: { [key: string]: Box<number> };
declare const strings: { [key: string]: Box<string> };
export const reversedNumbers = unbox(numbers);
export const reversedStrings = unbox(strings);
export const numberRead = reversedNumbers["first"];
export const stringRead = reversedStrings.second;
declare function inferRead<T>(value: { item: T }): T;
export const inferredNumber = inferRead({ item: reversedNumbers["other"] });
export const inferredString = inferRead({ item: reversedStrings.other });
declare const ordinary: { [key: string]: number };
export const ordinaryIndex = ordinary;
interface Chain { [key: string]: Chain }
type Deep<T> = { [K in keyof T]: Deep<T[K]> };
declare function undeep<T>(value: Deep<T>): T;
declare const chain: Chain;
export const reversedChain = undeep(chain);
export const oneRead = reversedChain.first;
export const manyReads = reversedChain.first.second.third.fourth.fifth;
"#;
    let case = TestCase::parse("probe/reverse-index", "reverse-index.ts", source);
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
        "reversedNumbers : { [x: string]: any; }",
        "reversedStrings : { [x: string]: any; }",
        "numberRead : number",
        "stringRead : string",
        "inferredNumber : number",
        "inferredString : string",
        "ordinaryIndex : { [key: string]: number; }",
        "reversedChain : { [x: string]: any; }",
        "oneRead : { [x: string]: any; }",
        "manyReads : { [x: string]: any; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// Native `resolveReverseMappedTypeMembers` filters by the source name type,
/// including computed names whose semantic key has no raw binder member.
/// Numeric-looking string keys must not pass the number constraint.
#[test]
fn reverse_mapped_filters_use_captured_computed_name_origins() {
    let source = r"// @strict: true
// @target: es2015
type Box<T> = { value: T };
declare function numbers<T>(value: { [K in keyof T & number]: Box<T[K]> }): T;
declare function strings<T>(value: { [K in keyof T & string]: Box<T[K]> }): T;
declare const positive: 3;
declare const negative: -4;
declare const quoted: '8';
const input = {
    [positive]: { value: 23 },
    [negative]: { value: 'negative' },
    [quoted]: { value: false },
    '01': { value: 37 },
    0: { value: 'zero' },
    4294967294: { value: true },
    '4294967295': { value: 'boundary' }
};
export const numericResult = numbers(input);
export const stringResult = strings(input);
declare const ordinary: { 3: Box<boolean>; '8': Box<string> };
export const ordinaryNumbers = numbers(ordinary);
export const ordinaryStrings = strings(ordinary);
";
    let case = TestCase::parse("probe/reverse-captured-keys", "reverse-captured-keys.ts", source);
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
    // Independently checked with tsgo 5b1047d declarations, --strict --target es2015.
    for wanted in [
        "numericResult : { 3: number; [-4]: string; 0: string; 4294967294: boolean; }",
        "stringResult : { \"8\": boolean; '01': number; '4294967295': string; }",
        "ordinaryNumbers : { 3: boolean; }",
        "ordinaryStrings : { '8': string; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
    assert!(tsr_conformance::diagnostics_suite::reported_for(&case).is_empty());
}
