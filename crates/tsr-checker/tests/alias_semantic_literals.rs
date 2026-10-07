//! Native `getDeclaredTypeOfTypeAlias` resolves pre-existing literal and parameter
//! bodies unchanged; a generic alias must not turn them into nominal objects.

use tsr_ast::{Node, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

fn declared_and_reference(source: &str, name: &str) -> (String, String) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let symbol = bound.lookup_local(root, name).unwrap();
    let annotation = parsed
        .source_file
        .statements
        .iter()
        .find_map(|statement| {
            let Statement::VariableStatement(statement) = statement else { return None };
            statement.declaration_list?.declarations.first()?.r#type
        })
        .unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let declared = checker.get_declared_type_of_symbol(symbol);
    let reference = checker.get_type_from_type_node(annotation);
    (checker.type_to_string(declared), checker.type_to_string(reference))
}

#[test]
fn generic_literal_aliases_retain_their_body_type() {
    for literal in ["\"fixed\"", "42", "-42", "true", "false", "null", "123n"] {
        let source = format!("type Fixed<T> = ({literal}); declare let value: Fixed<string>;");
        assert_eq!(declared_and_reference(&source, "Fixed"), (literal.into(), literal.into()));
    }
}

#[test]
fn parenthesized_keyword_bodies_remain_intrinsic_types() {
    for keyword in [
        "any",
        "unknown",
        "string",
        "number",
        "bigint",
        "boolean",
        "symbol",
        "void",
        "undefined",
        "never",
        "object",
    ] {
        let source =
            format!("type Primitive<T> = (({keyword})); declare let value: Primitive<string>;");
        assert_eq!(declared_and_reference(&source, "Primitive"), (keyword.into(), keyword.into()));
    }
}

#[test]
fn parenthesized_indexed_alias_body_substitutes_arguments() {
    assert_eq!(
        declared_and_reference(
            "type Get<T, K extends keyof T> = ((T[K])); declare let value: Get<{ item: string }, \"item\">;",
            "Get",
        ).1,
        "string",
    );
}

#[test]
fn parenthesized_parameter_body_retains_parameter_identity() {
    assert_eq!(
        declared_and_reference("type Id<T> = ((T)); declare let value: Id<string>;", "Id"),
        ("T".into(), "string".into()),
    );
}

#[test]
fn explicit_literal_union_keeps_its_alias_identity() {
    assert_eq!(
        declared_and_reference(
            "type Choice<T> = \"a\" | \"b\"; declare let value: Choice<string>;",
            "Choice"
        ),
        ("Choice<T>".into(), "Choice<string>".into()),
    );
}
