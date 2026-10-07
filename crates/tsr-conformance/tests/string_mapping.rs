//! Semantic string mappings compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn intrinsic_string_mappings_follow_pinned_unicode_and_template_semantics() {
    let source = r#"// @strict: true
// @target: es2020
declare let upper:Uppercase<"ßﬁa">; export const upperResult=upper;
declare let lower:Lowercase<"İΟΣ">; export const lowerResult=lower;
declare let capital:Capitalize<"ßfoo">; export const capitalResult=capital;
declare let uncapital:Uncapitalize<"İfoo">; export const uncapitalResult=uncapital;
declare let empty:Capitalize<"">; export const emptyResult=empty;
declare let sigma:Lowercase<"ʕΣ">; export const sigmaResult=sigma;
declare let ignorable:Lowercase<"ΣͅA">; export const ignorableResult=ignorable;
declare let recent:Lowercase<"ᲉΣ">; export const recentResult=recent;
declare let union:Uppercase<"ab"|"ß">; export const unionResult=union;
declare let neverMapped:Uppercase<never>; export const neverResult=neverMapped;
declare let twice:Uppercase<Uppercase<string>>; export const twiceResult=twice;
declare let anyMapped:Lowercase<any>; export const anyResult=anyMapped;
declare let pattern:Uppercase<`ab-${string}-ß`>; export const patternResult=pattern;
declare let numeric:Uppercase<`ab-${number}`>; export const numericResult=numeric;
declare let head:Capitalize<`${string}-tail`>; export const headResult=head;
declare let prefix:Capitalize<`prefix-${string}`>; export const prefixResult=prefix;
declare function mapUpper<T extends string>(value:T):Uppercase<T>;
export const instantiatedResult=mapUpper("hello");
type ExtractUpper<S extends string>=S extends `${infer T extends Uppercase<string>}` ? T : never;
declare let accepted:ExtractUpper<"ABC">; export const acceptedResult=accepted;
declare let rejected:ExtractUpper<"abc">; export const rejectedResult=rejected;
enum E { A="alpha", B=2,C }
declare let enumText:`tag-${E}`; export const enumTextResult=enumText;
declare let enumUpper:Uppercase<E.A>; export const enumUpperResult=enumUpper;
export function mappedAccept(x:string) { if(x==="ABC") {const y:Uppercase<string>=x;return y;} return undefined; }

declare function fromMapped<T extends string>(value:Uppercase<T>):T;
export function mappingIdentity<U extends string>(value:Uppercase<U>) { return fromMapped(value); }
declare function writtenTemplate(value:Uppercase<`a-${string}`>):void;
export const writtenTemplateResult=writtenTemplate;
type Normalized=Uppercase<`a${boolean}`>;
declare function namedUnion(value:Normalized):void;
export const namedUnionResult=namedUnion;
declare let patternCarrier:`${Uppercase<Lowercase<`${number}`>>}`;
export const patternCarrierResult=patternCarrier;
"#;
    let case = TestCase::parse("probe/string-mapping", "string-mapping.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "upperResult : \"SSFIA\"",
        "lowerResult : \"i̇ος\"",
        "capitalResult : \"SSfoo\"",
        "uncapitalResult : \"i̇foo\"",
        "emptyResult : \"\"",
        "sigmaResult : \"ʕς\"",
        "ignorableResult : \"σͅa\"",
        "recentResult : \"Ᲊσ\"",
        "unionResult : \"AB\" | \"SS\"",
        "neverResult : never",
        "twiceResult : Uppercase<string>",
        "anyResult : Lowercase<any>",
        "patternResult : `AB-${Uppercase<string>}-SS`",
        "numericResult : `AB-${Uppercase<`${number}`>}`",
        "headResult : `${Capitalize<string>}-tail`",
        "prefixResult : `Prefix-${string}`",
        "instantiatedResult : \"HELLO\"",
        "acceptedResult : \"ABC\"",
        "rejectedResult : never",
        "enumTextResult : \"tag-2\" | \"tag-3\" | \"tag-alpha\"",
        "enumUpperResult : \"ALPHA\"",
        "mappingIdentity : <U extends string>(value: Uppercase<U>) => U",
        "writtenTemplate : (value: Uppercase<`a-${string}`>) => void",
        "namedUnion : (value: Normalized) => void",
        "patternCarrierResult : Uppercase<Lowercase<`${number}`>>",
        "mappedAccept : (x: string) => Uppercase<string> | undefined",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
