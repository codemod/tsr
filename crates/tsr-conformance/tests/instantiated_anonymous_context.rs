//! Pinned native static-field parameter-default contexts, independent of alias spelling.
use tsr_ast::{Expression, Node, NodeId};
use tsr_conformance::{TestCase, diagnostics_suite, types_producer};
use tsr_core::{Arena, Idx};

fn case(source: &str, strict: bool) -> TestCase {
    TestCase::parse(
        "probe/instantiated-anonymous-context",
        "fixture.ts",
        &format!("// @strict: {strict}\n// @target: es2015\n{source}"),
    )
}

fn field_member(source: &str, strict: bool, precheck: bool) -> String {
    let test = case(source, strict);
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &test);
    let file = program.source_file("fixture.ts").expect("fixture");
    let file_id = file.source_file().node_id.expect("source id");
    let mut checker = types_producer::configured_checker(&program);
    checker.set_checked_files(vec![file_id]);
    if precheck {
        checker.check_source_file(
            file_id,
            tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
        );
    }
    let class = (0..program.nodes().len())
        .find_map(|index| match program.node_map().get(NodeId::from_usize(index)) {
            Some(Node::ClassExpression(class)) => Some(class),
            _ => None,
        })
        .expect("class default");
    let mut reads = Vec::new();
    for _ in 0..2 {
        let class_type = checker.check_expression(Expression::ClassExpression(class));
        let field = checker.get_type_of_property_of_type(class_type, "x").expect("field");
        let member = checker.get_type_of_property_of_type(field, "a").expect("member");
        reads.push(checker.type_to_string(member));
    }
    assert_eq!(reads[0], reads[1], "cold/warm drift: {source}");
    reads.remove(0)
}

#[test]
fn generic_class_defaults_retain_each_instantiated_literal() {
    for strict in [false, true] {
        for (annotation, initializer, wanted) in [
            ("I", r#"class { static x = { a: "right" }; }"#, "\"right\""),
            ("I<\"right\">", r#"class { static x = { a: "right" }; }"#, "\"right\""),
            ("I<\"right\">", r#"((class { static x = { a: "right" }; }))"#, "\"right\""),
            ("I<\"left\">", r#"class { static x = { a: "left" }; }"#, "\"left\""),
            // Context retains the wrong candidate, never substitutes the expected payload.
            ("I<\"right\">", r#"class { static x = { a: "wrong" }; }"#, "\"wrong\""),
        ] {
            let source = format!(
                "interface I<T = \"right\"> {{ x: {{ a: T }}; }}\nfunction f(c: {annotation} = {initializer}) {{}}"
            );
            for checked in [false, true] {
                assert_eq!(field_member(&source, strict, checked), wanted, "{source}");
            }
            let actual = diagnostics_suite::reported_for(&case(&source, strict));
            if wanted == "\"wrong\"" {
                assert_eq!(actual.len(), 1, "{actual:?}");
                assert_eq!((actual[0].line, actual[0].column, actual[0].code), (2, 12, 2322));
            } else {
                assert!(actual.is_empty(), "{source}: {actual:?}");
            }
        }
    }
}

#[test]
fn annotations_optionality_and_nonliteral_contexts_keep_their_boundaries() {
    for strict in [false, true] {
        for (source, wanted) in [
            (
                r#"interface I { x: { a: "right" }; } function f(c: I = class { static x = { a: "right" }; }) {}"#,
                "\"right\"",
            ),
            (
                r#"interface I<T> { x: { a: T }; } function f(c: I<"right"> = class { static x: { a: "left" } = { a: "left" }; }) {}"#,
                "\"left\"",
            ),
            (r#"function f(c = class { static x = { a: "right" }; }) {}"#, "string"),
            (
                r#"interface I<T> { x: { a?: T }; } function f(c: I<"right"> = class { static x = { a: "right" }; }) {}"#,
                "\"right\"",
            ),
            (
                r#"interface I<T> { x: { present?: T }; } function f(c: I<"right"> = class { static x = { a: "right" }; }) {}"#,
                "string",
            ),
            (
                r#"interface I<T> { x: { a: T }; } function f(c: I<unknown> = class { static x = { a: "right" }; }) {}"#,
                "string",
            ),
        ] {
            for checked in [false, true] {
                assert_eq!(field_member(source, strict, checked), wanted, "{source}");
            }
        }
    }
}
