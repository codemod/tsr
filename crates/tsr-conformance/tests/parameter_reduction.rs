//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/parameter_reduction", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn constrained_parameters_reduce_against_their_semantic_supertypes() {
    expect(
        r"// @strict: true
export function chain<T extends U,U extends V,V>(x:T,y:U,z:V,flag:boolean){return flag?x:flag?y:z;}
export function primitive<T extends number>(x:T,y:number,flag:boolean){return flag?x:y;}
export function boxed<T extends Number>(x:T,flag:boolean){return flag?x:1;}
export function object<T extends {value:number}>(x:T,y:{value:number},flag:boolean){return flag?x:y;}
export function unrelated<T,U>(x:T,y:U,flag:boolean){return flag?x:y;}
export function covered<T extends string|number>(x:T,y:string,z:number,flag:boolean){return flag?x:flag?y:z;}
export function partial<T extends string|number>(x:T,y:string,flag:boolean){return flag?x:y;}
export function unbounded<T>(x:T,y:{},flag:boolean){return flag?x:y;}
export function bounded<T extends {}>(x:T,y:{},flag:boolean){return flag?x:y;}
",
        &[
            "chain : <T extends U, U extends V, V>(x: T, y: U, z: V, flag: boolean) => V",
            "primitive : <T extends number>(x: T, y: number, flag: boolean) => number",
            "boxed : <T extends Number>(x: T, flag: boolean) => 1 | T",
            "object : <T extends { value: number; }>(x: T, y: { value: number; }, flag: boolean) => { value: number; }",
            "unrelated : <T, U>(x: T, y: U, flag: boolean) => T | U",
            "covered : <T extends string | number>(x: T, y: string, z: number, flag: boolean) => string | number",
            "partial : <T extends string | number>(x: T, y: string, flag: boolean) => string | T",
            "unbounded : <T>(x: T, y: {}, flag: boolean) => T | {}",
            "bounded : <T extends {}>(x: T, y: {}, flag: boolean) => {}",
        ],
    );
}

#[test]
fn generic_references_and_object_union_constraints_reduce_semantically() {
    expect(
        r#"// @strict: true
type Box<T>={value:T};
export function references<T,U extends Box<T>>(x:U,y:Box<T>,flag:boolean){return flag?x:y;}
type A={kind:"a";a:number};type B={kind:"b";b:string};
export function coveredObjects<T extends A|B>(x:T,a:A,b:B,flag:boolean){return flag?x:flag?a:b;}
export function partialObjects<T extends A|B>(x:T,a:A,flag:boolean){return flag?x:a;}
"#,
        &[
            "references : <T, U extends Box<T>>(x: U, y: Box<T>, flag: boolean) => Box<T>",
            "coveredObjects : <T extends A | B>(x: T, a: A, b: B, flag: boolean) => A | B",
            "partialObjects : <T extends A | B>(x: T, a: A, flag: boolean) => T | A",
        ],
    );
}
