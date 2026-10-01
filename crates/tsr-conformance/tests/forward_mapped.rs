//! Forward mapped property controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn forward_mapped_templates_substitute_property_and_index_keys() {
    let source = r#"// @strict: true
// @target: es2015
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };
declare let boxed: Boxified<{a:number; b?:string}>;
const value = boxed.a.value;
const maybe = boxed.b;
type RequiredBox<T> = { [P in keyof T]-?: Box<T[P]> };
declare let required: RequiredBox<{a?:string | null}>;
const requiredValue = required.a.value;
type Named = { [P in "x" | "y"]: { value: P } };
declare let named: Named;
const key = named.x.value;
type Dictionary = { [P in string]: { key: P } };
declare let dictionary: Dictionary;
const dictionaryKey = dictionary["hello"].key;
type Nested<T> = { [P in keyof T]: { inner: { value: T[P] } } };
declare let nested: Nested<{a:string}>;
const nestedValue = nested.a.inner.value;
"#;
    let case = TestCase::parse("probe/forward-mapped", "forward-mapped.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "value : number",
        "maybe : Box<string | undefined> | undefined",
        "requiredValue : string | null | undefined",
        "key : \"x\"",
        "dictionaryKey : string",
        "nestedValue : string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
