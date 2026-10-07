//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/adjusted_type_facts", "probe.ts", source);
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
fn captured_alias_members_keep_their_instantiations() {
    expect(
        r#"// @strict: true
type Methods<T>={kind:"a";value:T;get():T}|{kind:"b";value:T[];get():T[]};
export function methodRead({kind,value,get}:Methods<number>){if(kind==="a")return [value,get()] as const;throw 0;}
export function independent(a:Methods<number>,b:Methods<string>){if(a.kind==="a"&&b.kind==="a")return [a.get(),b.get()] as const;throw 0;}
type Access<T>={kind:"a";get value():T;set value(v:T)}|{kind:"b";get value():T[]};
export function accessor(v:Access<number>){if(v.kind==="a")return v.value;throw 0;}
type Callable<T>={kind:"a";(v:T):T}|{kind:"b";(v:T[]):T[]};
export function callable(v:Callable<number>){if(v.kind==="a")return v(1);throw 0;}
type Construct<T>={kind:"a";new(v:T):{value:T}}|{kind:"b";new(v:T[]):{value:T[]}};
export function construct(v:Construct<number>){if(v.kind==="a")return new v(1).value;throw 0;}
type Indexed<T>={kind:"a";[i:number]:T}|{kind:"b";[i:number]:T[]};
export function indexed(v:Indexed<number>){if(v.kind==="a")return v[0];throw 0;}
declare const key:unique symbol;
type Computed<T>={kind:"a";[key]:T}|{kind:"b";[key]:T[]};
export function computed(v:Computed<number>){if(v.kind==="a")return v[key];throw 0;}
type Overloaded<T>={kind:"a";get(v:string):T;get(v:number):T[]}|{kind:"b";get(v:string):T[]};
export function overloaded(v:Overloaded<number>){if(v.kind==="a")return [v.get("x"),v.get(0)] as const;throw 0;}
"#,
        &[
            "methodRead : ({ kind, value, get }: Methods<number>) => readonly [number, number]",
            "independent : (a: Methods<number>, b: Methods<string>) => readonly [number, string]",
            "accessor : (v: Access<number>) => number",
            "callable : (v: Callable<number>) => number",
            "construct : (v: Construct<number>) => number",
            "indexed : (v: Indexed<number>) => number",
            "computed : (v: Computed<number>) => number",
            "overloaded : (v: Overloaded<number>) => readonly [number, number[]]",
        ],
    );
}

#[test]
fn member_boundaries_preserve_signatures_and_alias_identity() {
    expect(
        r#"// @strict: true
export type U<T>={kind:"a";maybe?():T;generic<V>(v:V):[T,V];["x y"]():T}|{kind:"b";maybe?():T[];generic<V>(v:V):[T[],V];["x y"]():T[]};
export function optional(u:U<number>){if(u.kind==="a")return u.maybe?.();throw 0;}
export function generic(u:U<number>){if(u.kind==="a")return u.generic("x");throw 0;}
export function quoted(u:U<number>){if(u.kind==="a")return u["x y"]();throw 0;}
export function unchanged(u:U<number>){if(Math.random())u;return u;}
type I<T>={kind:"a";[n:number]:T}|{kind:"b";[n:number]:T[]};
export function unknownIndex(u:I<number>,n:number){if(u.kind==="a")return u[n];throw 0;}
"#,
        &[
            "optional : (u: U<number>) => number | undefined",
            "generic : (u: U<number>) => [number, string]",
            "quoted : (u: U<number>) => number",
            "unchanged : (u: U<number>) => U<number>",
            "unknownIndex : (u: I<number>, n: number) => number",
        ],
    );
}

#[test]
fn returned_literal_members_survive_reinstantiation() {
    expect(
        r#"// @strict: true
export function source<T>(x:T): {kind:"a";get():T;(v:T):T;[n:number]:T}|{kind:"b";get():T[]} {throw 0;}
export function instantiate(){const u=source(1);if(u.kind==="a")return [u.get(),u(1),u[0]] as const;throw 0;}
"#,
        &["instantiate : () => readonly [number, number, number]"],
    );
}

#[test]
fn computed_members_and_shadowed_parameters_keep_distinct_roles() {
    expect(
        r#"// @strict: true
// @noUncheckedIndexedAccess: true
declare const a:unique symbol; declare const b:unique symbol;
export function source<T>(): { [a]():T; [b]:(x:T)=>T; [n:number]:T } {throw 0;}
export function reads(){const x=source<number>();return [x[a](),x[b](1),x[0]] as const;}
export function shadow<T>(v:T):{get<T>(x:T):T;value:T}{throw 0;}
export function shadowRead(){const x=shadow(1);return [x.get("s"),x.value] as const;}
"#,
        &[
            "reads : () => readonly [number, number, number | undefined]",
            "shadowRead : () => readonly [\"s\", number]",
            "x : { [n: number]: number; [a](): number; [b]: (x: number) => number; }",
        ],
    );
}
