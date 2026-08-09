//! Where TS2524 stops climbing.
//!
//! `isInParameterInitializerBeforeContainingFunction` (`checker.go:12235`)
//! walks up from the `await` and gives up at the first **function-like**
//! parent. That boundary is the whole rule: a default value may legally
//! contain an `await`, provided the `await` belongs to a nested async function
//! rather than to the initializer itself.
//!
//! # Why this is a unit test and not a conformance row
//!
//! The corpus's eight TS2524 cases all report; none of them contains the
//! **negative** — a nested async arrow inside a default. A rule that never
//! stopped climbing would pass all eight and be wrong, so the conformance
//! suites cannot tell the two implementations apart. This file is the
//! falsifier §226 promised before the code was written.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

/// How many TS2524 a fixture reports.
fn ts2524_count(source: &str) -> usize {
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::default();
    let mut node_map = tsr_ast::NodeMap::default();
    let file = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut node_map,
    );
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let root: NodeId = file.source_file.node_id.expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().filter(|(_, d)| d.code() == "TS2524").count()
}

#[test]
fn an_await_directly_in_a_default_value_reports() {
    assert_eq!(ts2524_count("async function f(a = await 1) {}"), 1);
}

#[test]
fn an_await_inside_a_nested_async_arrow_is_legal() {
    // The arrow is the `await`'s containing function, so the climb stops there
    // and never reaches the parameter. Reporting here would be the defect the
    // eight passing conformance cases cannot see.
    assert_eq!(ts2524_count("async function f(a = async () => await 1) {}"), 0);
}

#[test]
fn an_await_inside_a_nested_async_function_expression_is_legal() {
    assert_eq!(ts2524_count("async function f(a = async function () { return await 1; }) {}"), 0);
}

#[test]
fn a_binding_elements_default_inside_a_parameter_reports() {
    // `inBindingInitializer`, upstream's sticky bit: the `await` belongs to the
    // binding element rather than to the parameter, and still counts.
    assert_eq!(ts2524_count("async function f({ a = await 1 }) {}"), 1);
}

#[test]
fn an_await_in_a_body_is_not_a_parameter_initializer() {
    assert_eq!(ts2524_count("async function f(a = 1) { await a; }"), 0);
}
