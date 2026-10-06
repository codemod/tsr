//! `Checker::get_base_types` — `getBaseTypes` (`checker.go:19167`) for a class
//! or interface symbol, which the `.types` writer reads through
//! `getTypeOfNode`'s class-extends arm.

use tsr_checker::Checker;
use tsr_core::Arena;

/// Render the base types of the top-level declaration named `name`.
fn base_types_of(source: &str, name: &str) -> Vec<String> {
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
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    checker.get_base_types(symbol).into_iter().map(|t| checker.type_to_string(t)).collect()
}

#[test]
fn a_class_base_applies_its_written_type_arguments() {
    assert_eq!(
        base_types_of("class Base<T, U> {}\nclass C extends Base<number, {}> {}", "C"),
        ["Base<number, {}>"]
    );
    assert_eq!(base_types_of("class Base {}\nclass C extends Base {}", "C"), ["Base"]);
}

#[test]
fn only_the_first_extends_element_of_a_class_counts() {
    assert_eq!(base_types_of("class A {}\nclass B {}\nclass C extends A, B {}", "C"), ["A"]);
}

#[test]
fn a_wrong_arity_class_base_has_no_base_type() {
    assert!(base_types_of("class A {}\nclass B extends A<number> {}", "B").is_empty());
}

#[test]
fn a_circular_class_base_has_no_base_type() {
    assert!(base_types_of("class A extends B {}\nclass B extends A {}", "A").is_empty());
    assert!(base_types_of("class A extends B {}\nclass B extends A {}", "B").is_empty());
}

#[test]
fn interface_bases_follow_every_extends_element() {
    assert_eq!(
        base_types_of(
            "interface A {}\ninterface B<T> {}\ninterface C extends A, B<string> {}",
            "C"
        ),
        ["A", "B<string>"]
    );
}
