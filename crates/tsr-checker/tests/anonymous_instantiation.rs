//! Anonymous object substitution follows instantiateAnonymousType and member mapping.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's initialiser.
fn type_of_initialiser(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|list| list.declarations).unwrap_or_default() {
            let Some(tsr_ast::BindingName::Identifier(identifier)) = declaration.name else {
                continue;
            };
            if identifier.text != name {
                continue;
            }
            let initialiser = declaration.initializer.expect("an initialiser");
            let id = checker.check_expression(initialiser);
            return checker.type_to_string(id);
        }
    }
    panic!("no declaration named {name}");
}

#[test]
fn a_generic_result_substitutes_an_anonymous_property() {
    let source = "declare function box<T>(value: T): { value: T }; const a = box(1);";
    assert_eq!(type_of_initialiser(source, "a"), "{ value: number; }");
}

#[test]
fn a_substituted_property_reads_its_instantiated_type() {
    let source =
        "declare function box<T>(value: T): { value: T }; const b = box(1); const a = b.value;";
    assert_eq!(type_of_initialiser(source, "a"), "number");
}

#[test]
fn nested_anonymous_properties_substitute_recursively() {
    let source =
        "declare function box<T>(value: T): { inner: { readonly value: T } }; const a = box(1);";
    assert_eq!(type_of_initialiser(source, "a"), "{ inner: { readonly value: number; }; }");
}

#[test]
fn optional_and_quoted_properties_keep_their_modifiers_and_names() {
    let source = "declare function box<T>(value: T): { readonly \"a b\"?: T }; const a = box(1);";
    assert_eq!(type_of_initialiser(source, "a"), "{ readonly \"a b\"?: number; }");
}

#[test]
fn composition_instantiates_the_second_generic_function_from_the_first() {
    let source = "interface Array<T> { length: number }
\
                  declare function compose<A, B, C>(f: (x: A) => B, g: (y: B) => C): (x: A) => C;
\
                  declare function list<T>(x: T): T[];
\
                  declare function box<V>(x: V): { value: V }; const a = compose(list, box);";
    assert_eq!(type_of_initialiser(source, "a"), "<T>(x: T) => { value: T[]; }");
}

#[test]
fn an_inferred_generic_object_result_substitutes_its_captured_property_type() {
    let source = "function box<T>(value: T) { return { value }; } const a = box(1);";
    assert_eq!(type_of_initialiser(source, "a"), "{ value: number; }");
}

#[test]
fn an_inferred_generic_object_property_can_be_read_after_substitution() {
    let source = "function box<T>(value: T) { return { renamed: value }; } const b = box(1); const a = b.renamed;";
    assert_eq!(type_of_initialiser(source, "a"), "number");
}

#[test]
fn spreading_an_instantiated_object_uses_its_mapped_property_types() {
    let source = "function box<T>(value: T) { return { value }; } const a = { ...box(1) };";
    assert_eq!(type_of_initialiser(source, "a"), "{ value: number; }");
}

#[test]
fn unchanged_member_spellings_survive_substitution() {
    let source =
        "declare function box<T>(value: T): { value: T; fixed: 'hello' }; const a = box(1);";
    assert_eq!(type_of_initialiser(source, "a"), "{ value: number; fixed: \"hello\"; }");
}
