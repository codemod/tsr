//! Native catch declarations preserve written initializers for grammar consumers.
use tsr_ast::{Expression, Statement};
use tsr_core::Arena;

#[test]
fn catch_initializer_remains_attached_without_parse_recovery_errors() {
    for source in ["try {} catch (e = 1) {}", "try {} catch (e: any = 1) {}"] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::TryStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected try")
        };
        let variable = statement.catch_clause.unwrap().variable_declaration.unwrap();
        let Some(Expression::NumericLiteral(initializer)) = variable.initializer else {
            panic!("expected written initializer")
        };
        assert_eq!(initializer.text, "1");
        assert_eq!(parsed.nodes.parent(initializer.node_id.unwrap()), variable.node_id);
    }
}
