//! Constructor member-expression precedence, proved against the pinned parser.

use tsr_ast::{Expression, Node, Statement, SyntaxKind};
use tsr_core::Arena;

#[test]
fn constructor_callees_consume_members_but_not_calls_or_optional_chains() {
    for (source, callee_kind, callee_text, arguments) in [
        ("new ns.C[key].D<T>(3)", SyntaxKind::PropertyAccessExpression, "ns.C[key].D", 1),
        ("new C[0]!()", SyntaxKind::NonNullExpression, "C[0]!", 0),
        ("new C[0]", SyntaxKind::ElementAccessExpression, "C[0]", 0),
        ("new new C[1](5)", SyntaxKind::NewExpression, "new C[1](5)", 0),
        ("new tag`x`(6)", SyntaxKind::TaggedTemplateExpression, "tag`x`", 1),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("{source}");
        };
        let Some(Expression::NewExpression(new)) = statement.expression else {
            panic!("expected constructor, not a call: {source}");
        };
        let callee = Node::from(new.expression.expect("callee"));
        let id = callee.node_id().expect("registered callee");
        let span = parsed.nodes.span(id);
        assert_eq!(parsed.nodes.kind(id), callee_kind, "{source}");
        assert_eq!(&source[span.start as usize..span.end as usize], callee_text, "{source}");
        assert_eq!(new.arguments.len(), arguments, "{source}");
        assert_eq!(parsed.nodes.parent(id), new.node_id, "the callee belongs to new");
        assert_eq!(new.type_arguments.len(), usize::from(source.contains("<T>")), "{source}");
    }
    for source in ["new C()[1].f(4)", "new C[0]?.f()"] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("{source}");
        };
        assert!(matches!(statement.expression, Some(Expression::CallExpression(_))), "{source}");
        let codes: Vec<_> =
            parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::code).collect();
        assert_eq!(codes, if source.contains("?.") { vec!["TS1209"] } else { vec![] });
    }
}

#[test]
fn empty_constructor_index_retains_its_missing_argument_and_native_diagnostic() {
    let source = "new C[];";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let codes: Vec<_> = parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::code).collect();
    assert_eq!(codes, ["TS1011"]);
    let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
        panic!()
    };
    let Some(Expression::NewExpression(new)) = statement.expression else { panic!() };
    let Some(Expression::ElementAccessExpression(index)) = new.expression else { panic!() };
    let missing = index
        .argument_expression
        .and_then(|argument| argument.node_id())
        .expect("missing identifier");
    assert_eq!(parsed.nodes.span(missing), tsr_core::Span::at(6));
    assert_eq!(parsed.nodes.parent(missing), index.node_id);
    assert_eq!(parsed.diagnostics[0].span, tsr_core::Span::at(6));
}
