//! Native resolver requireSymbol is a Checker-private fallback after lexical resolution.
use tsr_ast::{Node, NodeId};
use tsr_binder::FileInfo;
use tsr_checker::Checker;
use tsr_core::Arena;
#[test]
fn require_fallback_preserves_call_admission_identity_and_shadowing() {
    let arena = Arena::new();
    let source = "require(1); require(); require(1,2); require; missing(1); function local(require) { require(1); }";
    let mut parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("main.js"),
    );
    parsed
        .nodes
        .add_flags(parsed.source_file.node_id.unwrap(), tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "main.js", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut ids: Vec<_> = (0..parsed.nodes.len())
        .map(|raw| NodeId::new(u32::try_from(raw).unwrap()))
        .filter(
            |&id| matches!(parsed.node_map.get(id),Some(Node::Identifier(n)) if n.text=="require"),
        )
        .collect();
    ids.sort_by_key(|&id| parsed.nodes.span(id).start);
    let first = checker.resolve_identifier_symbol(ids[0]).unwrap();
    let ty = checker.intrinsic_type_of_resolved_identifier_symbol(&first).unwrap().unwrap();
    assert_eq!(checker.type_to_string(ty), "any");
    assert_eq!(checker.resolve_identifier_symbol(ids[0]).unwrap(), first);
    for &id in &ids[1..4] {
        let s = checker.resolve_identifier_symbol(id).unwrap();
        let ty = checker.intrinsic_type_of_resolved_identifier_symbol(&s).unwrap().unwrap();
        assert_eq!(checker.type_to_string(ty), "error");
    }
    let last = checker.resolve_identifier_symbol(*ids.last().unwrap()).unwrap();
    assert_ne!(last, first);
    assert_eq!(checker.intrinsic_type_of_resolved_identifier_symbol(&last).unwrap(), None);
    let other = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    assert!(other.intrinsic_type_of_resolved_identifier_symbol(&first).is_err());
}
