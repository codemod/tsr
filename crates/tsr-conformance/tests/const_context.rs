//! Const type-variable contexts compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn const_context_follows_the_contextual_type_variable() {
    let source = r#"// @strict: true
// @target: es2020
declare function direct<const T>(value:T):T;
export const tupleResult=direct(["a",["b","c"]]);
declare function mixed<const T,U>(a:T,b:U):[T,U];
export const mixedResult=mixed({x:1},{y:2});
declare function wrapped<const T>(value:{value:T}):T;
export const wrappedResult=wrapped({value:["a","b"]});
declare function homomorphic<const T>(value:{[P in keyof T]:T[P]}):T;
export const homomorphicResult=homomorphic({x:1});
type NotEmpty<T>=keyof T extends never ? never : T;
declare function conditional<const T extends Record<string,any>>(value:NotEmpty<T>):T;
export const conditionalResult=conditional({foo:""});
type NotEmptyMapped<T>=keyof T extends never ? never : {[K in keyof T]:T[K]};
declare function conditionalMapped<const T extends Record<string,any>>(value:NotEmptyMapped<T>):T;
export const conditionalMappedResult=conditionalMapped({foo:""});
declare function union<const T>(value:T | undefined):T;
export const unionResult=union({x:1});
declare function unrelated<const T,U>(a:T,b:U):U;
export const unrelatedResult=unrelated(1,["a","b"]);
declare function rest<const T extends {foo:unknown[]}[]>(...args:T):T;
rest({foo:["hello",123]},{foo:[true]});
"#;
    let case = TestCase::parse("probe/const-context", "const-context.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "tupleResult : readonly [\"a\", readonly [\"b\", \"c\"]]",
        "mixedResult : [{ readonly x: 1; }, { y: number; }]",
        "wrappedResult : readonly [\"a\", \"b\"]",
        "homomorphicResult : { readonly x: 1; }",
        "unionResult : { readonly x: 1; }",
        "unrelatedResult : string[]",
        "{foo:[\"hello\",123]} : { foo: [\"hello\", 123]; }",
        "{foo:[true]} : { foo: [true]; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
    // Conditional-target inference is still incomplete. The target's const
    // context nevertheless preserves both argument literals, matching the
    // pinned typeParameterConstModifiers own-node baselines.
    assert_eq!(
        lines.iter().filter(|line| line.as_str() == "{foo:\"\"} : { foo: \"\"; }").count(),
        2
    );
}
