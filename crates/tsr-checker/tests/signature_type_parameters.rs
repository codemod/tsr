//! Duplicate type parameters of one signature merge into one symbol, and
//! `getTypeParametersFromDeclaration` (`checker.go:19912`) appends each
//! symbol once (`typesWithDuplicateTypeParameters`).
//! `docs/parity/notes/r6-typesroots.md` §5.

use tsr_checker::Checker;
use tsr_core::Arena;

fn type_of_declaration(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    let site = bound.symbols().get(symbol).declarations[0];
    checker.type_to_string_at(id, site).expect("nameable type")
}

#[test]
fn a_repeated_type_parameter_is_listed_once() {
    assert_eq!(type_of_declaration("function f<T, T>() { }", "f"), "<T>() => void");
    assert_eq!(type_of_declaration("function f2<T, U, T>() { }", "f2"), "<T, U>() => void");
}

/// The control: distinct names all stay.
#[test]
fn distinct_type_parameters_all_stay() {
    assert_eq!(type_of_declaration("function g<T, U>() { }", "g"), "<T, U>() => void");
}
