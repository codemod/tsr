//! Definite native function types recover a missing arrow rather than rewinding.
use tsr_ast::{Statement, TypeNode};
use tsr_core::{Arena, Span};

#[test]
fn missing_function_type_arrow_retains_type_and_exact_diagnostic() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "var f: (x: number) number;");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect::<Vec<_>>(),
        [(1005, Span::new(19, 25), "'=>' expected.".to_owned())]
    );
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("expected variable")
    };
    let Some(TypeNode::FunctionTypeNode(function)) =
        statement.declaration_list.unwrap().declarations[0].r#type
    else {
        panic!("expected recovering function type")
    };
    assert_eq!(
        function.parameters[0].name.and_then(|name| match name {
            tsr_ast::BindingName::Identifier(id) => Some(id.text),
            _ => None,
        }),
        Some("x")
    );
    assert!(matches!(function.r#type, Some(TypeNode::KeywordTypeNode(_))));
}
