//! Default-filled semantic references retain written signature annotations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn user_generic_defaults_substitute_preceding_arguments() {
    let source = r"// @strict: true
interface Pair<T,U=T>{pair:[T,U]}
declare const input:any;
export const computed=input as Pair<number>;
export const member=computed.pair;
export const literal=input as Pair<1>;
export const literalMember=literal.pair;
export const explicit=input as Pair<number,string>;
export const explicitMember=explicit.pair;
export function identity(value:Pair<string>):Pair<string>{return value;}
export const called=identity(input);
type Triple<T,U=T,V=U[]>={values:[T,U,V]};
export const triple=input as Triple<boolean>;
export const tripleMember=triple.values;
export const partial=input as Triple<boolean,number>;
export const partialMember=partial.values;
type Values=Triple<number>;
export function aliases(value:Values[]):Values[]{return value;}
export function constrained<T extends Values[]>(value:Values[]):T{throw value;}
";
    let case = TestCase::parse("probe/partial-generic-defaults", "defaults.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for expected in [
        "member : [number, number]",
        "literalMember : [1, 1]",
        "explicitMember : [number, string]",
        "identity : (value: Pair<string>) => Pair<string>",
        "called : Pair<string, string>",
        "tripleMember : [boolean, boolean, boolean[]]",
        "partialMember : [boolean, number, number[]]",
        "aliases : (value: Values[]) => Values[]",
        "constrained : <T extends Values[]>(value: Values[]) => T",
    ] {
        assert!(lines.iter().any(|line| line == expected), "Missing {expected}: {lines:?}");
    }
}
