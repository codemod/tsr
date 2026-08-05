//! What a shorthand object-literal property must get right.
//!
//! `{ a }` is `{ a: T }` where `T` is the type of the *identifier expression*
//! `a` — `checkShorthandPropertyAssignment` (`checker.go:13689`) runs
//! `checkExpressionForMutableLocation` on the name itself. That is what makes
//! `{ a }` and `{ a: a }` the same type by construction rather than by
//! coincidence, and it is what these tests pin.
//!
//! Expected strings come from `.types` baselines under
//! `vendor/typescript-go/testdata/baselines/reference/submodule`, e.g.
//! `>{ name } : { name: string; }` and `>{ x } : { x: number; }`.

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
fn a_shorthand_property_takes_the_type_of_the_name() {
    // `>{ name } : { name: string; }` and `>{ x } : { x: number; }`.
    assert_eq!(type_of_last("var name: string;\nconst o = { name };"), "{ name: string; }");
    assert_eq!(type_of_last("var x: number;\nconst o = { x };"), "{ x: number; }");
}

#[test]
fn a_shorthand_is_the_same_type_as_the_longhand_it_abbreviates() {
    // The property that makes this a port of `checkShorthandPropertyAssignment`
    // rather than a lookalike: both spellings go through
    // `checkExpressionForMutableLocation` on the same expression, so they cannot
    // drift.
    let shorthand = type_of_last("var a: string;\nconst o = { a };");
    let longhand = type_of_last("var a: string;\nconst o = { a: a };");
    assert_eq!(shorthand, longhand);
    assert_eq!(shorthand, "{ a: string; }");
}

#[test]
fn a_shorthand_widens_at_the_property_boundary_like_any_other_member() {
    // Freshness stops at the property boundary, so a `const` of literal type 1
    // still gives `{ n: number; }`. This is the same
    // `checkExpressionForMutableLocation` widening the longhand path gets, and
    // it is why the shorthand must not simply reuse the symbol's declared type.
    assert_eq!(type_of_last("const n = 1;\nconst o = { n };"), "{ n: number; }");
}

#[test]
fn shorthand_and_longhand_members_mix_in_source_order() {
    assert_eq!(
        type_of_last("var a: string;\nvar b: number;\nconst o = { a, b: b };"),
        "{ a: string; b: number; }"
    );
}

#[test]
fn a_shorthand_naming_something_unresolvable_is_a_gap() {
    // A gap in a member is a gap in the literal, the rule the longhand path
    // already follows. The fixture uses an unresolvable annotation rather than
    // an unported expression form, so it stays a gap as more grammar lands.
    assert_eq!(type_of_last("var u: Unresolved;\nconst o = { u };"), "error");
}

// `{ a = 1 }` — the destructuring shorthand — is deliberately NOT tested here.
// The guard for it in `check_object_literal` is unobservable today: the only way
// to write the form is as an assignment target, and `check_binary_expression`
// gaps the whole assignment before the literal is checked. Confirmed by making
// that arm answer `never` and watching the result stay `error`. A test would
// pass whether or not the guard existed, which is a decoration rather than
// coverage. The guard stays because it becomes load-bearing the moment
// destructuring lands; the reasoning is beside the code.
