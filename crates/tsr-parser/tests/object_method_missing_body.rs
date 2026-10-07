//! Native object methods retain absent bodies instead of fabricated empty blocks.
use tsr_ast::{Expression, ObjectLiteralElementLike, Statement};
use tsr_core::Arena;

#[test]
fn absent_object_method_body_and_written_empty_body_remain_distinct() {
    for (source, missing) in [
        ("var x = { foo(); };", true),
        ("var x = { foo() };", true),
        ("var x = { foo() {} };", false),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected variable")
        };
        let Some(Expression::ObjectLiteralExpression(object)) =
            statement.declaration_list.unwrap().declarations[0].initializer
        else {
            panic!("expected object")
        };
        let ObjectLiteralElementLike::MethodDeclaration(method) = object.properties[0] else {
            panic!("expected method")
        };
        assert_eq!(method.body.is_none(), missing);
    }
}
