//! What the assignable relation must get right.
//!
//! Every fixture is written as **two type annotations** and asks whether the
//! first is assignable to the second, so that the question the test asks is the
//! question `getAssignmentReducedType` and `resolveCall` will ask.
//!
//! Two properties are load-bearing and each has a test whose *only* job is to
//! die under a specific mutation — noted on the test:
//!
//! - assignability is **directional**, so every positive case has a negative
//!   twin in the other direction wherever the relation is not symmetric;
//! - the relation **terminates** on a recursive type — though see
//!   `a_recursive_type_terminates`, whose mutation showed that the cycle guard
//!   is not yet what makes that true.
//!
//! See [`crates/tsr-checker/src/relater.rs`](../src/relater.rs) for what is
//! deliberately gapped: object types that are not the same `TypeId` answer
//! "not related", and that is asserted here too, so the gap is a fact of the
//! test suite rather than an unstated limit.

use tsr_ast::Statement;
use tsr_checker::{Checker, TypeId};
use tsr_core::Arena;

/// Parse, bind and check one source, then answer a question about it.
///
/// Same shape as the harness in `tests/unions.rs`, and for the same reason: the
/// checker borrows the arena, the parse result and the bind result, so all three
/// have to outlive it.
fn with_checker<R>(
    source: &str,
    ask: impl for<'a> FnOnce(&mut Checker<'a, '_>, &[Statement<'a>]) -> R,
) -> R {
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
    ask(&mut checker, parsed.source_file.statements)
}

/// The type of the annotation on the `index`th statement's first declaration.
fn annotation_type<'a>(
    checker: &mut Checker<'a, '_>,
    statements: &[Statement<'a>],
    index: usize,
) -> TypeId {
    let Statement::VariableStatement(statement) = statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    checker.get_type_from_type_node(annotation)
}

/// Is the first declaration's annotated type assignable to the second's?
///
/// The fixture is always two `declare`-free `let` statements, so that the two
/// types are written in source and nothing has to be constructed by hand.
fn assignable(source: &str) -> bool {
    with_checker(source, |checker, statements| {
        let source_type = annotation_type(checker, statements, 0);
        let target_type = annotation_type(checker, statements, 1);
        checker.is_type_assignable_to(source_type, target_type)
    })
}

/// Both directions at once, so a symmetric bug cannot pass as a correct answer.
fn both_ways(source: &str) -> (bool, bool) {
    with_checker(source, |checker, statements| {
        let a = annotation_type(checker, statements, 0);
        let b = annotation_type(checker, statements, 1);
        (checker.is_type_assignable_to(a, b), checker.is_type_assignable_to(b, a))
    })
}

/// A literal is assignable to its base primitive, and not the other way round.
///
/// **Directional**, which is the whole point: a relation implemented as "do the
/// flags overlap" passes the first half and fails the second.
///
/// Reddened by: replacing the `STRING_LIKE`/`STRING` arm's operands with
/// `s.intersects(STRING) && t.intersects(STRING_LIKE)` — the reversed arm makes
/// the second assertion true.
#[test]
fn a_string_literal_is_assignable_to_string_but_not_conversely() {
    let (forward, backward) = both_ways("let a: \"x\"; let b: string;");
    assert!(forward, "\"x\" -> string");
    assert!(!backward, "string -> \"x\"");
}

/// The same shape for numbers and bigints, which have their own arms.
///
/// Reddened by: deleting the `NUMBER_LIKE`/`NUMBER` arm.
#[test]
fn a_number_literal_is_assignable_to_number_but_not_conversely() {
    let (forward, backward) = both_ways("let a: 1; let b: number;");
    assert!(forward, "1 -> number");
    assert!(!backward, "number -> 1");
}

