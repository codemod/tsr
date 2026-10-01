//! Callable-object controls checked against pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn callable_exports_survive_rendering_and_generic_instantiation() {
    let source = r#"// @strict: true
// @target: es2020
export function factory() {
 function f(x:number) {return x;}
 f.value=1;
 f.method=function(text:string) {return text;};
 f.arrow=(text:string)=>text;
 return f;
}
export const f=factory();
export const returned=f(1);
export const value=f.value;
export const method=f.method("x");
export const arrow=f.arrow("x");
export function capture<T>(value:T) {
 function callable() {return value;}
 callable.value=value;
 return callable;
}
export const captured=capture(1);
export const capturedValue=captured.value;
export const capturedReturn=captured();
export function propertyOnly<T>(value:T) {
 function callable() {return true;}
 callable.value=value;
 return callable;
}
export const propertyCaptured=propertyOnly("x");
export const propertyValue=propertyCaptured.value;
export const propertyReturn=propertyCaptured();
export function ownGeneric<T>(value:T) {
 function callable<U>(inner:U) {return inner;}
 callable.value=value;
 return callable;
}
export const generic=ownGeneric(1);
export const genericValue=generic.value;
export const genericReturn=generic("x");
export function namedProperties() {
 function callable() {}
 callable[77]=1;
 callable["101"]=2;
 callable["🤪"]=3;
 callable["dashed-name"]=4;
 return callable;
}
export const named=namedProperties();
"#;
    let case = TestCase::parse("probe/callable-exports", "callable.ts", source);
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
        "f : { (x: number): number; value: number; method: (text: string) => string; arrow: (text: string) => string; }",
        "returned : number",
        "value : number",
        "method : string",
        "arrow : string",
        "capture : <T>(value: T) => { (): T; value: T; }",
        "captured : { (): number; value: number; }",
        "capturedValue : number",
        "capturedReturn : number",
        "propertyCaptured : { (): boolean; value: string; }",
        "propertyValue : string",
        "propertyReturn : boolean",
        "generic : { <U>(inner: U): U; value: number; }",
        "genericValue : number",
        r#"genericReturn : "x""#,
        r#"named : { (): void; 77: number; "101": number; "\uD83E\uDD2A": number; "dashed-name": number; }"#,
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
