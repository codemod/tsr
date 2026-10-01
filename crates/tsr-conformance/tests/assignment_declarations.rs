//! Assignment declaration controls grounded in pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "assignment.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

#[test]
fn descriptor_values_and_commonjs_assignment_unions() {
    let source = r#"// @strict: true
// @noImplicitAny: false
// @target: es2020
// @module: commonjs
// @allowJs: true
// @checkJs: true
// @filename: mod.js
Object.defineProperty(exports,"value",{value:42,writable:true});
Object.defineProperty(exports,"readonlyValue",{value:"fixed"});
Object.defineProperty(exports,"getter",{get() { return "read"; }});
Object.defineProperty(exports,"setter",{
    /** @param {boolean} value */
    set(value) {}
});
Object.defineProperty(exports,"both",{
    get() {return "get";},
    /** @param {boolean} value */
    set(value) {}
});
Object.defineProperty(exports,"empty",{});
exports.literals=1;
exports.literals=2;
exports.initial=undefined;
exports.initial="ready";
exports.nullOnly=null;
// @filename: validator.ts
import m = require("./mod");
export const value=m.value;
export const readonlyValue=m.readonlyValue;
export const getter=m.getter;
export const setter=m.setter;
export const both=m.both;
export const empty=m.empty;
export const literals=m.literals;
export const initial=m.initial;
export const nullOnly=m.nullOnly;
m.value=10;
m.readonlyValue="other";
m.getter="other";
m.setter=true;
"#;
    let lines = lines("probe/assignment-descriptors", source);
    for wanted in [
        "value : number",
        "readonlyValue : string",
        "getter : string",
        "setter : boolean",
        "both : string",
        "empty : any",
        "literals : 1 | 2",
        "initial : \"ready\"",
        "nullOnly : any",
        "m.value : number",
        "m.readonlyValue : any",
        "m.getter : any",
        "m.setter : boolean",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn synthetic_defaults_and_expando_container_boundaries() {
    let source = r#"// @module: commonjs
// @target: es2015
// @strict: false
// @allowJs: true
// @filename: default.js
exports.default = {bar() {return 0;}};
// @filename: factory.js
module.exports = function () {
    class A {}
    return {c:A.b=1};
};
// @filename: validator.ts
import foo from "./default";
foo.bar();
"#;
    let lines = lines("probe/assignment-boundaries", source);
    // The synthetic default denotes the namespace, so bar is absent.
    // General missing-property errorType-to-any recovery remains a gap.
    for wanted in ["foo.bar() : error", "foo.bar : error", "A.b : error", "b : any"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
