//! Native type entity right-side-of-dot recovery preserves following declarations.
use tsr_ast::{EntityName, Statement, TypeNode};
use tsr_core::{Arena, Span};

#[test]
fn dangling_type_dot_does_not_consume_the_following_namespace() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "var x: TypeModule1.\nnamespace TypeModule2 {}");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect::<Vec<_>>(),
        [(1003, Span::at(19), "Identifier expected.".to_owned())]
    );
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("expected variable")
    };
    let Some(TypeNode::TypeReferenceNode(reference)) =
        statement.declaration_list.unwrap().declarations[0].r#type
    else {
        panic!("expected reference")
    };
    let Some(EntityName::QualifiedName(name)) = reference.type_name else {
        panic!("expected qualified name")
    };
    let right = name.right.unwrap();
    assert_eq!(right.text, "");
    assert_eq!(parsed.nodes.span(right.node_id.unwrap()), Span::at(19));
    assert!(matches!(parsed.source_file.statements[1], Statement::ModuleDeclaration(_)));
}

#[test]
fn written_member_before_line_break_and_unicode_escape_remain_names() {
    for source in ["var x: A.B\nC;", "var x: A.\\u0062;"] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected variable")
        };
        let Some(TypeNode::TypeReferenceNode(reference)) =
            statement.declaration_list.unwrap().declarations[0].r#type
        else {
            panic!("expected reference")
        };
        let Some(EntityName::QualifiedName(name)) = reference.type_name else {
            panic!("expected qualified name")
        };
        assert_eq!(name.right.unwrap().text, if source.contains("\\u") { "b" } else { "B" });
    }
}
