//! `keyof` of an alias instance reduces the instance itself
//! (getIndexTypeEx's getReducedType, `checker.go:26685`), and in this port
//! the instance is the alias body: a distributed union with a never-reduced
//! constituent is reduced before shouldDeferIndexType asks
//! isGenericReducibleType (`docs/parity/notes/r5-mapped6.md` §3).

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
fn keyof_a_distributed_alias_instance_reads_its_reduced_constituents() {
    // mappedTypeNotMistakenlyHomomorphic: `Gen<ABC.A>` distributes into
    // `{ v: A } & { v: A; a: string } | { v: A } & { v: B; b: string }`, and
    // the second constituent reduces to never (its `v` is `A & B`).
    let source = "var x: keyof Gen<ABC.A>;
        enum ABC { A, B }
        type Gen<T extends ABC> = { v: T } & ({ v: ABC.A, a: string } | { v: ABC.B, b: string });";
    assert_eq!(type_of_annotation(source), "\"a\" | \"v\"");
}
