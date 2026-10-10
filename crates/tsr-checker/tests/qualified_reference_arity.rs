//! A qualified reference to a generic class written without its required
//! type arguments is `errorType` (`getTypeFromClassOrInterfaceReference`,
//! `checker.go:23170`), so the variable it annotates is `any` and
//! `checkIdentifier` assumes it initialized (`checker.go:11156`).
//! Expectations were checked against a native `tsgo` built from the pinned
//! submodule.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn codes(source: &str) -> Vec<String> {
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
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let mut codes: Vec<String> = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| d.code().clone())
        .filter(|code| code == "TS2454")
        .collect();
    codes.sort();
    codes
}

#[test]
fn a_missing_required_argument_makes_the_variable_any() {
    let source = "namespace A { export class B<T> { foo() {} } }\nfunction f() {\n  var b: A.B;\n  b.foo();\n}";
    assert!(codes(source).is_empty());
}

#[test]
fn a_defaulted_parameter_needs_no_argument() {
    let source = "namespace A { export class D<T = number> { foo() {} } }\nfunction f() {\n  var d: A.D;\n  d.foo();\n}";
    assert_eq!(codes(source), ["TS2454"]);
}
