//! Outcomes verified against pinned native tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case =
        TestCase::parse("probe/destructuring_iteration", "destructuring_iteration.ts", source);
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

fn expect(lines: &[String], wanted: &[&str]) {
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn array_like_indexing_preserves_unchecked_and_defaults() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
// @noUncheckedIndexedAccess: true
declare const values: string[];
const [arrayHead,...arrayRest] = values;
declare const tuple: [number,string?];
const [required,optional] = tuple;
declare const custom: {[Symbol.iterator]():{next():{value:number | undefined}}};
const [yielded=1,...yieldedRest] = custom;
declare const union: [string] | [number,boolean];
const [unionHead,unionOptional] = union;
declare const fixed: [string];
const [,defaulted=1] = fixed;
export const results = {arrayHead,arrayRest,required,optional,yielded,yieldedRest,unionHead,unionOptional,defaulted};
",
    );
    expect(
        &lines,
        &[
            "arrayHead : string | undefined",
            "arrayRest : string[]",
            "required : number",
            "optional : string | undefined",
            "yielded : number",
            "yieldedRest : (number | undefined)[]",
            "unionHead : string | number",
            "unionOptional : boolean | undefined",
            "defaulted : number",
        ],
    );
}

#[test]
fn generic_and_readonly_rest_slices() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
export function variadic<T extends [number,...string[]]>(value:T) { const [head,...tail] = value; return {head,tail}; }
export function genericTail<T extends unknown[]>(value:[number,...T]) { const [head,...tail] = value; return {head,tail}; }
declare const labeled: readonly [head:number, tail?:string];
const [labelHead,...labelTail] = labeled;
export const labelResults = {labelHead,labelTail};
declare const empty: readonly [];
const [...emptyRest] = empty;
export const emptyResult = emptyRest;
",
    );
    expect(
        &lines,
        &[
            "head : number",
            "tail : string[]",
            "tail : [...T]",
            "labelHead : number",
            "labelTail : [tail?: string | undefined]",
            "emptyResult : []",
        ],
    );
}

#[test]
fn inherited_index_values_compose_heritage_mappers() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
interface Base<T> { [key:number]:T }
interface Middle<U> extends Base<U[]> {}
interface Final extends Middle<string> {}
declare const nested: Final;
export const nestedValue = nested[0];
interface Narrow extends Base<number> { [key:number]:42 }
declare const narrow: Narrow;
export const narrowValue = narrow[0];
interface Generic<V> extends Base<V> {}
declare const generic: Generic<boolean>;
export const genericValue = generic[0];
interface Dictionary<T> { [key:string]:T }
interface NamedDictionary<U> extends Dictionary<U[]> {}
declare const dictionary: NamedDictionary<number>;
export const dictionaryValue = dictionary['key'];
interface Indices extends Array<number> { 0:42 }
declare const indices: Indices;
const [first,second,...rest] = indices;
export const indexed = {first,second,rest};
",
    );
    expect(
        &lines,
        &[
            "nestedValue : string[]",
            "narrowValue : 42",
            "genericValue : boolean",
            "dictionaryValue : number[]",
            "first : 42",
            "second : number",
            "rest : number[]",
        ],
    );
}

#[test]
fn tuple_union_rest_and_non_array_iterators_use_distinct_paths() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const union: [number,string,string] | [string,number,number];
export const [head,...tail] = union;
declare const mix: [boolean,string] | string[];
export const [mixedHead,...mixedTail] = mix;
export function generic<T extends [number,string,string] | [string,number,number]>(value:T) {
    const [first,...rest] = value;
    return {first,rest};
}
declare const readonlyUnion: readonly [head:number,last?:string] | readonly [head:string,last?:number];
export const [readonlyHead,...readonlyTail] = readonlyUnion;
declare const iterable: { [Symbol.iterator](): { next(): {value:number} } };
export const [custom,...customRest] = iterable;
declare const indexedIterable: { 0:string; [Symbol.iterator](): {next():{value:number}} };
export const [customIndexed] = indexedIterable;
interface Numbers extends Array<number> { 0: 42 }
declare const numbers: Numbers;
export const [derived,...derivedRest] = numbers;

export const headValue = head;
export const tailValue = tail;
export const mixedHeadValue = mixedHead;
export const mixedTailValue = mixedTail;
export const readonlyHeadValue = readonlyHead;
export const readonlyTailValue = readonlyTail;
export const customValue = custom;
export const customRestValue = customRest;
export const customIndexedValue = customIndexed;
export const derivedValue = derived;
export const derivedRestValue = derivedRest;
",
    );
    expect(
        &lines,
        &[
            "headValue : string | number",
            "tailValue : [string, string] | [number, number]",
            "mixedHeadValue : string | boolean",
            "mixedTailValue : (string | boolean)[]",
            "rest : [string, string] | [number, number]",
            "customValue : number",
            "customRestValue : number[]",
            "customIndexedValue : number",
            "derivedValue : 42",
            "derivedRestValue : number[]",
        ],
    );
}

#[test]
fn union_calls_compare_callback_shapes_without_erasing_nested_returns() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
export type Expression = ['and', ...Expression[]] | ['not', Expression] | 'true' | 'false';
declare const expression: ['and', ...Expression[]] | ['not', Expression];
const [operator, ...operands] = expression;
export const result = operands.every(child => child === 'true');
export const operandsValue = operands;
interface First<T> { apply(callback: (value:T) => number): 'first' }
interface Second<T> { apply(callback: (value:T) => number): 'second' }
declare const matched: First<string> | Second<string>;
export const matchedResult = matched.apply(value => value.length);
interface Other<T> { apply(callback: (value:T) => string): 'other' }
declare const different: First<string> | Other<string>;
export const differentResult = different.apply(value => { throw value; });
",
    );
    expect(
        &lines,
        &[
            "result : boolean",
            "operandsValue : Expression[] | [Expression]",
            "matchedResult : \"first\" | \"second\"",
        ],
    );
    // Differing nested returns require the second, parameter-intersection pass.
    expect(&lines, &["differentResult : \"first\" | \"other\""]);
}

#[test]
fn union_properties_mix_named_members_with_indexes() {
    let lines = assertions(
        r"// @strict: true
// @noUncheckedIndexedAccess: true
export declare const mixed: [boolean,string] | string[];
export const first = mixed[0];
export const [boundFirst] = mixed;
export declare const indexed: { [key:string]: number } | { p: string };
export const property = indexed.p;
export declare const onlyIndexes: { [key:string]: number } | { [key:string]: string };
export const indexedProperty = onlyIndexes.p;
",
    );
    expect(
        &lines,
        &[
            "first : string | boolean",
            "boundFirst : string | boolean",
            "property : string | number",
            "indexedProperty : string | number | undefined",
        ],
    );
}

#[test]
fn union_index_writes_keep_readonly_and_symbol_boundaries() {
    let source = include_str!(
        "../../../vendor/typescript-go/_submodules/TypeScript/tests/cases/conformance/types/union/unionTypeWithIndexSignature.ts"
    );
    let lines = assertions(source);
    // The pinned baseline renders invalid accesses as any; this raw producer
    // exposes unresolved errorType before the aligned verdict normalization.
    expect(&lines, &["ro.foo : any", "both[sym] : error", "m.bar : error"]);
}
