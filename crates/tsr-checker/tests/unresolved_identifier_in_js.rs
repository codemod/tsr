//! An unresolved identifier EXPRESSION in a `.js` file is `errorType`, not
//! `any` — unless the file carries `CommonJS` module machinery. §784.
//!
//! The §31 gate answers `any` for a name it cannot resolve, on the argument
//! that this port's own binding roads are incomplete and a wrong `errorType`
//! would be a lie about upstream. That argument has a structural escape hatch:
//! when the file carries import machinery the port *does* have gaps that could
//! explain the miss, so it stays honest and answers `errorType`. The gate's own
//! comment names "a JS/JSX file" as a second case of the same kind, but the
//! condition never tested for it.
//!
//! It should. `compiler/parsingDeepParenthensizedExpression` is a `.js`
//! fixture under `allowJs` with four undeclared names (`f`, `l`, `b`, `o`);
//! upstream reports TS2304 on each and the oracle records `error` on **325**
//! lines — the single most concentrated block of wrong lines in the baseline.
//! Adding the JS half converts 198 corpus lines with zero adverse.
//!
//! The `CommonJS` exception is the other half of the same argument, and it is
//! measured, not assumed: without it the arm costs **11 RIGHT->GAP**, every one
//! of them in a file binding names through `require`/`module.exports` — roads
//! this port only partly has, exactly the situation the ES-declaration test
//! already excuses. [`Checker::file_has_import_machinery`] cannot see those,
//! because it looks for ES `import`/`export` DECLARATIONS and a `CommonJS` file
//! has none.

use tsr_ast::{NodeFlags, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the initializer of the file's last variable statement.
///
/// `javascript` stamps [`NodeFlags::JAVASCRIPT_FILE`] on the root, which is
/// what a `.js` extension does in the loader
/// (`crates/tsr-compiler/src/loader.rs:639`) — there is no `ScriptKind::Js`.
fn type_of_initializer(source: &str, javascript: bool) -> String {
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
    checker.type_to_string(id)
}

/// `parsingDeepParenthensizedExpression` records `error` for its undeclared
/// `f`; a plain `.js` file with no module machinery is the same shape.
#[test]
fn an_unresolved_identifier_in_a_plain_js_file_is_error() {
    assert_eq!(type_of_initializer("var x = f;", true), "error");
}

/// The TS half is unchanged: with no machinery of any kind the gate still
/// answers `any`, because a TS file's unresolved name is upstream's TS2304 on
/// a source this port read in full.
#[test]
fn an_unresolved_identifier_in_a_plain_ts_file_is_still_any() {
    assert_eq!(type_of_initializer("var x = f;", false), "any");
}

/// The measured exception. A `.js` file that binds names through `CommonJS` is
/// one whose unresolved names this port may itself be failing to bind, so it
/// keeps the `any` the §31 gate gives every other incompletely-read file.
/// Reverting [`Checker::file_has_commonjs_machinery`] reddens this and costs
/// 11 RIGHT->GAP on the corpus.
#[test]
fn an_unresolved_identifier_in_a_commonjs_js_file_is_any() {
    assert_eq!(type_of_initializer("var lib = require('./lib');\nvar x = f;", true), "any");
}

/// `module.exports` counts as the same machinery as `require`.
#[test]
fn module_exports_is_commonjs_machinery_too() {
    assert_eq!(type_of_initializer("module.exports = {};\nvar x = f;", true), "any");
}
