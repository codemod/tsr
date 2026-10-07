//! Native improper generic constraints preserve unary expression syntax.
use tsr_ast::{Expression, Statement};
use tsr_core::Arena;

#[test]
fn improper_constraint_keeps_expression_and_does_not_consume_list_close() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "class C<T extends +1, U> {}");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Statement::ClassDeclaration(class) = parsed.source_file.statements[0] else {
        panic!("expected class")
    };
    let [first, second] = class.type_parameters else { panic!("expected two type parameters") };
    assert!(first.constraint.is_none());
    let Some(Expression::PrefixUnaryExpression(expression)) = first.expression else {
        panic!("expected unary recovery constraint")
    };
    assert_eq!(expression.operator.kind, tsr_ast::SyntaxKind::PlusToken);
    assert_eq!(second.name.unwrap().text, "U");
}
