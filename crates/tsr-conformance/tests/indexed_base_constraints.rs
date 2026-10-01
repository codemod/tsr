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

#[test]
fn intrinsic_aliases_optional_tuple_indexes_and_never_keys() {
    let source = r"// @strict: true
// @target: es2020
type Erased<T> = any;
type Text<T> = string;
type Empty<T> = never;
export function erased(value: Erased<number>) { return value; }
export function text(value: Text<number>) { return value; }
export function empty(value: Empty<number>) { return value; }
export function optional(value: [string, number?], index: number) { return value[index]; }
export function required(value: [string, number], index: number) { return value[index]; }
export function concreteNever(value: { a: string }, key: never) { return value[key]; }
export function concreteStringIndex(value: { [key: string]: number }, key: never) { return value[key]; }
export function noInfer<T>(value: NoInfer<T>) { return value; }
";
    assert_types(
        source,
        &[
            "erased : (value: any) => any",
            "text : (value: string) => string",
            "empty : (value: never) => never",
            "optional : (value: [string, number?], index: number) => string | number | undefined",
            "required : (value: [string, number], index: number) => string | number",
            "concreteNever : (value: { a: string; }, key: never) => never",
            "concreteStringIndex : (value: { [key: string]: number; }, key: never) => number",
            "value : NoInfer<T>",
        ],
    );
}

#[test]
fn concrete_indexes_preserve_recursive_objects_and_alias_substitution() {
    let source = r#"// @strict: true
// @target: es2020
interface Obj<T> { ref: T }
interface Rec { item: { value: string; ref?: Obj<Rec["item"]> } }
export function recursive(value: Rec["item"]) { return value.ref?.ref.value; }
type Length<T extends any[]> = T["length"];
export function length(value: Length<[string, number]>) { return value; }
type Tree<T, I extends any[] = []> = { 1: T; 0: { child: Tree<T, [any, ...I]> } }[Length<I> extends 2 ? 1 : 0];
export function terminal(value: Tree<string>) { return value.child.child; }
type Values = { a: string; b?: number };
type AB = Values["a" | "b"];
export function union(value: AB) { return value; }
export function empty(value: Values[never]) { return value; }
type Entry<T, K extends keyof T> = T[K];
export function symbolic<T, K extends keyof T>(value: Entry<T, K>) { return value; }
type Lookup<T> = { value: T }["value"];
export function text(value: Lookup<string>) { return value; }
export function number(value: Lookup<number>) { return value; }
interface Parent { inherited: number }
interface Child extends Parent { own: string }
export function inherited(value: Child[keyof Child]) { return value; }
"#;
    assert_types(
        source,
        &[
            "recursive : (value: Rec[\"item\"]) => string | undefined",
            "length : (value: 2) => 2",
            "terminal : (value: Tree<string>) => string",
            "union : (value: AB) => AB",
            "empty : (value: Values[never]) => never",
            "symbolic : <T, K extends keyof T>(value: Entry<T, K>) => Entry<T, K>",
            "text : (value: string) => string",
            "number : (value: number) => number",
            "inherited : (value: Child[keyof Child]) => string | number",
        ],
    );
}

#[test]
fn callable_structural_relations_resolve_mapped_method_filters() {
    let source = r"// @strict: true
// @target: es2020
type Fields<T> = { [K in keyof T]: T[K] extends Function ? never : K }[keyof T];
type Data<T> = { readonly [K in Fields<T>]: T[K] };
declare const data: Data<{ name: string; count: number; method(): void }>;
export const name = data.name;
export const count = data.count;
type IsFunction<T> = T extends Function ? true : false;
export function arrow(value: IsFunction<() => void>) { return value; }
export function constructor(value: IsFunction<new () => object>) { return value; }
export function primitive(value: IsFunction<string>) { return value; }
export function plainObject(value: IsFunction<{ value: number }>) { return value; }
type HasValue<T> = T extends { value: number } ? true : false;
export function requiredField(value: HasValue<() => void>) { return value; }
";
    assert_types(
        source,
        &[
            "data.name : string",
            "data.count : number",
            "arrow : (value: true) => true",
            "constructor : (value: true) => true",
            "primitive : (value: IsFunction<string>) => false",
            "plainObject : (value: IsFunction<{ value: number; }>) => false",
            "requiredField : (value: false) => false",
        ],
    );
}

#[test]
fn rejected_generic_overload_keeps_inference_for_its_failure_type() {
    let source = r"// @strict: false
// @target: es2020
declare function choose(): number;
declare function choose<T>(callback: (value: string) => T, other?: number): T;
export const value = choose((value: number) => value.toFixed());
";
    assert_types(source, &["choose((value: number) => value.toFixed()) : string"]);
}

