//! Getter return expressions consume raw annotations without accessor resolution.

use tsr_ast::{Node, NodeId};
use tsr_checker::Checker;
use tsr_core::Arena;

fn returned_object_type(source: &str) -> String {
    let arena = Arena::new();
    let mut parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("control.js"),
    );
    let root = parsed.source_file.node_id.unwrap();
    parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        tsr_binder::BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.js", text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    let expression = (0..parsed.nodes.len())
        .map(|index| NodeId::new(u32::try_from(index).unwrap()))
        .find_map(|id| match parsed.node_map.get(id) {
            Some(Node::ReturnStatement(node)) => node.expression,
            _ => None,
        })
        .unwrap();
    let ty = checker.check_expression(expression);
    checker.type_to_string(ty)
}

#[test]
fn getter_annotation_precedes_divergent_setter_annotation() {
    assert_eq!(
        returned_object_type(
            "const obj = { get value(): { tag: 'a' } { return { tag: 'a' }; }, \
             set value(v: { tag: 'b' }) {} };",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn paired_setter_supplies_unannotated_getter_context() {
    assert_eq!(
        returned_object_type(
            "class Holder { get value() { return { tag: 'a' }; } \
             set value(v: { tag: 'a' | 'b' }) {} }",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn jsdoc_setter_annotation_contextualizes_getter_return() {
    assert_eq!(
        returned_object_type(
            "class C { /** @param {{ tag: 'a' | 'b' }} value */ \
             set value(value) {} get value() { return { tag: 'a' }; } }",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn getter_jsdoc_return_annotation_precedes_paired_setter_annotation() {
    assert_eq!(
        returned_object_type(
            "class C { /** @returns {{ tag: 'a' | 'b' }} */ \
             get value() { return { tag: 'a' }; } \
             /** @param {{ tag: string }} value */ set value(value) {} }",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn jsdoc_return_annotation_contextualizes_function_return() {
    assert_eq!(
        returned_object_type(
            "/** @return {{ tag: 'a' | 'b' }} */ function f() { return { tag: 'a' }; }",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn jsdoc_parameter_annotation_contextualizes_default_callback() {
    assert_eq!(
        returned_object_type(
            "/** @param {() => { tag: 'a' | 'b' }} value */ \
             function f(value = function() { return { tag: 'a' }; }) {}",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn invalid_setter_this_parameter_does_not_replace_value_annotation_context() {
    assert_eq!(
        returned_object_type(
            "class C { get value() { return { tag: 'a' }; } \
             set value(this: C, value: { tag: 'a' | 'b' }) {} }",
        ),
        "{ tag: \"a\"; }",
    );
}

#[test]
fn unannotated_getter_does_not_invent_literal_context() {
    assert_eq!(
        returned_object_type("const obj = { get value() { return { tag: 'a' }; } };"),
        "{ tag: string; }",
    );
}
