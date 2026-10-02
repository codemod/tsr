//! Outcomes verified against pinned native tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/inherited_signatures", "inherited_signatures.ts", source);
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
fn inherited_call_construct_and_shadowed_parameters() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
interface Callable<T> { (value:T):T[] }
interface Middle<U> extends Callable<U[]> {}
interface Final extends Middle<string> {}
declare const final:Final;
export const callResult=final(['a']);
declare const direct: Callable<number>;
export const directResult=direct(1);
interface Constructor<T> { new(value:T):T[] }
interface DerivedCtor<U> extends Constructor<U[]> {}
declare const construct:DerivedCtor<string>;
export const newResult=new construct(['a']);
interface Shadow<T> { <T>(value:T):T }
interface DerivedShadow<U> extends Shadow<U> {}
declare const shadow: DerivedShadow<string>;
export const shadowResult=shadow(1);
interface Defaulted<T=string> { (value:T):T[] }
interface DefaultDerived extends Defaulted {}
declare const defaulted:DefaultDerived;
export const defaultResult=defaulted('a');
namespace NS { export interface Base<T> { (value:T):T } }
interface Qualified extends NS.Base<number> {}
declare const qualified:Qualified;
export const qualifiedResult=qualified(1);
function mixed<T>(ctor: { (this:{},v:T):void;new(v:T):void } | { (v:T):void;new(v:T):void }, t:T) { return new ctor(t); }
export { mixed };
",
    );
    expect(
        &lines,
        &[
            "callResult : string[][]",
            "directResult : number[]",
            "newResult : string[][]",
            "shadowResult : 1",
            "defaultResult : string[]",
            "qualifiedResult : number",
            "mixed : <T>(ctor: { (this: {}, v: T): void; new (v: T): void; } | { (v: T): void; new (v: T): void; }, t: T) => void",
        ],
    );
}

#[test]
fn defaults_qualified_aliases_and_deep_heritage() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
interface Parent<T=string,U=T[]> { (value:T):U; item:T; read():U; [key:number]:U }
interface Child extends Parent {}
declare const child:Child;
export const called=child('a');
export const item=child.item;
export const read=child.read();
export const indexed=child[0];
interface Partial extends Parent<number> {}
declare const partial:Partial;
export const partialResult=partial(1);
namespace Outer { export namespace Inner { export interface Base<T=string> { (value:T):T[]; value:T; } } }
import Alias = Outer.Inner;
interface Qualified extends Alias.Base {}
declare const qualified:Qualified;
export const qualifiedResult=qualified('a');
export const qualifiedValue=qualified.value;
interface Overloaded extends Parent<number> { (value:string):string }
declare const overload:Overloaded;
export const own=overload('a');
export const inherited=overload(1);
interface C0 { (value:number):number }
interface C1 extends C0 {} interface C2 extends C1 {} interface C3 extends C2 {}
interface C4 extends C3 {} interface C5 extends C4 {} interface C6 extends C5 {}
interface C7 extends C6 {} interface C8 extends C7 {} interface C9 extends C8 {}
declare const deep:C9;
export const deepResult=deep(1);
",
    );
    expect(
        &lines,
        &[
            "called : string[]",
            "item : string",
            "read : string[]",
            "indexed : string[]",
            "partialResult : number[]",
            "qualifiedResult : string[]",
            "qualifiedValue : string",
            "own : string",
            "inherited : number[]",
            "deepResult : number",
        ],
    );
}

#[test]
fn never_discriminants_and_invalid_heritage() {
    let lines = assertions(
        r#"// @strict: true
// @target: es2015
declare const impossible: { (): 1; tag: "a" } & { tag: number };
export const impossibleResult = impossible();
declare const optional: { (): 2; tag?: "a" } & { tag?: number };
export const optionalResult = optional();
declare const alreadyNever: { (): 3; tag: never } & { tag: number };
export const neverResult = alreadyNever();
declare const nonLiteral: { (): 4; tag: string } & { tag: number };
export const nonLiteralResult = nonLiteral();
declare function choose(tag: "bad", cb: (x: string) => void): 1;
declare function choose(tag: "good", cb: (x: number) => void): 2;
export const chosen = choose("good", x => { const copied = x; });
interface Forward<T = U, U = number> { (): T; value: T }
interface Invalid extends Forward {}
declare const invalid: Invalid;
export const invalidResult = invalid();
interface CycleA extends CycleB {}
interface CycleB extends CycleA {}
declare const cycle: CycleA;
export const cycleResult = cycle();
"#,
    );
    expect(
        &lines,
        &[
            "impossibleResult : error",
            "optionalResult : 2",
            "neverResult : 3",
            "nonLiteralResult : 4",
            "chosen : 2",
            "copied : number",
            "invalidResult : error",
            "cycleResult : error",
        ],
    );
}

#[test]
fn overloads_preserve_the_first_checked_callback_context() {
    let lines = assertions(
        r"// @strict: false
interface Overload {
 (handler:(req:string)=>void):void;
 (handler:(req:number,res:number)=>void):void;
}
declare const use:Overload;
use((req,res)=>{});
",
    );
    expect(&lines, &["(req,res)=>{} : (req: any, res: any) => void", "req : any", "res : any"]);
}

#[test]
fn javascript_empty_defaults_become_any() {
    let lines = assertions(
        r#"// @module: commonjs
// @target: es2015
// @allowJs: true
// @outDir: ./built
// @filename: library.d.ts
export class Foo<T = {}, U = {}> {
    props: T;
    state: U;
    constructor(props: T, state: U);
}

// @filename: component.js
import { Foo } from "./library";
export class MyFoo extends Foo {
    member;
}

// @filename: typed_component.ts
import { MyFoo } from "./component";
export class TypedFoo extends MyFoo {
    constructor() {
        super({x: "string", y: 42}, { value: undefined });
        this.props.x;
        this.props.y;
        this.state.value;
        this.member;
    }
}"#,
    );
    expect(&lines, &["this.props.x : any", "this.props.y : any", "this.state.value : any"]);
}

#[test]
fn super_uses_the_instantiated_base() {
    let lines = assertions(
        r"// @strict: true
// @target: es2015
class Base<T=string> { value!:T; read():T {return this.value;} }
class Derived extends Base<number> { get(){return super.read();} }
class Defaulted extends Base { get(){return super.read();} }
class ArrayDerived extends Array { get(){return super.length;} }
",
    );
    expect(
        &lines,
        &["get : () => number", "get : () => string", "super.length : number", "super : any[]"],
    );
}

#[test]
fn missing_required_heritage_arguments_do_not_become_unknown() {
    let lines = assertions(
        r"// @strict: true
interface Required<T> { ():T; value:T }
interface Missing extends Required {}
declare const missing:Missing;
export const got=missing();
export const prop=missing.value;
",
    );
    expect(&lines, &["got : error", "prop : error"]);
}
