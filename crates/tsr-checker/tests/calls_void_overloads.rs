//! Native minimum arity includes trailing void acceptance during overload choice.
use tsr_ast::Statement;
use tsr_checker::Checker;

#[test]
fn trailing_void_does_not_eliminate_the_applicable_overload() {
    let source = "declare function f(value: string, unused: void): 'string'; declare function f(value: number): 'number'; const a = f('value'); const b = f(1);";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "void.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for (index, expected) in [(2, "\"string\""), (3, "\"number\"")] {
        let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
            panic!()
        };
        let ty = checker.check_expression(
            statement.declaration_list.unwrap().declarations[0].initializer.unwrap(),
        );
        assert_eq!(checker.type_to_string(ty), expected);
    }
}
