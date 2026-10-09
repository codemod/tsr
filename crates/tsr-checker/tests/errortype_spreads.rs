//! Object spreads by identity (ADR-0048): `checkObjectLiteral` answers
//! `errorType` once a spread fails, and `anyType` for an any-flagged spread
//! source (`checker.go:13304`, `:13336`, `getSpreadType`'s first line);
//! `docs/parity/notes/r5-errorsplit6.md` §6.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

#[derive(Debug, PartialEq, Eq)]
enum Identity {
    NativeError,
    Gap,
    Any,
    Other(String),
}

/// What the initializer of the file's last variable statement answered.
fn last_initializer(source: &str) -> Identity {
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

/// TS2698: a spread of a non-object makes the literal `errorType`, and a
/// later valid spread does not revive it.
#[test]
fn an_invalid_spread_is_native_error() {
    assert_eq!(last_initializer("const o = { ...1 };\n"), Identity::NativeError);
    assert_eq!(
        last_initializer("declare const a: { x: number };\nconst o = { ...1, ...a, y: 2 };\n"),
        Identity::NativeError
    );
}

/// The control: a valid spread computes.
#[test]
fn a_valid_spread_computes() {
    assert_eq!(
        last_initializer("declare const a: { x: number };\nconst o = { ...a };\n"),
        Identity::Other("{ x: number; }".to_string())
    );
}

/// An any-flagged source is `getSpreadType`'s `anyType`, upstream's
/// `errorType` included.
#[test]
fn spreading_an_unresolved_name_is_any() {
    assert_eq!(last_initializer("const o = { ...missing };\n"), Identity::Any);
}
