//! Native JS calls enforce maximum arity while untyped inputs stay optional.
use tsr_checker::{Checker, check::FileContext};

#[test]
fn js_extra_arguments_are_reported_without_requiring_untyped_parameters() {
    let source = "function fixed(value) {} fixed(); fixed(1, 2); function noArgs() {} noArgs(1);";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse_with_script_kind(&arena, source, tsr_parser::ScriptKind::Js);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "arity.js", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.unwrap(),
        FileContext { ambient: false, has_parse_errors: false },
    );
    let actual: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, diagnostic)| diagnostic.code() == "TS2554")
        .map(|(_, diagnostic)| diagnostic.text())
        .collect();
    assert_eq!(actual, ["Expected 0-1 arguments, but got 2.", "Expected 0 arguments, but got 1."]);
}
