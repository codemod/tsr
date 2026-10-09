//! A property access through an unresolved type reference is upstream's
//! `errorType` (ADR-0048; `docs/parity/notes/r5-errorsplit5.md` §7).
//!
//! Upstream's unresolved reference is any-flagged with an alias, so
//! `isAnyLike`, then `isErrorType(apparentType)`, answer `errorType`
//! (`checker.go:11314-11320`). `conformance/parserRealSource11` holds 3,000 of
//! the corpus's lines of this shape.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn last_initializer_is_native_error(source: &str) -> bool {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.ts"));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let initializer = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initializer");
    let id = checker.check_expression(initializer);
    id == checker.intrinsics().native_error
}

#[test]
fn a_property_access_through_an_unresolved_reference_is_native_error() {
    assert!(last_initializer_is_native_error("declare var x: Missing;\nvar y = x.p;"));
}

/// The control: a resolved receiver's missing member is not this arm.
#[test]
fn a_property_access_through_a_declared_type_is_not_this_arm() {
    assert!(!last_initializer_is_native_error(
        "interface I { p: number }\ndeclare var x: I;\nvar y = x.p;"
    ));
}
