//! TS17006 / TS17007 — `parseUnaryExpressionOrHigher` (`parser.go:4694`):
//! a simple unary expression or a type assertion may not be the left
//! operand of `**`. Positions and texts were checked against a native
//! `tsgo` built from the pinned submodule.
use tsr_core::{Arena, Span};

fn reports(source: &str) -> Vec<(u32, Span, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect()
}

#[test]
fn a_prefix_operator_left_of_exponentiation_reports_over_the_unary_expression() {
    assert_eq!(
        reports("-x ** 2;"),
        [(
            17006,
            Span::new(0, 2),
            "An unary expression with the '-' operator is not allowed in the left-hand side of an exponentiation expression. Consider enclosing the expression in parentheses.".to_owned()
        )]
    );
}

#[test]
fn a_keyword_operator_is_named_as_written() {
    let found = reports("typeof x ** 2;");
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].0, found[0].1), (17006, Span::new(0, 8)));
    assert!(found[0].2.contains("'typeof'"));
}

#[test]
fn only_the_outermost_unary_reports() {
    let found = reports("- -x ** 2;");
    assert_eq!(found.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), [(17006, Span::new(0, 4))]);
}

#[test]
fn an_update_expression_may_be_the_left_operand() {
    assert!(reports("++x ** 2;").is_empty());
    assert!(reports("(-x) ** 2;").is_empty());
}

#[test]
fn a_type_assertion_left_of_exponentiation_is_ts17007() {
    let found = reports("<number>x ** 2;");
    assert_eq!(found.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), [(17007, Span::new(0, 9))]);
}

#[test]
fn a_right_operand_reports_at_its_own_start() {
    let found = reports("1 + -x ** 2;");
    assert_eq!(found.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(), [(17006, Span::new(4, 6))]);
}
