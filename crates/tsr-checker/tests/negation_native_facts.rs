//! Native `checkPrefixUnaryExpression` truthiness switch at pinned 5b1047d.
use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::{Arena, CompilerOptions, Tristate};

fn negation(source: &str, strict: bool) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "control must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.apply_compiler_options(&CompilerOptions {
        strict_null_checks: Tristate::from_bool(strict),
        ..CompilerOptions::default()
    });
    let Statement::VariableStatement(statement) = parsed.source_file.statements.last().unwrap()
    else {
        panic!("last control statement is a variable");
    };
    let expression = statement.declaration_list.unwrap().declarations[0].initializer.unwrap();
    let result = checker.check_expression(expression);
    checker.type_to_string(result)
}

#[test]
fn empty_and_mixed_truthiness_facts_select_boolean() {
    for strict in [false, true] {
        for source in [
            "declare const value: never; const result = !value;",
            "declare const value: unknown; const result = !value;",
            "declare const value: string; const result = !value;",
            "const result = !notDeclared;",
        ] {
            assert_eq!(negation(source, strict), "boolean", "{source}, strict={strict}");
        }
    }
}

#[test]
fn decided_truthiness_keeps_native_literal_results() {
    for (source, expected) in [
        ("const result = !1;", "false"),
        ("const result = !0;", "true"),
        ("const result = !null;", "true"),
        ("const result = !\"text\";", "false"),
    ] {
        assert_eq!(negation(source, true), expected, "{source}");
    }
}
