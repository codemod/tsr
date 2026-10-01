//! Const initializer values compared with pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn template_folding_follows_const_symbols_and_initializers() {
    let source = r#"// @strict: true
// @target: es2020
const a = "1";
const b = a + " 2";
export const chained = `${b} 3`;
export const chainedConst = `${b} 3` as const;
const arithmetic = (1 + 2) * 3;
export const arithmeticTemplate = `value:${arithmetic}`;
const annotated: string = "fixed";
export const annotatedTemplate = `${annotated}`;
let mutable = "fixed";
export const mutableTemplate = `${mutable}`;
const literalAnnotated: "fixed" = "fixed";
export const literalAnnotatedTemplate = `${literalAnnotated}`;
namespace Values { export const prefix = "a" + "b"; }
export const namespaceTemplate = `${Values.prefix}c`;
export function shadow() { const b = "inner" + " value"; return `${b}!`; }
export function deferred() { return `${later}`; }
const later = "late" + " value";
const object = {field:"value"};
export const propertyTemplate = `${object.field}`;
"#;
    let case = TestCase::parse("probe/constant-variables", "constant-variables.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "chained : \"1 2 3\"",
        "chainedConst : \"1 2 3\"",
        "arithmeticTemplate : \"value:9\"",
        "annotatedTemplate : string",
        "mutableTemplate : string",
        "literalAnnotatedTemplate : string",
        "namespaceTemplate : \"abc\"",
        "shadow : () => string",
        "deferred : () => string",
        "propertyTemplate : string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn template_values_decline_forward_cycles_annotations_mutable_and_tagged_sources() {
    let source = r#"// @strict: true
// @target: es2020
export const before = `${after}`;
const after = "late" + " value";
const self = `${self}`;
export const selfTemplate = `${self}`;
const first = second + "a";
const second = first + "b";
export const cycleTemplate = `${first}`;
let literalMutable: "fixed" = "fixed";
export const mutableLiteralTemplate = `${literalMutable}`;
declare function tag(parts:TemplateStringsArray,value:string):string;
const text = "a" + "b";
export const tagged = tag`${text}`;
let t1 = "foo" as const;
let t2 = "bar" as const;
export const nestedConst = `${`(${t1})`}-${`(${t2})`}` as const;
"#;
    let case = TestCase::parse("probe/constant-guards", "constant-guards.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "before : string",
        "selfTemplate : string",
        "cycleTemplate : string",
        "mutableLiteralTemplate : string",
        "tagged : string",
        "nestedConst : \"(foo)-(bar)\"",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
