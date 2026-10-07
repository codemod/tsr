//! Const callback returns compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn const_callback_return_contexts_preserve_literal_sources() {
    let source = r#"// @strict: true
// @target: es2020
declare function callback<const T>(value:()=>T):T;
declare function ordinary<T>(value:()=>T):T;
export const tupleResult=callback(()=>["a",["b","c"]]);
export const objectResult=callback(()=>({x:{y:1},a:[true,"s"]}));
export const conciseResult=callback(()=>"a");
export const blockResult=callback(()=>{return "a"});
export const booleanResult=callback(()=>true);
export const numberResult=callback(()=>1);
export const bigintResult=callback(()=>1n);
const existing={x:1};
export const existingResult=callback(()=>existing);
export const ordinaryResult=ordinary(()=>"a");
export const unionResult=callback(()=>{if(Math.random())return "a";return "b";});
declare function mutable<const T extends string[]>(value:()=>T):T;
export const mutableResult=mutable(()=>["a","b"]);
export const functionResult=callback(function(){return ["a","b"]});

export const templateResult=callback(()=>`a${Math.random()}`);
export const blockTemplateResult=callback(()=>{return `a${Math.random()}`});
declare function generator<const Y,const R>(value:()=>Generator<Y,R>):[Y,R];
export const generatorResult=generator(function*(){yield 10;return "1"});
"#;
    let case = TestCase::parse("probe/const-callbacks", "const-callbacks.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "tupleResult : readonly [\"a\", readonly [\"b\", \"c\"]]",
        "objectResult : { readonly x: { readonly y: 1; }; readonly a: readonly [true, \"s\"]; }",
        "conciseResult : \"a\"",
        "blockResult : \"a\"",
        "booleanResult : true",
        "numberResult : 1",
        "bigintResult : 1n",
        "existingResult : { x: number; }",
        "ordinaryResult : string",
        "unionResult : \"a\" | \"b\"",
        "mutableResult : [\"a\", \"b\"]",
        "functionResult : readonly [\"a\", \"b\"]",
        "templateResult : `a${number}`",
        "blockTemplateResult : `a${number}`",
        "generatorResult : [10, \"1\"]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
