//! Variable tuple inference controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn fixed_constraints_split_variadic_and_rest_slices() {
    let source = r#"// @strict: true
// @target: es2015
type Prefix<T extends unknown[]> = T extends [...infer B extends [any, any], ...(infer C)[]] ? [...B, C] : never;
type Suffix<T extends unknown[]> = T extends [...(infer C)[], ...infer B extends [any, any]] ? [C, ...B] : never;
type Front = Prefix<[a: 0, b: 1, ...c: 2[]]>;
declare const front: Front;
type Back = Suffix<[...a: 0[], b: 1, c: 2]>;
declare const back: Back;
type FixedFront = Prefix<[0, 1, 2]>;
declare const fixedFront: FixedFront;
type FixedBack = Suffix<[0, 1, 2]>;
declare const fixedBack: FixedBack;
type RestParams = [y: string] | [y: number];
type Signature = (x: string, ...rest: RestParams) => void;
type MergedParams = Parameters<Signature>;
declare function pair<T>(value: [T, T]): T;
const arrayPair = pair([1, 2] as number[]);
declare function head<T extends unknown[]>(value: [...T, number?]): T;
const speculative = head(["x", 1]);
export function result() { return { front, back, fixedFront, fixedBack, arrayPair, speculative }; }
"#;
    let case = TestCase::parse("probe/variable-tuples", "variable-tuples.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "front : [a: 0, b: 1, 2]",
        "back : [0, b: 1, c: 2]",
        "fixedFront : [0, 1, 2]",
        "fixedBack : [0, 1, 2]",
        "MergedParams : [x: string, y: string] | [x: string, y: number]",
        "arrayPair : number",
        "speculative : [string]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
