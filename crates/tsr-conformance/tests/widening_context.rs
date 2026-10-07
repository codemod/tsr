//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/widening_context", "probe.ts", source);
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
fn widening_context_normalizes_siblings_and_nested_properties() {
    expect(
        r#"// @strict: true
export let flat = [{a:0}, {a:1,b:"x"}, {a:2,b:"y",c:true}][0];
export function reads(){return [flat.a,flat.b,flat.c] as const;}
export let nested = [{pos:{x:0,y:0}}, {pos:!true?{a:"x"}:{b:0}}][0];
export function nestedReads(){return [nested.pos.x,nested.pos.a,nested.pos.b] as const;}
export let empty = !true ? {a:0,b:0} : {};
declare let source: {a:number};
export let spread = !true ? {...source} : {b:"x"};
export let spreadReads = [spread.b] as const;
export let readonly = [{a:1},{b:"x"}] as const;
declare function f<T>(...items:T[]):T;
export let inferred=f({a:1,b:2},{a:"abc"},{});
"#,
        &[
            "flat : { a: number; b?: undefined; c?: undefined; } | { a: number; b: string; c?: undefined; } | { a: number; b: string; c: boolean; }",
            "reads : () => readonly [number, string | undefined, boolean | undefined]",
            "nestedReads : () => readonly [number | undefined, string | undefined, number | undefined]",
            "empty : { a: number; b: number; } | { b?: undefined; a?: undefined; }",
            "spread : { b?: undefined; a: number; } | { b: string; }",
            "spreadReads : readonly [string | undefined]",
            "readonly : readonly [{ readonly a: 1; }, { readonly b: \"x\"; }]",
            "inferred : { a: number; b: number; } | { b?: undefined; a: string; } | { b?: undefined; a?: undefined; }",
        ],
    );
}

#[test]
fn normalized_methods_accessors_and_array_properties_preserve_their_members() {
    expect(
        r#"// @strict: true
export let methods = [{a:1,m(){return 1;}},{b:2,n(){return "x";}}][0];
export let accessors = [{get x(){return 1;}, y:"x"},{z:true}][0];
export let nestedArrays = [{p:[{a:1},{b:2}]},{p:[{c:true}]}][0];
"#,
        &[
            "methods : { a: number; m(): number; b?: undefined; n?: undefined; } | { a?: undefined; m?: undefined; b: number; n(): string; }",
            "accessors : { readonly x: number; y: string; z?: undefined; } | { readonly x?: undefined; y?: undefined; z: boolean; }",
            "nestedArrays : { p: ({ b?: undefined; a: number; } | { a?: undefined; b: number; })[]; } | { p: { c: boolean; }[]; }",
        ],
    );
}

#[test]
fn exact_optional_missing_members_read_as_undefined() {
    expect(
        r#"// @strict: true
// @exactOptionalPropertyTypes: true
export let value = [{a:1},{b:"x"}][0];
export function read(){return [value.a,value.b] as const;}
"#,
        &["read : () => readonly [number | undefined, string | undefined]"],
    );
}
