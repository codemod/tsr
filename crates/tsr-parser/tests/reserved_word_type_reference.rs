//! Native parseNonArrayType default references preserve reserved names and recovery.
use tsr_ast::{EntityName, Statement, TypeNode};
use tsr_core::{Arena, Span};

#[test]
fn reserved_type_names_do_not_terminate_a_parameter_or_alias() {
    for keyword in ["break", "return", "private", "function"] {
        let source = format!("type T = {keyword}; function f(x: {keyword}) {{}}");
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[0] else {
            panic!("expected alias");
        };
        let Some(TypeNode::TypeReferenceNode(reference)) = alias.r#type else {
            panic!("expected reserved-word reference");
        };
        let Some(EntityName::Identifier(name)) = reference.type_name else {
            panic!("expected reserved-word name");
        };
        assert_eq!(name.text, keyword);
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[1] else {
            panic!("expected function, not recovery statements");
        };
        let Some(TypeNode::TypeReferenceNode(parameter_type)) = function.parameters[0].r#type
        else {
            panic!("expected parameter type reference");
        };
        let Some(EntityName::Identifier(name)) = parameter_type.type_name else {
            panic!("expected parameter name");
        };
        assert_eq!(name.text, keyword);
        assert!(function.body.is_some());
    }
}

#[test]
fn missing_type_is_a_zero_width_reference_with_native_type_expected_diagnostic() {
    for (source, diagnostic) in [("type T = ;", Span::new(9, 10)), ("type T =", Span::at(8))] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|d| (d.message.code(), d.span, d.text()))
                .collect::<Vec<_>>(),
            [(1110, diagnostic, "Type expected.".to_owned())]
        );
        let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[0] else {
            panic!("expected alias");
        };
        let Some(TypeNode::TypeReferenceNode(reference)) = alias.r#type else {
            panic!("native missing type must remain a reference, not keyword any");
        };
        let Some(EntityName::Identifier(name)) = reference.type_name else {
            panic!("expected missing name")
        };
        assert_eq!(name.text, "");
        assert_eq!(parsed.nodes.span(name.node_id.unwrap()), Span::at(8));
    }
}
