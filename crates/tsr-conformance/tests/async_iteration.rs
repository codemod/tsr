//! Bounded `ForAwaitOf` controls checked against pinned native tsgo 5b1047d.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @target: esnext
declare const promises: Iterable<Promise<"sync">>;
declare const asynchronous: AsyncIterable<Promise<"fast">>;
declare const mixed: AsyncIterable<"async"> | Iterable<Promise<37>>;
declare const invalidUnion: AsyncIterable<number> | { count: number };
declare const dual: {
    [Symbol.asyncIterator](): { next(): Promise<{done:false,value: "asyncFirst"}> };
    [Symbol.iterator](): { next(): {done:false,value: "syncSecond"} };
};
declare const structural: {
    [Symbol.asyncIterator](): { next(): Promise<{done:false,value: Promise<"unawaitedValue">}> };
};
declare const completed: { [Symbol.asyncIterator](): { next(): Promise<{done:true,value: string}> } };
declare const yielding: { [Symbol.asyncIterator](): { next(): Promise<{done:false,value: 73}> } };
declare const completedUnion: typeof completed | typeof yielding;
declare const neverYield: AsyncIterable<never>;
declare const malformed: { [Symbol.asyncIterator](): { next(): Promise<{}> } };
declare const partial: {
    [Symbol.asyncIterator](): { next(): Promise<{done:false} | {done:false,value: number}> };
};
declare const invalidNext: {
    [Symbol.asyncIterator](): { next: number };
    [Symbol.iterator](): { next(): {value: Promise<"fallback">} };
};
declare const optionalAsync: {
    [Symbol.asyncIterator]?: () => { next(): Promise<{value: "optional"}> };
    [Symbol.iterator](): { next(): {value: Promise<"fallbackOptional">} };
};
declare const requiredAsync: {
    [Symbol.asyncIterator](required: number): { next(): Promise<{value: "required"}> };
    [Symbol.iterator](): { next(): {value: Promise<"fallbackRequired">} };
};
declare const recoveredReturn: {
    [Symbol.asyncIterator](): { next: number; return(): Promise<{value: "returned"}> };
};
declare const inherited: Inherited;
interface Inherited extends AsyncIterable<"inherited"> {}
type Token = { token: "alias" };
declare const aliased: Iterable<Promise<Token>>;
export async function consume() {
    for await (const syncValue of promises) {}
    for await (const fastValue of asynchronous) {}
    for await (const mixedValue of mixed) {}
    for await (const invalidValue of invalidUnion) {}
    for await (const dualValue of dual) {}
    for await (const structuralValue of structural) {}
    for await (const completedValue of completed) {}
    for await (const completedUnionValue of completedUnion) {}
    for await (const neverValue of neverYield) {}
    for await (const malformedValue of malformed) {}
    for await (const partialValue of partial) {}
    for await (const invalidNextValue of invalidNext) {}
    for await (const optionalValue of optionalAsync) {}
    for await (const requiredValue of requiredAsync) {}
    for await (const returnedValue of recoveredReturn) {}
    for await (const inheritedValue of inherited) {}
    for await (const aliasValue of aliased) {}
    for (const ordinaryValue of promises) {}
}
export async function generic<T>(values: Iterable<T>, asyncValues: AsyncIterable<T>) {
    for await (const genericSyncValue of values) {}
    for await (const genericAsyncValue of asyncValues) {}
}
export async function constrained<T extends Iterable<Promise<"constrained">>>(values: T) {
    for await (const constrainedValue of values) {}
}
"#;

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/async_iteration", "async_iteration.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

#[test]
fn async_first_and_sync_fallback_preserve_native_yield_boundaries_and_reruns() {
    let wanted = [
        "syncValue : \"sync\"",
        "fastValue : \"fast\"",
        "mixedValue : \"async\" | 37",
        "invalidValue : any",
        "dualValue : \"asyncFirst\"",
        "structuralValue : Promise<\"unawaitedValue\">",
        "completedValue : any",
        "completedUnionValue : 73",
        "neverValue : never",
        "malformedValue : any",
        "partialValue : any",
        "invalidNextValue : \"fallback\"",
        "optionalValue : \"fallbackOptional\"",
        "requiredValue : \"fallbackRequired\"",
        "returnedValue : \"returned\"",
        "inheritedValue : \"inherited\"",
        "aliasValue : Token",
        "ordinaryValue : Promise<\"sync\">",
        "genericSyncValue : Awaited<T>",
        "genericAsyncValue : Awaited<T>",
        "constrainedValue : \"constrained\"",
    ];
    let first = assertions(SOURCE);
    for line in wanted {
        assert!(first.iter().any(|actual| actual == line), "missing {line}: {first:?}");
    }
    assert_eq!(first, assertions(SOURCE), "fresh Checker rerun changed assertions");
}

const INHERITANCE: &str = r#"// @strict: true
// @target: esnext
interface Base<T> extends AsyncIterable<T> {}
interface Derived<T> extends Base<T> {}
interface Merged<T> extends Iterable<Promise<T>> {}
interface Merged<T> { [Symbol.asyncIterator](): {next(): Promise<{value: "mergedAsync"}>}; }
declare const inherited: Derived<"genericInherited">;
declare const merged: Merged<"notSync">;
export async function consumeInherited() {
    for await (const inheritedGenericValue of inherited) {}
    for await (const mergedValue of merged) {}
}
"#;

#[test]
fn merged_and_inherited_generic_interfaces_keep_async_precedence() {
    let lines = assertions(INHERITANCE);
    for wanted in ["inheritedGenericValue : \"genericInherited\"", "mergedValue : \"mergedAsync\""]
    {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

const MODULE: &str = r"// @strict: true
// @target: esnext
// @filename: values.ts
export const values: Iterable<Promise<number>> = null as any;
// @filename: consumer.ts
import * as fileModule from './values';
declare const unknown: unknown;
export async function consumeModule() {
    for await (const moduleValue of fileModule) {}
    for await (const unknownValue of unknown) {}
}
";

#[test]
fn module_and_unknown_receivers_do_not_borrow_an_exported_iterables_yield() {
    let lines = assertions(MODULE);
    for wanted in ["moduleValue : any", "unknownValue : any"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
