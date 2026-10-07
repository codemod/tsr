//! Native call selection strips nullable callee while retaining diagnostics.
use tsr_ast::Statement;
use tsr_checker::Checker;

#[test]
fn ordinary_nullable_calls_keep_nonnullable_return_types() {
    for (source, expected) in [
        (
            "declare const maybe: ((value: number) => string) | undefined; const result = maybe(1);",
            "string",
        ),
        (
            "interface Method { run?(): number } declare const method: Method; const result = method.run();",
            "number",
        ),
    ] {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "nullable.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        let Statement::VariableStatement(statement) = parsed.source_file.statements.last().unwrap()
        else {
            panic!()
        };
        let expression = statement.declaration_list.unwrap().declarations[0].initializer.unwrap();
        let ty = checker.check_expression(expression);
        assert_eq!(checker.type_to_string(ty), expected);
    }
}
