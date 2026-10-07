//! Namespace exports admit bare import-equals aliases by semantic target meaning.
use tsr_ast::{Node, NodeId};
use tsr_binder::{FileInfo, SymbolFlags};
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn exported_entity_alias_preserves_export_identity_at_type_query_reference() {
    let arena = Arena::new();
    let source = "namespace A { export class C {} } namespace B { export import alias = A; export let value: typeof alias; }";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "main.ts", text: source },
    );
    let root = parsed.source_file.node_id.unwrap();
    let b = bound.lookup_local(root, "B").unwrap();
    let alias = bound.symbols().get(b).exports["alias"];
    let reference = (0..parsed.nodes.len())
        .map(|raw| NodeId::new(u32::try_from(raw).unwrap()))
        .find(|&id| {
            matches!(parsed.node_map.get(id), Some(Node::Identifier(n)) if n.text == "alias")
                && parsed
                    .nodes
                    .parent(id)
                    .is_some_and(|p| parsed.nodes.kind(p) == tsr_ast::SyntaxKind::TypeQuery)
        })
        .unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let ty = checker.get_type_of_symbol(alias);
    assert_eq!(checker.type_to_string(ty), "typeof A");
    let resolved = bound.resolve_name_with_export_alias(
        &parsed.nodes,
        &parsed.node_map,
        reference,
        "alias",
        SymbolFlags::VALUE,
        |candidate, mask| {
            let target = checker.resolve_alias(candidate)?;
            Some(bound.symbols().get(target).flags.intersects(mask))
        },
    );
    assert_eq!(resolved, Some(alias));
}
