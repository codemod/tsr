//! Optional parameter/write and initializer/read controls from pinned tsgo 5b1047d1.
use tsr_conformance::{TestCase, types_baseline::FileTypes};

const OPTIONAL_SOURCE: &str = r"// @allowJs: true
// @checkJs: true
// @strict: true
// @noEmit: true
// @filename: parameters.js
/**
 * @param {number} [p]
 * @param {number=} q
 * @param {number} [r=101]
 */
function optional(p, q, r) {
    p;
    q;
    r;
    p = undefined;
    q = undefined;
    r = undefined;
    p;
    q;
    r;
}
optional();
optional(undefined, undefined, undefined);
optional(1, 2, 3);
";

const DEFAULT_SOURCE: &str = r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @noEmit: true
// @filename: defaults.js
/** @param {string=} d */
function defaulted(d = "value") {
    d;
    d = undefined;
    d;
    d = "next";
    d;
}
defaulted();
defaulted(undefined);
defaulted("ok");
/** @param {string=} u */
function undefinedDefault(u = undefined) { u; }
/** @param {string | null=} n */
function nullableDefault(n = null) { n; }
/** @param {string | undefined} e */
function explicitUndefined(e = "value") { e; }
/** @param {string=} a @param {string=} c */
function dependentDefault(a = "seed", c = a) { c; }
/** @param {string} [b=documented] */
function documentedDefault(b) { b; }
"#;

const NEGATIVE_SOURCE: &str = r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @noEmit: true
// @filename: negatives.js
/** @param {number} [p] @param {number=} q @param {number} [r=101] */
function invalid(p, q, r) {
    p = "bad";
    q = null;
    r = true;
}
/** @param {string=} d */
function defaulted(d = "value") {
    d = 42;
    d = undefined;
}
"#;

const RECURSIVE_SOURCE: &str = r"// @allowJs: true
// @checkJs: true
// @strict: true
// @noEmit: true
// @filename: recursive.js
/** @param {string=} cycle */
function recursive(cycle = cycle) { cycle; }
";

fn assertions(source: &str) -> Vec<tsr_conformance::types_producer::Assertion> {
    let case = TestCase::parse("probe/jsdoc-optional-parameters", "parameters.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|file| FileTypes { file: file.name.clone(), assertions: Vec::new() })
        .collect();
    tsr_conformance::types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .into_iter()
        .flatten()
        .collect()
}

fn identifier_types(
    rows: &[tsr_conformance::types_producer::Assertion],
    name: &str,
) -> Vec<String> {
    rows.iter().filter(|row| row.text == name).map(|row| row.type_string.clone()).collect()
}

fn write_assignability(source: &str) -> Vec<bool> {
    let case = TestCase::parse("probe/jsdoc-parameter-writes", "parameters.ts", source);
    let arena = tsr_core::Arena::new();
    let program = tsr_conformance::types_producer::program_for_case(&arena, &case);
    let mut checker = tsr_conformance::types_producer::configured_checker(&program);
    let file = program.source_file(&case.files[0].name).expect("fixture file");
    let mut stack = vec![tsr_ast::Node::SourceFile(file.source_file())];
    let mut writes = Vec::new();
    while let Some(node) = stack.pop() {
        tsr_ast::push_children(node, &mut stack);
        if let tsr_ast::Node::BinaryExpression(binary) = node
            && binary
                .operator_token
                .is_some_and(|token| token.kind == tsr_ast::SyntaxKind::EqualsToken)
        {
            let target = checker.check_expression(binary.left.expect("write target"));
            let value = checker.check_expression(binary.right.expect("write value"));
            writes.push((
                program.nodes().span(binary.node_id.expect("registered")).start,
                checker.is_type_assignable_to(value, target),
            ));
        }
    }
    writes.sort_unstable_by_key(|(position, _)| *position);
    writes.into_iter().map(|(_, accepts)| accepts).collect()
}

#[test]
fn optional_tags_preserve_declared_writes_and_flow_reads_without_initializers() {
    let rows = assertions(OPTIONAL_SOURCE);
    for name in ["p", "q", "r"] {
        assert_eq!(
            identifier_types(&rows, name),
            ["number | undefined", "number | undefined", "number | undefined", "undefined"],
            "{name}: declaration, entry read, write target, read after undefined"
        );
    }
    assert!(rows.iter().any(|row| {
        row.text == "optional"
            && row.type_string == "(p?: number, q?: number | undefined, r?: number) => void"
    }));
}

#[test]
fn actual_defaults_narrow_only_entry_reads_and_keep_undefined_write_types() {
    let rows = assertions(DEFAULT_SOURCE);
    assert_eq!(
        identifier_types(&rows, "d"),
        [
            "string | undefined",
            "string",
            "string | undefined",
            "undefined",
            "string | undefined",
            "string"
        ]
    );
    assert_eq!(identifier_types(&rows, "u"), ["string | undefined", "string | undefined"]);
    assert_eq!(identifier_types(&rows, "n"), ["string | null | undefined", "string | null"]);
    assert_eq!(identifier_types(&rows, "e"), ["string | undefined", "string"]);
    assert_eq!(identifier_types(&rows, "c"), ["string | undefined", "string"]);
    assert_eq!(identifier_types(&rows, "b"), ["string | undefined", "string | undefined"]);
    assert!(rows.iter().any(|row| {
        row.text == "defaulted" && row.type_string == "(d?: string | undefined) => void"
    }));
}

#[test]
fn parameter_write_relations_match_native_without_the_diagnostic_consumer() {
    assert_eq!(write_assignability(OPTIONAL_SOURCE), [true, true, true]);
    assert_eq!(write_assignability(DEFAULT_SOURCE), [true, true]);
    assert_eq!(write_assignability(NEGATIVE_SOURCE), [false, false, false, false, true]);
}

#[test]
fn recursive_initializer_facts_retain_undefined_without_recursing_forever() {
    // Native also reports TS2372 for the self-reference. This pins the type
    // query, not that separate grammar emitter.
    assert_eq!(
        identifier_types(&assertions(RECURSIVE_SOURCE), "cycle"),
        ["string | undefined", "string | undefined", "string | undefined"]
    );
}

#[test]
fn conditional_cast_defaulted_parameter_keeps_its_native_read_type() {
    let source = include_str!(
        "../../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler/returnConditionalExpressionJSDocCast.ts"
    );
    assert_eq!(
        identifier_types(&assertions(source), "type"),
        ["string | undefined", "string", "string"]
    );
}

#[test]
fn diagnostic_traversal_accepts_native_optional_and_defaulted_undefined_writes() {
    for source in [OPTIONAL_SOURCE, DEFAULT_SOURCE] {
        let case =
            TestCase::parse("probe/jsdoc-diagnostic-positive-writes", "parameters.ts", source);
        assert!(tsr_conformance::diagnostics_suite::reported_for(&case).is_empty());
    }
}

#[test]
fn diagnostic_traversal_rejects_only_native_incompatible_parameter_writes() {
    let case =
        TestCase::parse("probe/jsdoc-diagnostic-negative-writes", "parameters.ts", NEGATIVE_SOURCE);
    let mut actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    actual.sort_unstable();
    // Native rejects string/null/boolean into number and number into string,
    // but accepts undefined at 10:5 even with the actual string initializer.
    assert_eq!(actual, [(3, 5, 2322), (4, 5, 2322), (5, 5, 2322), (9, 5, 2322)]);
}
