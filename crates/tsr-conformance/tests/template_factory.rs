//! Semantic template factory controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn templates_normalize_literal_spans_and_substitute_generic_holes() {
    let source = r#"// @strict: true
// @target: es2015
declare let union:`pre${"a"|"b"}-${1|2}`;
export const unionResult=union;
declare function make<T extends string>(value:T):`prefix-${T}`;
export const made=make("value");
type Repeat<T extends string>=`x${T}y`;
declare let alias:Repeat<"a"|"b">;
export const aliasResult=alias;
declare let nested:`pre${`in${number}`}post`;
export const nestedResult=nested;
declare let pure:`${string}`;
export const pureResult=pure;
declare let bottom:`${never}`;
export const neverResult=bottom;
declare let boolean:`${boolean}`;
export const booleanResult=boolean;
declare let nullable:`x${null|undefined}`;
export const nullableResult=nullable;
type Pat<T extends string|number|bigint|boolean|null|undefined>=`${T}`;
export function nullish(value:Pat<null|undefined>) {return value;}
type Digit="0"|"1"|"2"|"3"|"4"|"5"|"6"|"7"|"8"|"9";
declare let eliminated:`${Digit}${Digit}${Digit}${Digit}${Digit}${never}`;
export const eliminatedResult=eliminated;
"#;
    let case = TestCase::parse("probe/template-factory", "template-factory.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "unionResult : \"prea-1\" | \"prea-2\" | \"preb-1\" | \"preb-2\"",
        "made : \"prefix-value\"",
        "aliasResult : \"xay\" | \"xby\"",
        "nestedResult : `prein${number}post`",
        "pureResult : string",
        "neverResult : never",
        "booleanResult : \"false\" | \"true\"",
        "nullableResult : \"xnull\" | \"xundefined\"",
        "nullish : (value: Pat<null | undefined>) => \"null\" | \"undefined\"",
        "eliminatedResult : never",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
