//! Native void is always a keyword type; other keyword namespace heads may qualify.
use tsr_ast::{Statement, TypeNode};
use tsr_core::{Arena, Span};

#[test]
fn void_followed_by_dot_stays_void_and_recovers_the_next_declaration() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "var v : void.x;");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect::<Vec<_>>(),
        [(1005, Span::new(12, 13), "',' expected.".to_owned())]
    );
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("expected variable")
    };
    let Some(TypeNode::KeywordTypeNode(keyword)) =
        statement.declaration_list.unwrap().declarations[0].r#type
    else {
        panic!("void must remain keyword type")
    };
    assert_eq!(keyword.kind, tsr_ast::SyntaxKind::VoidKeyword);
}

#[test]
fn string_and_any_namespace_heads_remain_qualified_references() {
    for source in ["var v: string.x;", "var v: any.x;"] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected variable")
        };
        assert!(matches!(
            statement.declaration_list.unwrap().declarations[0].r#type,
            Some(TypeNode::TypeReferenceNode(_))
        ));
    }
}
