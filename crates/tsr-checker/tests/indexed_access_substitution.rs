//! Indexed accesses retain both operands until generic arguments are known.

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
fn an_indexed_generic_result_resolves_after_argument_inference() {
    let source = "declare function pick<T, K extends keyof T>(object: T, key: K): T[K];\n\
                  const a = pick({ value: 1 }, \"value\");";
    assert_eq!(type_of_initialiser(source, "a"), "number");
}

#[test]
fn an_indexed_generic_result_resolves_written_type_arguments() {
    let source = "declare function pick<T, K extends keyof T>(): T[K];\n\
                  const a = pick<{ x: number; y: string }, \"y\">();";
    assert_eq!(type_of_initialiser(source, "a"), "string");
}

#[test]
fn a_union_of_property_keys_returns_a_union_of_property_types() {
    let source = "declare function pick<T, K extends keyof T>(): T[K];\n\
                  const a = pick<{ x: number; y: string }, \"x\" | \"y\">();";
    assert_eq!(type_of_initialiser(source, "a"), "string | number");
}

#[test]
fn a_number_index_resolves_the_substituted_array_element() {
    let source = "interface Array<T> { length: number }\n\
                  declare function element<T>(): T[number]; const a = element<string[]>();";
    assert_eq!(type_of_initialiser(source, "a"), "string");
}

#[test]
fn an_inferred_indexed_result_keeps_its_operands_until_the_call() {
    let source = "function pick<T, K extends keyof T>(object: T, key: K) { return object[key]; }\n\
                  const a = pick({ value: 1 }, \"value\");";
    assert_eq!(type_of_initialiser(source, "a"), "number");
}

#[test]
fn keyof_resolves_after_the_object_type_is_substituted() {
    let source = "declare function keys<T>(object: T): keyof T;\n\
                  const a = keys({ x: 1, y: \"s\" });";
    assert_eq!(type_of_initialiser(source, "a"), "\"x\" | \"y\"");
}

#[test]
fn a_keyof_default_resolves_before_indexed_access_substitution() {
    let source = "declare function values<T, K extends keyof T = keyof T>(): T[K];\n\
                  const a = values<{ x: number; y: string }>();";
    assert_eq!(type_of_initialiser(source, "a"), "string | number");
}

#[test]
fn a_numeric_property_name_contributes_a_numeric_key_type() {
    let source = "declare function keys<T>(): keyof T; const a = keys<{ 1: number }>();";
    assert_eq!(type_of_initialiser(source, "a"), "1");
}

#[test]
fn a_quoted_numeric_property_name_keeps_a_string_key_type() {
    let source = "declare function keys<T>(): keyof T; const a = keys<{ \"1\": number }>();";
    assert_eq!(type_of_initialiser(source, "a"), "\"1\"");
}

#[test]
fn keyof_defaults_include_inherited_properties() {
    let source = "interface Base { x: number } interface Derived extends Base { y: string }\n\
                  declare function values<T, K extends keyof T = keyof T>(): T[K];\n\
                  const a = values<Derived>();";
    assert_eq!(type_of_initialiser(source, "a"), "string | number");
}

#[test]
fn keyof_excludes_private_and_protected_properties() {
    let source = "class C { x: number; private y: string; protected z: boolean }\n\
                  declare function values<T, K extends keyof T = keyof T>(): T[K];\n\
                  const a = values<C>();";
    assert_eq!(type_of_initialiser(source, "a"), "number");
}

#[test]
fn a_string_index_signature_contributes_string_and_number_keys() {
    let source = "declare function keys<T>(): keyof T;\n\
                  const a = keys<{ [key: string]: number }>();";
    assert_eq!(type_of_initialiser(source, "a"), "string | number");
}

#[test]
fn keyof_an_enum_object_excludes_its_reverse_mapping_index() {
    let source = "enum E { A, B }\n\
                  declare function values<T, K extends keyof T = keyof T>(): T[K];\n\
                  const a = values<typeof E>();";
    assert_eq!(type_of_initialiser(source, "a"), "E");
}
