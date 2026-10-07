//! Native 5b1047d getMinArgumentCountEx/getParameterCount rest-type controls.
use tsr_ast::Node;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn diagnostics(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_strict_null_checks(true);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, diagnostic)| diagnostic.text()).collect()
}

#[test]
fn annotated_array_rest_pattern_does_not_impose_tuple_bounds() {
    let source = "interface Array<T> { [index: number]: T; }
        function array(...[a, b]: number[]) {}
        function tuple(...[a, b]: [number, number]) {}
        function implied(...[[a, b], [c, d]]) {}
        array(); array(1); array(1, 2, 3);
        tuple(1); tuple(1, 2, 3); implied([1, 2]);";
    assert_eq!(
        diagnostics(source),
        [
            "Expected 2 arguments, but got 1.",
            "Expected 2 arguments, but got 3.",
            "Expected 2 arguments, but got 1."
        ]
    );
}

#[test]
fn required_parameter_after_default_keeps_its_minimum() {
    assert_eq!(
        diagnostics("function f(a = 1, b: number) {} f(1); f(1, 2);"),
        ["Expected 2 arguments, but got 1."]
    );
}
