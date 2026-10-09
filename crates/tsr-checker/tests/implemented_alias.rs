//! `class C implements Alias` where `Alias` is a type alias
//! (`checkClassLikeDeclaration`'s implemented-type loop, `checker.go:4365`).
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
        .map(|(_, d)| d.code().to_string())
        .filter(|code| matches!(code.as_str(), "TS2416" | "TS2420" | "TS2559" | "TS2720"))
        .collect();
    codes.sort();
    codes
}

#[test]
fn an_intersection_alias_is_related_like_an_interface() {
    let source = "declare class Foo { x: string; }\ndeclare class Bar { y: string; }\ntype Wrapper = Foo & Bar;\nclass Baz implements Wrapper {\n    x!: number;\n    y!: string;\n}";
    assert_eq!(codes(source), ["TS2416"]);
}

#[test]
fn an_object_alias_missing_a_member_is_ts2420() {
    let source =
        "type Shape = { x: string; y: string };\nclass C implements Shape {\n    x = \"\";\n}";
    assert_eq!(codes(source), ["TS2420"]);
}

#[test]
fn a_satisfied_alias_does_not_report() {
    let source = "type Shape = { x: string };\nclass C implements Shape {\n    x = \"\";\n}";
    assert!(codes(source).is_empty());
}
