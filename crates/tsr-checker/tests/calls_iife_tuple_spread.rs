//! Native effective tuple arguments determine annotated IIFE arity.
use tsr_checker::{Checker, check::FileContext};

#[test]
fn annotated_iife_tuple_spreads_report_effective_arity() {
    let source = "declare const pair: [number, number]; ((a: number) => a)(...pair); ((a: number, b: number, c: number) => a)(...pair);";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "iife.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_strict_null_checks(true);
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
    assert_eq!(actual, ["Expected 1 arguments, but got 2.", "Expected 3 arguments, but got 2."]);
}
