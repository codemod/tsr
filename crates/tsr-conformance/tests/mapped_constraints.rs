//! Constrained mapped inference compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn mapped_key_constraints_infer_keys_and_templates() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T> = {value:T};
declare function keyed<T,K extends PropertyKey>(value:{[P in K]:T}):[T,K];
export const uniform = keyed({a:1,b:2});
declare function picked<T,K extends keyof T>(value:Pick<T,K>):T;
export const selected = picked({a:1,b:"s"});
type BoxPick<T,K extends keyof T> = {[P in K]:Box<T[P]>};
declare function unpick<T,K extends keyof T>(value:BoxPick<T,K>):[T,K];
export const unpicked = unpick({a:{value:1},b:{value:"s"}});
declare function filtered<T>(value:{[P in keyof T & ("a"|"b")]:Box<T[P]>}):T;
export const limited = filtered({a:{value:1},c:{value:"s"}});
declare function chained<T,K extends L,L extends keyof T>(value:{[P in K]:Box<T[P]>}):[T,K,L];
export const chain = chained({a:{value:1},b:{value:"s"}});
"#;
    let case = TestCase::parse("probe/mapped-constraints", "mapped-constraints.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "uniform : [number, \"a\" | \"b\"]",
        "selected : { a: number; b: string; }",
        "unpicked : [{ a: number; b: string; }, \"a\" | \"b\"]",
        "limited : { a: number; }",
        "chain : [{ a: number; b: string; }, \"a\" | \"b\", \"a\" | \"b\"]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
