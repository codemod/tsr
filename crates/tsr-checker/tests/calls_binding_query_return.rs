//! Native function/constructor annotation identities survive unresolved query returns.
use tsr_ast::{NodeId, SyntaxKind};
use tsr_checker::Checker;

#[test]
fn binding_query_function_and_constructor_keep_callable_shape() {
    for source in [
        "type F6 = ({ a: string }) => typeof string;",
        "type G6 = new ({ a: string }) => typeof string;",
    ] {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "binding.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let id = (0..parsed.nodes.len())
            .map(|n| NodeId::new(u32::try_from(n).unwrap()))
            .find(|&id| {
                matches!(
                    parsed.nodes.kind(id),
                    SyntaxKind::FunctionType | SyntaxKind::ConstructorType
                )
            })
            .unwrap();
        let ty = checker.get_type_from_type_node(
            tsr_ast::TypeNode::try_from(parsed.node_map.get(id).unwrap()).unwrap(),
        );
        assert_eq!(
            checker.type_to_string(ty),
            if source.starts_with("type F6") { "F6" } else { "G6" }
        );
    }
}
