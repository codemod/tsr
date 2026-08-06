//! What a function type node in annotation position denotes, stated as
//! questions about the printed type.
//!
//! Assertions go through `Checker::type_to_string` for the reason
//! `tests/types.rs` gives: the printed form is what a `.types` baseline
//! compares, and a type that is internally right but prints wrong fails
//! conformance identically.

use tsr_ast::{Node, Statement, SyntaxKind};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the annotation of the first `var`/`let`/`const` in the source.
///
/// The annotation rather than the initialiser, because a function *type node*
/// only ever appears in type position — typing the initialiser would exercise
/// `checkFunctionExpression` instead, which is a different arm.
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
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let annotation = declaration.r#type.expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

/// The syntax kind the first declaration's annotation parses to.
///
/// Every fixture below is checked against this before its type is asserted. A
/// test whose fixture quietly stopped parsing as the node it names would still
/// pass — `error` is the expected answer for several of them — so the kind is
/// pinned separately rather than inferred from the printed type.
fn annotation_kind(source: &str) -> SyntaxKind {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration")
        .r#type
        .expect("an annotation");
    Node::from(annotation).node_id().map(|id| parsed.nodes.kind(id)).expect("a registered node")
}

/// [`type_of_annotation`], having first asserted the annotation really is a
/// function type node — so a fixture that stopped parsing as one fails loudly
/// rather than passing through some other arm that happens to print alike.
fn type_of_function_annotation(source: &str) -> String {
    assert_eq!(
        annotation_kind(source),
        SyntaxKind::FunctionType,
        "the fixture's annotation must be a function type node"
    );
    type_of_annotation(source)
}

#[test]
fn a_function_type_prints_its_parameters_and_return_type() {
    assert_eq!(
        type_of_function_annotation("let f: (x: number) => string;"),
        "(x: number) => string"
    );
}

#[test]
fn a_function_type_with_no_parameters_prints_empty_parentheses() {
    assert_eq!(type_of_function_annotation("let f: () => void;"), "() => void");
}

/// The `?` is the node builder's, from `isOptionalParameter` — not the source
/// text's. `signature_to_string` emits `?: ` rather than `: `.
#[test]
fn an_optional_parameter_keeps_its_question_mark() {
    assert_eq!(type_of_function_annotation("let f: (x?: number) => void;"), "(x?: number) => void");
}

/// `...` is carried on the [`Parameter`] and re-emitted, so a rest parameter
/// does not silently print as an ordinary one.
#[test]
fn a_rest_parameter_keeps_its_ellipsis() {
    assert_eq!(type_of_function_annotation("let f: (...xs: any) => void;"), "(...xs: any) => void");
}

/// A function type node carries type parameters, and `signature_to_string`
/// renders them ahead of the parameter list.
#[test]
fn a_generic_function_type_prints_its_type_parameters() {
    assert_eq!(type_of_function_annotation("let f: <T>(x: T) => T;"), "<T>(x: T) => T");
}

/// The 523 lines the measurement attributed to nesting: the inner function type
/// has to resolve through the same arm for the outer one to print at all.
#[test]
fn a_nested_function_type_resolves_through_the_same_arm() {
    assert_eq!(
        type_of_function_annotation("let f: (g: (x: number) => void) => void;"),
        "(g: (x: number) => void) => void"
    );
}

/// A gap in a parameter is a gap in the whole function type. `(x: Nope) => void`
/// is not `(x: any) => void` — the same call `get_signature_from_declaration`
/// already makes, and the reason this arm cannot be written as a fallback.
#[test]
fn a_parameter_whose_type_is_a_gap_makes_the_function_type_a_gap() {
    // `bd tsr-eep`: an **unresolved** name now prints itself, because
    // upstream reports `Cannot find name` and renders the name anyway. So an
    // unresolved reference is no longer an example of "a type this port cannot
    // compute"; a **tuple** still is, and is used instead. The rule under test
    // is unchanged.
    assert_eq!(type_of_function_annotation("let f: (x: keyof string) => void;"), "error");
    assert_eq!(type_of_function_annotation("let f: (x: Nope) => void;"), "(x: Nope) => void");
}

