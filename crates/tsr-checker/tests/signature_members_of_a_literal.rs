//! Call and construct signatures as **members** of a type literal:
//! `{ (): string }` and `{ new (): C }`.
//!
//! `get_type_from_type_literal` has always known how to print these — it writes
//! the `new ` prefix itself — but `signature_parts_of` had no arm for either
//! declaration, so `get_signature_from_declaration` answered `None` and the
//! literal's all-or-nothing rule gapped the **whole** type. The prefix was
//! correct and unreachable.
//!
//! # The distinction these tests exist to pin
//!
//! A construct **type node** (`new () => T`) is still a gap, deliberately:
//! [`Signature`] carries no construct flag, so printing one would emit
//! `() => T` where upstream emits `new () => T`. The **member** form has no such
//! problem because the caller supplies the prefix. Those two live one word
//! apart in the source and are easy to conflate later, which is why the pair is
//! asserted together rather than in separate tests.
//!
//! Spellings are taken from the baselines, not reasoned about:
//! `{ new (): c1; }` appears 18 times in
//! `baselines/reference/submodule` and `{ (): string; ... }` is recorded on
//! `>foo : { (): string; (): string; }`.

use tsr_ast::{NodeMap, NodeTable, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The minimal globals an array type needs, as `tests/arrays.rs` uses.
const LIB: &str = "interface Array<T> {}\ninterface ReadonlyArray<T> {}\n";

/// Bind `LIB` and `source` as one program, then print the annotation on
/// `source`'s last variable statement.
fn type_of_last_annotation(source: &str) -> String {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();

    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );

    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );

    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let statement = file
        .source_file
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            Statement::VariableStatement(statement) => Some(statement),
            _ => None,
        })
        .expect("a variable statement");
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

#[test]
fn a_construct_signature_member_prints_its_new_prefix() {
    // `{ new (): c1; }` is recorded 18 times in the baselines.
    assert_eq!(
        type_of_last_annotation("interface c1 {}\nvar x: { new (): c1 };"),
        "{ new (): c1; }"
    );
    // With parameters, and beside a property — the member ordering rule already
    // in force puts signatures first.
    assert_eq!(
        type_of_last_annotation("var x: { new (n: number): boolean; p: string };"),
        "{ new (n: number): boolean; p: string; }"
    );
}

#[test]
fn a_call_signature_member_prints_without_one() {
    assert_eq!(type_of_last_annotation("var x: { (): string };"), "{ (): string; }");
    // Two call signatures in one literal, the shape `>foo : { (): string;
    // (): string; }` records.
    assert_eq!(
        type_of_last_annotation("var x: { (): string; (): string };"),
        "{ (): string; (): string; }"
    );
    // A call and a construct signature together, so the prefix is asserted to
    // apply to one and not the other.
    assert_eq!(
        type_of_last_annotation("var x: { (): string; new (): string };"),
        "{ (): string; new (): string; }"
    );
}

#[test]
fn the_construct_type_node_is_still_a_gap_and_that_is_the_distinction() {
    // `Signature` has no construct flag, so a construct *type node* would print
    // `() => c1` where upstream prints `new () => c1` — a wrong answer rather
    // than a gap. The member form above is answerable only because its caller
    // supplies the `new `. If someone ever "unifies" the two, this assertion is
    // what fails.
    assert_eq!(type_of_last_annotation("interface c1 {}\nvar x: new () => c1;"), "error");
    // The function type node remains answered, so the gap above is specific to
    // the construct form and not to type nodes generally.
    assert_eq!(type_of_last_annotation("interface c1 {}\nvar x: () => c1;"), "() => c1");
}

#[test]
fn a_literal_with_a_member_this_port_cannot_render_still_gaps() {
    // The arms remove *one* reason for a literal to gap, not all of them. A
    // construct signature whose return type is a gap takes the whole literal
    // with it, on `get_type_from_type_literal`'s all-or-nothing rule.
    // `unknownThing` was the gap here until `bd tsr-eep` made an unresolved
    // name print itself. A **tuple** return is genuinely unported and keeps the
    // all-or-nothing rule under test.
    assert_eq!(type_of_last_annotation("var x: { new (): [string] };"), "error");
    assert_eq!(type_of_last_annotation("var x: { (): [string] };"), "error");
    assert_eq!(
        type_of_last_annotation("var x: { new (): unknownThing };"),
        "{ new (): unknownThing; }"
    );
}

// DELIBERATELY NOT TESTED HERE: the alias-mediated chain, `p: X[]` where
// `type X = { new (): c1 }`.
//
// It is what motivated this slice — 98.5% of every `Alias[]` annotation in the
// corpus — and these arms are **not sufficient** for it. The literal now
// resolves, so the alias resolves, so the array resolves; but a non-generic
// type alias over a type *literal* does not take the alias's name here, and the
// result prints `{ new (): c1; }[]` where upstream prints `X[]`.
//
// That is a pre-existing defect and not one these arms introduced: the same
// `type X = { p: string }; var x: X[]` already printed `{ p: string; }[]`
// before, with no signature member anywhere in it. What the arms do is widen
// its reach — those 2,003 lines move from *gap* to *wrong answer*, which is
// gradient-neutral and discipline-negative.
//
// No assertion is written for it in either direction: asserting `X[]` would
// fail, and asserting `{ new (): c1; }[]` would pin an answer this port knows
// to be wrong. `get_type_from_union_type_node` and its intersection twin
// already implement exactly the naming rule that is missing
// (`crate::declared`, via `alias_symbol_for_type_node`); extending it to a type
// literal is the item that makes this chain pay, and it lives in a file this
// slice does not own.
