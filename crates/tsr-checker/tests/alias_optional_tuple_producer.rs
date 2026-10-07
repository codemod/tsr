//! Native getTypeFromOptionalTypeNode applies optionality before tuple creation.
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn optional_tuple_slot_retains_undefined_in_semantic_type() {
    let source = "declare const value: [string, number?];";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let ty = checker.get_type_of_symbol(bound.lookup_local(root, "value").unwrap());
    assert_eq!(checker.type_to_string(ty), "[string, (number | undefined)?]");
}
