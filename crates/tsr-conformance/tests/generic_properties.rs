//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/generic_properties", "probe.ts", source);
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
fn semantic_parameter_properties_participate_in_native_relations() {
    expect(
        r"// @strict: true
export function recursive<T extends {next:T}>(x:T,y:{next:T},flag:boolean){return flag?x:y;}
interface Box<T>{value:T}
export function same<T>(x:Box<T>,y:{value:T},flag:boolean){return flag?x:y;}
export function constrained<T,U extends T>(x:Box<T>,y:Box<U>,flag:boolean){return flag?x:y;}
export function unrelated<T,U>(x:Box<T>,y:Box<U>,flag:boolean){return flag?x:y;}
export function concrete(x:Box<number>,y:Box<string>,flag:boolean){return flag?x:y;}
export function readonlyMember<T>(x:{value:T},y:{readonly value:T},flag:boolean){return flag?x:y;}
export function optionalMember<T>(x:{value:T},y:{value?:T},flag:boolean){return flag?x:y;}
",
        &[
            "recursive : <T extends { next: T; }>(x: T, y: { next: T; }, flag: boolean) => { next: T; }",
            "same : <T>(x: Box<T>, y: { value: T; }, flag: boolean) => Box<T>",
            "constrained : <T, U extends T>(x: Box<T>, y: Box<U>, flag: boolean) => Box<T>",
            "unrelated : <T, U>(x: Box<T>, y: Box<U>, flag: boolean) => Box<T> | Box<U>",
            "concrete : (x: Box<number>, y: Box<string>, flag: boolean) => Box<string> | Box<number>",
            "readonlyMember : <T>(x: { value: T; }, y: { readonly value: T; }, flag: boolean) => { readonly value: T; }",
            "optionalMember : <T>(x: { value: T; }, y: { value?: T; }, flag: boolean) => { value?: T; }",
        ],
    );
}

#[test]
fn receiver_maps_use_parameter_identity_across_shadowed_members() {
    expect(
        r#"// @strict: true
class Holder<T> {
 value!:T;
 choose<T>(v:T,flag:boolean){return flag?this.value:v;}
 pair<T>(v:T){return [this.value,v] as const;}
 own<T extends {tag:string}>(v:T){return v.tag;}
}
class Derived<V> extends Holder<V>{}
export function numberChoice(h:Holder<number>){return h.choose("x",true);}
export function stringChoice(h:Holder<string>){return h.choose(1,true);}
export function inherited(h:Derived<number>){return h.choose("x",false);}
export function pair(h:Holder<number>){return h.pair("x");}
export function retained(h:Holder<number>){return h.own({tag:"ok"});}
"#,
        &[
            "numberChoice : (h: Holder<number>) => number | \"x\"",
            "stringChoice : (h: Holder<string>) => string | 1",
            "inherited : (h: Derived<number>) => number | \"x\"",
            "pair : (h: Holder<number>) => readonly [number, string]",
            "retained : (h: Holder<number>) => string",
        ],
    );
}
