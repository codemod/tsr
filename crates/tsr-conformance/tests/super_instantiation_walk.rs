//! Native preorder controls: repeated `super` text cannot substitute for nodes.

use tsr_ast::{Node, SyntaxKind};
use tsr_core::Arena;

fn walk(source: &str) -> Vec<(SyntaxKind, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    tsr_conformance::types_producer::assertions_for_file(
        &Node::SourceFile(parsed.source_file),
        source,
        &parsed.nodes,
        &parsed.node_map,
        |_| String::new(),
    )
    .into_iter()
    .map(|assertion| (assertion.kind, assertion.text))
    .collect()
}

#[test]
fn malformed_super_keeps_the_property_keyword_and_missing_name_in_order() {
    use SyntaxKind::{
        BinaryExpression, Identifier, NumericLiteral, PropertyAccessExpression, SuperKeyword,
    };
    assert_eq!(
        walk("super; super += 3; super foo;"),
        [
            (PropertyAccessExpression, "super"),
            (SuperKeyword, "super"),
            (Identifier, ""),
            (BinaryExpression, "super += 3"),
            (PropertyAccessExpression, "super"),
            (SuperKeyword, "super"),
            (Identifier, ""),
            (NumericLiteral, "3"),
            (PropertyAccessExpression, "super foo"),
            (SuperKeyword, "super"),
            (Identifier, "foo"),
        ]
        .map(|(kind, text)| (kind, text.to_string())),
    );
}

#[test]
fn member_and_binary_instantiations_keep_native_type_argument_ownership() {
    use SyntaxKind::{
        BinaryExpression, CallExpression, ExpressionWithTypeArguments, Identifier, NumericLiteral,
        PropertyAccessExpression,
    };
    assert_eq!(
        walk("f<T>.g; f<T> || fallback; super<T>(7);"),
        [
            (PropertyAccessExpression, "f<T>.g"),
            (ExpressionWithTypeArguments, "f<T>"),
            (Identifier, "f"),
            (Identifier, "g"),
            (BinaryExpression, "f<T> || fallback"),
            (ExpressionWithTypeArguments, "f<T>"),
            (Identifier, "f"),
            (Identifier, "fallback"),
            (CallExpression, "super<T>(7)"),
            (SyntaxKind::SuperKeyword, "super"),
            (NumericLiteral, "7"),
        ]
        .map(|(kind, text)| (kind, text.to_string())),
    );
}
