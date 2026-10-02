//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/generic_assignability", "probe.ts", source);
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

const SOURCE: &str = r#"// @strict: true
// @target: esnext
export type Q<T> = number extends T ? (n:number)=>void : never;
export function retain<T>(generic:Q<T>) { return generic; }
export function read<T>(callback:Q<T>) { callback(10); }
export const concrete=retain<number>((value)=>value.toFixed());
export const union=retain<number|string>((value)=>value.toFixed());
export type Nothing=Q<string>;
export type Bound<T extends string> = number extends T ? "yes":"no";
export function bounded<T extends string>(deferred:Bound<T>){return deferred;}

export type GenericUnion<T> = number extends T|string ? "yes":"no";
export function preserveUnion<T>(unionOperand:GenericUnion<T>){return unionOperand;}
export type GenericIntersection<T> = number extends T&number ? "yes":"no";
export function preserveIntersection<T>(intersectionOperand:GenericIntersection<T>){return intersectionOperand;}
declare function apply<T,U>(value:T, callback:(arg:T)=>U):U;
declare function applyWith<T,U>(value:T, callback:(arg:T)=>U, extra:U):U;
export function inference<T>(value:T) {
 const first=apply(1,(arg:T)=>"");
 const second=applyWith(1,(arg:T)=>"","");
 const third=applyWith(1,(arg:T)=>"",1);
 return {first,second,third};
}
"#;

#[test]
fn generic_extends_operands_defer_and_invalid_callbacks_keep_native_inference() {
    expect(
        SOURCE,
        &[
            "generic : Q<T>",
            "callback(10) : void",
            "concrete : (n: number) => void",
            "union : (n: number) => void",
            "Nothing : never",
            "deferred : Bound<T>",
            "unionOperand : GenericUnion<T>",
            "intersectionOperand : GenericIntersection<T>",
            "first : string",
            "second : string",
            "third : string",
        ],
    );
}
