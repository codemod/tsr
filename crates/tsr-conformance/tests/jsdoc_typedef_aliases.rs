//! JSDoc typedef alias controls from pinned tsgo 5b1047d1.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "typedefs.ts", source);
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
fn top_level_aliases_are_nameable_and_nested_aliases_use_their_body() {
    let got = lines(
        "probe/jsdoc-typedef-nameability",
        r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
/** @type {ForwardTopLevel} */
const forward = { value: "top" };
/** @typedef {{ value: string }} ForwardTopLevel */
;
function use() {
    /** @typedef {{ value: string }} Nested */
    ;
    /** @type {Nested} */
    const nested = { value: "local" };
    nested.value;
}
"#,
    );
    for expected in
        ["forward : ForwardTopLevel", "nested : { value: string; }", "nested.value : string"]
    {
        assert!(got.iter().any(|line| line == expected), "missing {expected}: {got:#?}");
    }
}

#[test]
fn typedef_owned_templates_retain_the_actual_parameter_name() {
    let got = lines(
        "probe/jsdoc-generic-typedef",
        r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
/**
 * @template out T
 * @typedef {Object} JSDocBox
 */
/** @type {JSDocBox<string>} */
const box = { value: "x" };
"#,
    );
    assert!(got.iter().any(|line| line == "box : JSDocBox<string>"), "{got:#?}");
}

#[test]
fn complete_inline_generic_typedef_substitutes_member_types() {
    let got = lines(
        "probe/jsdoc-inline-generic-typedef",
        r"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
/**
 * @template T
 * @typedef {{ value: T, items: T[] }} Box
 */
/** @param {Box<string>} box */
function read(box) {
    box.value;
    box.items;
    box.items[0];
}
",
    );
    for expected in
        ["box : Box<string>", "box.value : string", "box.items : string[]", "box.items[0] : string"]
    {
        assert!(got.iter().any(|line| line == expected), "missing {expected}: {got:#?}");
    }
}
