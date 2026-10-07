//! Const rest inference source views compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn const_rest_preserves_literal_sources_and_mutable_constraints() {
    let source = r#"// @strict: true
// @target: es2020
declare function mutable<const T extends unknown[]>(...args:T):T;
export const mutableResult=mutable("a",1,true);
export const nestedResult=mutable({x:{y:1},a:[true,"s"]});
const existing={x:1};export const existingResult=mutable(existing);
let tuple:[string,number]=["a",1];export const existingTupleResult=mutable(tuple);
declare function readonlyRest<const T extends readonly unknown[]>(...args:T):T;
export const readonlyResult=readonlyRest("a",1,true);
declare function variadic<const T extends unknown[]>(...args:[...T]):T;
export const variadicResult=variadic("a",1,true);
declare function constrained<const T extends {foo:unknown[]}[]>(...args:T):T;
export const constrainedResult=constrained({foo:["hello",123]},{foo:[true]});
declare function ordinary<T extends unknown[]>(...args:T):T;
export const ordinaryResult=ordinary("a",1,true);
"#;
    let case = TestCase::parse("probe/const-rest", "const-rest.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "mutableResult : [\"a\", 1, true]",
        "nestedResult : [{ readonly x: { readonly y: 1; }; readonly a: readonly [true, \"s\"]; }]",
        "existingResult : [{ x: number; }]",
        "existingTupleResult : [[string, number]]",
        "readonlyResult : readonly [\"a\", 1, true]",
        "variadicResult : [\"a\", 1, true]",
        "constrainedResult : [{ readonly foo: [\"hello\", 123]; }, { readonly foo: [true]; }]",
        "ordinaryResult : [string, number, boolean]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
