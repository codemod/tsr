//! Contextual generic return inference controls.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn generic_return_annotations_supply_callback_parameters() {
    let source = r"// @strict: true
// @target: es2020
type Mapper<T,U>=(value:T)=>U;
declare function wrap<T,U>(callback:Mapper<T,U>):Mapper<T,U>;
declare function arrayize<T,U>(callback:Mapper<T,U>):Mapper<T,U[]>;
export const mapper:Mapper<string,number>=wrap(value=>value.length);
export const nested:Mapper<string,number[]>=arrayize(wrap(nestedValue=>nestedValue.length));
export const twice:Mapper<string,number[][]>=arrayize(arrayize(wrap(deep=>deep.length)));
";
    let case = TestCase::parse(
        "probe/contextual-generic-returns",
        "contextual-generic-returns.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    assert!(
        lines.iter().any(|line| line == "value=>value.length : (value: string) => number"),
        "{lines:?}"
    );
    assert!(lines.iter().any(|line| line=="nestedValue=>nestedValue.length : (nestedValue: string) => number"), "{lines:?}");
    assert!(
        lines.iter().any(|line| line == "deep=>deep.length : (deep: string) => number"),
        "{lines:?}"
    );
}
