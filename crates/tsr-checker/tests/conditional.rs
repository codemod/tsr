//! What `c ? a : b` must get right.
//!
//! The answer is the union of the two branches, but built at a reduction this
//! port does not have — see `check_conditional_expression`. These tests pin both
//! halves of that: the shapes where `UnionReductionLiteral` and
//! `UnionReductionSubtype` must agree, and the shape where they demonstrably do
//! not and the answer is therefore a gap.
//!
//! Every expected string was read out of a real `.types` baseline under
//! `vendor/typescript-go/testdata/baselines/reference/submodule` first.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the `index`th statement.
fn type_of_initialiser_at(source: &str, index: usize) -> String {
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

    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initialiser = declaration.initializer.expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

/// Type the initialiser of the last statement, after any set-up declarations.
fn type_of_last(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let last = parsed.source_file.statements.len() - 1;
    type_of_initialiser_at(source, last)
}

#[test]
fn two_literal_branches_are_the_union_of_both() {
    // `>bob ? 1 : 2 : 1 | 2` and `>cond ? 0 : 1 : 0 | 1` in the baselines. Note
    // the second: the printed order is the union's sort order, not source order,
    // so answering the branches in the order written would still be wrong.
    assert_eq!(type_of_last("var c: boolean;\nconst x = c ? 1 : 2;"), "1 | 2");
    assert_eq!(type_of_last("var c: boolean;\nconst x = c ? 1 : 0;"), "0 | 1");
}

#[test]
fn two_branches_of_one_type_collapse_to_that_type() {
    // `>e ? c : d : string`. `getUnionTypeEx` returns the constituent itself
    // when there is one after deduplication, which is upstream's own rule and
    // not a reduction difference.
    assert_eq!(
        type_of_last("var c: boolean;\nvar a: string;\nvar b: string;\nconst x = c ? a : b;"),
        "string"
    );
}

#[test]
fn a_literal_branch_beside_its_base_primitive_reduces_to_the_primitive() {
    // This is the case that makes the fence sound rather than merely cautious:
    // literal reduction already removes `1` when `number` is present, so
    // `UnionReductionSubtype` has nothing left to do and the two reductions
    // agree. If they did not, this would have to be a gap too.
    assert_eq!(type_of_last("var c: boolean;\nvar n: number;\nconst x = c ? 1 : n;"), "number");
}

#[test]
fn branches_of_different_primitives_are_a_two_member_union() {
    assert_eq!(
        type_of_last("var c: boolean;\nvar s: string;\nvar n: number;\nconst x = c ? s : n;"),
        "string | number"
    );
}

#[test]
fn an_object_branch_is_a_gap_because_subtype_reduction_would_collapse_it() {
    // `>true ? a : b : { Foo?: Base; }` — upstream prints ONE object type where
    // this port would print a two-member union, because `UnionReductionSubtype`
    // drops a constituent assignable to another. Emitting `D | B` here would be
    // a wrong line on every such baseline, so it gaps.
    //
    // **Two DISTINCT types, one assignable to the other.** This fixture used to
    // be `var p: A; var q: A;` — two *identical* types, which plain
    // deduplication collapses without any reduction at all, as the neighbouring
    // `two_branches_of_one_type_collapse_to_that_type` demonstrates for
    // `string`. So it never exercised the mechanism its own comment named, and
    // would have passed under an implementation that simply refused every
    // object-typed branch. Repaired after that was measured rather than noticed.
    // **The fifteenth unported-stand-in fixture to come due, and this one
    // asserted the mechanism itself.** Until the ninth session this read
    // `error`; the decidability-gated `removeSubtypes`
    // (`checker-notes-assign.md` §9) now removes `D` — a strict subtype of
    // `B` — exactly as upstream's `UnionReductionSubtype` does, and the
    // conditional answers the reduced union.
    assert_eq!(
        type_of_last(
            "var c: boolean;\ninterface B { a: string; }\ninterface D extends B { b: string; }\n\
             var p: D;\nvar q: B;\nconst x = c ? p : q;"
        ),
        "B"
    );
    // Two identical object types need no reduction — dedup alone answers `A`.
    // This assertion read `error` until the ninth session and its own comment
    // called the conservatism "the one that would be cheapest to close first";
    // `checker-notes-assign.md` §7 closed it, and the identity test runs on
    // the regular (freshness-stripped) forms so a fresh and a regular spelling
    // of one literal count as one type.
    assert_eq!(
        type_of_last(
            "var c: boolean;\ninterface A { a: string; }\nvar p: A;\nvar q: A;\nconst x = c ? p : q;"
        ),
        "A"
    );
    // The control that makes both assertions mean something: dedup demonstrably
    // works for a non-object type, so neither line above is passing because
    // conditionals are broken.
    assert_eq!(
        type_of_last("var c: boolean;\nvar p: string;\nvar q: string;\nconst x = c ? p : q;"),
        "string"
    );
}

#[test]
fn a_gap_in_a_branch_is_a_gap_in_the_conditional() {
    // Printing the branch we could type would claim the conditional has that
    // type, which is a wrong line rather than a missing one.
    //
    // The unportable branch is a variable with an unresolvable annotation, so
    // this exercises the `branch == error` arm specifically. An object-typed
    // branch would also gap, but through the subtype-reduction fence instead —
    // a different rule, tested above.
    assert_eq!(type_of_last("var c: boolean;\nvar u: Unresolved;\nconst x = c ? 1 : u;"), "error");
}

#[test]
fn a_nullable_branch_is_kept_and_not_reduced_away() {
    // Neither reduction removes `undefined` beside a primitive —
    // `reduceVoidUndefined` is a `UnionReductionSubtype` flag that
    // `getUnionTypeEx` does not set here — so this stays a two-member union.
    assert_eq!(
        type_of_last("var c: boolean;\nvar s: string;\nconst x = c ? s : null;"),
        "string | null"
    );
}
