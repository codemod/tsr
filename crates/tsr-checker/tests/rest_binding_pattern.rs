//! An unannotated REST parameter with a binding-pattern name takes the
//! pattern's implied type: the binding-pattern arm of
//! `getTypeForVariableLikeDeclaration` (`checker.go:16790`) answers before
//! the rest parameter's `anyArrayType` fallback (`restParameterWithBindingPattern1`).
//! `docs/parity/notes/r6-typesroots.md` §6.

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
fn an_object_rest_pattern_is_its_implied_type() {
    assert_eq!(
        type_of_declaration("function a(...{a, b}) { }", "a"),
        "(...{ a, b }: { a: any; b: any; }) => void"
    );
}

/// An array pattern's implied tuple spreads into the signature's parameters,
/// as native prints it (probed with the pinned `tsgo`).
#[test]
fn an_array_rest_pattern_spreads_its_implied_tuple() {
    assert_eq!(type_of_declaration("function f(...[a, b]) { }", "f"), "(a: any, b: any) => void");
}
