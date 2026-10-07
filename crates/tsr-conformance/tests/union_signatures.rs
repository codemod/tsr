//! Outcomes verified against pinned native tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/union_signatures", "union_signatures.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
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
fn combined_domains_and_alpha_mapped_generics() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const domains: ((x:'a'|'b')=>1) | ((x:'b'|'c')=>2);
export const domainResult = domains('b');
declare const generic: (<T>(x:T)=>T[]) | (<U>(x:U)=>Promise<U>);
export const genericResult = generic(1);
declare const constrained: (<T,U extends T>(x:T,y:U)=>U[]) | (<A,B extends A>(x:A,y:B)=>Promise<B>);
export const constrainedResult = constrained(1,2);
declare const defaults: (<T=string>()=>T[]) | (<U=number>()=>Promise<U>);
export const defaultResult = defaults();
",
    );
    expect(
        &lines,
        &[
            "domainResult : 1 | 2",
            "genericResult : number[] | Promise<number>",
            "constrainedResult : number[] | Promise<number>",
            "defaultResult : string[] | Promise<string>",
        ],
    );
}

#[test]
fn rest_positions_and_one_overloaded_constituent() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const tuples: ((...args:[name:string,count?:number])=>1) | ((name:string)=>2);
export const tupleResult = tuples('a',1);
declare const oneOverload: { (x:'a'):1; (x:'b'):2 } | ((x:'b')=>3);
export const overloadResult = oneOverload('b');
declare const incompatible: { (x:string):1; (x:number):2 } | { (x:boolean):3; (x:bigint):4 };
// @ts-ignore: no common call signature
export const invalid = incompatible(null);
",
    );
    expect(&lines, &["tupleResult : 1 | 2", "overloadResult : 2 | 3"]);
    // The checker keeps unsupported call resolution as error; the baseline
    // pipeline normalizes this error-call result to native's any.
    expect(&lines, &["invalid : error"]);
}

#[test]
fn composite_predicates_match_positions_and_exclude_assertions() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const predicateNames: ((a:unknown)=>a is string) | ((b:unknown)=>b is number);
export function named(x:unknown) { if(predicateNames(x)) return x; throw 0; }
declare const falseMember: ((a:unknown)=>a is string) | ((b:unknown)=>false);
export function falseBranch(x:unknown) { if(falseMember(x)) return x; throw 0; }
declare const differentPositions: ((a:unknown,b:unknown)=>a is string) | ((a:unknown,b:unknown)=>b is number);
export function positions(x:unknown,y:unknown) { if(differentPositions(x,y)) return x; throw 0; }
declare const asserts: ((a:unknown)=>asserts a is string) | ((a:unknown)=>asserts a is number);
export function assertions(x:unknown) { asserts(x); return x; }
",
    );
    expect(
        &lines,
        &[
            "named : (x: unknown) => string | number",
            "falseBranch : (x: unknown) => string",
            "positions : (x: unknown, y: unknown) => unknown",
            "assertions : (x: unknown) => unknown",
        ],
    );
}

#[test]
fn union_constructors_preserve_instances_and_abstract_rejection() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
declare const constructors: (new(x:string)=>{a:1}) | (new(x:string)=>{b:2});
export const instance = new constructors('a');
class Concrete {}
abstract class Abstract { a!: string; }
declare const classes: typeof Concrete | typeof Abstract;
// @ts-ignore: abstract constituent
export const invalidInstance = new classes();
declare const abstractSignature: (abstract new(x:string)=>string) | (new(x:string)=>number);
// @ts-ignore: abstract constituent
export const invalidSignature = new abstractSignature('a');
",
    );
    expect(
        &lines,
        &["instance : { a: 1; } | { b: 2; }", "invalidInstance : any", "invalidSignature : any"],
    );
}

#[test]
fn array_member_fallback_and_intersected_callback_context() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
interface Fizz { id:number; fizz:string }
interface Buzz { id:number; buzz:string }
declare const values: Fizz[] | Buzz[];
export const filtered = values.filter(value => value.id > 0);
export const found = values.find(value => value.id > 0);
export const every = values.every(value => value.id > 0);
declare function isFizz(value:Fizz|Buzz):value is Fizz;
export const narrowed = values.filter(isFizz);
declare const readonlyValues: readonly Fizz[] | Buzz[];
export const readonlyFiltered = readonlyValues.filter(isFizz);
declare const callbacks: ((cb:(value:string)=>void)=>1) | ((cb:(value:number)=>void)=>2);
export const contextual = callbacks(value => { const capture = value; });
",
    );
    expect(
        &lines,
        &[
            "filtered : (Buzz | Fizz)[]",
            "found : Buzz | Fizz | undefined",
            "every : boolean",
            "narrowed : Fizz[]",
            "readonlyFiltered : Fizz[]",
            "contextual : 1 | 2",
            "capture : string | number",
        ],
    );
}

#[test]
fn zero_argument_overloads_keep_the_call_receiver() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
type A = { a:string }; type B = { b:number }; type C = { c:string }; type D = { d:number };
type F0 = () => void; type F1 = (this:A) => void;
interface F3 { (this:A):void; (this:B):void }
interface F4 { (this:C):void; (this:D):void }
declare const receiver: A & C & { f0:F0|F3; f2:F1|F4 };
export const result0 = receiver.f0();
export const result2 = receiver.f2();
declare const u: ((a:number)=>number) | ((a:string)=>boolean);
// @ts-ignore: argument does not satisfy intersected domain
export const rejected = u(10);
",
    );
    expect(&lines, &["result0 : void", "result2 : void", "rejected : number | boolean"]);
}
