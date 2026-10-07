//! Getter return expressions consume raw annotations without accessor resolution.

use tsr_ast::{Node, NodeId};
use tsr_checker::Checker;
use tsr_core::Arena;

fn returned_object_type(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
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
fn unannotated_getter_does_not_invent_literal_context() {
    assert_eq!(
        returned_object_type("const obj = { get value() { return { tag: 'a' }; } };"),
        "{ tag: string; }",
    );
}
