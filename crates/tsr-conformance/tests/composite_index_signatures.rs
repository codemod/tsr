//! Composite and class index controls grounded in pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn composite_and_static_index_semantics() {
    let source = r#"// @strict: true
// @target: es2020
declare const union: { [key: string]: number } | { [key: string]: string };
declare const intersection: { [key: string]: "a" | "b" } & { [key: string]: "b" | "c" };
declare const distinct: { [key: string]: number } & { [key: number]: 42 };
declare const key: string;
declare const index: number;
export const unionValue = union[key];
export const intersectionValue = intersection[key];
export const distinctNumber = distinct[index];
export const distinctString = distinct[key];
export const contextual: { [key: `cb-${string}`]: (n: number) => string } & { tag: string } = {
 tag: "tag", "cb-main": n => n.toFixed()
};
export class Base {
 static [key: string]: number;
 static [key: number]: 42;
 [key: string]: boolean;
}
export class Derived extends Base {}
declare const instance: Base;
export const staticString = Base[key];
export const staticNumber = Base[index];
// @ts-expect-error Static index signatures are not inherited.
export const inheritedString = Derived[key];
export const instanceString = instance[key];
export const Expression = class {
 static [key: string]: number;
 [key: string]: boolean;
};
export const expressionStatic = Expression[key];
export const expressionInstance = new Expression()[key];
export const namedContext: { [key: string]: (n: any) => any } & { special: (s: string) => string } = {
 special: s => s
};
// Recursive contextual union from contextualTypeShouldBeLiteral.
interface TestObject { type?: "object"; items: { [k: string]: TestGeneric }; }
interface TestString { type: "string"; }
type TestGeneric = (TestString | TestObject) & { [k: string]: any };
const test: TestGeneric = { items: { hello: { type: "string" } } };
"#;
    let case = TestCase::parse("probe/composite-index", "indexes.ts", source);
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
    for wanted in [
        "unionValue : string | number",
        r#"intersectionValue : "b""#,
        "distinctNumber : 42",
        "distinctString : number",
        "n => n.toFixed() : (n: number) => string",
        "staticString : number",
        "staticNumber : 42",
        "instanceString : boolean",
        "expressionStatic : number",
        "expressionInstance : boolean",
        "s => s : (s: string) => string",
        r#"hello : { type: "string"; }"#,
        // The absent inherited static index declines. Native errorType-to-any
        // recovery on this unannotated declaration remains a separate gap.
        "inheritedString : error",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
