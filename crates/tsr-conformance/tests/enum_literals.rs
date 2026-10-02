//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/enum_literals", "probe.ts", source);
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
fn enum_keys() {
    expect(
        r"// @strict: true
// @target: esnext
enum Words { A = 'a', B = 'b', Quoted = 'a b' }
enum Numbers { Zero, One, Negative = -2, Bits = 1 << 3 }
enum Single { Item = 'item' }
enum Equal { A = 'same', B = 'same' }
export let words = { [Words.A]: 1, [Words.B]: true, [Words.Quoted]: 'x' };
export let numbers = { [Numbers.Zero]: 'zero', [Numbers.One]: 1, [Numbers.Negative]: true, [Numbers.Bits]: 'bits' };
export let single = { [Single.Item]: 1 };
export let equal = { [Equal.A]: 1, [Equal.B]: true };
export let wordsRead = words[Words.A];
export let quotedRead = words[Words.Quoted];
export let numberRead = numbers[Numbers.Negative];
export let singleRead = single[Single.Item];
export let equalRead = equal[Equal.B];
export let copied = { ...words, [Words.A]: false };
export let methods = { [Words.Quoted]() { return 1; }, [Numbers.Negative]() { return true; } };
export let constant = { [Words.A]: Words.A, [Numbers.One]: Numbers.One } as const;
export let array = [Words.A, Words.B];
export let literalUnion: Words.A | Words.B;
export let mixedUnion: Words.A | 'a';
export let numericUnion: Numbers.One | 1;
export function stringMethod(value: Words.A) { return value.toUpperCase(); }
export function numberMethod(value: Numbers.One) { return value.toFixed(); }
",
        &[
            "words : { a: number; b: boolean; \"a b\": string; }",
            "numbers : { 0: string; 1: number; [-2]: boolean; 8: string; }",
            "single : { item: number; }",
            "equal : { same: boolean; }",
            "wordsRead : number",
            "quotedRead : string",
            "numberRead : boolean",
            "singleRead : number",
            "equalRead : boolean",
            "copied : { b: boolean; \"a b\": string; a: boolean; }",
            "methods : { \"a b\"(): number; [-2](): boolean; }",
            "constant : { readonly a: Words.A; readonly 1: Numbers.One; }",
            "array : Words[]",
            "stringMethod : (value: Words.A) => string",
            "numberMethod : (value: Numbers.One) => string",
        ],
    );
}

#[test]
fn enum_semantics() {
    expect(
        r"// @strict: true
// @target: esnext
export enum Numeric { Zero, One, Two }
export enum Text { Empty = '', A = 'a', B = 'b' }
declare let number: Numeric;
declare let text: Text;
declare let truthy: Text.A | Text.B;
export const notTruthy = !truthy;
export const notZero = !Numeric.Zero;
export const notOne = !Numeric.One;
export const notEmpty = !Text.Empty;
export let numericAnd = number && number;
export let numericOr = number || number;
export let textAnd = text && text;
export function narrowText(value: Text) { if (value) { return value; } }
declare function pick(value: Text.A): 'first';
declare function pick(value: Text.B): 'second';
declare function pick(value: Text): 'other';
export const pickedA = pick(Text.A);
export const pickedB = pick(Text.B);
declare function numeric(value: Numeric.One): 'one';
declare function numeric(value: number): 'number';
export const pickedOne = numeric(Numeric.One);
export const pickedTwo = numeric(Numeric.Two);
export const regular: Numeric.One = Numeric.One;
export let regularCopy = regular;
export let freshCopy = Numeric.One;
enum Index { Zero='0', NotNumeric='00' }
export let arrayRead = ['x'][Index.Zero];
export let badArrayRead = ['x'][Index.NotNumeric];
",
        &[
            "notTruthy : false",
            "notZero : true",
            "notOne : false",
            "notEmpty : true",
            "numericAnd : Numeric",
            "numericOr : Numeric",
            "textAnd : Text",
            "narrowText : (value: Text) => Text.A | Text.B | undefined",
            "pickedA : \"first\"",
            "pickedB : \"second\"",
            "pickedOne : \"one\"",
            "pickedTwo : \"number\"",
            "regularCopy : Numeric.One",
            "freshCopy : Numeric",
            "arrayRead : string",
        ],
    );
}

#[test]
fn property_index_inference() {
    expect(
        r"// @strict: true
// @target: esnext
enum Key { Zero, One }
declare function numeric<T>(arg: { [key:number]: T }): T;
declare function stringy<T>(arg: { [key:string]: T }): T;
export const numericEnum = numeric({[Key.Zero]: 'x'});
export const numericLiteral = numeric({0:'x',1: 'y',other: false});
export const stringLiteral = stringy({a: 'x',b: 'y'});
export const filtered = numeric({'01': false, '1': 'x'});
declare let optional: {0?: string};
export const optionalValue = numeric(optional);
interface Declared { 0: string }
declare let declared: Declared;
export const interfaceValue = numeric(declared);
declare let indexed: {[key:string]: number};
export const compatibleIndex = numeric(indexed);
",
        &[
            "numericEnum : string",
            "numericLiteral : string",
            "stringLiteral : string",
            "filtered : string",
            "optionalValue : string",
            "compatibleIndex : number",
        ],
    );
}

#[test]
fn enum_context_after_inference() {
    expect(
        r"// @strict: false
// @target: esnext
export enum Kind { A = 'a', B = 'b' }
export function assign<T>(source: T) { return Object.assign({}, source, {kind: Kind.A}); }
declare function identity<T>(t: T): T;
export function nested<T>(source: T) { return identity(Object.assign({}, source, {kind: Kind.A})); }
export const deferred = <T>(source: T) => ({make: () => identity(Object.assign({}, source, {kind: Kind.A}))});
",
        &[
            "assign : <T>(source: T) => T & { kind: Kind; }",
            "nested : <T>(source: T) => T & { kind: Kind; }",
            "deferred : <T>(source: T) => { make: () => T & { kind: Kind; }; }",
            "{kind: Kind.A} : { kind: Kind.A; }",
        ],
    );
}

#[test]
fn non_strict_enum_truthiness() {
    expect(
        r"// @strict: false
// @target: esnext
enum Numeric { Zero, One, Two }
enum Text { Empty = '', A = 'a', B = 'b' }
declare let number: Numeric;
declare let truthy: Text.A | Text.B;
export const notTruthy = !truthy;
export const notZero = !Numeric.Zero;
export const notOne = !Numeric.One;
export const notEmpty = !Text.Empty;
export let numericAnd = number && number;
export let numericOr = number || number;
",
        &[
            "notTruthy : boolean",
            "notZero : true",
            "notOne : boolean",
            "notEmpty : true",
            "numericAnd : Numeric",
            "numericOr : Numeric",
        ],
    );
}

#[test]
fn non_generic_context_keeps_loop_flow() {
    // Native controlFlowIterationErrorsAsync, with its explicit return type.
    expect(
        r"// @strict: true
// @target: esnext
declare function myQuery(input: { lastId: number | undefined }): Promise<{ entities: number[] }>;
async function myFunc(): Promise<void> {
    let lastId: number | undefined = undefined;
    while (true) {
        const { entities } = await myQuery({lastId});
        lastId = entities[entities.length - 1];
    }
}",
        &[
            "entities : number[]",
            "lastId = entities[entities.length - 1] : number",
            "entities[entities.length - 1] : number",
        ],
    );
}
