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
