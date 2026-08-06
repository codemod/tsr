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
    // already follows. The fixture used an unresolvable annotation "so it stays
    // a gap as more grammar lands" — `bd tsr-eep` falsified that: an unresolved
    // name prints itself now. A **tuple** annotation is genuinely unported and
    // keeps the fixture testing the rule it names.
    assert_eq!(type_of_last("var u: keyof string;\nconst o = { u };"), "error");
    assert_eq!(type_of_last("var u: Unresolved;\nconst o = { u };"), "{ u: Unresolved; }");
}

// `{ a = 1 }` — the destructuring shorthand — is deliberately NOT tested here.
// The guard for it in `check_object_literal` is unobservable today: the only way
// to write the form is as an assignment target, and `check_binary_expression`
// gaps the whole assignment before the literal is checked. Confirmed by making
// that arm answer `never` and watching the result stay `error`. A test would
// pass whether or not the guard existed, which is a decoration rather than
// coverage. The guard stays because it becomes load-bearing the moment
// destructuring lands; the reasoning is beside the code.

// ------------------------------------------------ numeric property names ---

#[test]
fn a_numeric_property_name_prints_unquoted() {
    // `{ 0: number; }` appears 63 times in the baselines and `{ 1: string; }`
    // 30 — numeric names are what an array-like object literal looks like, so
    // this is not an exotic spelling.
    assert_eq!(type_of_last("const o = { 0: 1 };"), "{ 0: number; }");
    assert_eq!(type_of_last("const o = { 1: \"a\" };"), "{ 1: string; }");
}

#[test]
fn a_numeric_name_prints_its_value_not_its_spelling() {
    // The same `normalise_number` the numeric *literal type* uses, which is what
    // stops `1e3` printing one way as a name and another as a type. Writing the
    // source text through unchanged is the plausible shortcut and fails here.
    assert_eq!(type_of_last("const o = { 1.0: 1 };"), "{ 1: number; }");
    assert_eq!(type_of_last("const o = { 1e3: 1 };"), "{ 1000: number; }");
}

#[test]
fn a_non_identifier_string_name_is_re_quoted() {
    // `{ "a-b": string; }` is 6 baseline lines, `{ "resolution-mode": string; }`
    // 24, `{ "a b": number; }` 8, `{ "@ns/dep": string; }` 10.
    assert_eq!(type_of_last(r#"const o = { "a-b": 1 };"#), r#"{ "a-b": number; }"#);
    assert_eq!(type_of_last(r#"const o = { "a b": 1 };"#), r#"{ "a b": number; }"#);
    assert_eq!(type_of_last(r#"const o = { "@ns/dep": "x" };"#), r#"{ "@ns/dep": string; }"#);
}

#[test]
fn the_source_spelling_is_discarded_in_both_directions() {
    // The pair that makes the rule visible: upstream decides quoting from
    // whether the NAME IS IDENTIFIER TEXT, not from how it was written. A
    // quoted-but-valid identifier loses its quotes; an invalid one gains them.
    // Echoing the source spelling passes the first case and is wrong on both.
    assert_eq!(type_of_last(r#"const o = { "a": 1 };"#), "{ a: number; }");
    assert_eq!(type_of_last(r#"const o = { "a-b": 1 };"#), r#"{ "a-b": number; }"#);
}

#[test]
fn a_quoted_name_is_escaped_by_the_same_function_a_string_literal_type_uses() {
    // The parser unescapes and `printing::quote` re-escapes, which is the same
    // round trip `types.rs` pins for a string literal TYPE. Sharing the one
    // function is what keeps a name and a literal from escaping differently.
    assert_eq!(type_of_last(r#"const o = { "a\"b": 1 };"#), r#"{ "a\"b": number; }"#);
    assert_eq!(type_of_last(r#"const o = { "a\\b": 1 };"#), r#"{ "a\\b": number; }"#);
}
