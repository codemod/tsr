//! Authoritative construct-only type lists must never become call candidates.
use tsr_ast::Statement;
use tsr_checker::Checker;

#[test]
fn constructor_only_type_does_not_resolve_as_a_call() {
    let source = "declare const C: new (value: number) => { value: number }; const called = C(1);";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "kind.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let ty = checker.get_type_of_symbol(bound.lookup_local(root, "C").unwrap());
    let Statement::VariableStatement(statement) = parsed.source_file.statements[1] else {
        panic!()
    };
    let tsr_ast::Expression::CallExpression(call) =
        statement.declaration_list.unwrap().declarations[0].initializer.unwrap()
    else {
        panic!()
    };
    assert!(
        checker
            .resolve_call_signature_with_type_arguments(ty, Some(call.arguments), false)
            .is_none()
    );
}
