//! Type-node return preparation preserves a callable before slot demand.
use tsr_ast::{NodeId, SyntaxKind};
use tsr_checker::Checker;

#[test]
fn supported_query_returns_complete_without_collapsing_the_type() {
    for source in [
        "type F6 = ({ a: string }) => typeof string;",
        "type G6 = new ({ a: string }) => typeof string;",
    ] {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "lazy.ts", text: source },
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
        checker.prepare_signature_type_node_return(id);
        let node = tsr_ast::TypeNode::try_from(parsed.node_map.get(id).unwrap()).unwrap();
        let ty = checker.get_type_from_type_node(node);
        let signature = checker.resolve_call_signature_with_type_arguments(ty, None, false).unwrap();
        let image = checker.instantiate_signature_with_type_arguments(&signature, &[]).unwrap().unwrap();
        assert_eq!(checker.type_to_string(image.r#type), "any");
        assert_eq!(
            checker.type_to_string(ty),
            if source.starts_with("type F6") { "F6" } else { "G6" }
        );
    }
}
