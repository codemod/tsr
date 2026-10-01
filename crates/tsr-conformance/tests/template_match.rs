//! Template matching and constrained inference compared with pinned tsgo.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn template_matching_extracts_segments_and_constrained_literals() {
    let source = r#"// @strict: true
// @target: es2020
type Split<S extends string> = S extends `${infer A}.${infer B}` ? [A,B] : never;
declare let split:Split<"left.right.more">;
export const splitResult=split;
type Chars<S extends string> = S extends `${infer A}${infer B}` ? [A,B] : never;
declare let chars:Chars<"😀abc">;
export const charsResult=chars;
declare let empty:Chars<"">;
export const emptyResult=empty;
type Num<S extends string>=S extends `${infer N extends number}` ? N : never;
declare let num:Num<"42">;
export const numResult=num;
declare let noncanonical:Num<"1.0">;
export const noncanonicalResult=noncanonical;
declare let bad:Num<"x">;
export const badResult=bad;
type Big<S extends string>=S extends `${infer N extends bigint}` ? N : never;
declare let big:Big<"123456789012345678901234567890">;
export const bigResult=big;
type Bool<S extends string>=S extends `${infer B extends boolean}` ? B : never;
declare let bool:Bool<"true">;
export const boolResult=bool;
declare function middle<T extends string>(value:`pre${T}post`):T;
export const middleResult=middle("prevaluepost");
declare function symbolic<T extends string>(value:`pre${T}post`):T;
declare let symbolicSource:`pre${number}post`;
export const symbolicResult=symbolic(symbolicSource);

declare let radix:Num<"0x10">; export const radixResult=radix;
declare let whitespace:Num<"  ">; export const whitespaceResult=whitespace;
declare let minusZero:Num<"-0">; export const minusZeroResult=minusZero;
declare let separator:Num<"1_0">; export const separatorResult=separator;
declare let infinity:Num<"Infinity">; export const infinityResult=infinity;
declare let bigintRadix:Big<"0xffffffffffffffffffff">; export const bigintRadixResult=bigintRadix;
declare let bigintZero:Big<"-0">; export const bigintZeroResult=bigintZero;
declare let bigintLeading:Big<"01">; export const bigintLeadingResult=bigintLeading;
declare let boolBad:Bool<"x">; export const boolBadResult=boolBad;
declare let splitBad:Split<"nodot">; export const splitBadResult=splitBad;
declare let splitUnion:Split<"a.b"|"x.y">; export const splitUnionResult=splitUnion;
export function overlap(x:`foo-${string}`,y:`${string}-bar`) { if(x===y) return x; return undefined; }
export function disjoint(x:`foo-${string}`,y:`baz-${string}`) { if(x===y) return x; return undefined; }
type RecordNum<S extends string>=S extends `${infer N extends number}` ? { value:N } : never;
declare let recordNum:RecordNum<"42">;
export const recordNumResult=recordNum.value;
type Action<T,P>=P extends void ? {type:T} : {type:T,payload:P};
declare let actionNumber:Action<string,number>;
export const actionNumberResult=actionNumber.payload;
declare let actionBoolean:Action<number,boolean>;
export const actionBooleanResult=actionBoolean.payload;
"#;
    let case = TestCase::parse("probe/template-match", "template-match.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "splitResult : [\"left\", \"right.more\"]",
        "charsResult : [\"😀\", \"abc\"]",
        "emptyResult : never",
        "numResult : 42",
        "noncanonicalResult : number",
        "badResult : never",
        "bigResult : 123456789012345678901234567890n",
        "boolResult : true",
        "middleResult : \"value\"",
        "symbolicResult : `${number}`",
        "recordNumResult : 42",
        "actionNumberResult : number",
        "actionBooleanResult : boolean",
        "radixResult : number",
        "whitespaceResult : number",
        "minusZeroResult : number",
        "separatorResult : never",
        "infinityResult : never",
        "bigintRadixResult : bigint",
        "bigintZeroResult : bigint",
        "bigintLeadingResult : never",
        "boolBadResult : never",
        "splitBadResult : never",
        "splitUnionResult : [\"a\", \"b\"] | [\"x\", \"y\"]",
        "overlap : (x: `foo-${string}`, y: `${string}-bar`) => `foo-${string}` | undefined",
        "disjoint : (x: `foo-${string}`, y: `baz-${string}`) => undefined",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
