//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/generic_assignability", "probe.ts", source);
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

/// Recursive variance stops only the circular occurrence, not the independent
/// input/output evidence. Pinned native rejects exactly the five negative
/// assignments; the asymmetric positive twins remain diagnostic-free.
#[test]
fn recursive_variance_diagnostics_preserve_asymmetric_assignments() {
    let source = r"// @strict: true
// @target: es2015
interface Phantom<T> { next: Phantom<T[]> }
interface Derived<T> extends Phantom<T> {}
declare const phantom: Phantom<{ left: string }>;
declare const derived: Derived<{ right: number }>;
const phantomForward: Phantom<{ left: string }> = derived;
const phantomReverse: Derived<{ right: number }> = phantom;
interface Value<T> { value: T; next: Value<(input: T) => void> }
declare const text: Value<string>;
declare const literal: Value<'a'>;
const valuePositive: Value<string> = literal;
const valueNegative: Value<'a'> = text;
const valueUnrelated: Value<number> = text;
interface Invariant<T> { value: T; next: { consume: (input: T) => void; recurse: Invariant<(input: T) => void> } }
declare const invariantText: Invariant<string>;
const invariantNegative: Invariant<unknown> = invariantText;
interface Fn<A, B> { (a: A): B; then<C>(next: Fn<B, C>): Fn<A, C> }
declare const fn: Fn<string, number>;
const inputNegative: Fn<unknown, number> = fn;
const inputPositive: Fn<'a', number> = fn;
const outputPositive: Fn<string, unknown> = fn;
const outputNegative: Fn<string, 0> = fn;
";
    let case = TestCase::parse("probe/recursive-variance", "recursive-variance.ts", source);
    let mut diagnostics: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    assert_eq!(
        diagnostics,
        vec![(11, 7, 2322), (12, 7, 2322), (15, 7, 2322), (18, 7, 2322), (21, 7, 2322)]
    );
}
