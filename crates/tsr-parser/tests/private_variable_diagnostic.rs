//! Native variable and catch private-name diagnostics retain their written declarations.
use tsr_ast::{BindingName, Statement};
use tsr_core::{Arena, Span};

#[test]
fn private_variable_uses_specific_native_message_and_preserves_initializer() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "const #foo = 3;");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect::<Vec<_>>(),
        [(
            18029,
            Span::new(6, 10),
            "Private identifiers are not allowed in variable declarations.".to_owned()
        )]
    );
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("expected variable")
    };
    let declaration = statement.declaration_list.unwrap().declarations[0];
    let Some(BindingName::Identifier(name)) = declaration.name else {
        panic!("expected consumed name")
    };
    assert_eq!(name.text, "#foo");
    assert!(declaration.initializer.is_some());
}
