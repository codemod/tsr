//! `checkNonNullExpression` on a property access receiver
//! (`checkPropertyAccessExpression`, `checker.go:11249`) is an ordinary
//! `c.error`, so it reports in a file that also has parse errors.
//! Expectations were checked against a native `tsgo` built from the pinned
//! submodule.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn codes(source: &str, has_parse_errors: bool) -> Vec<String> {
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
    checker.set_strict_null_checks(true);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors });
    let mut codes: Vec<String> = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| d.code().clone())
        .filter(|code| matches!(code.as_str(), "TS2532" | "TS18048" | "TS18050"))
        .collect();
    codes.sort();
    codes
}

#[test]
fn a_nullable_receiver_reports_in_a_file_with_parse_errors() {
    let source = "declare const a: { x: number } | undefined;\na.x;";
    assert_eq!(codes(source, true), ["TS18048"]);
}

#[test]
fn a_null_receiver_reports_in_a_file_with_parse_errors() {
    assert_eq!(codes("null.x;", true), ["TS18050"]);
}
