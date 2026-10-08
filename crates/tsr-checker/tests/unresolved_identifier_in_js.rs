//! An unresolved identifier EXPRESSION is upstream's `errorType`
//! (`checker.go:11048`), whatever module machinery the file carries — with
//! one held exception, a `.js` file with no `CommonJS` machinery.
//!
//! History. §784 made the §31 gate answer the port's gap (printed `error`) in
//! a plain `.js` file and the `any` stand-in in a `CommonJS` one, because the
//! gate then chose how an unresolved name PRINTED. ADR-0048 moved that choice
//! to the baseline writer, and r5-errorsplit4 measured the gate's arms against
//! a native build (`docs/parity/notes/r5-errorsplit4.md` §2): every line it
//! kept is `errorType` natively. The identity is asserted here, not the
//! spelling.
//!
//! The plain-`.js` arm stays the gap for now (§3 there): in a case with no
//! `.errors.txt` the writer's fast path prints a call through an `errorType`
//! callee, and the call road answers `any` where `resolveCallExpression`
//! answers `errorType` (`checker.go:8516`). That fix is in `calls.rs`, held as
//! a diff with the switch.

use tsr_ast::{NodeFlags, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the initializer of the file's last variable statement.
///
/// `javascript` stamps [`NodeFlags::JAVASCRIPT_FILE`] on the root, which is
/// what a `.js` extension does in the loader
/// (`crates/tsr-compiler/src/loader.rs:639`) — there is no `ScriptKind::Js`.
fn is_native_error(source: &str, javascript: bool) -> bool {
    let arena = Arena::new();
    let mut parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    if javascript {
        parsed
            .nodes
            .add_flags(parsed.source_file.node_id.expect("a root id"), NodeFlags::JAVASCRIPT_FILE);
    }
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.js", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initializer = declaration.initializer.expect("an initializer");
    let id = checker.check_expression(initializer);
    id == checker.intrinsics().native_error
}

/// `parsingDeepParenthensizedExpression`'s undeclared `f` in a plain `.js`
/// file is `errorType` natively, but stays the port's gap until the held
/// `calls.rs` arm lands (module docs).
#[test]
fn an_unresolved_identifier_in_a_plain_js_file_is_held_as_the_gap() {
    assert!(!is_native_error("var x = f;", true));
}

/// The TS half: the same `unknownSymbol` exit.
#[test]
fn an_unresolved_identifier_in_a_plain_ts_file_is_native_error() {
    assert!(is_native_error("var x = f;", false));
}

/// `CommonJS` machinery does not bring an undeclared name into scope; the
/// probe found no line where native resolves one the port misses.
#[test]
fn an_unresolved_identifier_in_a_commonjs_js_file_is_native_error() {
    assert!(is_native_error("var lib = require('./lib');\nvar x = f;", true));
}

/// ES import machinery neither: only a name found as an ALIAS keeps the gap.
#[test]
fn an_unresolved_identifier_beside_an_import_is_native_error() {
    assert!(is_native_error("import { g } from './lib';\nvar x = f;", false));
}

/// A name found only as an alias may be a value the port does not reach, so
/// it stays the port's gap (19 of 34 corpus lines are values natively).
#[test]
fn a_name_found_only_as_an_alias_stays_the_gap() {
    assert!(!is_native_error("import { f } from './lib';\nvar x = f;", false));
}

/// `arguments` outside any function is `unknownSymbol` natively.
#[test]
fn top_level_arguments_is_native_error() {
    assert!(is_native_error("var x = arguments;", false));
}
