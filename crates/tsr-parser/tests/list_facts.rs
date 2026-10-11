//! `ParsedSourceFile::list_facts`: the empty `<>` lists and trailing commas
//! native's `NodeList` records, keyed by the owner node
//! (docs/parity/notes/r7-parser.md §10).

use tsr_ast::SyntaxKind;
use tsr_core::Arena;
use tsr_parser::ListKind;

/// `(owner kind, list kind, open, close, empty, trailing comma)` for `source`.
fn facts(source: &str) -> Vec<(SyntaxKind, ListKind, u32, u32, bool, Option<u32>)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    parsed
        .list_facts
        .iter()
        .map(|&(owner, fact)| {
            (
                parsed.nodes.kind(owner),
                fact.kind,
                fact.open,
                fact.close,
                fact.empty,
                fact.trailing_comma,
            )
        })
        .collect()
}

#[test]
fn an_empty_type_parameter_list_belongs_to_its_declaration() {
    assert_eq!(
        facts("function f<>() {}"),
        vec![(SyntaxKind::FunctionDeclaration, ListKind::TypeParameters, 10, 11, true, None)]
    );
}

#[test]
fn empty_type_argument_lists_belong_to_the_call_new_and_reference() {
    assert_eq!(
        facts("foo<>();"),
        vec![(SyntaxKind::CallExpression, ListKind::TypeArguments, 3, 4, true, None)]
    );
    assert_eq!(
        facts("new C<>();"),
        vec![(SyntaxKind::NewExpression, ListKind::TypeArguments, 5, 6, true, None)]
    );
    assert_eq!(
        facts("type T = A<>;"),
        vec![(SyntaxKind::TypeReference, ListKind::TypeArguments, 10, 11, true, None)]
    );
}

#[test]
fn a_parameter_list_trailing_comma_belongs_to_the_innermost_owner() {
    // The inner function type's comma is the function type's; the outer
    // comma is the declaration's.
    assert_eq!(
        facts("function f(cb: (a,) => void,) {}"),
        vec![
            (SyntaxKind::FunctionType, ListKind::Parameters, 15, 18, false, Some(17)),
            (SyntaxKind::FunctionDeclaration, ListKind::Parameters, 10, 28, false, Some(27)),
        ]
    );
    assert_eq!(
        facts("interface I { [k: string,]: any }"),
        vec![(SyntaxKind::IndexSignature, ListKind::Parameters, 14, 25, false, Some(24))]
    );
}

#[test]
fn a_refused_type_argument_list_leaves_no_fact() {
    assert!(facts("a < b > c;").is_empty());
    assert!(facts("x = f<>;").len() == 1);
}
