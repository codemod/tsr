//! Invalid-token recovery in variable declaration lists.
//!
//! `parseDelimitedList` (`parser.go:649`) skips a token that belongs to no
//! current or enclosing parsing context, then retries the list. An invalid
//! Unicode escape's backslash is such a token, so `var arg\u003` declares both
//! `arg` and `u003`; it does not leave `u003` to become an expression statement.

use tsr_ast::{BindingName, Statement};
use tsr_core::Arena;
use tsr_parser::parse;

fn declaration_names(source: &str) -> (Vec<String>, Vec<String>) {
    let arena = Arena::new();
    let parsed = parse(&arena, source);
    let names = parsed
        .source_file
        .statements
        .iter()
        .flat_map(|statement| match statement {
            Statement::VariableStatement(statement) => statement
                .declaration_list
                .map_or(&[] as &[&tsr_ast::VariableDeclaration<'_>], |list| list.declarations),
            _ => &[],
        })
        .filter_map(|declaration| match declaration.name {
            Some(BindingName::Identifier(identifier)) => Some(identifier.text.to_string()),
            _ => None,
        })
        .collect();
    let diagnostics = parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::code).collect();
    (names, diagnostics)
}

#[test]
fn an_invalid_escape_between_declarations_is_skipped() {
    let (names, diagnostics) = declaration_names(r"var arg\u003");
    assert_eq!(names, ["arg", "u003"]);
    assert_eq!(diagnostics, ["TS1127"]);
}

#[test]
fn an_invalid_escape_after_a_comma_stays_in_the_declaration_list() {
    let (names, diagnostics) = declaration_names(r"var arg, \u003;");
    assert_eq!(names, ["arg", "u003"]);
    assert_eq!(diagnostics, ["TS1127"]);
}

#[test]
fn a_nondigit_escape_tail_is_still_a_declaration() {
    let (names, diagnostics) = declaration_names(r"var arg\uxxxx");
    assert_eq!(names, ["arg", "uxxxx"]);
    assert_eq!(diagnostics, ["TS1127"]);
}

#[test]
fn an_invalid_identifier_start_recovers_to_the_identifier_tail() {
    let (names, diagnostics) = declaration_names(r"var \u0031a;");
    assert_eq!(names, ["u0031a"]);
    assert_eq!(diagnostics, ["TS1127"]);
}

#[test]
fn an_invalid_token_at_the_end_does_not_manufacture_a_declaration() {
    let (names, diagnostics) = declaration_names("var \\");
    assert!(names.is_empty());
    assert_eq!(diagnostics, ["TS1127"]);
}

#[test]
fn ordinary_comma_separated_declarations_are_unchanged() {
    let (names, diagnostics) = declaration_names("var arg, u003;");
    assert_eq!(names, ["arg", "u003"]);
    assert!(diagnostics.is_empty());
}
