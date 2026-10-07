//! Inferred empty arrays retain a distinct identity from written element types.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn assignment_arrays_widen_only_inferred_empty_elements() {
    for strict in [true, false] {
        let source = format!(
            "// @strict: {strict}\n// @noImplicitAny: false\n// @target: es2020\n{}",
            r"
export function values() {
 function callable() {}
 callable.items=[];
 return callable.items;
}
declare const writtenNever: never[];
declare const writtenUndefined: undefined[];
export function writtenTypes() {
 function callable() {}
 callable.never=writtenNever;
 callable.undefined=writtenUndefined;
 return callable;
}
const annotated: {():void; items:string[]} = () => {};
annotated.items=[];
export const annotatedItems=annotated.items;
export const emptyResult=values();
export const preserved=writtenTypes();
"
        );
        let case = TestCase::parse("probe/empty-assignment-arrays", "arrays.ts", &source);
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
        for wanted in [
            "values : () => any[]",
            "emptyResult : any[]",
            "annotatedItems : string[]",
            "preserved : { (): void; never: never[]; undefined: undefined[]; }",
        ] {
            assert!(
                lines.iter().any(|line| line == wanted),
                "strict={strict}, missing {wanted}: {lines:?}"
            );
        }
    }
}

#[test]
fn javascript_initializers_widen_inferred_empty_arrays() {
    let source = r"// @strict: true
// @noImplicitAny: false
// @target: es2020
// @allowJs: true
// @checkJs: true
// @filename: initializers.js
export const items=[];
export class Store { items=[]; }
export function initial(value=[]) { return value; }
/** @type {never[]} */
export const written=[];
";
    let case = TestCase::parse("probe/js-empty-initializers", "initializers.js", source);
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
    for wanted in ["items : any[]", "initial : (value?: any[]) => any[]", "written : never[]"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
