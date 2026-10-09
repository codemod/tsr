//! `keyof` reads late-bound (computed symbol) properties: getLiteralTypeFromProperties
//! walks getPropertiesOfType, which includes the members lateBindMember adds
//! (`checker.go:26717`; `docs/parity/notes/r5-mapped6.md` §4).

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first statement's annotation.
fn type_of_annotation(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

#[test]
fn keyof_an_interface_reads_its_computed_symbol_key() {
    // keyofAndIndexedAccessErrors: `keyof String` includes the
    // `[Symbol.iterator]` member's `unique symbol`.
    let source = "var x: K;
        declare const sym: unique symbol;
        interface J { [sym]: 1 }
        type K = keyof J & symbol;";
    assert_eq!(type_of_annotation(source), "unique symbol");
}
