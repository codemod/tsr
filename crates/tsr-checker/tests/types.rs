//! What the checker must get right, stated as questions about printed types.
//!
//! Assertions go through [`Checker::type_to_string`] rather than through
//! `TypeId`s, because the printed form is what `.types` baselines compare and a
//! type that is internally right but prints wrong fails conformance identically.

use tsr_ast::{Expression, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the first `const`/`let` in the source.
fn type_of_initialiser(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes);

    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initialiser = declaration.initializer.expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_literal_expression_has_its_own_literal_type() {
    // The expression `"a"` is `"a"`, not `string` — the distinction the whole
    // literal-type machinery exists for.
    assert_eq!(type_of_initialiser(r#"const x = "a";"#), r#""a""#);
    assert_eq!(type_of_initialiser("const x = 1;"), "1");
    assert_eq!(type_of_initialiser("const x = true;"), "true");
    assert_eq!(type_of_initialiser("const x = false;"), "false");
    assert_eq!(type_of_initialiser("const x = null;"), "null");
    assert_eq!(type_of_initialiser("const x = 1n;"), "1n");
}

#[test]
fn a_numeric_literal_type_prints_its_value_not_its_spelling() {
    // `1.0` and `0x1` are both the type `1`. Getting this from the source text
    // would be wrong in a way every numeric baseline would catch.
    assert_eq!(type_of_initialiser("const x = 1.0;"), "1");
    assert_eq!(type_of_initialiser("const x = 0x10;"), "16");
    assert_eq!(type_of_initialiser("const x = 1_000;"), "1000");
    assert_eq!(type_of_initialiser("const x = 1.5;"), "1.5");
}

#[test]
fn a_string_literal_type_is_quoted_and_escaped_as_typescript_prints_it() {
    // Whole-line comparison means the quoting is under test, not cosmetic.
    assert_eq!(type_of_initialiser(r"const x = 'a';"), r#""a""#);
    assert_eq!(type_of_initialiser(r#"const x = "a\"b";"#), r#""a\"b""#);
    assert_eq!(type_of_initialiser(r#"const x = "a\\b";"#), r#""a\\b""#);
}

#[test]
fn parentheses_do_not_change_a_type() {
    assert_eq!(type_of_initialiser(r#"const x = ("a");"#), r#""a""#);
    assert_eq!(type_of_initialiser("const x = ((1));"), "1");
}

#[test]
fn an_unported_expression_form_is_error_not_any() {
    // `errorType` and `anyType` both print `any`, and conflating them is how a
    // gap becomes an assertion. Everything unported must land on `error`.
    let arena = Arena::new();
    let source = "const x = f();";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    assert_eq!(id, checker.intrinsics().error, "an unported form must be errorType");
    assert_ne!(id, checker.intrinsics().any, "and must not be anyType");
}

#[test]
fn identical_literals_are_one_interned_type() {
    // Literal identity is TypeId equality, which is what makes `"a" === "a"`
    // decidable without comparing strings on every relation check.
    let arena = Arena::new();
    let source = r#"const a = "x"; const b = "x"; const c = "y";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes);

    let mut ids = Vec::new();
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(statement) = statement else { continue };
        let initialiser = statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|d| d.initializer)
            .expect("an initialiser");
        ids.push(checker.check_expression(initialiser));
    }
    assert_eq!(ids[0], ids[1], r#"two occurrences of "x" must be one type"#);
    assert_ne!(ids[0], ids[2], r#""x" and "y" must not be"#);
}

#[test]
fn the_intrinsics_that_print_alike_are_still_distinct_types() {
    // `anyType` and `errorType` both print `any`; merging them would silently
    // change which errors cascade. Upstream keeps them apart by pointer identity.
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: "" },
    );
    let checker = Checker::new(&bound, &parsed.nodes);
    let intrinsics = checker.intrinsics();

    assert_eq!(checker.type_to_string(intrinsics.any), "any");
    assert_eq!(checker.type_to_string(intrinsics.error), "error");
    assert_ne!(intrinsics.any, intrinsics.error);
}

#[test]
fn a_literal_type_widens_to_its_primitive() {
    // The rule behind `let x = "a"` being `string` while `const x = "a"` is `"a"`.
    let arena = Arena::new();
    let source = r#"const x = "a";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");

    let literal = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(literal), r#""a""#);
    let widened = checker.widen_literal(literal);
    assert_eq!(checker.type_to_string(widened), "string");

    // Widening a non-literal is the identity, not an error.
    let string = checker.intrinsics().string;
    let again = checker.widen_literal(string);
    assert_eq!(again, string);
}

#[test]
fn an_expression_is_typed_once_however_many_times_it_is_asked_for() {
    // The memo is the whole reason `&mut self` returning `TypeId` was chosen; if
    // it were bypassed the checker would be exponential on nested expressions.
    let arena = Arena::new();
    let source = r#"const x = "a";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser: Expression<'_> = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");

    // Counting *types* cannot test this: interning already means a repeated
    // literal produces no new type, so that assertion passes with the memo
    // removed. Counting computations is what distinguishes the two.
    let first = checker.check_expression(initialiser);
    assert_eq!(checker.computations(), 1, "the first ask computes");
    let second = checker.check_expression(initialiser);

    assert_eq!(first, second);
    assert_eq!(checker.computations(), 1, "the second ask is served from the memo");
}
