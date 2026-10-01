//! Overload and freshness controls checked against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

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
