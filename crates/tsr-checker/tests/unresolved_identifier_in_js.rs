//! Native unknown value identifiers return errorType independently of module form.

use tsr_ast::{NodeFlags, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the initializer of the file's last variable statement.
///
/// `javascript` stamps [`NodeFlags::JAVASCRIPT_FILE`] on the root, which is
/// what a `.js` extension does in the loader
/// (`crates/tsr-compiler/src/loader.rs:639`) — there is no `ScriptKind::Js`.
fn type_of_initializer(source: &str, javascript: bool) -> String {
    let arena = Arena::new();
    let mut parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    if javascript {
        parsed
            .nodes
            .add_flags(parsed.source_file.node_id.expect("a root id"), NodeFlags::JAVASCRIPT_FILE);
    }
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.js", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initializer = declaration.initializer.expect("an initializer");
    let id = checker.check_expression(initializer);
    checker.type_to_string(id)
}

/// `parsingDeepParenthensizedExpression` records `error` for its undeclared
/// `f`; a plain `.js` file with no module machinery is the same shape.
#[test]
fn an_unresolved_identifier_in_a_plain_js_file_is_error() {
    assert_eq!(type_of_initializer("var x = f;", true), "error");
}

