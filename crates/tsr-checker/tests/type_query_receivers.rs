//! TS2532 / TS18048 on the left of a qualified name in a type query
//! (`checkQualifiedName`'s `checkNonNullExpression(left)`, `checker.go:8122`).
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
        .filter(|code| {
            matches!(
                code.as_str(),
                "TS2531" | "TS2532" | "TS2533" | "TS18047" | "TS18048" | "TS18049"
            )
        })
        .collect();
    codes.sort();
    codes
}

#[test]
fn a_possibly_undefined_qualified_left_is_ts2532() {
    let source = "interface D { a?: { b?: { c?: string } } }\ndeclare const foo: D;\ntype C = typeof foo.a.b;";
    assert_eq!(codes(source), ["TS2532"]);
}

#[test]
fn a_possibly_undefined_identifier_left_names_itself() {
    let source = "declare const u: { x: number } | undefined;\ntype X = typeof u.x;";
    assert_eq!(codes(source), ["TS18048"]);
}

#[test]
fn a_narrowed_left_does_not_report() {
    let source = "interface D { a?: { b?: { c?: string } } }\nfunction f(foo: D) {\n    if (foo.a) {\n        type B = typeof foo.a.b;\n    }\n}";
    assert!(codes(source).is_empty());
}

#[test]
fn every_level_reports_innermost_first() {
    let source = "interface D { a?: { b?: { c?: string } } }\ndeclare const foo: D;\ntype C = typeof foo.a.b.c;";
    assert_eq!(codes(source), ["TS2532", "TS2532"]);
}
