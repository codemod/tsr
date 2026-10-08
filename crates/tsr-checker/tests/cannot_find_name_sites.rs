//! Where `onFailedToResolveSymbol` is reached at all: names native resolves,
//! or never resolves, must not report TS2304.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn messages(source: &str) -> Vec<String> {
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
    let has_parse_errors = !file.diagnostics.is_empty();
    let root: NodeId = file.source_file.node_id.expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors });
    checker.diagnostics().iter().map(|(_, d)| d.text()).collect()
}

/// Outside any function `arguments` is an ordinary name: a declared global
/// resolves (`conformance/emitArrowFunctionWhenUsingArguments03`), an
/// undeclared one is TS2304.
#[test]
fn a_declared_arguments_resolves_outside_functions() {
    assert!(
        !messages("var arguments;\nvar a = () => arguments;")
            .contains(&"Cannot find name 'arguments'.".to_string())
    );
    assert_eq!(messages("var a = () => arguments;"), ["Cannot find name 'arguments'."]);
}
