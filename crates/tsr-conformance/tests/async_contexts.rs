//! Async contextual return slots compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn async_callback_contexts_preserve_const_sources() {
    let source = r#"// @strict: true
// @target: es2020
declare function direct<const T>(value:()=>T):T;
declare function promised<const T>(value:()=>Promise<T>):T;
declare function ordinary<T>(value:()=>Promise<T>):T;
export const directResult=direct(async()=>"foo");
export const directBlockResult=direct(async()=>{return "foo"});
export const promisedResult=promised(async()=>"foo");
export const promisedBlockResult=promised(async()=>{return "foo"});
export const tupleResult=promised(async()=>["a",["b","c"]]);
export const objectResult=promised(async()=>({x:{y:1},a:[true,"s"]}));
export const ordinaryResult=ordinary(async()=>"foo");
const existing={x:1};let tuple:[string,number]=["a",1];
export const existingResult=promised(async()=>existing);
export const existingTupleResult=promised(async()=>tuple);
declare function mutable<const T extends string[]>(value:()=>Promise<T>):T;
export const mutableResult=mutable(async()=>["a","b"]);
export const fixedLiteral:()=>Promise<"foo">=async()=>"foo";
export const fixedUndefined:()=>Promise<undefined>=async()=>{};
export const fixedVoid:()=>Promise<void>=async()=>{};
declare function generator<const Y,const R>(value:()=>AsyncGenerator<Y,R>):[Y,R];
export const generatorResult=generator(async function*(){yield 10;return "1"});
export const yieldedTupleResult=generator(async function*(){yield ["a",1];return true});
export const yieldedVariableResult=generator(async function*(){yield tuple;return false});
export const ordinaryGenerator=async function*(){yield 10;return "1"};
export const delegatedGenerator=async function*(){yield* [Promise.resolve(1)]};
declare class StateMachine<T>{onDone:(value:T)=>void}
export const unionContext:()=>Promise<{count:number}>|StateMachine<{count:number}>=
    async()=>Promise.reject("failure");
declare function nullable<T>(fn:((value:T)=>void)|null|undefined):T;
export const nullableResult=nullable((value:{count:number})=>{});
"#;
    let case = TestCase::parse("probe/async-contexts", "async-contexts.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "directResult : Promise<\"foo\">",
        "directBlockResult : Promise<\"foo\">",
        "promisedResult : \"foo\"",
        "promisedBlockResult : \"foo\"",
        "tupleResult : readonly [\"a\", readonly [\"b\", \"c\"]]",
        "objectResult : { readonly x: { readonly y: 1; }; readonly a: readonly [true, \"s\"]; }",
        "ordinaryResult : string",
        "existingResult : { x: number; }",
        "existingTupleResult : [string, number]",
        "mutableResult : [\"a\", \"b\"]",
        "fixedLiteral : () => Promise<\"foo\">",
        "fixedUndefined : () => Promise<undefined>",
        "fixedVoid : () => Promise<void>",
        "generatorResult : [10, \"1\"]",
        "yieldedTupleResult : [readonly [\"a\", 1], true]",
        "yieldedVariableResult : [[string, number], false]",
        "ordinaryGenerator : () => AsyncGenerator<number, string, unknown>",
        "delegatedGenerator : () => AsyncGenerator<number, void, unknown>",
        "unionContext : () => Promise<{ count: number; }> | StateMachine<{ count: number; }>",
        "Promise.reject(\"failure\") : Promise<{ count: number; }>",
        "nullableResult : { count: number; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// Pinned native 5b1047d infers async bodies when a written union's incompatible
/// inputs give a completed absent contextual signature. Wrong async returns
/// and genuinely untyped parameters remain errors.
#[test]
fn completed_absent_context_does_not_erase_async_callback_returns() {
    let source = r#"// @strict: true
// @target: es2020
type Ambiguous = ((value: number) => Promise<boolean>) | ((value: string) => Promise<boolean>);
const valid: Ambiguous = async () => false;
const validBlock: Ambiguous = async () => { return false; };
const invalid: Ambiguous = async () => { return 123; };
const emptyInvalid: Ambiguous = async () => {};
const blockInvalid: Ambiguous = async () => { return "wrong"; };
const untyped = async value => value;
"#;
    let case = TestCase::parse("probe/async-completed-absence", "probe.ts", source);
    let actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .iter()
        .map(|d| (d.line, d.column, d.code))
        .collect();
    assert_eq!(actual, [(4, 7, 2322), (5, 7, 2322), (6, 7, 2322), (7, 23, 7006)]);
}