/// And likewise the return annotation.
#[test]
fn a_return_type_that_is_a_gap_makes_the_function_type_a_gap() {
    // `bd tsr-eep`: an **unresolved** name now prints itself, because
    // upstream reports `Cannot find name` and renders the name anyway. So an
    // unresolved reference is no longer an example of "a type this port cannot
    // compute"; a **tuple** still is, and is used instead. The rule under test
    // is unchanged.
    assert_eq!(type_of_function_annotation("let f: (x: number) => keyof string;"), "error");
    assert_eq!(type_of_function_annotation("let f: (x: number) => Nope;"), "(x: number) => Nope");
}

/// A destructuring parameter is one of the forms `get_signature_from_declaration`
/// refuses: `parameterToParameterDeclarationName` invents a name this port has
/// no equivalent of, and an invented name is compared verbatim.
#[test]
fn a_destructuring_parameter_makes_the_function_type_a_gap() {
    assert_eq!(type_of_function_annotation("let f: ({ a }: any) => void;"), "error");
}

/// Calling a function-typed value now resolves, with **no code in `calls.rs`**.
///
/// `resolve_call_signature` (`crate::calls`) gets from a callee's type back to
/// its declaration through `TypeData::Anonymous`'s symbol. Once a function type
/// node carries the `__type` symbol that `bindFunctionOrConstructorType`
/// (`binder.go:985`) creates, `f(1)` resolves through the existing path — which
/// is upstream's own design showing through, not a coincidence: the whole point
/// of that binder function is to make `(x: number) => string` and
/// `{ (x: number): string }` indistinguishable downstream.
///
/// This test was written asserting `error`, before the binder arm existed, and
/// **watched to flip to `string`** when it landed. That order is the reason it
/// is worth keeping: it is the difference between observing the bonus and
/// assuming it.
#[test]
fn a_call_through_a_function_typed_value_resolves() {
    let source = "declare const f: (x: number) => string;\nconst y = f(1);";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[1] else {
        panic!("the second statement must be the call");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration")
        .initializer
        .expect("an initialiser");
    let tsr_ast::Expression::CallExpression(call) = initialiser else {
        panic!("the fixture's initialiser must be a call");
    };
    // The positive control, without which this test would pass for any reason at
    // all — including the callee never reaching this slice's arm. The callee's
    // type has to be the function type this module builds *before* the failure
    // below can be attributed to call resolution rather than to the annotation.
    let callee = checker.check_expression(call.expression.expect("a callee"));
    assert_eq!(checker.type_to_string(callee), "(x: number) => string");
    let id = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(id), "string");
}

/// `new (x: number) => C` reaches the same function upstream and prints with a
/// leading `new `, which `signature_to_string` does not emit. It stays a gap
/// rather than printing as though the `new` were absent.
///
/// **This is a regression guard, not a test of code that exists**, and it is
/// recorded as one rather than counted among the rest: no mutation of
/// `function_types.rs` or of the dispatch arm can turn it red, because it pins
/// the *absence* of an arm. Its falsifier is someone adding
/// `ConstructorTypeNode` to `signature_parts_of` or to `get_type_from_type_node`
/// without also giving [`Signature`] a construct flag and
/// `signature_to_string` the `new ` / `abstract new ` prefix — the slice
/// `signatures.rs:646` declines by name. What *is* verified here is that it
/// cannot pass vacuously: the fixture is asserted to parse as a constructor type
/// node, so the test fails loudly rather than silently if the grammar moves.
#[test]
fn a_constructor_type_is_still_a_gap() {
    let source = "let f: new (x: number) => any;";
    assert_eq!(
        annotation_kind(source),
        SyntaxKind::ConstructorType,
        "the fixture must be a constructor type node for this guard to mean anything"
    );
    assert_eq!(type_of_annotation(source), "error");
}