/// `never` is assignable to everything; everything is not assignable to `never`.
///
/// **The arm order is NOT tested here, and cannot be in this port.** The claim
/// this test originally carried — that hoisting `t.intersects(NEVER) => false`
/// above `s.intersects(NEVER) => true` flips `never -> never` — was run and came
/// back **green**: `never` is one interned type, so `never -> never` is settled
/// by the `source == target` identity check before `is_simple_type_related_to`
/// is ever called. Distinguishing the two orders needs two distinct `never`
/// types, which this port does not have. Recorded rather than deleted because
/// the order still matters upstream and will matter here the day it does.
///
/// What *is* verified: the three answers below.
///
/// Reddened by: deleting the `s.intersects(NEVER)` half of the first arm, which
/// flips `never -> string`.
#[test]
fn never_is_assignable_to_everything_including_never() {
    assert!(assignable("let a: never; let b: string;"), "never -> string");
    assert!(assignable("let a: never; let b: never;"), "never -> never");
    assert!(!assignable("let a: string; let b: never;"), "string -> never");
}

/// `any` relates in both directions; `unknown` only as a target.
///
/// Reddened by: deleting the `t.intersects(UNKNOWN)` arm, which flips the
/// second assertion.
#[test]
fn any_relates_both_ways_and_unknown_only_as_a_target() {
    let (forward, backward) = both_ways("let a: string; let b: any;");
    assert!(forward, "string -> any");
    assert!(backward, "any -> string");
    assert!(assignable("let a: string; let b: unknown;"), "string -> unknown");
    assert!(!assignable("let a: unknown; let b: string;"), "unknown -> string");
}

/// A source union needs *every* constituent related; a target union needs *one*.
///
/// The two halves are the two quantifiers, and swapping either one is a real bug
/// that this test is shaped to catch: `("x" | 1) -> string` must be **false**
/// because `1` is not a string, while `"x" -> (string | number)` must be true.
///
/// Reddened by: changing the source-union arm's `.all(` to `.any(`, which makes
/// the second assertion true.
#[test]
fn a_source_union_is_universal_and_a_target_union_existential() {
    assert!(assignable("let a: \"x\" | \"y\"; let b: string;"), "(\"x\"|\"y\") -> string");
    assert!(!assignable("let a: \"x\" | 1; let b: string;"), "(\"x\"|1) -> string");
    assert!(assignable("let a: \"x\"; let b: string | number;"), "\"x\" -> (string|number)");
    assert!(!assignable("let a: boolean; let b: string | number;"), "boolean -> (string|number)");
}

/// A source union is decomposed *before* a target union is searched.
///
/// This is the ordering claim in `structured_type_related_to`'s doc comment, and
/// it is not decoration: union interning makes `"x" | "y"` a single type that
/// does **not** appear in `("x" | "y" | "z")`'s constituent list, so a relation
/// that searched the target first would answer `false` for a pair that is
/// plainly assignable.
///
/// Reddened by: moving the target-union arm above the source-union arm.
#[test]
fn a_union_is_assignable_to_a_wider_union() {
    let (forward, backward) = both_ways("let a: \"x\" | \"y\"; let b: \"x\" | \"y\" | \"z\";");
    assert!(forward, "(\"x\"|\"y\") -> (\"x\"|\"y\"|\"z\")");
    assert!(!backward, "(\"x\"|\"y\"|\"z\") -> (\"x\"|\"y\")");
}

/// A target intersection needs every constituent; a source intersection needs one.
///
/// **The obvious fixture does not work, and that is the finding.** Written as
/// `string & number` this test was decoration: `crate::intersections` reduces an
/// intersection of disjoint primitives to `never`, so the target was `never` and
/// the intersection arm was never reached — the `.all(` → `.any(` mutation left
/// it green. Two interfaces do not reduce, so `I & J` is a real
/// `TypeData::Intersection` and the arm actually runs. Verified by probing what
/// `let a: string & number` resolves to, not by reading the reduction code.
///
/// Reddened by: changing the target-intersection arm's `.all(` to `.any(`, which
/// makes `I -> (I & J)` true; and by changing the source-intersection arm's
/// `.any(` to `.all(`, which makes `(I & J) -> I` false.
#[test]
fn a_target_intersection_is_universal_and_a_source_intersection_existential() {
    let source = "interface I {}\ninterface J {}\nlet a: I;\nlet b: I & J;";
    with_checker(source, |checker, statements| {
        let i = annotation_type(checker, statements, 2);
        let both = annotation_type(checker, statements, 3);
        assert!(!checker.is_type_assignable_to(i, both), "I -> (I & J) must fail on the J half");
        assert!(checker.is_type_assignable_to(both, i), "(I & J) -> I");
    });
}

