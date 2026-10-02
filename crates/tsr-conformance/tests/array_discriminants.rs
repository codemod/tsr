//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/array_discriminants", "probe.ts", source);
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
fn array_binding_discriminants_follow_positions_and_const_like_guards() {
    expect(
        r#"// @strict: true
type Args = ["A", number] | ["B", string];
export function rest(...[kind, value]: Args) { if (kind === "A") return value; throw 0; }
export function local(args: Args) { const [kind, value] = args; if (kind === "B") return value; throw 0; }
export function hole(args: ["A", boolean, number] | ["B", boolean, string]) { const [kind, , value] = args; if (kind === "A") return value; throw 0; }
export function absent(args: ["A", number] | ["B"]) { const [kind, value] = args; if (kind === "B") return value; throw 0; }
export function mutable(args: Args) { let [kind, value] = args; if (kind === "A") return value; throw 0; }
export function assigned([kind, value]: Args) { value = 0; if (kind === "B") return value; throw 0; }
export function defaulted([kind = "A", value]: Args) { if (kind === "A") return value; throw 0; }
export function exhausted(args: Args) { const [kind, value] = args; switch(kind) { case "A": throw 0; case "B": throw 0; default: return value; } }
"#,
        &[
            "rest : (...[kind, value]: Args) => number",
            "local : (args: Args) => string",
            "hole : (args: [\"A\", boolean, number] | [\"B\", boolean, string]) => number",
            "absent : (args: [\"A\", number] | [\"B\"]) => undefined",
            "mutable : (args: Args) => string | number",
            "assigned : ([kind, value]: Args) => number",
            "defaulted : ([kind, value]: Args) => string | number",
            "exhausted : (args: Args) => never",
        ],
    );
}

#[test]
fn generic_alias_members_are_instantiated_before_dependent_narrowing() {
    expect(
        r"// @strict: true
type Result<T> = {ok:false;data:undefined}|{ok:true;data:T};
declare function query():Result<number>;
export function read() {const {ok,data}=query();if(ok)return data;throw 0;}
export function typed(result:Result<number>) {const {ok,data}=result;if(ok)return data;throw 0;}
export function parameter({ok,data}:Result<number>) {if(ok)return data;throw 0;}
export function generic<T>({ok,data}:Result<T>) {if(ok)return data;throw 0;}
",
        &[
            "read : () => number",
            "typed : (result: Result<number>) => number",
            "parameter : ({ ok, data }: Result<number>) => number",
            "generic : <T>({ ok, data }: Result<T>) => T",
        ],
    );
}

#[test]
fn instantiated_alias_frames_stay_distinct_through_nested_and_recursive_bindings() {
    expect(
        r#"// @strict: true
type Result<T>={ok:false;data:undefined}|{ok:true;data:T};
export function independent(a:Result<number>,b:Result<string>){const {ok:ok1,data:data1}=a;const{ok:ok2,data:data2}=b;const both=ok1&&ok2;if(both)return [data1,data2] as const;throw 0;}
export function constrained<T extends Result<number>>(arg:T){const {ok,data}=arg;if(ok)return data;throw 0;}
type Methods<T>={kind:"a";value:T;get():T}|{kind:"b";value:T[];get():T[]};
export function methodRead({kind,value,get}:Methods<number>){if(kind==="a")return [value,get()] as const;throw 0;}
type Recursive<T>={kind:"a";value:T;next:Recursive<T>}|{kind:"b";value:T[];next:Recursive<T>};
export function recursive({kind,value}:Recursive<number>){if(kind==="a")return value;throw 0;}
type Alias<T>=Result<T>;
export function chained({ok,data}:Alias<string>){if(ok)return data;throw 0;}
export function nested({outer:{ok,data}}:{outer:Result<number>}){if(ok)return data;throw 0;}
export function returned<T>({ok,data}:Result<T>){return data;}
"#,
        &[
            "independent : (a: Result<number>, b: Result<string>) => readonly [number, string]",
            "constrained : <T extends Result<number>>(arg: T) => number",
            "recursive : ({ kind, value }: Recursive<number>) => number",
            "chained : ({ ok, data }: Alias<string>) => string",
            "nested : ({ outer: { ok, data } }: { outer: Result<number>; }) => number",
            "returned : <T>({ ok, data }: Result<T>) => T | undefined",
            // Native returns readonly [number, number]. Method-bearing alias
            // literals still decline; do not leak the uninstantiated T.
            "methodRead : ({ kind, value, get }: Methods<number>) => any",
        ],
    );
}

#[test]
fn static_numeric_properties_capture_instantiations_while_computed_members_decline() {
    expect(
        r#"// @strict: true
declare const key: unique symbol;
type U<T>={kind:"a";[key]:T}|{kind:"b";[key]:T[]};
export function computed(u:U<number>){const {kind}=u;if(kind==="a")return u[key];throw 0;}
type N<T>={kind:"a";0:T}|{kind:"b";0:T[]};
export function numeric(u:N<number>){const {kind,0:value}=u;if(kind==="a")return value;throw 0;}
"#,
        &[
            "numeric : (u: N<number>) => number",
            // Native returns number. Computed members are not captured yet.
            "computed : (u: U<number>) => any",
        ],
    );
}
