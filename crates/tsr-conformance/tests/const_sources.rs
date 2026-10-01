//! Const literal inference source views compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn const_sources_preserve_literal_origins_and_mutable_variables() {
    let source = r#"// @strict: true
// @target: es2020
// @strict: true
// @target: es2020
declare function direct<const T>(value:T):T;
export const tupleResult=direct(["a",["b","c"]]);
export const nestedResult=direct({x:{y:1},a:[true,"s"]});
declare function mixed<const T,U>(a:T,b:U):[T,U];
export const mixedResult=mixed({x:1},{y:2});
declare function wrapped<const T>(value:{value:T}):T;
export const wrappedResult=wrapped({value:["a","b"]});
declare function mutable<const T extends string[]>(value:T):T;
export const mutableResult=mutable(["a","b"]);
declare function homomorphic<const T>(value:{[P in keyof T]:T[P]}):T;
export const homomorphicResult=homomorphic({x:1});
const existing={x:1};export const existingResult=direct(existing);
let tuple:[string,number]=["a",1];export const existingTupleResult=direct(tuple);

export const retainedChild=direct({child:existing});
const part=["a",1] as const;
export const spreadResult=direct([...part,true]);
export const literalSpreadResult=direct([...([ ["a",1] as const ] as const)]);
export const objectSpreadResult=direct({...existing,nested:{n:1}});
export const parenthesizedResult=direct((["a",1]));
const replacement={nested:{n:2}};
// @ts-ignore: the deliberate override tests last-write source origins.
export const replacedOrigin=direct({nested:{n:1},...replacement});
export const literalOverride=direct({...replacement,nested:{n:1}});
interface CustomArray<T> extends Array<T> {}
declare function custom<const T extends CustomArray<string>>(value:T):T;
export const customResult=custom(["a","b"]);
"#;
    let case = TestCase::parse("probe/const-sources", "const-sources.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "tupleResult : readonly [\"a\", readonly [\"b\", \"c\"]]",
        "nestedResult : { readonly x: { readonly y: 1; }; readonly a: readonly [true, \"s\"]; }",
        "mixedResult : [{ readonly x: 1; }, { y: number; }]",
        "wrappedResult : readonly [\"a\", \"b\"]",
        "mutableResult : [\"a\", \"b\"]",
        "homomorphicResult : { readonly x: 1; }",
        "existingResult : { x: number; }",
        "existingTupleResult : [string, number]",
        "retainedChild : { readonly child: { x: number; }; }",
        "spreadResult : readonly [\"a\", 1, true]",
        "literalSpreadResult : readonly [readonly [\"a\", 1]]",
        "objectSpreadResult : { readonly x: number; readonly nested: { readonly n: 1; }; }",
        "parenthesizedResult : readonly [\"a\", 1]",
        "replacedOrigin : { readonly nested: { n: number; }; }",
        "literalOverride : { readonly nested: { readonly n: 1; }; }",
        "customResult : [\"a\", \"b\"]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
