//! Union inference controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn union_matches_preserve_candidate_priority() {
    let source = r#"// @strict: true
// @target: es2020
declare function nested<T>(value:T[]|T[][]):T;
declare const values:string[]|string[][];
export const nestedResult=nested(values);
declare function promised<T>(value:T|PromiseLike<T>):T;
declare const mixed:Promise<string>|string;
export const promisedResult=promised(mixed);
export const simplePromiseResult=promised(Promise.resolve("foo"));
declare function either<T,U>(value:T|U):[T,U];
export const eitherResult=either(1);
declare function optional<T>(value:((value:T)=>void)|null|undefined):T;
declare const nullable:((value:{count:number})=>void)|null|undefined;
export const nullableResult=optional(nullable);
declare function primitive<T>(first:T,second:T|string):T;
export const primitiveResult=primitive(1,"foo");
declare function identity<T>(value:T|null):T;
declare const source:string|null;
export const identityResult=identity(source);
"#;
    let case = TestCase::parse("probe/union-continuation", "union-continuation.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "nestedResult : string",
        "promisedResult : string",
        "simplePromiseResult : string",
        "eitherResult : [number, number]",
        "nullableResult : { count: number; }",
        "primitiveResult : 1",
        "identityResult : string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn recursive_promise_overloads_preserve_the_common_candidate() {
    let source = r"// @strict: false
// @target: es2020
interface Promise<T> {
    then<U>(success?: (value:T)=>U,error?: (error:any)=>U,progress?: (progress:any)=>void):Promise<U>;
    done<U>(success?: (value:T)=>any,error?: (error:any)=>any,progress?: (progress:any)=>void):void;
}
interface IPromise<T> {
    then<U>(success?: (value:T)=>IPromise<U>,error?: (error:any)=>IPromise<U>,progress?: (progress:any)=>void):IPromise<U>;
    then<U>(success?: (value:T)=>IPromise<U>,error?: (error:any)=>U,progress?: (progress:any)=>void):IPromise<U>;
    then<U>(success?: (value:T)=>U,error?: (error:any)=>IPromise<U>,progress?: (progress:any)=>void):IPromise<U>;
    then<U>(success?: (value:T)=>U,error?: (error:any)=>U,progress?: (progress:any)=>void):IPromise<U>;
    done?<U>(success?: (value:T)=>any,error?: (error:any)=>any,progress?: (progress:any)=>void):void;
}
declare function legacy():IPromise<number>;
declare function modern():Promise<number>;
declare const first:Promise<number>;
const recursivePromise=first.then(modern,legacy,legacy).then(legacy,legacy,legacy);
";
    let case = TestCase::parse("probe/recursive-promises", "recursive-promises.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    let wanted = "recursivePromise : Promise<IPromise<number>>";
    assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
}
