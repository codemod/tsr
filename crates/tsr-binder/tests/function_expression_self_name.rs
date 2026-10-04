//! Native nameresolver.go:233-244, restored with stable callable publication.
use tsr_ast::{Node, NodeId};
use tsr_binder::{BindResult, SymbolFlags};
use tsr_core::Arena;

fn with_bound(
    source: &str,
    test: impl FnOnce(
        &BindResult<'_>,
        &tsr_ast::NodeTable,
        &tsr_ast::NodeMap<'_>,
        NodeId,
        NodeId,
        NodeId,
    ),
) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let mut pending = vec![Node::SourceFile(parsed.source_file)];
    let mut function = None;
    let mut reference = None;
    while let Some(node) = pending.pop() {
        tsr_ast::push_children(node, &mut pending);
        match node {
            Node::FunctionExpression(node) if node.name.is_some_and(|n| n.text == "named") => {
                function = node.node_id;
            }
            Node::ReturnStatement(node) => reference = node.expression.and_then(|n| n.node_id()),
            _ => {}
        }
    }
    test(&bound, &parsed.nodes, &parsed.node_map, root, function.unwrap(), reference.unwrap());
}

#[test]
fn self_name_has_value_meaning_only_and_does_not_leak_to_parent_scope() {
    with_bound(
        "const f = function named() { return named; };",
        |bound, nodes, map, root, f, reference| {
            let own = bound.symbol_of(f).unwrap();
            assert_eq!(bound.symbols().get(own).value_declaration, Some(f));
            assert_eq!(
                bound.resolve_name(nodes, map, reference, "named", SymbolFlags::VALUE),
                Some(own)
            );
            assert_eq!(bound.resolve_name(nodes, map, reference, "named", SymbolFlags::TYPE), None);
            assert_eq!(bound.resolve_name(nodes, map, root, "named", SymbolFlags::VALUE), None);
        },
    );
}

#[test]
fn locals_precede_self_name_and_excluding_the_expression_reaches_outer_scope() {
    with_bound(
        "const named = 91; const f = function named() { return named; };",
        |bound, nodes, map, root, f, reference| {
            let own = bound.symbol_of(f).unwrap();
            let outer = bound.lookup_local(root, "named").unwrap();
            assert_ne!(own, outer);
            assert_eq!(
                bound.resolve_name(nodes, map, reference, "named", SymbolFlags::VALUE),
                Some(own)
            );
            assert_eq!(
                bound.resolve_name_excluding(
                    nodes,
                    map,
                    reference,
                    "named",
                    SymbolFlags::VALUE,
                    Some(f)
                ),
                Some(outer)
            );
        },
    );
    with_bound(
        "const f = function named(named: number) { return named; };",
        |bound, nodes, map, _, f, reference| {
            let own = bound.symbol_of(f).unwrap();
            let parameter = bound.lookup_local(f, "named").unwrap();
            assert_ne!(own, parameter);
            assert_eq!(
                bound.resolve_name(nodes, map, reference, "named", SymbolFlags::VALUE),
                Some(parameter)
            );
        },
    );
}
