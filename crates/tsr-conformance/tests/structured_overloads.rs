//! Overload and freshness controls checked against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Pinned tsgo rejects both recursive union orders and selects the fallback.
/// A later valid recursive branch still selects the concrete overload.
#[test]
fn recursive_union_assumptions_do_not_select_an_inapplicable_overload() {
    let source = r#"// @strict: true
interface A { next: B; value: string }
interface B { next: A }
interface C { next: D; value: number }
interface D { next: C }
interface E { next: D; value: string }
interface F { next: G; value: string }
interface G { next: F }
declare const a: A;
declare function select(x: C | E): "invalid";
declare function select(x: any): "fallback";
declare function reverse(x: E | C): "invalid";
declare function reverse(x: any): "fallback";
declare function compatible(x: C | F): "recursive";
declare function compatible(x: any): "fallback";
export const rejected = select(a);
export const reversed = reverse(a);
export const accepted = compatible(a);
"#;
    let case = TestCase::parse("probe/recursive-union-overloads", "recursive-union.ts", source);
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
    for wanted in ["rejected : \"fallback\"", "reversed : \"fallback\"", "accepted : \"recursive\""]
    {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn structured_overloads_respect_freshness_and_inference_boundaries() {
    let source = r#"// @strict: true
// @target: es2020
declare function choose(x: {s: string}): string;
declare function choose(x: {n: number}): number;
export const fresh = choose({s: "", n: 0});
const value = {s: "", n: 0};
export const regular = choose(value);
export const spread = choose({...value});
export const own = choose({...value, extra: true});
function factory() { return {s:"", n:0}; }
export const returned = choose(factory());
declare function nested(value:{a:{x:number}}):string;
declare function nested(value:boolean):number;
const nestedValue = {a:{x:1,y:2}, method(){}};
export const nestedRegular = nested(nestedValue);
export const nestedFresh = nested({a:{x:1,y:2}});

declare function indexed(x: {s:string; [key:string]:unknown}): string;
declare function indexed(x: {n:number}): number;
export const acceptedIndex = indexed({s:"", n:0});
declare function union(x: {s:string} | {n:number}): string;
declare function union(x: {other:boolean}): number;
export const acceptedUnion = union({s:"", n:0});
declare function intersection(x: {s:string} & {n:number}): string;
declare function intersection(x: {other:boolean}): number;
export const acceptedIntersection = intersection({s:"", n:0});
export function narrowed(node:Node|null) {if (isElement(node)) return node.tagName;}
function isElement(node: Node|null): node is Element {return node!==null && node.nodeType===1;}

interface Named { name: string; }
declare function identity<T extends Named>(value:T):T;
export const inferred = identity({name:"a", extra:1});
type TypeFunction<R = unknown> = (...args:any[])=>R;
interface Options {type: TypeFunction; default?:unknown;}
type Values<T> = {[K in keyof T]: T[K] extends {type: TypeFunction<infer R>} ? R : never};
declare function reverse<T extends {[key:string]:Options}>(value:Readonly<T>):Values<T>;
const reversedValue = reverse({flag:{type:Boolean, default:false}});
export const reversed = reversedValue.flag;
"#;
    let case = TestCase::parse("probe/structured-overloads", "structured.ts", source);
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
    for wanted in [
        "fresh : never",
        "regular : string",
        "spread : string",
        "own : never",
        "returned : string",
        "nestedRegular : string",
        "nestedFresh : never",
        "acceptedIndex : string",
        "acceptedUnion : string",
        "acceptedIntersection : string",
        "narrowed : (node: Node | null) => string | undefined",
        "inferred : { name: string; extra: number; }",
        "reversed : boolean",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// Pinned tsgo 5b1047d1: generic overloads whose returns print alike still
/// infer and check per candidate (chooseOverload), with no callback argument.
#[test]
fn agreeing_generic_returns_still_select_by_applicability() {
    let source = r"// @target: es2015
interface Promise<T> {
    then<U>(success?: (value: T) => Promise<U>, error?: (error: any) => Promise<U>, progress?: (progress: any) => void): Promise<U>;
    then<U>(success?: (value: T) => U, error?: (error: any) => U, progress?: (progress: any) => void): Promise<U>;
}
interface IPromise<T> {
    then<U>(success?: (value: T) => IPromise<U>, error?: (error: any) => IPromise<U>, progress?: (progress: any) => void): IPromise<U>;
    then<U>(success?: (value: T) => U, error?: (error: any) => U, progress?: (progress: any) => void): IPromise<U>;
}
declare var s1: Promise<number>;
declare function legacy(): IPromise<number>;
declare function native(): Promise<number>;
var viaLegacy = s1.then(legacy, legacy, legacy);
var viaNative = s1.then(native, native, native);
";
    let case = TestCase::parse("probe/agreeing-overloads", "agreeing-overloads.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["viaLegacy : Promise<IPromise<number>>", "viaNative : Promise<number>"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// Pinned tsgo 5b1047d1: inferSignatureInstantiationForOverloadFailure uses
/// `CheckModeSkipGenericFunctions` only for a single generic signature under a
/// non-generic contextual signature. Ordinary and successful generic callback
/// inference must retain their candidates.
#[test]
fn overload_failure_skips_only_contextualized_generic_function_values() {
    let source = r"// @target: es2015
// @strict: false
interface Promise<T> {
  then<U>(success?: (value:T)=>U, error?: (e:any)=>U, progress?: (p:any)=>void): Promise<U>;
}
interface IPromise<T> { value:T }
declare function tooWide<T>(x:T, cb:(a:T)=>T): IPromise<T>;
declare function id<T>(x:T):T;
declare function plain(x:number):number;
declare var source: Promise<number>;
var failed=source.then(tooWide,tooWide,tooWide);
var identityOk=source.then(id,id,id);
var plainOk=source.then(plain,plain,plain);
interface Hybrid {
  <T>(x:T, cb:(a:T)=>T):IPromise<T>;
  new<T>(x:T):IPromise<T>;
}
declare var hybrid:Hybrid;
var callAndConstruct=source.then(hybrid,hybrid,hybrid);
interface MemberCallback<U> {
  (value:number):U;
  marker:string;
}
interface MemberPromise<T> {
  then<U>(success?:MemberCallback<U>, error?:MemberCallback<U>):MemberPromise<U>;
}
declare var memberSource:MemberPromise<number>;
var contextualMembers=memberSource.then(tooWide,tooWide);
";
    let case = TestCase::parse("probe/skip-generic-overload-failure", "skip-generic.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "failed : Promise<unknown>",
        "identityOk : Promise<any>",
        "plainOk : Promise<number>",
        "callAndConstruct : Promise<IPromise<unknown>>",
        "contextualMembers : MemberPromise<IPromise<unknown>>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
