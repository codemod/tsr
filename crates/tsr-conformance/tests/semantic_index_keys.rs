//! Semantic index lookup compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn semantic_index_keys_and_contexts() {
    let source = r#"// @strict: true
// @target: es2020
// @strict: true
// @target: es2020
type Funcs={ [key:`s${string}`]:(value:string)=>void; [key:`n${string}`]:(value:number)=>void };
export const funcs:Funcs={sfoo:text=>text.length,nfoo:count=>count*2};
declare const overlapping:{[key:`foo-${string}`]:"a"|"b";[key:`${string}-bar`]:"b"|"c"};
export const left=overlapping["foo-test"];
export const right=overlapping["test-bar"];
export const overlap=overlapping["foo-test-bar"];
interface SymbolIndex{[key:symbol]:number}
declare const symbolIndex:SymbolIndex;
declare const key:unique symbol;
export const symbolic=symbolIndex[key];
interface UnionIndex{[key:string|symbol]:number}
declare const unionIndex:UnionIndex;
export const stringUnion=unionIndex["value"];
export const symbolUnion=unionIndex[key];
type Tagged=string&{__tag:void};
interface TaggedIndex{[key:Tagged]:boolean}
declare const taggedIndex:TaggedIndex;
declare const tagged:Tagged;
export const taggedRead=taggedIndex[tagged];
interface NumericIndex{[key:number]:boolean}
declare const numericIndex:NumericIndex;
declare const numeric:`${number}`;
export const numericRead=numericIndex[numeric];
"#;
    let case = TestCase::parse("probe/semantic-index-keys", "semantic-index.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "text=>text.length : (text: string) => number",
        "count=>count*2 : (count: number) => number",
        "left : \"a\" | \"b\"",
        "right : \"b\" | \"c\"",
        "overlap : \"b\"",
        "symbolic : number",
        "stringUnion : number",
        "symbolUnion : number",
        "taggedRead : boolean",
        "numericRead : boolean",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn non_strict_nullable_keys_do_not_select_index_signatures() {
    let source = r"// @strict: false
interface Numeric {[key:number]:string}
interface Textual {[key:string]:number}
declare const numeric:Numeric;
declare const textual:Textual;
export const numericNull=numeric[null];
export const numericUndefined=numeric[undefined];
export const textualNull=textual[null];
export const textualUndefined=textual[undefined];
";
    let case = TestCase::parse("probe/nullable-index-keys", "nullable-index.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for name in ["numericNull", "numericUndefined", "textualNull", "textualUndefined"] {
        // Invalid keys decline. General tsgo errorType-to-any recovery is
        // still incomplete for this unannotated declaration shape.
        let wanted = format!("{name} : error");
        assert!(lines.iter().any(|line| line == &wanted), "missing {wanted}: {lines:?}");
    }
}
