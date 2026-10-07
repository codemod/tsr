//! Native this/asserts predicates parse outside return positions for grammar checking.
use tsr_ast::{Statement, TypeNode};
use tsr_core::Arena;

#[test]
fn predicates_in_ordinary_type_annotations_keep_their_written_subject() {
    for source in [
        "function f(x: this is string) {}",
        "function f(x: asserts this is string) {}",
        "function f(x: asserts x) {}",
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[0] else {
            panic!("expected function")
        };
        let Some(TypeNode::TypePredicateNode(predicate)) = function.parameters[0].r#type else {
            panic!("expected predicate, not recovery reference")
        };
        assert_eq!(predicate.asserts_modifier.is_some(), source.contains("asserts"));
        assert!(function.body.is_some());
    }
}

#[test]
fn identifier_is_prefix_wins_over_asserts_modifier() {
    // `parseTypeOrTypePredicate` tries `asserts is` as the predicate prefix
    // before `parseNonArrayType`'s `asserts` arm.
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "function f(asserts: any): asserts is string {}");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Statement::FunctionDeclaration(function) = parsed.source_file.statements[0] else {
        panic!("expected function")
    };
    let Some(TypeNode::TypePredicateNode(predicate)) = function.r#type else {
        panic!("expected predicate")
    };
    assert!(predicate.asserts_modifier.is_none());
    assert!(predicate.r#type.is_some());
}
