//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/widening_context", "probe.ts", source);
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
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn index_relations_use_semantic_infos_and_native_inference_rules() {
    expect(
        r#"// @strict: true
export let indexed = [{["a" as string]:1},{b:"x"}][0];
export let numeric = [{[0 as number]:1},{tag:"x",0:2}][0];
type Rel<S,T> = S extends T ? true:false;
interface Members { a:number }
class ClassMembers { a=1 }
type AliasMembers = { a:number };
declare const interfaceIndex: Rel<Members, {[x:string]:number}>;
declare const classIndex: Rel<ClassMembers, {[x:string]:number}>;
declare const aliasIndex: Rel<AliasMembers, {[x:string]:number}>;
declare const numericIndex: Rel<{0:number;tag:string}, {[x:number]:number}>;
declare const badNumericIndex: Rel<{0:string;tag:number}, {[x:number]:number}>;
declare const optionalIndex: Rel<{a?:number}, {[x:string]:number}>;
declare const optionalNumericIndex: Rel<{0?:number}, {[x:number]:number}>;
declare const mixedIndex: Rel<{():number;[x:string]:string}, {():number;[x:string]:number}>;
declare const mixedSame: Rel<{():number;[x:string]:number}, {():number;[x:string]:number}>;
declare const anyIndex: Rel<Members, {[x:string]:any}>;
export function checks(){return [interfaceIndex,classIndex,aliasIndex,numericIndex,badNumericIndex,optionalIndex,optionalNumericIndex,mixedIndex,mixedSame,anyIndex] as const;}
declare const patternIndex: Rel<{"x-a":number; y:string}, {[k:`x-${string}`]:number}>;
declare const badPatternIndex: Rel<{"x-a":string; y:number}, {[k:`x-${string}`]:number}>;
declare const unique: unique symbol;
declare const symbolIndex: Rel<{[unique]:number; tag:string}, {[k:symbol]:number}>;
export function keyChecks(){return [patternIndex,badPatternIndex,symbolIndex] as const;}
"#,
        &[
            "indexed : { [x: string]: number; b?: undefined; } | { b: string; }",
            "numeric : { [x: number]: number; tag?: undefined; 0?: undefined; } | { tag: string; 0: number; }",
            "checks : () => readonly [false, false, true, true, false, true, false, false, true, true]",
            "keyChecks : () => readonly [true, false, true]",
        ],
    );
}

#[test]
fn exact_optional_index_relations_remove_missing_but_retain_explicit_undefined() {
    expect(
        r"// @strict: true
// @exactOptionalPropertyTypes: true
type Rel<S,T> = S extends T ? true:false;
declare const optional: Rel<{a?:number}, {[x:string]:number}>;
declare const explicit: Rel<{a?:number|undefined}, {[x:string]:number}>;
declare const numeric: Rel<{0?:number}, {[x:number]:number}>;
export function exactChecks(){return [optional,explicit,numeric] as const;}
",
        &["exactChecks : () => readonly [true, false, true]"],
    );
}
