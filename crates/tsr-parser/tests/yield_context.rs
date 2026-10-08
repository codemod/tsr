//! `isYieldExpression`/`isIdentifier` read the generator's yield context.
use tsr_ast::{Expression, Statement};
use tsr_core::{Arena, Span};

fn expression_of(statement: Statement<'_>) -> Expression<'_> {
    let Statement::ExpressionStatement(statement) = statement else {
        panic!("expected expression statement")
    };
    statement.expression.unwrap()
}

#[test]
fn yield_outside_a_generator_is_a_call_or_operand_unless_an_operand_follows() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "yield(1);\nyield * 2;\nyield;\nyield 3;");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let statements = parsed.source_file.statements;
    assert!(matches!(expression_of(statements[0]), Expression::CallExpression(_)));
    assert!(matches!(expression_of(statements[1]), Expression::BinaryExpression(_)));
    assert!(matches!(expression_of(statements[2]), Expression::Identifier(_)));
    assert!(matches!(expression_of(statements[3]), Expression::YieldExpression(_)));
}

#[test]
fn yield_in_a_generator_is_no_identifier_in_a_unary_operand() {
    let arena = Arena::new();
    let source = "function* f() { <number> yield 0; }";
    let parsed = tsr_parser::parse(&arena, source);
    let at = u32::try_from(source.find("yield").unwrap()).unwrap();
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span)).collect::<Vec<_>>(),
        [(1109, Span::new(at, at + 5))]
    );
}

#[test]
fn arrow_bodies_and_property_initializers_leave_the_yield_context() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(
        &arena,
        "function* f() { const g = () => yield; class C { x = yield; } yield; }",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}