/// `undefined` and `null` are *not* interchangeable under the strict reading.
///
/// This is the `strictNullChecks`-on assumption the module documents, asserted
/// so that anyone who later plumbs the option through has a test that tells them
/// which reading is currently hard-coded.
///
/// Reddened by: adding `|| t.intersects(TypeFlags::NULL)` to the `UNDEFINED`
/// arm's target test.
#[test]
fn undefined_and_null_do_not_relate_to_each_other() {
    let (forward, backward) = both_ways("let a: undefined; let b: null;");
    assert!(!forward, "undefined -> null");
    assert!(!backward, "null -> undefined");
    assert!(assignable("let a: undefined; let b: void;"), "undefined -> void");
}

/// Object types relate only to themselves, and this is a **gap**, not a rule.
///
/// `interface I { x: string }` and a structurally identical `interface J` are
/// assignable in TypeScript and answer `false` here, because structural
/// comparison is not ported. The test asserts the current answer so that the day
/// structural comparison lands, it goes red and is *deleted* rather than
/// quietly contradicted.
///
/// Reddened by: nothing — this is a characterisation test of a gap, labelled as
/// one rather than counted as a verified behaviour. Mutating
/// `structured_type_related_to`'s trailing `false` to `true` was tried and left
/// it **green**, which located the gap precisely: a pair of plain object types
/// is rejected by the composite gate in `is_related_to` and never reaches the
/// structural arm at all.
#[test]
fn two_structurally_identical_interfaces_are_a_gap() {
    let source = "interface I { x: string }\ninterface J { x: string }\n\
                  let a: I;\nlet b: J;\nlet c: I;";
    with_checker(source, |checker, statements| {
        let i = annotation_type(checker, statements, 2);
        let j = annotation_type(checker, statements, 3);
        let i_again = annotation_type(checker, statements, 4);
        assert!(checker.is_type_assignable_to(i, i_again), "a type is assignable to itself");
        assert!(!checker.is_type_assignable_to(i, j), "GAP: structural comparison is not ported");
    });
}

/// A self-referential type terminates — **but the guard is not what makes it do
/// so, and that is the finding.**
///
/// This test was written to redden under "remove the
/// `self.results.insert((source, target), true)` that parks the pair before
/// recursing". The mutation was run and the test stayed **green, in
/// milliseconds**. The reason is structural: the only cycles in a type graph run
/// through an object type's *members*, and this module does not walk members
/// (see `relater.rs`). `interface I { x: I | string }` is a cycle in the type
/// graph and not a cycle in this walk, because the walk stops at `I`.
///
/// So the cycle cache and [`tsr_checker::relater::MAX_DEPTH`] are currently
/// **unexercised**. They are kept, not deleted, because the loop they guard
/// appears the moment structural comparison lands — but they are recorded here
/// as untested rather than counted as verified, which is the distinction
/// `bd tsr-el3.2` exists to force.
///
/// The assertions below are real; only the mutation claim was wrong.
#[test]
fn a_recursive_type_terminates() {
    let source = "interface I { x: I | string }\nlet a: I | string;\nlet b: I | string | number;";
    with_checker(source, |checker, statements| {
        // Statement 0 is the interface, so the two annotations are 1 and 2.
        let narrow = annotation_type(checker, statements, 1);
        let wide = annotation_type(checker, statements, 2);
        assert!(checker.is_type_assignable_to(narrow, wide), "(I|string) -> (I|string|number)");
        assert!(!checker.is_type_assignable_to(wide, narrow), "(I|string|number) -> (I|string)");
    });
}
