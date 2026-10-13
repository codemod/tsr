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

#[test]
fn nongeneric_recursive_receivers_do_not_capture_method_parameters() {
    // Reduced from Excalidraw's clipboard receiver. Native 5b1047d retains
    // List when freshening either method's T; the old port recursively cloned
    // the receiver and its generic signatures until it exhausted memory.
    let source = r#"// @strict: true
// @target: esnext
type Item<K extends string = string> = { type: K; value: string };
type List = Item[] & {
    findByType: typeof findByType;
    getData: typeof getData;
    getFiles: typeof getFiles;
};
const findByType = function<T extends string>(this: List, type: T): Item<T> | null {
    return this.find((item): item is Item<T> => item.type === type) || null;
};
const getData = function<T extends string>(this: List, type: T): string | null {
    const item = this.find((item): item is Item<string> => item.type === type);
    return item?.value ?? null;
};
const getFiles = function(this: List): Item[] {
    return this.filter(item => item.type === "file");
};
declare const list: List;
const text = list.getData("text");
const file = list.findByType("file");
"#;
    let case = TestCase::parse("probe/recursive-receiver", "recursive-receiver.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["text : string | null", "file : Item<\"file\"> | null"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
