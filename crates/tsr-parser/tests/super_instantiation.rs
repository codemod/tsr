//! Pinned parser recovery ownership and type-argument precedence controls.

use tsr_ast::{
    Expression, FunctionBody, MemberName, Node, ObjectLiteralElementLike, Statement, SyntaxKind,
};
use tsr_core::{Arena, Span};

#[test]
fn malformed_super_owns_a_property_and_preserves_the_following_token() {
    for (source, name_text, name_span, property_span) in [
        ("super;", "", Span::at(5), Span::new(0, 5)),
        ("super foo;", "foo", Span::new(6, 9), Span::new(0, 9)),
        ("super += 3;", "", Span::at(5), Span::new(0, 5)),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert_eq!(parsed.diagnostics.len(), 1, "{source}");
        assert_eq!(parsed.diagnostics[0].code(), "TS1034", "{source}");
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("{source}");
        };
        let expression = statement.expression.expect("expression");
        let expression = if let Expression::BinaryExpression(assignment) = expression {
            assert_eq!(assignment.operator_token.unwrap().kind, SyntaxKind::PlusEqualsToken);
            assert!(matches!(assignment.right, Some(Expression::NumericLiteral(_))));
            assignment.left.unwrap()
        } else {
            expression
        };
        let Expression::PropertyAccessExpression(property) = expression else {
            panic!("native recovery is a property, not a bare keyword: {source}");
        };
        let Some(Expression::KeywordExpression(keyword)) = property.expression else { panic!() };
        assert_eq!(keyword.kind, SyntaxKind::SuperKeyword);
        let Some(MemberName::Identifier(name)) = property.name else { panic!() };
        assert_eq!(name.text, name_text);
        assert_eq!(parsed.nodes.span(name.node_id.unwrap()), name_span);
        assert_eq!(parsed.nodes.span(property.node_id.unwrap()), property_span);
        assert_eq!(parsed.nodes.parent(name.node_id.unwrap()), property.node_id);
        assert_eq!(parsed.nodes.parent(keyword.node_id.unwrap()), property.node_id);
    }
}

#[test]
fn super_asi_recovery_reports_both_native_locations() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "super\nlet next=1;");
    assert_eq!(parsed.source_file.statements.len(), 2);
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.code(), d.span)).collect::<Vec<_>>(),
        [("TS1003".to_string(), Span::at(5)), ("TS1034".to_string(), Span::new(6, 9))],
    );
}

#[test]
fn super_call_absorbs_type_arguments_and_retains_raw_diagnostic_range() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "super <T> (7);");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.code(), d.span)).collect::<Vec<_>>(),
        [("TS2754".to_string(), Span::new(5, 9))],
    );
    let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
        panic!()
    };
    let Some(Expression::CallExpression(call)) = statement.expression else { panic!() };
    assert_eq!(call.type_arguments.len(), 1);
    assert_eq!(call.arguments.len(), 1);
    let Some(Expression::KeywordExpression(keyword)) = call.expression else { panic!() };
    assert_eq!(keyword.kind, SyntaxKind::SuperKeyword);
    assert_eq!(parsed.nodes.parent(keyword.node_id.unwrap()), call.node_id);
}

#[test]
fn instantiation_binary_and_unary_boundaries_stay_distinct() {
    for (source, left_kind) in [
        ("f<T> || fallback;", SyntaxKind::ExpressionWithTypeArguments),
        ("f<T> ?? fallback;", SyntaxKind::ExpressionWithTypeArguments),
        ("f<T> && fallback;", SyntaxKind::ExpressionWithTypeArguments),
        ("f<T> + value;", SyntaxKind::BinaryExpression),
        ("f<T> - value;", SyntaxKind::BinaryExpression),
        ("f<T>\n+value;", SyntaxKind::BinaryExpression),
        ("f<T>value;", SyntaxKind::BinaryExpression),
        ("f<T>['g'];", SyntaxKind::BinaryExpression),
        ("0 < u >>> 0;", SyntaxKind::NumericLiteral),
        ("0 < u >> 0;", SyntaxKind::NumericLiteral),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("{source}");
        };
        let Some(Expression::BinaryExpression(binary)) = statement.expression else {
            panic!("{source}");
        };
        let left = Node::from(binary.left.unwrap()).node_id().unwrap();
        assert_eq!(parsed.nodes.kind(left), left_kind, "{source}");
    }
    // In the NoIn context, the `in` belongs to the loop, not the initializer.
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "for (f<T> in obj) {}");
    assert!(parsed.diagnostics.is_empty());
    let Statement::ForInOrOfStatement(statement) = parsed.source_file.statements[0] else {
        panic!()
    };
    let initializer = Node::from(statement.initializer.unwrap()).node_id().unwrap();
    assert_eq!(parsed.nodes.kind(initializer), SyntaxKind::ExpressionWithTypeArguments);
}

#[test]
fn accessor_missing_bodies_do_not_consume_following_property_names() {
    for (source, body_state) in [
        ("({get a(), set b(v), typeof: 2, super: 3});", 1),
        ("({get a(); set b(v); typeof: 2, super: 3});", 0),
        ("({get a()\nset b(v)\ntypeof: 2, super: 3});", 0),
        ("({get a(){return 1;}, set b(v){super.f(v);}, typeof: 2, super: 3});", 2),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("{source}");
        };
        let Some(Expression::ParenthesizedExpression(paren)) = statement.expression else {
            panic!()
        };
        let Some(Expression::ObjectLiteralExpression(object)) = paren.expression else { panic!() };
        assert_eq!(object.properties.len(), 4, "following names stay object properties: {source}");
        let ObjectLiteralElementLike::GetAccessorDeclaration(getter) = object.properties[0] else {
            panic!()
        };
        let ObjectLiteralElementLike::SetAccessorDeclaration(setter) = object.properties[1] else {
            panic!()
        };
        for body in [getter.body, setter.body] {
            match (body_state, body) {
                (0, None) => {}
                (1, Some(FunctionBody::Block(block))) => assert!(block.statements.is_empty()),
                (2, Some(FunctionBody::Block(block))) => assert_eq!(block.statements.len(), 1),
                _ => panic!("wrong body ownership for {source}"),
            }
        }
    }
}
