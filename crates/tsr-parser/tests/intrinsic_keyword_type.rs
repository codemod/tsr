//! `intrinsic` is a keyword type only as a whole type alias body.
use tsr_ast::{Statement, SyntaxKind, TypeNode};
use tsr_core::Arena;

#[test]
fn intrinsic_is_a_reference_outside_an_alias_body() {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse(&arena, "let i: intrinsic; type U = intrinsic; type V = (intrinsic);");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Statement::VariableStatement(variable) = parsed.source_file.statements[0] else {
        panic!("expected variable")
    };
    assert!(matches!(
        variable.declaration_list.unwrap().declarations[0].r#type,
        Some(TypeNode::TypeReferenceNode(_))
    ));
    let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[1] else {
        panic!("expected alias")
    };
    assert!(matches!(
        alias.r#type,
        Some(TypeNode::KeywordTypeNode(keyword)) if keyword.kind == SyntaxKind::IntrinsicKeyword
    ));
    let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[2] else {
        panic!("expected alias")
    };
    let Some(TypeNode::ParenthesizedTypeNode(inner)) = alias.r#type else {
        panic!("expected parenthesized")
    };
    assert!(matches!(inner.r#type, Some(TypeNode::TypeReferenceNode(_))));
}
