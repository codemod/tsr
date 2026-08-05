//! Index signature members of a **type literal**: `{ [k: string]: number }`.
//!
//! Index signatures landed for `interface` first, where the type prints by name
//! and the members are never rendered. In a type literal there is no name, so
//! the member has to *print* — and until this slice `get_type_from_type_literal`
//! gapped the whole literal on any member it could not render, which made every
//! `{ [k: string]: T }` annotation an `errorType` before any lookup happened.
//!
//! Two things the baselines decide and no amount of reasoning would:
//!
//! - the bracketed name is printed **as written** (`[key: string]`, `[x: string]`);
//! - members are **grouped, not printed in source order** — index signatures
//!   before properties, whatever the source says
//!   (`baselines/reference/submodule/conformance/noUncheckedIndexedAccess.types:377`).

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
fn an_index_signature_prints_its_bracketed_parameter() {
    // 26 corpus lines are exactly `{ [x: string]: number; }`.
    assert_eq!(type_of_annotation("var x: { [x: string]: number };"), "{ [x: string]: number; }");
    assert_eq!(type_of_annotation("var x: { [x: number]: string };"), "{ [x: number]: string; }");
}

#[test]
fn the_parameter_name_is_printed_as_written() {
    // `[key: string]: string` and `[x: string]: string` are both in the
    // baselines, 52 and 70 lines, so the name cannot be normalised to one
    // spelling.
    assert_eq!(
        type_of_annotation("var x: { [key: string]: string };"),
        "{ [key: string]: string; }"
    );
    assert_eq!(type_of_annotation("var x: { [s: string]: boolean };"), "{ [s: string]: boolean; }");
}

#[test]
fn readonly_precedes_the_bracket() {
    // `{ readonly [variable: string]: unknown; }` — 14 corpus lines.
    assert_eq!(
        type_of_annotation("var x: { readonly [k: string]: number };"),
        "{ readonly [k: string]: number; }"
    );
}

#[test]
fn index_signatures_print_before_properties_whatever_the_source_order() {
    // The source has the property *first*; upstream still prints the index
    // signature first, because `createTypeNodesFromResolvedType` emits index
    // infos before properties.
    assert_eq!(
        type_of_annotation("var x: { a: string; b: string; [key: string]: string };"),
        "{ [key: string]: string; a: string; b: string; }"
    );
    // And the same literal written the other way round prints identically.
    assert_eq!(
        type_of_annotation("var x: { [key: string]: string; a: string; b: string };"),
        "{ [key: string]: string; a: string; b: string; }"
    );
}

#[test]
#[ignore = "a call signature member gaps today: `get_signature_from_declaration` has no \
            `CallSignatureDeclaration` arm, in signatures.rs — another workstream's file. \
            Same reason as `signature_members.rs`'s ignored case. Deliberately not rewritten \
            to assert today's `error`, which would pin the inferior answer; the *ordering* it \
            asserts is what this slice added and is covered live by \
            `a_method_groups_with_the_properties_and_not_with_the_call_signatures`."]
fn a_call_signature_prints_before_an_index_signature() {
    // Call signatures, then index infos, then properties — three groups, and
    // this fixture writes them in exactly the reverse order.
    assert_eq!(
        type_of_annotation("var x: { p: number; [k: string]: number; (): number };"),
        "{ (): number; [k: string]: number; p: number; }"
    );
}

#[test]
fn a_method_groups_with_the_properties_and_not_with_the_call_signatures() {
    // A method is a `Member::Signature` in this port but a *property* upstream,
    // so it must stay after the index signature rather than joining the call
    // signatures at the front.
    assert_eq!(
        type_of_annotation("var x: { m(): void; [k: string]: number };"),
        "{ [k: string]: number; m(): void; }"
    );
}

#[test]
fn an_unrenderable_index_signature_gaps_the_whole_literal() {
    // The reject-the-whole-literal rule is unchanged: a member this port cannot
    // render makes the type `error`, never a partial object type.
    //
    // A `symbol` key is not the `string` or `number` intrinsic, which is the
    // same gap `index_info_of` takes for the lookup side — so a literal can
    // never print an index signature that `a[i]` then fails to find.
    assert_eq!(type_of_annotation("var x: { [k: symbol]: number };"), "error");
    // A value type that is itself a gap.
    assert_eq!(type_of_annotation("var x: { [k: string]: keyof T };"), "error");
    // And the property beside it does not leak out on its own.
    assert_eq!(type_of_annotation("var x: { a: string; [k: symbol]: number };"), "error");
}
