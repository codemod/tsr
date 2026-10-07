//! Pinned tsgo 5b1047d resolveObjectTypeMembers concrete-this controls.
use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn return_type(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "receiver.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements.last().unwrap()
    else {
        panic!("expected variable");
    };
    let expression = statement.declaration_list.unwrap().declarations[0].initializer.unwrap();
    let ty = checker.check_expression(expression);
    checker.type_to_string(ty)
}

#[test]
fn inherited_call_returns_concrete_derived_receiver() {
    assert_eq!(
        return_type(
            "interface Base<T> { (value: T): this; }\ninterface Derived extends Base<string> { extra: number; }\ndeclare const derived: Derived;\nconst answer = derived('value');"
        ),
        "Derived"
    );
}

#[test]
fn separate_receivers_do_not_share_polymorphic_this() {
    assert_eq!(
        return_type(
            "interface Base<T> { (value: T): this; }\ninterface Left extends Base<string> { left: number; }\ninterface Right extends Base<string> { right: number; }\ndeclare const left: Left;\ndeclare const right: Right;\nconst first = left('value');\nconst answer = right('value');"
        ),
        "Right"
    );
}

#[test]
fn generic_receiver_maps_outer_parameter_and_this() {
    assert_eq!(
        return_type(
            "interface Base<T> { (value: T): this; }\ninterface Derived<T> extends Base<T> { extra: T; }\ndeclare const derived: Derived<string>;\nconst answer = derived('value');"
        ),
        "Derived<string>"
    );
}
