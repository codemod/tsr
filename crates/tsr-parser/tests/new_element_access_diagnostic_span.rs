//! Empty constructor indices report at pinned native `TokenFullStart`, before trivia.

use tsr_ast::{Expression, Statement};
use tsr_core::{Arena, Span};

#[test]
fn empty_constructor_index_and_its_missing_child_precede_trivia() {
    // Both parseElementAccessExpressionRest and createMissingIdentifier use
    // native nodePos(), before current-token trivia.
    for (source, diagnostic_start) in [
        ("new C[];", 6),
        ("new C[      ];", 6),
        ("new C[/*comment*/];", 6),
        ("new C[/*é*/\r\n    ];", 6),
        ("new ns.C[/*é*/\r\n\t ];", 9),
        ("new\nC[\r ];", 6),
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
        assert_eq!(parsed.nodes.span(missing_id), Span::at(diagnostic_start), "{source:?}");
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
