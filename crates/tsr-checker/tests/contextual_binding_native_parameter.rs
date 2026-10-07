//! Native getContextualTypeForBindingElement preserves contextual parameter types.

use tsr_ast::{Node, NodeId};
use tsr_checker::Checker;
use tsr_core::Arena;

fn default_parameter_type(source: &str) -> String {
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
    checker.type_to_string(ty)
}

#[test]
fn callback_default_projects_the_contextual_holder_not_implicit_any() {
    assert_eq!(
        default_parameter_type(
            "const handler: (value: { cb?: (n: number) => number }) => void = \
             ({ cb = n => n }) => {};",
        ),
        "number",
    );
}

#[test]
fn invalid_rest_default_still_receives_native_element_context() {
    // TS1186 does not prevent contextual checking of the initializer; native
    // types n as number before reporting the rest/default assignment errors.
    assert_eq!(
        default_parameter_type(
            "declare const input: [(n: number) => number]; \
             let [...rest = n => n]: [(n: number) => number] = input;",
        ),
        "number",
    );
}
