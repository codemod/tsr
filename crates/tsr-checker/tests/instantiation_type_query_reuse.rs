//! The node builder reuses an instantiation expression's own `typeof` node
//! for the type computed from it (`createAnonymousTypeNodeEx`,
//! `nodebuilderimpl.go:2816`): `var xs3: typeof Array<number>` records
//! `>xs3 : typeof Array<number>` (`arrayTypeOfTypeOf`).
//! `docs/parity/notes/r6-typesroots.md` §4.

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
fn a_type_query_with_arguments_prints_as_written() {
    let source = "declare function f<T>(x: T): T;\nvar v: typeof f<number>;";
    assert_eq!(type_of_declaration(source, "v"), "typeof f<number>");
}

#[test]
fn a_nested_type_query_argument_prints_as_written() {
    let source = "declare function f<T>(x: T): T;\nvar x = 1;\nvar v: typeof f<typeof x>;";
    assert_eq!(type_of_declaration(source, "v"), "typeof f<typeof x>");
}

/// The control: the expression form has no `typeof` node to reuse.
#[test]
fn an_instantiation_expression_prints_its_signatures() {
    let source = "declare function f<T>(x: T): T;\nvar v = f<number>;";
    assert_eq!(type_of_declaration(source, "v"), "(x: number) => number");
}
