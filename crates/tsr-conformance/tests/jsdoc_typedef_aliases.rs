//! JSDoc typedef alias controls from pinned tsgo 5b1047d1.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "typedefs.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
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
    box.value = 42;
    box.value = undefined;
    box.equals = undefined;
    box.explicit = undefined;
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
                    let parameter = checker.signatures_of_type(callee).expect("signature")[0]
                        .parameters[0]
                        .clone();
                    let target = checker.parameter_type(&parameter);
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
        // The numeric write precedes the rejected undefined one: this port's
        // assignment-target read narrows by the earlier write (a flow-lane
        // divergence the reparsed type literal now shares with written ones).
        assert_eq!(
            writes,
            [
                (implicit.to_string(), true),
                (implicit.to_string(), false),
                (implicit.to_string(), !exact),
                ("string | undefined".to_string(), true),
                ("string | undefined".to_string(), true),
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

const COMPLETE_SOURCE: &str = r"// @allowJs: true
// @checkJs: true
// @strict: true
// @noEmit: true
// @filename: complete.js
/** @template T @typedef {Object} Box @property {T} value @property {T[]} items @property {{ deep: T }} nested @property {T} [optional] */
/** @param {Box<string>} box */
function read(box) {
    box.value;
    box.items;
    box.nested.deep;
    box.optional;
    box.toString();
    box.absent;
}
/** @typedef {object} Flat @property {number} count */
/** @param {Flat} flat */
function plain(flat) {
    flat.count;
    flat.absent;
}
function scope() {
    /** @typedef {Object} Local @property {number} present */
    ;
    /** @param {Local} local */
    function readLocal(local) {
        local.present;
        local.absent;
    }
}
";

fn configured_typedef_diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let case = TestCase::parse("probe/jsdoc-member-completeness", "complete.ts", source);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let file = program.source_file("complete.js").expect("fixture file");
    let id = file.source_file().node_id.expect("registered file");
    checker.set_checked_files([id]);
    checker.check_source_file(
        id,
        tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
    );
    let mut diagnostics: Vec<_> = checker
        .diagnostics()
        .iter()
        .map(|(_, diagnostic)| {
            let (line, column) = tsr_conformance::symbols_baseline::line_and_character(
                file.text(),
                diagnostic.span.start,
            );
            (line + 1, column + 1, diagnostic.message.code())
        })
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn complete_sibling_typedefs_report_only_native_absent_members() {
    // The three TS2339 positions are pinned-native outputs, not computed from
    // the implementation. Parameters have no initializer to infer members from.
    assert_eq!(
        configured_typedef_diagnostics(COMPLETE_SOURCE),
        [(9, 9, 2339), (15, 10, 2339), (23, 15, 2339)]
    );
}

#[test]
fn present_sibling_members_and_object_builtins_remain_valid() {
    let source = COMPLETE_SOURCE
        .replace("box.absent;", "box.value;")
        .replace("flat.absent;", "flat.count;")
        .replace("local.absent;", "local.present;");
    assert!(configured_typedef_diagnostics(&source).is_empty());
}

/// Native rejects the reverse covariant assignment, the forward contravariant
/// assignment (a.js:36), and both invariant assignments. In particular, the
/// positive contravariant assignment at a.js:37 must not default to covariance.
#[test]
fn declared_jsdoc_variance_orders_both_assignment_directions() {
    let source = include_str!(
        "../../../vendor/typescript-go/_submodules/TypeScript/tests/cases/conformance/jsdoc/jsdocTemplateTag8.ts"
    );
    let case = TestCase::parse("probe/jsdoc-declared-variance", "variance.ts", source);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let file = program.source_file("a.js").expect("fixture file");
    let mut stack = vec![tsr_ast::Node::SourceFile(file.source_file())];
    let mut assignments = Vec::new();
    while let Some(node) = stack.pop() {
        tsr_ast::push_children(node, &mut stack);
        if let tsr_ast::Node::BinaryExpression(binary) = node
            && binary
                .operator_token
                .is_some_and(|token| token.kind == tsr_ast::SyntaxKind::EqualsToken)
        {
            let target = checker.check_expression(binary.left.expect("assignment target"));
            let source = checker.check_expression(binary.right.expect("assignment value"));
            assignments.push((
                program.nodes().span(binary.node_id.expect("registered")).start,
                checker.is_type_assignable_to(source, target),
            ));
        }
    }
    assignments.sort_unstable_by_key(|(position, _)| *position);
    let outcomes: Vec<_> = assignments.iter().map(|(_, related)| *related).collect();
    assert_eq!(outcomes, [true, false, false, true, false, false]);
}

#[test]
fn unannotated_jsdoc_objects_measure_variance_instead_of_defaulting_covariant() {
    let source = r"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
/** @template T @typedef {{ value: T }} InlineOutput */ ;
/** @template T @typedef {{ accept: (value: T) => void }} InlineInput */ ;
/** @template T @typedef {Object} SiblingOutput @property {T} value */ ;
/** @template T @typedef {Object} SiblingInput @property {(value: T) => void} accept */ ;
/** @template T @typedef {{ tag: string }} Independent */ ;
";
    let case = TestCase::parse("probe/jsdoc-measured-variance", "variance.ts", source);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let root = program.source_file("a.js").unwrap().source_file().node_id.unwrap();
    let text = checker.intrinsics().string;
    let wide = checker.intrinsics().unknown;
    let unrelated = checker.intrinsics().number;
    for (name, other, expected) in [
        ("InlineOutput", wide, [true, false]),
        ("InlineInput", wide, [false, true]),
        ("SiblingOutput", wide, [true, false]),
        ("SiblingInput", wide, [false, true]),
        ("Independent", unrelated, [true, true]),
    ] {
        let symbol = program.binder().lookup_local(root, name).expect("typedef symbol");
        let narrow = checker.create_type_reference_public(symbol, vec![text]);
        let other = checker.create_type_reference_public(symbol, vec![other]);
        assert_eq!(
            [
                checker.is_type_assignable_to(narrow, other),
                checker.is_type_assignable_to(other, narrow),
            ],
            expected,
            "{name}"
        );
    }
}

#[test]
fn diagnostic_traversal_reports_native_sibling_member_positions() {
    let case =
        TestCase::parse("probe/jsdoc-diagnostic-completeness", "complete.ts", COMPLETE_SOURCE);
    let mut actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    actual.sort_unstable();
    // Pinned native: generic, plain, and function-local siblings. No parameter
    // initializer can supply the members or fake the JSDoc annotation.
    assert_eq!(actual, [(9, 9, 2339), (15, 10, 2339), (23, 15, 2339)]);
}

#[test]
fn diagnostic_traversal_preserves_native_positive_sibling_accesses() {
    let source = COMPLETE_SOURCE
        .replace("box.absent;", "box.value;")
        .replace("flat.absent;", "flat.count;")
        .replace("local.absent;", "local.present;");
    let case = TestCase::parse("probe/jsdoc-diagnostic-present", "complete.ts", &source);
    assert!(tsr_conformance::diagnostics_suite::reported_for(&case).is_empty());
}

#[test]
fn diagnostic_traversal_respects_native_declared_variance() {
    let source = include_str!(
        "../../../vendor/typescript-go/_submodules/TypeScript/tests/cases/conformance/jsdoc/jsdocTemplateTag8.ts"
    );
    let case = TestCase::parse("probe/jsdoc-diagnostic-variance", "variance.ts", source);
    let mut actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    actual.sort_unstable();
    // Native rejects 36:1 and accepts 37:1. The separate TS1274 grammar
    // diagnostic at 59:14 remains unsupported; no assignment is suppressed.
    assert_eq!(actual, [(18, 1, 2322), (36, 1, 2322), (55, 1, 2322), (56, 1, 2322)]);
}
