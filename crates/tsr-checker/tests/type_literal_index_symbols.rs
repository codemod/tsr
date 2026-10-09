//! A computed member of a type literal that does not late-bind: it joins the
//! index symbol when its name is an entity-name expression assignable to
//! `string | number | symbol` (`hasLateBindableIndexSignature`,
//! `checker.go:19971`), and is dropped otherwise (the binder gives it an
//! anonymous symbol, `binder.go:978`). `docs/parity/notes/r6-typesroots.md` §3.
//! Expectations are quoted from the cited `.types` baselines.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the file-scope variable `name`, printed at its declaration.
/// Parse errors are allowed: unresolved names are the point.
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

/// `parserComputedPropertyName14`: an `any` name keys a `number` index.
#[test]
fn a_method_with_an_unresolved_name_is_a_number_index() {
    assert_eq!(
        type_of_declaration("declare var e: any;\nvar v: { [e](): number };", "v"),
        "{ [x: number]: () => number; }"
    );
}

/// `propertyAssignment`: an unannotated property is the implicit `any`.
#[test]
fn an_unannotated_property_is_an_any_index() {
    assert_eq!(
        type_of_declaration("declare var index: any;\nvar v: { [index]; };", "v"),
        "{ [x: number]: any; }"
    );
}

/// A `string` name keys a `string` index.
#[test]
fn a_string_name_is_a_string_index() {
    assert_eq!(
        type_of_declaration("declare var s: string;\nvar v: { [s](): void };", "v"),
        "{ [x: string]: () => void; }"
    );
}

/// `computedPropertyNamesDeclarationEmit4_ES6`: not an entity-name
/// expression, so neither a member nor an index.
#[test]
fn a_name_that_is_not_an_entity_name_is_dropped() {
    assert_eq!(type_of_declaration("var v: { [\"\" + \"\"](): void; };", "v"), "{}");
}
