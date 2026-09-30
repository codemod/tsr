//! Receiver inference controls compared with pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn generic_this_inference_uses_receivers_through_access_wrappers_and_optional_chains() {
    let source = r#"// @strict: true
// @target: es2015
interface Receiver { value: number; method<T>(this: T): T; }
declare const receiver: Receiver;
declare const maybe: Receiver | undefined;
const member = receiver.method();
const indexed = receiver["method"]();
const wrapped = (receiver.method)();
const detachedMethod = receiver.method;
const detached = detachedMethod();
const optional = maybe?.method();
declare function work(value: number): string;
const called = work.call(undefined, 1);
export function result() { return { member, indexed, wrapped, detached, optional, called }; }
"#;
    let case = TestCase::parse("probe/this-argument", "this-argument.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "member : Receiver",
        "indexed : Receiver",
        "wrapped : Receiver",
        "detached : void",
        "optional : Receiver | undefined",
        "called : string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
