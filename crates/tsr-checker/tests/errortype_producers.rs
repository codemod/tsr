//! Producers that answer upstream's `errorType` (or `anyType`) by identity
//! (ADR-0048), switched by r5-errorsplit5 after a line-by-line check against
//! the native identity probe (`docs/parity/notes/r5-errorsplit5.md`).
//!
//! Each test names the `checker.go` line it mirrors. Identity is asserted,
//! not spelling: the checker prints both of the port's error identities
//! `any`, and the baseline writer decides how a line is spelled.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// What the initializer of the file's last variable statement answered.
#[derive(Debug, PartialEq, Eq)]
enum Identity {
    /// Upstream's `errorType` (`Intrinsics::native_error`).
    NativeError,
    /// The port's gap (`Intrinsics::error`).
    Gap,
    /// Upstream's `anyType`.
    Any,
    /// Anything else, printed.
    Other(String),
}

fn identity_of(checker: &mut Checker<'_, '_>, id: tsr_checker::TypeId) -> Identity {
    let intrinsics = checker.intrinsics();
    if id == intrinsics.native_error {
        Identity::NativeError
    } else if id == intrinsics.error {
        Identity::Gap
    } else if id == intrinsics.any {
        Identity::Any
    } else {
        Identity::Other(checker.type_to_string(id))
    }
}

/// What the first `super` keyword of the file answered.
fn super_identity(source: &str) -> Identity {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.ts"));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    #[allow(clippy::cast_possible_truncation)]
    let id = (0..parsed.nodes.len() as u32)
        .map(tsr_ast::NodeId::new)
        .find(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::SuperKeyword)
        .expect("a `super`");
    let node = parsed.node_map.get(id).expect("a mapped node");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let type_id = checker.check_expression(expression);
    identity_of(&mut checker, type_id)
}

fn initializer_identity(file: &str, source: &str) -> Identity {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(file));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: file, text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let initializer = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initializer");
    let id = checker.check_expression(initializer);
    identity_of(&mut checker, id)
}

/// `checkNonNullType`'s tail (`checker.go:7429`): `null` has no non-nullable
/// remainder, so the operand is `errorType`, and `+` over an `isErrorType`
/// operand answers `errorType` (`checker.go:12436`).
/// `compiler/operatorAddNullUndefined`.
#[test]
fn adding_null_to_null_is_native_error() {
    assert_eq!(initializer_identity("t.ts", "var x = null + null;"), Identity::NativeError);
}

#[test]
fn adding_undefined_to_a_number_is_native_error() {
    assert_eq!(initializer_identity("t.ts", "var x = 1 + undefined;"), Identity::NativeError);
}

/// The control: a string operand skips `checkNonNullType`, so `null` reaches
/// the string arm (`checker.go:12428`).
#[test]
fn adding_null_to_a_string_is_still_string() {
    assert_eq!(
        initializer_identity("t.ts", "var x = \"a\" + null;"),
        Identity::Other("string".to_owned())
    );
}

/// The control for the other arm: two `any` operands that are not
/// `isErrorType` answer `anyType` (`checker.go:12438`).
#[test]
fn adding_any_to_any_is_any() {
    assert_eq!(initializer_identity("t.ts", "declare var a: any; var x = a + a;"), Identity::Any);
}

/// `getJsxType` with no `JSX` namespace in scope answers `errorType`
/// (`jsx.go:1303`). `conformance/jsxUnclosedParserRecovery`.
#[test]
fn a_jsx_element_without_a_jsx_namespace_is_native_error() {
    assert_eq!(initializer_identity("t.tsx", "var x = <div />;"), Identity::NativeError);
}

/// `checkJsxFragment` turns that `errorType` into `anyType` (`jsx.go:123`).
/// `compiler/jsxFactoryButNoJsxFragmentFactory`.
#[test]
fn a_jsx_fragment_without_a_jsx_namespace_is_any() {
    assert_eq!(initializer_identity("t.tsx", "var x = <></>;"), Identity::Any);
}

/// The control: a declared `JSX.Element` is the element's type, fragment or
/// not.
#[test]
fn a_declared_jsx_element_is_the_element_type() {
    let source = "declare namespace JSX { interface Element { e: 1 } }\nvar x = <></>;";
    assert_eq!(initializer_identity("t.tsx", source), Identity::Other("Element".to_owned()));
}

/// `super` in an object-literal method is upstream's `anyType`
/// (`checker.go:7917`). `compiler/superInObjectLiterals_ES6`.
#[test]
fn super_in_an_object_literal_method_is_any() {
    assert_eq!(super_identity("var o = { m() { return super.x; } };"), Identity::Any);
}

/// A function container is never a legal `super` container
/// (`checker.go:7907`): `errorType`. `compiler/superErrors`.
#[test]
fn super_in_a_plain_function_is_native_error() {
    assert_eq!(super_identity("function f() { return super.x; }"), Identity::NativeError);
}

/// A class without `extends` reports TS2335 and answers `errorType`
/// (`checker.go:7924`). `conformance/superCallInConstructorWithNoBaseType`.
#[test]
fn super_in_a_class_without_a_base_is_native_error() {
    assert_eq!(super_identity("class C { m() { return super.x; } }"), Identity::NativeError);
}

/// A static block is a static member container (`ast/utilities.go:1835`),
/// so its `super` is the base constructor type. `compiler/classFieldSuperAccessible`.
#[test]
fn super_in_a_static_block_is_the_base_constructor() {
    let source = "class B { static n = 1; }\nclass C extends B { static { super.n; } }";
    assert_eq!(super_identity(source), Identity::Other("typeof B".to_owned()));
}

/// An element access through an unresolved type reference answers the
/// receiver itself: upstream's unresolved reference is any-flagged with an
/// alias, so `isErrorType` holds and `checkElementAccessExpression` returns
/// `objectType` (`checker.go:8153`). `compiler/recursiveTypeRelations`.
#[test]
fn an_element_access_through_an_unresolved_reference_is_the_reference() {
    let source = "declare var x: Missing;\nvar y = x[0];";
    assert_eq!(initializer_identity("t.ts", source), Identity::Other("Missing".to_owned()));
}
