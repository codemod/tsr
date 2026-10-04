//! Empty constructor indices report at pinned native `TokenFullStart`, before trivia.

use tsr_ast::{Expression, Statement};
use tsr_core::{Arena, Span};

#[test]
fn empty_constructor_index_diagnostic_precedes_trivia_without_moving_its_missing_child() {
    // Diagnostic starts come from pinned parseElementAccessExpressionRest/nodePos.
    // Missing children retain their canonical token position, after SkipTrivia.
    for (source, diagnostic_start, missing_start) in [
        ("new C[];", 6, 6),
        ("new C[      ];", 6, 12),
        ("new C[/*comment*/];", 6, 17),
        ("new C[/*é*/\r\n    ];", 6, 18),
        ("new ns.C[/*é*/\r\n\t ];", 9, 19),
        ("new\nC[\r ];", 6, 8),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert_eq!(parsed.diagnostics.len(), 1, "{source:?}: {:?}", parsed.diagnostics);
        assert_eq!(parsed.diagnostics[0].code(), "TS1011", "{source:?}");
        assert_eq!(parsed.diagnostics[0].span, Span::at(diagnostic_start), "{source:?}");
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("{source:?}");
        };
        let Some(Expression::NewExpression(new)) = statement.expression else {
            panic!("{source:?}");
        };
        let Some(Expression::ElementAccessExpression(index)) = new.expression else {
            panic!("{source:?}");
        };
        let missing_id = index
            .argument_expression
            .and_then(|argument| argument.node_id())
            .expect("missing identifier");
        assert_eq!(parsed.nodes.span(missing_id), Span::at(missing_start), "{source:?}");
        assert_eq!(parsed.nodes.parent(missing_id), index.node_id, "{source:?}");
    }
}

#[test]
fn constructor_index_with_a_written_argument_has_no_empty_index_diagnostic() {
    for source in ["new C[/*comment*/0];", "new ns.C[/*é*/\r\n'key']();"] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source:?}: {:?}", parsed.diagnostics);
    }
}
