//! Native missing named-import list recovery after a default binding comma.
use tsr_ast::{NamedImportBindings, Statement};
use tsr_core::{Arena, Span};

#[test]
fn missing_named_imports_report_open_brace_without_consuming_from() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "import defaultBinding, from 'm';");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect::<Vec<_>>(),
        [(1005, Span::new(23, 27), "'{' expected.".to_owned())]
    );
    let Statement::ImportDeclaration(import) = parsed.source_file.statements[0] else {
        panic!("expected import")
    };
    let clause = import.import_clause.unwrap();
    assert_eq!(clause.name.unwrap().text, "defaultBinding");
    let Some(NamedImportBindings::NamedImports(bindings)) = clause.named_bindings else {
        panic!("native missing list must stay allocated")
    };
    assert!(bindings.elements.is_empty());
    assert!(matches!(import.module_specifier, Some(tsr_ast::Expression::StringLiteral(_))));
}
