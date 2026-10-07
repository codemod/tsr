//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/intersection_constructors", "probe.ts", source);
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
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn ordinary_constructors_remain_overloads() {
    expect(
        r"// @strict: true
interface A { a:string }
interface B { b:number }
declare const overload: (new(x:string)=>A) & (new(x:number)=>B);
export const str = new overload('a');
export const num = new overload(1);
declare const fields: { label:string } & (new(x:number)=>B);
export const withFields = new fields(1);
",
        &["str : A", "num : B", "withFields : B"],
    );
}

#[test]
fn mixins_intersect_returns_in_constituent_order() {
    expect(
        r"// @strict: true
interface A { a:string }
interface B { b:number }
interface M { mixed:boolean }
declare const mix: (new(...args:any[])=>M) & (new(x:number)=>B);
export const mixed = new mix(1);
declare const all: (new(...args:any[])=>A) & (new(...args:any[])=>B);
export const allMixed = new all();
declare const generic: (new<T>(value:T)=>{value:T}) & (new(...args:any[])=>M);
export const inferred = new generic(1);
export const explicit = new generic<string>('a');
",
        &[
            "mixed : M & B",
            "allMixed : A & B",
            "inferred : { value: number; } & M",
            "explicit : { value: string; } & M",
        ],
    );
}

#[test]
fn abstract_mixins_are_discarded_before_abstract_signature_check() {
    expect(
        r"// @strict: true
interface B { b:number }
interface M { mixed:boolean }
declare const abstractMix: (abstract new(...args:any[])=>M) & (new(x:number)=>B);
export const abstractMixin = new abstractMix(1);
declare const abstractBase: (new(...args:any[])=>M) & (abstract new(x:number)=>B);
export const abstractOrdinary = new abstractBase(1);
",
        &["abstractMixin : M & B", "abstractOrdinary : any"],
    );
}

#[test]
fn constrained_constructor_uses_its_apparent_signatures() {
    expect(
        r"// @strict: true
interface B { b:number }
function construct<T extends new(x:number)=>B>(base:T) { return new base(1); }
",
        &["construct : <T extends new (x: number) => B>(base: T) => B", "new base(1) : B"],
    );
}

#[test]
fn only_any_array_rest_signatures_are_mixins() {
    expect(
        r"// @strict: true
interface A { a:string }
interface B { b:number }
declare const anyRest: (new(...args:any)=>A)&(new(x:number)=>B);
export const anyMixed=new anyRest(1);
declare const tuple: (new(...args:[any])=>A)&(new(x:number)=>B);
export const tupleResult=new tuple(1);
declare const readonlyArray: (new(...args:readonly any[])=>A)&(new(x:number)=>B);
export const readonlyResult=new readonlyArray(1);
declare const genericRest: (new<T>(...args:any[])=>A)&(new(x:number)=>B);
export const genericResult=new genericRest(1);
declare const unknownArray: (new(...args:unknown[])=>A)&(new(x:number)=>B);
export const unknownResult=new unknownArray(1);
class Parent<T=string> { constructor(public item:T) {} }
class Child<T=string> extends Parent<T> {}
export const inferredChild=new Child(1);
export const explicitChild=new Child<boolean>(true);
class Overload { constructor(value:string); constructor(value:number); constructor(value:unknown) {} }
export const overloadedClass=new Overload(1);
class Callback { constructor(read:(value:number)=>string) {} }
export const contextual=new Callback(value=> {const copied=value;return '';});
abstract class Abstract<T> { constructor(value:T) {} }
export const abstractGeneric=new Abstract(1);
",
        &[
            "anyMixed : A & B",
            "tupleResult : A",
            "readonlyResult : A & B",
            "genericResult : A",
            "unknownResult : A",
            "inferredChild : Child<number>",
            "explicitChild : Child<boolean>",
            "overloadedClass : Overload",
            "contextual : Callback",
            "copied : number",
            "abstractGeneric : any",
        ],
    );
}

#[test]
fn constructor_candidates_follow_native_ordering() {
    expect(
        r#"// @strict: true
interface A {a:number}
interface B {b:string}
declare const specialized:(new(x:string)=>A)&(new(x:"x")=>B);
export const literalResult=new specialized("x");
export const broadResult=new specialized("y");
interface C {new(x:string):A;new(x:number):B}
interface D extends C {new(x:boolean):{d:boolean}}
declare const inherited:D;
export const inheritedNumber=new inherited(1);
export const own=new inherited(true);
declare class First {first:number}
declare class Last {last:string}
declare const defaults:typeof First & typeof Last;
export const defaultResult=new defaults();
interface BaseCtor {new():{base:number}}
interface OwnCtor extends BaseCtor {new():{own:string}}
declare const ownCtor:OwnCtor;
export const ownWins=new ownCtor();
"#,
        &[
            "literalResult : B",
            "broadResult : A",
            "inheritedNumber : B",
            "own : { d: boolean; }",
            "defaultResult : Last",
            "ownWins : { own: string; }",
        ],
    );
}

#[test]
fn iterable_constructor_inference_visits_computed_members_and_tuple_arguments() {
    expect(
        r"// @strict: true
// @target: esnext
// @lib: esnext
const s: symbol = Symbol('s');
export const weakSymbols=new WeakSet([s]);
const pairs=[[{},1],[{},2]] as const;
export const weakPairs=new WeakMap(pairs);
declare function elements<T>(values:Iterable<T>):T[];
export const inferredElements=elements([1,2]);
declare function entries<K,V>(values:Iterable<readonly [K,V]>):[K,V];
export const inferredEntries=entries(pairs);
interface Recursive<T> {next:Recursive<T>;value:T}
declare const recursive:Recursive<string>;
declare function recursiveValue<T>(value:Recursive<T>):T;
export const inferredRecursive=recursiveValue(recursive);
interface Source {next:Source;value:string}
declare const source:Source;
export const inferredStructural=recursiveValue(source);
",
        &[
            "weakSymbols : WeakSet<symbol>",
            "weakPairs : WeakMap<{}, 1 | 2>",
            "inferredElements : number[]",
            "inferredEntries : [{}, 1 | 2]",
            "inferredRecursive : string",
            "inferredStructural : string",
        ],
    );
}
