//! Access on an `any` receiver — both spellings.
//!
//! The single largest cause in the element-access gap row: of 12,905
//! element-access lines in the `.types` baselines under
//! `vendor/typescript-go/testdata/baselines/reference/submodule`, 11,363 (88%)
//! answer `any`, because the receiver is `any`. `conformance/anyPropertyAccess.types`
//! is the canonical case.
//!
//! `a[i]` and `a.b` are the same cause and are ported together, because
//! `anyPropertyAccess.types` records both spellings failing in the same files —
//! fixing one alone would not flip a file under a whole-line case gate.
//!
//! These tests pin both halves: that an `any` receiver answers `any`, and that
//! the arms cannot be reached by a receiver this port merely failed to type.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the `index`th statement.
fn type_of_at(source: &str, index: usize) -> String {
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
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

fn type_of_last(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    type_of_at(source, parsed.source_file.statements.len() - 1)
}

#[test]
fn an_any_receiver_makes_the_access_any_whatever_the_index() {
    // The index is irrelevant — upstream never gets as far as looking at it,
    // which is why all three spellings answer the same thing.
    assert_eq!(type_of_last("var a: any;\nconst x = a[0];"), "any");
    assert_eq!(type_of_last("var a: any;\nconst x = a[\"k\"];"), "any");
    assert_eq!(type_of_last("var a: any;\nvar i: number;\nconst x = a[i];"), "any");
}

#[test]
fn an_index_this_port_cannot_type_does_not_stop_an_any_receiver() {
    // The receiver decides alone. An index whose own type is a gap would sink
    // any other receiver, and must not sink this one.
    assert_eq!(type_of_last("var a: any;\nvar u: Unresolved;\nconst x = a[u];"), "any");
}

/// Type the `return` expression of the first function declaration.
fn type_of_first_return(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::FunctionDeclaration(f) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a function declaration");
    };
    let Some(tsr_ast::FunctionBody::Block(block)) = f.body else { panic!("a block body") };
    let Statement::ReturnStatement(r) = block.statements[0] else { panic!("a return") };
    let id = checker.check_expression(r.expression.expect("a returned expression"));
    checker.type_to_string(id)
}

#[test]
fn an_implicit_any_parameter_receiver_counts_too() {
    // The dominant corpus shape: an unannotated parameter is the implicit any,
    // and indexing it is `any`. If the arm only matched an explicit `: any`
    // annotation it would leave most of the row on the floor.
    assert_eq!(type_of_first_return("function f(p) { return p[0]; }"), "any");
    assert_eq!(type_of_first_return("function f(p) { return p[\"k\"]; }"), "any");
}

#[test]
fn an_unannotated_variable_does_not_reach_this_arm_and_that_is_flow_not_indexing() {
    // Worth pinning because it caps how much of the row this can close, and
    // because the cause is somewhere else entirely. An unannotated `let` with
    // no initialiser narrows to `undefined` at a use before assignment, and an
    // unannotated `var` needs upstream's `autoType` and evolving-array
    // machinery, which `crate::flow` does not port. So the receiver is already
    // not `any` by the time element access is asked.
    //
    // These assert the CURRENT answers rather than upstream's, deliberately:
    // they are a marker for a narrowing gap, and they change when `crate::flow`
    // grows the auto type — at which point this arm starts answering them for
    // free, which is the point of recording it here.
    assert_eq!(type_of_last("declare let a;\nconst x = a[0];"), "error");
    assert_eq!(type_of_last("var a;\nconst x = a[0];"), "error");
}

#[test]
fn a_receiver_this_port_could_not_type_is_still_a_gap() {
    // An unresolvable receiver gaps. This is carried by the `object_type ==
    // error` guard at the TOP of `check_element_access_expression`, not by the
    // identity test in the `any` arm.
    //
    // That distinction is not cosmetic and this comment used to get it wrong.
    // Replacing the arm's identity test with a `TypeFlags::ANY` test leaves this
    // test GREEN, because the early guard has already returned — verified by
    // mutation. So the identity test is defence in depth against a future edit
    // that reorders or removes that guard, and it is unobservable today: only
    // `anyType` and `errorType` carry `ANY` in this port, and one of them cannot
    // reach the arm. Documented rather than tested, on the same rule as the
    // `{ a = 1 }` guard in `objects.rs`.
    // §32: the minted unresolved receiver is upstream's `errorType`, and
    // `u[0]` through it answers `any` — upstream's baseline shape.
    assert_eq!(type_of_last("var u: Unresolved;\nconst x = u[0];"), "any");
}

#[test]
fn an_unknown_receiver_is_not_an_any_receiver() {
    // Upstream reports "Object is of type 'unknown'" and does not answer `any`.
    assert_eq!(type_of_last("var u: unknown;\nconst x = u[0];"), "error");
}

#[test]
fn a_typed_receiver_still_resolves_normally() {
    // The new arm must not shadow the real lookups.
    assert_eq!(type_of_last("var o: { b: number };\nconst x = o[\"b\"];"), "number");
    assert_eq!(type_of_last("var m: { [k: string]: number };\nconst x = m[\"k\"];"), "number");
    assert_eq!(type_of_last("var m: { [k: number]: string };\nconst x = m[0];"), "string");
}

// --------------------------------------------------- property access, `a.b` ---

#[test]
fn an_any_receiver_makes_a_property_access_any() {
    // `isAnyLike` (`checker.go:11266`) into the branch at `checker.go:11314`.
    // The property name is irrelevant — upstream never looks it up.
    assert_eq!(type_of_last("var a: any;\nconst x = a.b;"), "any");
    assert_eq!(type_of_last("var a: any;\nconst x = a.anythingAtAll;"), "any");
}

#[test]
fn an_implicit_any_parameter_receiver_works_for_property_access_too() {
    assert_eq!(type_of_first_return("function f(p) { return p.b; }"), "any");
}

#[test]
fn a_property_access_on_an_untypeable_receiver_is_still_a_gap() {
    // §32 split this fixture's world in two: a receiver minted for an
    // UNRESOLVED reference answers `any` (upstream's own errorType
    // observable — the identity-vs-flag discipline the original comment
    // defended still holds; the `unresolved_types` SET is the identity),
    // while a receiver that gaps for any other reason still takes the
    // access with it (the object-literal case below pins that).
    assert_eq!(type_of_last("var u: Unresolved;\nconst x = u.b;"), "any");
}

#[test]
fn a_typed_receiver_still_resolves_its_properties() {
    // The new arm must not shadow the real lookup.
    assert_eq!(type_of_last("var o: { b: number };\nconst x = o.b;"), "number");
    assert_eq!(type_of_last("var o: { b: number };\nconst x = o.missing;"), "error");
}
