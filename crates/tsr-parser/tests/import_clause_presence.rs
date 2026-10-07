//! Native nil import clauses remain distinct from allocated missing named-import lists.
use tsr_core::Arena;

#[test]
fn incomplete_import_does_not_publish_a_fabricated_clause() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "import\nimport('m');");
    // parseModuleSpecifier accepts an arbitrary expression even though checking
    // later requires a string literal; it must not manufacture an ImportClause.
    let tsr_ast::Statement::ImportDeclaration(first) = parsed.source_file.statements[0] else {
        panic!("expected import declaration recovery")
    };
    assert!(first.import_clause.is_none());
    assert!(matches!(first.module_specifier, Some(tsr_ast::Expression::CallExpression(_))));
}
