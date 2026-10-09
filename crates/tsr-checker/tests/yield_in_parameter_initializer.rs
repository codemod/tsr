//! TS2523 — `checkGrammarYieldExpression`'s parameter-initializer arm
//! (`grammarchecks.go:1783`), mirroring TS2524's climb
//! (`await_in_parameter_initializer.rs`). Every expectation below was
//! checked against a native `tsgo` built from the pinned submodule.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn ts2523_count(source: &str) -> usize {
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
    checker.diagnostics().iter().filter(|(_, d)| d.code() == "TS2523").count()
}

#[test]
fn a_yield_in_a_generators_parameter_default_reports() {
    assert_eq!(ts2523_count("function* g(a = yield 1) {}"), 1);
    assert_eq!(ts2523_count("async function* g(a = yield) {}"), 1);
}

#[test]
fn a_binding_elements_default_inside_a_generator_parameter_reports() {
    assert_eq!(ts2523_count("function* g({ b = yield }: any = 0) {}"), 1);
}

#[test]
fn a_method_generators_parameter_default_reports() {
    assert_eq!(ts2523_count("const o = { *m(z = yield 3) {} };"), 1);
}

#[test]
fn a_yield_with_an_operand_outside_a_generator_still_reports() {
    // `isYieldExpression` takes `yield 2` as a yield outside the yield
    // context too; native reports TS1163 and TS2523 together.
    assert_eq!(ts2523_count("function f(c = yield 2) {}"), 1);
}

#[test]
fn a_bare_yield_outside_a_generator_is_a_name() {
    assert_eq!(ts2523_count("function h(d = yield) {}"), 0);
}

#[test]
fn a_yield_inside_a_nested_function_is_not_a_parameter_initializer() {
    assert_eq!(ts2523_count("function* g(a = function* () { yield 1; }) {}"), 0);
}
