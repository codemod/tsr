//! Native getContextualTypeForBindingElement preserves contextual parameter types.

use tsr_ast::{Node, NodeId};
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn callback_default_projects_the_contextual_holder_not_implicit_any() {
    let source = "const handler: (value: { cb?: (n: number) => number }) => void = \
        ({ cb = n => n }) => {};";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let parameter = (0..parsed.nodes.len())
        .map(|index| NodeId::new(u32::try_from(index).unwrap()))
        .find(|&id| {
            matches!(parsed.node_map.get(id), Some(Node::ParameterDeclaration(node))
                if node.r#type.is_none()
                    && matches!(node.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "n"))
        })
        .unwrap();
    let symbol = bound.symbol_of(parameter).unwrap();
    let ty = checker.get_type_of_symbol(symbol);
    assert_eq!(checker.type_to_string(ty), "number");
}
