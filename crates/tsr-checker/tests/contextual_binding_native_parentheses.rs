//! Parentheses preserve native initializer implied-pattern context.

use tsr_ast::{Node, NodeId};
use tsr_checker::Checker;
use tsr_core::Arena;

fn literal_type(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let literal = (0..parsed.nodes.len())
        .map(|index| NodeId::new(u32::try_from(index).unwrap()))
        .find_map(|id| match parsed.node_map.get(id) {
            Some(Node::ObjectLiteralExpression(node)) => Some(node),
            _ => None,
        })
        .unwrap();
    let ty = checker.check_expression(tsr_ast::Expression::ObjectLiteralExpression(literal));
    checker.type_to_string(ty)
}

#[test]
fn parenthesized_initializer_keeps_optional_binding_property() {
    assert_eq!(literal_type("const { x = 1 } = (({ x: 2 }));"), "{ x?: number; }");
}

#[test]
fn annotation_precedes_implied_pattern_through_parentheses() {
    assert_eq!(literal_type("const { x = 1 }: { x: number } = (({ x: 2 }));"), "{ x: number; }",);
}
