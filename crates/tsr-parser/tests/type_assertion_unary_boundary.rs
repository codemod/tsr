//! Native type assertions belong to unary grammar, not new-expression primary operands.
use tsr_ast::{Expression, Statement};
use tsr_core::{Arena, Span};

#[test]
fn new_cannot_consume_an_unparenthesized_type_assertion() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "var x = new <any>C();");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect::<Vec<_>>(),
        [(1109, Span::new(12, 13), "Expression expected.".to_owned())]
    );
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("expected variable")
    };
    assert!(matches!(
        statement.declaration_list.unwrap().declarations[0].initializer,
        Some(Expression::BinaryExpression(_))
    ));
}

#[test]
fn ordinary_and_parenthesized_constructor_assertions_still_parse() {
    for source in ["var x = <any>C();", "var x = new (<any>C)();"] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
    }
}
