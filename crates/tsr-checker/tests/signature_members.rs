//! Signature members of a type literal: `{ m(): void }`, `{ (): number }`.
//!
//! The spelling is the point. A **method member** prints `m(): void`, with a
//! colon; a property holding a function type prints `m: () => void`, with an
//! arrow. Same signature, two forms, chosen by position — so the fixtures below
//! assert the colon form and would pass on neither renderer by accident.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Signature members print with a colon, not an arrow".

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first statement's annotation.
fn type_of_annotation(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

#[test]
fn a_method_member_prints_with_a_colon_and_not_an_arrow() {
    // `{ foo(): void; }` is what upstream records. `{ foo: () => void; }` is a
    // *different* member — a property holding a function type — and rendering
    // one as the other is the single thing this slice has to get right.
    assert_eq!(type_of_annotation("var x: { foo(): void };"), "{ foo(): void; }");
    assert_eq!(
        type_of_annotation("var x: { fn(a: number): string };"),
        "{ fn(a: number): string; }"
    );
}

#[test]
fn a_method_member_carries_everything_a_signature_can() {
    assert_eq!(type_of_annotation("var x: { m(a?: string): void };"), "{ m(a?: string): void; }");
    assert_eq!(type_of_annotation("var x: { m<T>(a: T): T };"), "{ m<T>(a: T): T; }");
    // No return annotation and no body is `any`, which is a computed answer
    // rather than a gap — upstream's rule for an ambient signature.
    assert_eq!(type_of_annotation("var x: { m() };"), "{ m(): any; }");
}

#[test]
fn properties_and_methods_mix_in_source_order() {
    // Members keep declaration order, as they do for an object literal, so the
    // two spellings appear side by side exactly as written.
    assert_eq!(type_of_annotation("var x: { a: string; m(): void };"), "{ a: string; m(): void; }");
    assert_eq!(type_of_annotation("var x: { m(): void; a: string };"), "{ m(): void; a: string; }");
}

/// **§930.1 RETIRED this test's subject, and the name is kept so that is
/// visible.** Every assertion it held has moved, in three steps:
///
/// - `bd tsr-eep` — an unresolved *name* prints itself.
/// - §929 — an unresolvable *parameter or return annotation* keeps its written
///   spelling instead of collapsing the signature.
/// - §930/§930.1 — an unresolvable *member annotation* does the same, and an
///   **accessor** is now a property rather than a whole-literal decline.
///
/// Upstream resolves a get/set pair to one property symbol and prints it as a
/// property: `{ get a(): string }` is `{ readonly a: string; }`, the `readonly`
/// coming from there being no setter. Measured: **57 `WRONG->RIGHT` + 17
/// `GAP->RIGHT` against 5 `GAP->WRONG`, zero `RIGHT->WRONG`.**
///
/// **`get_type_from_type_literal`'s all-or-nothing rule itself still stands** —
/// it is the reason the remaining `return error` arms exist. What no longer
/// stands is that this file had an example of it.
#[test]
fn a_member_this_port_still_cannot_render_gaps_the_whole_literal() {
    assert_eq!(type_of_annotation("var x: { get a(): string };"), "{ readonly a: string; }");
    assert_eq!(
        type_of_annotation("var x: { a: string; get b(): string };"),
        "{ a: string; readonly b: string; }"
    );
    // A method whose parameter type is a gap used to take the literal with it.
    // **§929 ended that**: the parameter keeps its written spelling with an `any`
    // type, because upstream's carries `errorType` and the node builder reuses
    // the written annotation node. +442 on the corpus with zero
    // `RIGHT->WRONG`.
    //
    // The two accessor assertions above still carry the rule this test is named
    // for; this line now records the shape §929 produces instead.
    assert_eq!(
        type_of_annotation("var x: { m(a: keyof string): void };"),
        "{ m(a: keyof string): void; }"
    );
}

#[test]
#[ignore = "blocked on bd tsr-qk9: signature_parts_of has no arm for \
            CallSignatureDeclaration or ConstructSignatureDeclaration, so \
            get_signature_from_declaration returns None. The dispatch and the \
            rendering are in place and tested by the method cases; two match \
            arms in signatures.rs — another workstream's file — make these pass. \
            Deliberately not rewritten to assert today's `error`, which would \
            pin the inferior answer."]
fn a_call_or_construct_signature_member_prints_without_a_name() {
    assert_eq!(type_of_annotation("var x: { (): number };"), "{ (): number; }");
    assert_eq!(type_of_annotation("var x: { (a: string): number };"), "{ (a: string): number; }");
    assert_eq!(type_of_annotation("class C {}\nvar x: { new (): C };"), "{ new (): C; }");
    // Two call signatures are two members, not one.
    assert_eq!(type_of_annotation("var x: { (): void; (): void };"), "{ (): void; (): void; }");
}

/// §930.1's regression legs: the accessor arm must not print a member twice,
/// and `readonly` must come from the *absence of a setter* rather than from
/// get-ness.
#[test]
fn a_get_set_pair_is_one_property_and_is_not_readonly() {
    assert_eq!(
        type_of_annotation("var x: { get a(): string; set a(v: string); };"),
        "{ a: string; }"
    );
    // A setter alone is a writable property, not a readonly one.
    assert_eq!(type_of_annotation("var x: { set b(v: number); };"), "{ b: number; }");
}
