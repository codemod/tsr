//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/spread_members", "probe.ts", source);
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
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn spread_members_retain_native_symbols_and_indexes() {
    expect(
        r"// @strict: true
// @target: esnext
interface Box<T> { value: T; method(x: T): T; }
declare let box: Box<number>;
export let instance = { ...box };
export let frozenMethod = { ...box } as const;
export let literalMethod = { ...{ m(x: number) { return x; } } };
declare let optionalMethod: { m?(x: number): string };
export let optionalCopy = { ...optionalMethod };
export let methodMerge = { ...literalMethod, ...optionalMethod };
export let setOnly = { ...{ set x(value: number) {} } };
export let getter = { ...{ get x() { return 1; } } };
class Base { base = true; inherited() {} }
class C extends Base { public own = 1; private secret = 'x'; protected hidden = 2; #private = 3; method() {} get accessor() { return 1; } }
declare let c: C;
export let classCopy = { ...c };
export let privateCollision = { secret: true, hidden: true, ...c };
class Parameter { constructor(private secret: string, public open: number) {} }
declare let parameter: Parameter;
export let parameterCopy = { secret: true, ...parameter };
declare let left: { [key: string]: number };
declare let right: { [other: string]: string };
export let one = { ...left };
export let both = { ...left, ...right };
export let drops = { ...left, a: true };
export let starts = { a: true, ...left };
export let readIndex = both['anything'];
export let readonlyIndex = { ...left } as const;
declare let maybeIndex: { [key: string]: number } | undefined;
export let partialIndex = { ...maybeIndex };

declare const unique: unique symbol;
declare let namedSymbol: { [unique]: string };
export let symbolCopy = { ...namedSymbol };
",
        &[
            "instance : { value: number; method(x: number): number; }",
            "frozenMethod : { readonly value: number; readonly method: (x: number) => number; }",
            "literalMethod : { m(x: number): number; }",
            "optionalCopy : { m?(x: number): string; }",
            "methodMerge : { m: ((x: number) => number) | ((x: number) => string); }",
            "setOnly : { x: undefined; }",
            "getter : { x: number; }",
            "classCopy : { base: boolean; own: number; }",
            "privateCollision : { base: boolean; own: number; }",
            "parameterCopy : { open: number; }",
            "one : { [key: string]: number; }",
            "both : { [x: string]: string | number; }",
            "drops : { a: boolean; }",
            "starts : { a: boolean; }",
            "readIndex : string | number",
            "readonlyIndex : { readonly [key: string]: number; }",
            "partialIndex : { [key: string]: number; }",
            "symbolCopy : { [unique]: string; }",
        ],
    );
}