#[test]
fn generic_indexed_objects_defer_until_their_arguments_are_known() {
    let source = r#"// @strict: true
// @target: es2020
type Mapped<T> = { [K in keyof T]: { item: T[K] } };
export function mapped<T extends { a: string }>(value: Mapped<T>["a"]) { return value; }
export function mappedItem<T extends { a: string }>(value: Mapped<T>["a"]) { return value.item; }
type Choice<T> = T extends string ? { a: 1 } : { a: 2 };
export function conditional<T>(value: Choice<T>["a"]) { return value; }
export function conditionalMember<T>(value: Choice<T>) { return value.a; }
export function union<T extends { a: string }>(value: (T | { a: number })["a"]) { return value; }
export function intersection<T extends { a: string }>(value: (T & { b: number })["a"]) { return value; }
export function fixed<T extends unknown[]>(value: [string, ...T][0]) { return value; }
export function rest<T extends unknown[]>(value: [string, ...T][1]) { return value; }
export function pattern(value: { [key: `a${string}`]: number }[`a${string}`]) { return value; }
export function stringIndex<T extends string>(value: { [key: string]: number }[T]) { return value; }
interface Box<T> { item: T }
export function box<T>(value: Box<T>["item"]) { return value; }
export const concrete = mapped<{ a: "literal" }>({ item: "literal" });
export const conditionalConcrete = conditional<string>(1);
type AsyncChoice<T> = T extends { select: string } ? Promise<1> : Promise<2>;
declare function chooseAsync<T extends { select?: string }>(value: T): AsyncChoice<T>;
export async function deferredAsync<T extends { select?: string }>(value: T) { return await chooseAsync(value); }
export const asyncConcrete = deferredAsync({ select: "yes" });
"#;
    assert_types(
        source,
        &[
            "mapped : <T extends { a: string; }>(value: Mapped<T>[\"a\"]) => Mapped<T>[\"a\"]",
            "mappedItem : <T extends { a: string; }>(value: Mapped<T>[\"a\"]) => T[\"a\"]",
            "conditional : <T>(value: Choice<T>[\"a\"]) => Choice<T>[\"a\"]",
            "conditionalMember : <T>(value: Choice<T>) => 1 | 2",
            "union : <T extends { a: string; }>(value: (T | { a: number; })[\"a\"]) => (T | { a: number; })[\"a\"]",
            "intersection : <T extends { a: string; }>(value: (T & { b: number; })[\"a\"]) => (T & { b: number; })[\"a\"]",
            "fixed : <T extends unknown[]>(value: [string, ...T][0]) => string",
            "rest : <T extends unknown[]>(value: [string, ...T][1]) => [string, ...T][1]",
            "pattern : (value: number) => number",
            "stringIndex : <T extends string>(value: number) => number",
            "box : <T>(value: Box<T>[\"item\"]) => T",
            "concrete : { item: \"literal\"; }",
            "conditionalConcrete : 1",
            "deferredAsync : <T extends { select?: string; }>(value: T) => Promise<AsyncChoice<T>>",
            "asyncConcrete : Promise<Promise<1>>",
        ],
    );
}

#[test]
fn constrained_sources_select_overloads_and_preserve_generic_indexes() {
    let source = r#"// @strict: true
// @target: es2020
// @noUncheckedIndexedAccess: true
declare function pick(value: string): "string";
declare function pick(value: number): "number";
declare function pick(value: string | number): "both";
declare function pick(value: unknown): "unknown";
export function text<T extends string>(value: T) { return pick(value); }
export function numeric<T extends number>(value: T) { return pick(value); }
export function both<T extends string | number>(value: T) { return pick(value); }
export function unconstrained<T>(value: T) { return pick(value); }
export function chained<T extends string, U extends T>(value: U) { return pick(value); }
export function nullable<T extends string | undefined>(value: T) { return pick(value); }
export function arrayElement<T extends string[]>(value: T[number]) { return pick(value); }
export function tupleIndex<N extends number>(value: ["a"][], key: N) { return value[key]; }
export function objectIndex<K extends string>(value: { [key: string]: string; a: string }, key: K) { return value[key]; }
interface Callable<T> { (value: T): T }
declare function preserve<T extends (value: string) => string>(value: T): T;
declare const callable: Callable<string>;
export const preserved = preserve(callable);
type Unwrap<T> = T extends null | undefined ? T : T extends PromiseLike<infer U> ? Unwrap<U> : T;
type CustomPromise<T> = { then<U>(f: ((value: T) => U | PromiseLike<U>) | null | undefined): CustomPromise<U> };
export type Unwrapped = Unwrap<Promise<string | Promise<CustomPromise<number> | null> | undefined>>;
interface Iterator<Input, Output> { (value: Input, index: any, list: any): Output }
declare function all<T>(list: T[], iterator?: Iterator<T, boolean>): T;
declare function identity<T>(value: T): T;
export const empty = all([], identity);
export const dynamic = all([true as any], identity);
type Wrapped<T> = { secret: T };
type Unbox<T> = T extends Wrapped<infer U> ? U : T;
declare function set<T, K extends keyof T>(object: T, key: K, value: Unbox<T[K]>): Unbox<T[K]>;
export class Box {
    prop!: Wrapped<string>;
    method() { return set(this, "prop", "hi"); }
}
"#;
    assert_types(
        source,
        &[
            "text : <T extends string>(value: T) => \"string\"",
            "numeric : <T extends number>(value: T) => \"number\"",
            "both : <T extends string | number>(value: T) => \"both\"",
            "unconstrained : <T>(value: T) => \"unknown\"",
            "chained : <T extends string, U extends T>(value: U) => \"string\"",
            "nullable : <T extends string | undefined>(value: T) => \"unknown\"",
            "arrayElement : <T extends string[]>(value: T[number]) => \"string\"",
            "tupleIndex : <N extends number>(value: [\"a\"][], key: N) => [\"a\"][][N]",
            "objectIndex : <K extends string>(value: { [key: string]: string; a: string; }, key: K) => { [key: string]: string; a: string; }[K]",
            "preserved : Callable<string>",
            "Unwrapped : string | number | null | undefined",
            "empty : never",
            "dynamic : any",
            "set(this, \"prop\", \"hi\") : Unbox<this[\"prop\"]>",
        ],
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
