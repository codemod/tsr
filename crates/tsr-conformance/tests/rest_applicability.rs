//! Pinned-tsgo controls for this and rest parameter overload applicability.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/rest-applicability", "applicability.ts", source);
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

#[test]
fn this_and_rest_overloads_use_instantiated_signatures() {
    let source = r#"// @strict: true
// @target: es2015
export declare function foo(a:number,b:string):string;
export const bound = foo.bind(undefined);
export const partial = foo.bind(undefined,1);
export const called = foo.call(undefined,1,"x");
export const applied = foo.apply(undefined,[1,"x"]);
export class C { constructor(a:number,b:string) {} }
export const ctorBound = C.bind(undefined);
export const ctorPartial = C.bind(undefined,1);
export const instance = new ctorPartial("x");
export declare function choose<T>(...args:[T,string]):"pair";
export declare function choose<T>(...args:T[]):"many";
export const one = choose(1);
export const pair = choose(1,"x");
export const many = choose(1,2);
export const extra = choose(1,2,3);
export declare const obj: {kind:"right";select<T>(this:{kind:"wrong"},x:T):"wrong";select<T>(this:{kind:"right"},x:T):"right";};
export const selected = obj.select(1);
export declare const voidObj:{select<T>(this:void,x:T):T;select(x:string):string};
export const voidReceiver = voidObj.select(1);

export type Chain<T> = { next<U>(value:U):Chain<T & {value:U}> };
export const chain = <T>(seed:T) => (Object.assign({}, {
    next:<U>(value:U)=>chain(Object.assign({},seed,{value}))
}) as Chain<T>);
export const next = chain({}).next(1);
export const asserted = () => ("x" as "x");
export const constAsserted = () => ("x" as const);
"#;
    let lines = assertions(source);
    for wanted in [
        "bound : (a: number, b: string) => string",
        "partial : (b: string) => string",
        "called : string",
        "applied : string",
        "ctorBound : typeof C",
        "ctorPartial : new (b: string) => C",
        "instance : C",
        "one : \"many\"",
        "pair : \"pair\"",
        "many : \"many\"",
        "extra : \"many\"",
        "selected : \"right\"",
        "voidReceiver : 1",
        "chain : <T>(seed: T) => Chain<T>",
        "next : Chain<{ value: number; }>",
        "asserted : () => \"x\"",
        "constAsserted : () => \"x\"",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn non_strict_function_members_keep_function_fallback() {
    let lines = assertions(
        r#"// @strict: true
// @strictBindCallApply: false
// @target: es2015
declare function foo(a:number,b:string):string;
export const bound = foo.bind(undefined);
export const called = foo.call(undefined,1,"x");
export const applied = foo.apply(undefined,[1,"x"]);
class C { constructor(a:number,b:string) {} }
export const ctorBound = C.bind(undefined);
"#,
    );
    for wanted in ["bound : any", "called : any", "applied : any", "ctorBound : any"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn failed_tuple_rest_overloads_recover_with_effective_parameter_count() {
    // Native reports TS2575 but recovers the return type from the three-position candidate.
    let lines = assertions(
        r#"// @strict: true
declare function fail<T>(...args:[boolean]):"single";
declare function fail<T>(...args:[number,number,number]):"triple";
export const failed = fail("bad","bad");
"#,
    );
    assert!(lines.iter().any(|line| line == "failed : \"triple\""), "{lines:?}");
}
