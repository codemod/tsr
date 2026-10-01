//! Outcomes verified against pinned native tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/iteration", "iteration.ts", source);
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
fn semantic_iteration_uses_inherited_methods_and_yield_arms() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
export declare function gather<T extends unknown[]>(...args:T):T;
interface Numbers extends Array<number> {}
interface ReadonlyNumbers extends ReadonlyArray<number> {}
declare const numbers: Numbers;
declare const readonlyNumbers: ReadonlyNumbers;
export const a = [...numbers];
export const b = gather(...numbers);
export const c = [...readonlyNumbers];
export const d = gather(...readonlyNumbers);
declare const iterable: { [Symbol.iterator](): { next(): {done:false,value:number} | {done:true,value:string} } };
export const e = [...iterable];
export const f = gather(...iterable);
declare const other: { [Symbol.iterator](): { next(): {value:string} } };
export const g = [...other];
export const h = [...new Map<string,number>()];
export const i = [...(null as unknown as typeof iterable | typeof other)];
declare const overloaded: { [Symbol.iterator](): { next(): {value:number} }; [Symbol.iterator](required:number): { next(): {value:string} } };
export const j = [...overloaded];
declare const returned: { [Symbol.iterator](): { next(): {done:false,value:number}; return(): {done:false,value:boolean} } };
export const k = [...returned];
",
    );
    expect(
        &lines,
        &[
            "a : number[]",
            "b : number[]",
            "c : number[]",
            "d : number[]",
            "e : number[]",
            "f : number[]",
            "g : string[]",
            "h : [string, number][]",
            "i : (string | number)[]",
            "j : number[]",
            "k : (number | boolean)[]",
        ],
    );
}

#[test]
fn builtin_iterator_return_and_indirect_heritage_substitution() {
    let source = r"// @strict: true
// @target: es2015
export declare const intrinsic: BuiltinIteratorReturn;
export const iterator = [1][Symbol.iterator]();
export const result = iterator.next();
export const value = result.value;
interface Base<A> { get(): A; own<A>(x: A): A }
interface Middle<B> extends Base<B[]> {}
interface Outer<C> extends Middle<C> {}
declare const instance: Outer<string>;
export const inherited = instance.get();
export const shadowed = instance.own(1);
";
    for (directives, intrinsic, result, value) in [
        (
            "// @strict: true\n",
            "intrinsic : undefined",
            "result : IteratorResult<number, undefined>",
            "value : number | undefined",
        ),
        (
            "// @strict: true\n// @strictBuiltinIteratorReturn: false\n",
            "intrinsic : any",
            "result : IteratorResult<number, any>",
            "value : any",
        ),
        (
            "// @strict: false\n",
            "intrinsic : any",
            "result : IteratorResult<number, any>",
            "value : any",
        ),
        (
            "// @strict: false\n// @strictBuiltinIteratorReturn: true\n",
            "intrinsic : undefined",
            "result : IteratorResult<number, undefined>",
            "value : number",
        ),
    ] {
        let lines =
            assertions(&format!("{directives}{}", source.replace("// @strict: true\n", "")));
        expect(&lines, &[intrinsic, result, value, "inherited : string[]", "shadowed : 1"]);
    }
}

#[test]
fn final_results_are_not_yields_and_optional_methods_contribute_independently() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const finished: { [Symbol.iterator](): { next(): { done: true, value: string } } };
export const a = [...finished];
declare const missing: { [Symbol.iterator](): { next(): { done: true } } };
export const b = [...missing];
declare const optional: { [Symbol.iterator](): { next?: () => { value: string } } };
export const c = [...optional];
declare const badReturn: { [Symbol.iterator](): { next(): { value: string }; return: number } };
export const d = [...badReturn];
declare const badNext: { [Symbol.iterator](): { next: number } };
export const e = [...badNext];
declare const malformed: { [Symbol.iterator](): { next(): {} } };
export const f = [...malformed];
declare const doneUnion: { [Symbol.iterator](): { next(): { done: false, value: number } | {done:true} } };
export const g = [...doneUnion];
declare const onlyReturns: { [Symbol.iterator](): { next(): {done: true, value: string}; return(): {done: true, value: boolean} } };
export const h = [...onlyReturns];
declare const throwing: { [Symbol.iterator](): { next(): {value:number}; throw(): {value:boolean} } };
export const i = [...throwing];
",
    );
    // Invalid next signatures and missing value diagnostics remain separate
    // work; these checks cover the native outcomes this port can establish.
    expect(
        &lines,
        &[
            "a : any[]",
            "b : any[]",
            "d : string[]",
            "g : number[]",
            "h : any[]",
            "i : (number | boolean)[]",
        ],
    );
}

#[test]
fn recursive_iterator_return_does_not_invalidate_its_callable() {
    let lines = assertions(
        r"// @target: ES6
// @noImplicitAny: true
class MyStringIterator {
    next() { return v; }
    [Symbol.iterator]() { return this; }
}
for (var v of new MyStringIterator) { }
",
    );
    expect(&lines, &["next : () => any"]);
    let lines = assertions(
        r"// @target: ES6
// @noImplicitAny: true
class MyStringIterator {
    [Symbol.iterator]() { return v; }
}
for (var v of new MyStringIterator) { }
",
    );
    expect(&lines, &["[Symbol.iterator] : () => any"]);
}

#[test]
fn written_nullish_types_stay_distinct_from_widening_global_type_queries() {
    let lines = assertions(
        r"// @strict: false
// @target: es2015
export declare const writtenUndefined: undefined;
export declare const writtenNull: null;
export declare const queried: typeof undefined;
function shadow(undefined: undefined) {
    const local: typeof undefined = undefined;
    return local;
}
",
    );
    expect(
        &lines,
        &[
            "writtenUndefined : undefined",
            "writtenNull : null",
            "queried : any",
            "local : undefined",
        ],
    );
}

#[test]
fn iterable_unions_distinguish_absent_yields_from_never() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const returns: { [Symbol.iterator](): { next(): {done:true, value:string} } };
declare const yields: { [Symbol.iterator](): { next(): {done:false, value:number} } };
export const union = [...(null as unknown as typeof returns | typeof yields)];
declare const never: Iterable<never>;
export const empty = [...never];
",
    );
    expect(&lines, &["union : number[]", "empty : never[]"]);
}
