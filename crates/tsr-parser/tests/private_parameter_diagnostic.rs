//! Native private parameter diagnostics preserve the written identifier and type.
use tsr_ast::{BindingName, Statement};
use tsr_core::{Arena, Span};

#[test]
fn private_parameter_uses_native_message_without_losing_the_identifier() {
    for (source, span) in [
        ("function f(#x: number) {}", Span::new(11, 13)),
        ("function f(...#x: string[]) {}", Span::new(14, 16)),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|d| (d.message.code(), d.span, d.text()))
                .collect::<Vec<_>>(),
            [(18009, span, "Private identifiers cannot be used as parameters.".to_owned())]
        );
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[0] else {
            panic!("expected function")
        };
        let Some(BindingName::Identifier(name)) = function.parameters[0].name else {
            panic!("expected consumed private identifier")
        };
        assert_eq!(name.text, "#x");
        assert!(function.parameters[0].r#type.is_some());
        assert!(function.body.is_some());
    }
}
