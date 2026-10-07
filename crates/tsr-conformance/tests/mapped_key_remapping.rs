//! Key remapping controls checked against pinned tsgo with strict mode enabled.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn remapped_keys_collisions_and_conditional_templates() {
    let source = r#"// @strict: true
// @target: es2020
type Getters<T> = { [K in keyof T & string as `get${Capitalize<K>}`]: () => T[K] };
declare const getters: Getters<{ foo: string; bar: number }>;
export const foo = getters.getFoo();
export const bar = getters.getBar();
declare function double<T>(value: T): { [K in keyof T & string as `${K}1` | `${K}2`]: T[K] };
export const doubled = double({ a: 1, b: "s" });
declare function merge<T>(value: T): { [K in keyof T as "value"]: T[K] };
export const merged = merge({ a: 1, b: "s" });
declare const optional: { readonly a?: number; b: string };
export const mergedOptional = merge(optional);
type DirectDefs<T extends {name: string; type: unknown}> = { [P in T as P['name']]: P['type'] };
declare const defs: DirectDefs<{name:"x";type:number} | {name:"y";type:string} | {name:"x";type:boolean}>;
export const x = defs.x;
export const y = defs.y;
export type Keys = keyof Getters<{ foo: string; bar: number }>;
declare function prefix<T>(value: T): { [K in keyof T & string as `prefix-${K}`]: T[K] };
export const quoted = prefix({ a: 1 });
declare function identity<T>(value: T): { [K in keyof T as K]: T[K] };
export const primitive = identity(1);
export const tupleLength = identity([1,"s"] as const).length;
type Empty = { [K in never as "value"]: number };
export type EmptyKeys = keyof Empty;

export function getterKeys() { let k!: Keys; return k; }
export function emptyKeys() { let k!: EmptyKeys; return k; }
type IndexValue<K> = [K] extends ["a"] ? 1 : 2;
declare const indexCollision: { [K in "a" | "b" as string]: IndexValue<K> };
export const indexValue = indexCollision.anything;
declare const filtered: { [K in "a" | "b" as Exclude<K,"a">]: K };
export const filteredValue = filtered.b;

declare function select<T, U>(value: T, marker: U): { [K in keyof T as T[K] extends U ? K : never]: T[K] };
export const selected = select({ a: 1, b: "s" }, "marker");
declare function conditionalValues<T>(value: T): { [K in keyof T]: T[K] extends number ? 1 : 2 };
export const checkedValues = conditionalValues({ a: 1, b: "s" });

declare function filterHandlers<T extends object>(data: T, handlers: {
    [K in keyof T as T[K] extends string ? K : never]: (value: T[K], key: K) => void
}): void;
filterHandlers({ foo: 0, bar: "" }, { bar: (validValue, validKey) => {} });
filterHandlers({ foo: 0, bar: "" }, { foo: (invalidValue, invalidKey) => {} });
"#;
    let case = TestCase::parse("probe/key-remapping", "mapped.ts", source);
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
        "foo : string",
        "bar : number",
        "doubled : { a1: number; a2: number; b1: string; b2: string; }",
        "merged : { value: string | number; }",
        "mergedOptional : { readonly value?: string | number | undefined; }",
        "x : number | boolean",
        "y : string",
        r#"quoted : { "prefix-a": number; }"#,
        "primitive : number",
        "tupleLength : 2",
        r#"getterKeys : () => "getBar" | "getFoo""#,
        r#"emptyKeys : () => "value""#,
        "indexValue : 1 | 2",
        r#"filteredValue : "b""#,
        "selected : { b: string; }",
        "checkedValues : { a: 1; b: 2; }",
        "validValue : string",
        r#"validKey : "bar""#,
        "invalidValue : any",
        "invalidKey : any",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
