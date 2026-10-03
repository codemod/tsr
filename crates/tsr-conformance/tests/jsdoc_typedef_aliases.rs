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

/// Native `reparser::reparseUnhosted/reparseJSDocTypeLiteral` puts every
/// property annotation under the typedef's own template scope. Parameters
/// deliberately have no initializer from which these types could be inferred.
#[test]
fn sibling_properties_share_the_alias_mapper_for_every_annotation_shape() {
    let got = lines(
        "probe/jsdoc-sibling-lexical-mapper",
        r"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
/** @template T @typedef {Object} Leaf @property {T} item */
/**
 * @template T
 * @typedef {Object} Box
 * @property {T} value
 * @property {T[]} items
 * @property {{ deep: T, leaves: Leaf<T>[] }} nested
 * @property {Leaf<T>} leaf
 * @property {T} [optional]
 * @property {T=} optionalEquals
 */
/** @template T @typedef {{ other: T }} Other */
/** @param {Box<string>} box @param {Other<number>} other @param {Box<number>} numbers */
function read(box, other, numbers) {
    box.value;
    box.items;
    box.items[0];
    box.nested.deep;
    box.nested.leaves[0].item;
    box.leaf.item;
    box.optional;
    box.optionalEquals;
    other.other;
    numbers.nested.deep;
    numbers.leaf.item;
    box.missing;
}
",
    );
    for expected in [
        "read : (box: Box<string>, other: Other<number>, numbers: Box<number>) => void",
        "box : Box<string>",
        "box.value : string",
        "box.items : string[]",
        "box.items[0] : string",
        "box.nested.deep : string",
        "box.nested.leaves[0].item : string",
        "box.leaf.item : string",
        "box.optional : string | undefined",
        "box.optionalEquals : string | undefined",
        "other.other : number",
        "numbers.nested.deep : number",
        "numbers.leaf.item : number",
        // Native rejects this with TS2339 and prints any; the port exposes
        // its error sentinel. This is a negative lookup control, not a gain.
        "box.missing : error",
    ] {
        assert!(got.iter().any(|line| line == expected), "missing {expected}: {got:#?}");
    }
}

const OPTIONAL_SOURCE: &str = r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: optional.js
/**
 * @template T
 * @typedef {Object} Maybe
 * @property {T} [value]
 * @property {T=} equals
 * @property {T | undefined} [explicit]
 */
/** @param {Maybe<string>} box */
function writes(box) {
    box.value;
    box.equals;
    box.explicit;
    box.value = "ok";
    box.value = undefined;
    box.equals = undefined;
    box.explicit = undefined;
    box.value = 42;
}
/** @param {Maybe<string>} box */
function acceptsMissing(box) {}
acceptsMissing({});
"#;

/// Pinned native permits {T=} and explicit undefined unions in exact mode;
/// only the bracketed implicit optional rejects undefined writes. Missing
/// properties and string writes work in both modes.
#[test]
fn sibling_optional_properties_distinguish_missing_from_explicit_undefined() {
    for exact in [false, true] {
        let source = format!("// @exactOptionalPropertyTypes: {exact}\n{OPTIONAL_SOURCE}");
        let got = lines("probe/jsdoc-sibling-optionality", &source);
        for expected in [
            "box.value : string | undefined",
            "box.equals : string | undefined",
            "box.explicit : string | undefined",
        ] {
            assert!(got.iter().any(|line| line == expected), "missing {expected}: {got:#?}");
        }
        let case = TestCase::parse("probe/jsdoc-sibling-optionality", "optional.ts", &source);
        let arena = tsr_core::Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let mut checker = types_producer::configured_checker(&program);
        let mut writes = Vec::new();
        let mut omissions = Vec::new();
        let file = program.source_file("optional.js").expect("fixture file");
        let mut stack = vec![tsr_ast::Node::SourceFile(file.source_file())];
        while let Some(node) = stack.pop() {
            tsr_ast::push_children(node, &mut stack);
            match node {
                tsr_ast::Node::BinaryExpression(binary)
                    if binary
                        .operator_token
                        .is_some_and(|token| token.kind == tsr_ast::SyntaxKind::EqualsToken) =>
                {
                    let target = checker.check_expression(binary.left.expect("write target"));
                    let source = checker.check_expression(binary.right.expect("write value"));
                    writes.push((
                        program.nodes().span(binary.node_id.expect("registered")).start,
                        checker.type_to_string(target),
                        checker.is_type_assignable_to(source, target),
                    ));
                }
                tsr_ast::Node::CallExpression(call) => {
                    let callee = checker.check_expression(call.expression.expect("callee"));
                    let target = checker.signatures_of_type(callee).expect("signature")[0]
                        .parameters[0]
                        .r#type;
                    let source = checker.check_expression(call.arguments[0]);
                    omissions.push(checker.is_type_assignable_to(source, target));
                }
                _ => {}
            }
        }
        writes.sort_unstable_by_key(|(position, _, _)| *position);
        let writes: Vec<_> =
            writes.into_iter().map(|(_, target, accepts)| (target, accepts)).collect();
        let implicit = if exact { "string" } else { "string | undefined" };
        // Native reports TS2412 on the implicit undefined write only in exact
        // mode, and TS2322 on the numeric write in both. Check the relation
        // itself: diagnostics_suite's checker does not install its JSDoc table.
        assert_eq!(
            writes,
            [
                (implicit.to_string(), true),
                (implicit.to_string(), !exact),
                ("string | undefined".to_string(), true),
                ("string | undefined".to_string(), true),
                (implicit.to_string(), false),
            ],
            "exact={exact}"
        );
        assert_eq!(omissions, [true], "optional properties may all be omitted");
    }
}

#[test]
fn non_generic_sibling_aliases_keep_lexical_nameability() {
    let got = lines(
        "probe/jsdoc-sibling-nameability",
        r"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
/** @typedef {Object} Top @property {string} value */
/** @param {Top} top */
function readTop(top) { top.value; }
function scope() {
    /** @typedef {Object} Local @property {number[]} values */
    ;
    /** @param {Local} local */
    function readLocal(local) { local.values[0]; }
}
",
    );
    for expected in [
        "top : Top",
        "top.value : string",
        "local : { values: number[]; }",
        "local.values[0] : number",
    ] {
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
