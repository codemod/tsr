//! Native anonymous callable publication precedes signature resolution.
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn callable_alias_has_a_declared_object_before_signature_demand() {
    let source = "type F6 = ({ a: string }) => typeof string; type C = new () => Missing; type Good = (x: string) => number;";
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
    for name in ["F6", "C", "Good"] {
        let symbol = bound.lookup_local(root, name).unwrap();
        let ty = checker.get_declared_type_of_symbol(symbol);
        let tsr_checker::types::TypeData::Anonymous { symbol: owner, .. } =
            checker.type_of(ty).data
        else {
            panic!("callable alias must publish its anonymous semantic object");
        };
        let declaration = bound.symbols().get(symbol).declarations[0];
        let Some(tsr_ast::Node::TypeAliasDeclaration(alias)) = parsed.node_map.get(declaration)
        else {
            panic!("alias declaration");
        };
        let body = alias.r#type.unwrap().node_id().unwrap();
        assert_eq!(bound.symbol_of(body), Some(owner));
        assert_eq!(checker.type_to_string(ty), name);
        assert_eq!(checker.get_declared_type_of_symbol(symbol), ty);
    }
}
