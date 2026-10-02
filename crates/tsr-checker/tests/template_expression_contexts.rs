//! `checkTemplateExpression` (`checker.go:7976`) answers a template literal
//! type, rather than `string`, when its contextual type has a string-literal,
//! template-literal or string-constrained type-variable constituent
//! (`isTemplateLiteralContextualType`, `checker.go:8008`). Shapes from
//! `conformance/templateLiteralTypes2`; see
//! `docs/architecture/checker-99-template-expression-contexts.md`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement.
fn type_of_last(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let index = parsed.source_file.statements.len() - 1;
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("the last statement must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_template_annotation_makes_a_template_literal_type() {
    assert_eq!(
        type_of_last("declare const s: string;\nconst d: `abc${string}` = `abc${s}`;"),
        "`abc${string}`"
    );
    // A union with a string-literal constituent is a template context too.
    assert_eq!(
        type_of_last("declare const s: string;\nconst d: \"a\" | number = `abc${s}`;"),
        "`abc${string}`"
    );
}

#[test]
fn a_plain_string_annotation_stays_string() {
    assert_eq!(type_of_last("declare const s: string;\nconst d: string = `abc${s}`;"), "string");
}

#[test]
fn a_string_constrained_type_parameter_context_makes_a_template() {
    // `>g2(`xyz-${s}`) : `xyz-${string}`` versus `>g1(`xyz-${s}`) : string`.
    assert_eq!(
        type_of_last(
            "declare function g2<T extends string>(x: T): T;\n\
             declare const s: string;\nconst x = g2(`xyz-${s}`);"
        ),
        "`xyz-${string}`"
    );
    assert_eq!(
        type_of_last(
            "declare function g1<T>(x: T): T;\ndeclare const s: string;\nconst x = g1(`xyz-${s}`);"
        ),
        "string"
    );
}
