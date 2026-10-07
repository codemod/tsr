//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/dependent_inference", "probe.ts", source);
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
fn dependent_candidate_constraints() {
    expect(
        r"// @strict: true
// @target: esnext
declare function backward<A, B extends A>(a: A, b: B): [A, B];
declare function forward<A extends B, B>(a: A, b: B): [A, B];
declare function nested<A, B extends {value: A}>(a: A, b: B): B;
declare function chain<A extends B, B extends C, C>(a:A,b:B,c:C):[A,B,C];
declare function keyed<T,K extends keyof T>(source:T,key:K):K;
export const accepted=backward(1,2);
export const rejected=backward(1,'x');
export const before=forward('x',1);
export const structured=nested(1,{value:'x', extra:true});
export const validStructure=nested(1,{value:2, extra:true});
export const chained=chain(true,'x',1);
export const validKey=keyed({a:1},'a');
export const invalidKey=keyed({a:1},'b');
",
        &[
            "accepted : [number, number]",
            "rejected : [number, number]",
            "before : [number, number]",
            "structured : { value: number; }",
            "validStructure : { value: number; extra: boolean; }",
            "chained : [number, number, number]",
            "validKey : \"a\"",
            "invalidKey : \"a\"",
        ],
    );
}

#[test]
fn recursive_candidate_constraints() {
    expect(
        r"// @strict: true
// @target: esnext
declare function self<T extends {same:T}>(value:T):T;
declare function pair<A extends {b:B}, B extends {a:A}>(a:A,b:B):[A,B];
declare function defaults<A, B extends A=never>(a:A,b:B):B;
declare function returned<A, B extends A>(a:A): B;
export const recursive=self({});
export const deeper=self({same:1});
export const mutual=pair({},{});
export const defaultIgnored=defaults(1,'x');
export const contextual: string | number = returned('x');
export const contextualCall = ((): string | number => returned('x'));
",
        &[
            "recursive : { same: {}; }",
            "deeper : { same: { same: number; }; }",
            "mutual : [{ b: { a: {}; }; }, { a: {}; }]",
            "defaultIgnored : number",
        ],
    );
}

#[test]
fn const_signature_sources() {
    expect(
        r"// @strict: true
// @target: esnext
declare function mutable<const T extends unknown[]>(cb:(...args:T)=>void):T;
declare function readonly<const T extends readonly unknown[]>(cb:(...args:T)=>void):T;
declare function indirect<const T>(value:{get:()=>T}):T;
export const mut=mutable((a:{test:number},b:string)=>{});
export const ro=readonly((a:{test:number},b:string)=>{});
export const nested=indirect({get:()=>[1,2]});
declare const source:{test:number};
export const variable=indirect({get:()=>source});
",
        &[
            "mut : [a: { test: number; }, b: string]",
            "ro : readonly [a: { test: number; }, b: string]",
            "nested : readonly [1, 2]",
            "variable : { test: number; }",
        ],
    );
}

#[test]
fn const_signature_rest_shapes() {
    expect(
        r"// @strict: true
// @target: esnext
declare function ro<const T extends readonly unknown[]>(cb:(...args:T)=>void):T;
declare function mut<const T extends unknown[]>(cb:(...args:T)=>void):T;
export const optional=ro((a:number,b?:string)=>{});
export const variadic=ro((a:number,...tail:string[])=>{});
export const fixed=ro((...args:[a:number,b:string])=>{});
export const arrayRest=ro((...args:number[])=>{});
export const invalidReadonly=mut((...args:readonly number[])=>{});
export const nested=ro((a:{value:number}, b:[number,string])=>{});
export const optionalRead=optional[1];
",
        &[
            "variadic : readonly [a: number, ...tail: string[]]",
            "fixed : readonly [a: number, b: string]",
            "arrayRest : number[]",
            "invalidReadonly : unknown[]",
            "nested : readonly [a: { value: number; }, b: [number, string]]",
            "optionalRead : string | undefined",
        ],
    );
}
