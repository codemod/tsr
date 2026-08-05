//! The type of an object-literal **member symbol**.
//!
//! `var o = { a: 1 }; o.a` answered `errorType` even though the literal's own
//! type printed `{ a: number; }` correctly. The lookup was never the problem —
//! `objects.rs` gives the literal an `__object` symbol as its members table, so
//! `get_property_of_type` finds `a`. What failed was the step after:
//! `get_type_of_symbol` on a PROPERTY whose declaration is a `PropertyAssignment`,
//! which the variable/parameter/property worker had no arm for.
//!
//! Measured at 8,549 lines across 1,571 cases; top ten cases are 24.9% of the
//! row, so it is broadly spread rather than one file.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement.
fn type_of_last(source: &str) -> String {
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
fn a_member_access_widens_at_the_property_boundary() {
    // **The failure mode this arm had to avoid.** Freshness stops at the
    // property boundary, so the member is `number`, not `1`. Typing the
    // initialiser without the widening would turn 2,500 numeric-member gaps
    // into 2,500 wrong answers, which is a worse trade than leaving them.
    assert_eq!(type_of_last("var o = { a: 1 };\nvar x = o.a;"), "number");
    assert_eq!(type_of_last("var o = { a: \"s\" };\nvar x = o.a;"), "string");
    assert_eq!(type_of_last("var o = { a: true };\nvar x = o.a;"), "boolean");
}

#[test]
fn const_does_not_change_the_member_type() {
    // `as const` is what keeps `1`, and it is unported. A bare `const` binding
    // does not: the widening is a property of the property boundary, not of how
    // the object was bound.
    assert_eq!(type_of_last("const o = { a: 1 };\nvar x = o.a;"), "number");
}

#[test]
fn a_shorthand_member_symbol_has_a_type_too() {
    // `checkShorthandPropertyAssignment` (`checker.go:16613`) reads the NAME,
    // which is the same rule `objects.rs` follows for the literal's text.
    assert_eq!(type_of_last("var n = 1;\nvar o = { n };\nvar x = o.n;"), "number");
}

#[test]
fn a_nested_member_is_the_inner_object_type() {
    assert_eq!(type_of_last("var o = { a: { b: 1 } };\nvar x = o.a;"), "{ b: number; }");
}

#[test]
fn the_member_symbol_agrees_with_the_literals_printed_text() {
    // The anti-drift property, and the reason this arm calls the *same*
    // `check_expression_for_mutable_location` that `objects.rs` calls rather
    // than a second rule that happens to agree today. If these two ever
    // disagree, one of the call sites has grown its own widening.
    let literal = type_of_last("var q = { a: 1, b: \"s\" };");
    assert_eq!(literal, "{ a: number; b: string; }");
    assert_eq!(type_of_last("var o = { a: 1, b: \"s\" };\nvar x = o.a;"), "number");
    assert_eq!(type_of_last("var o = { a: 1, b: \"s\" };\nvar x = o.b;"), "string");
}

#[test]
fn a_member_whose_initialiser_this_port_cannot_type_is_still_a_gap() {
    // The arm inherits the initialiser's answer, gap included. The fixture uses
    // an unresolvable annotation so it stays a gap however much grammar lands.
    assert_eq!(type_of_last("var u: Unresolved;\nvar o = { a: u };\nvar x = o.a;"), "error");
}
