//! `T` versus `T | undefined`: what a `?` adds, and what it must not.
//!
//! The risk in this rule is **over-application**, not under-application. It
//! turns a right answer into a wrong one wherever it fires and should not, and
//! it fires on a syntactic token that appears in several places meaning several
//! things. So most of these are negative: `!` is not `?`, a defaulted parameter
//! is not optional, and an already-optional type does not gain a second
//! `undefined`.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the local `name`, wherever in the file it is declared.
fn type_of(source: &str, name: &str) -> String {
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
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("`{name}` is declared nowhere");
}

/// The printed type of a member of a top-level class or interface.
fn type_of_member(source: &str, owner: &str, member: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::HasNodeId::node_id(parsed.source_file).expect("registered");
    let owner = bound.lookup_local(root, owner).expect("the owner is declared at the top level");
    let member = *bound
        .symbols()
        .get(owner)
        .members
        .get(member)
        .expect("the member is declared on the owner");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let ty = checker.get_type_of_symbol(member);
    checker.type_to_string(ty)
}

#[test]
fn an_optional_property_is_the_annotation_or_undefined() {
    assert_eq!(type_of_member("interface I { p?: string }", "I", "p"), "string | undefined");
    assert_eq!(type_of_member("class C { p?: number; }", "C", "p"), "number | undefined");
}

#[test]
fn a_required_property_is_left_alone() {
    // The other half of the same rule, and the one that catches a version that
    // appends `| undefined` unconditionally.
    assert_eq!(type_of_member("interface I { p: string }", "I", "p"), "string");
    assert_eq!(type_of_member("class C { p: number; }", "C", "p"), "number");
}

#[test]
fn an_optional_parameter_is_the_annotation_or_undefined() {
    assert_eq!(type_of("function f(p?: string) { return p; }", "p"), "string | undefined");
}

#[test]
fn a_definite_assignment_assertion_is_not_optionality() {
    // `p!: string` and `p?: string` differ by one character and carry the same
    // *field* on the node — the postfix token. Reading its presence rather than
    // its kind makes `!` optional, which is backwards: `!` asserts the value is
    // there.
    assert_eq!(type_of_member("class C { p!: string; }", "C", "p"), "string");
}

#[test]
fn a_defaulted_parameter_is_not_optional() {
    // `isOptionalParameter` (`utilities.go:303`) says a parameter with an
    // initialiser is optional *to a caller*, and `isOptionalDeclaration`
    // (`:299`) — which is what this rule uses — says it is not, because the
    // initialiser supplies the value when the argument is absent. Conflating
    // them appends `| undefined` to every defaulted parameter in the corpus.
    assert_eq!(type_of("function f(p: string = \"a\") { return p; }", "p"), "string");
}

#[test]
fn an_optional_property_that_is_already_undefined_gains_nothing() {
    // Asserts the answer, not the guard. `getOptionalType`'s first early return
    // (`checker.go:18643`) is what upstream uses to reach it, but here
    // `get_union_type`'s dedup and one-element collapse already do — mutating
    // the guard away turns this green. Kept because the answer is worth pinning
    // and the guard's redundancy is documented at the guard.
    assert_eq!(type_of_member("interface I { p?: undefined }", "I", "p"), "undefined");
}

#[test]
fn an_optional_property_already_carrying_undefined_gains_nothing() {
    // Same again: upstream reaches this through its second early return, which
    // tests the *first* constituent because union members are ordered. Here the
    // dedup gets there first, so this pins the answer and not the guard.
    assert_eq!(
        type_of_member("interface I { p?: string | undefined }", "I", "p"),
        "string | undefined"
    );
}

#[test]
fn optionality_applies_to_the_annotation_and_not_to_an_initialiser() {
    // Upstream reaches `addOptionalityEx` on the *annotation* branch of
    // `getTypeForVariableLikeDeclaration` (`checker.go:16695`); the initialiser
    // branch is a separate call this port has not made
    // (`checker.go:18015`). A property with an initialiser and no annotation is
    // therefore unchanged — stated as a limit rather than left to be found.
    assert_eq!(type_of_member("class C { p = 1; }", "C", "p"), "number");
}

#[test]
fn a_variable_cannot_be_optional() {
    // There is no `?` on a variable declaration, so this must be untouched. It
    // is the cheapest guard against a rule that keys on something other than the
    // question token.
    assert_eq!(type_of("declare const x: string;", "x"), "string");
}
