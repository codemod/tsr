//! TS1262 — `checkContextualIdentifier`'s `await` arm (`binder.go:1311`),
//! gated on `ast.IsInTopLevelContext` (`ast/utilities.go:1778`), whose first
//! step moves a class or function declaration's name to the declaration.
//! Expectations were checked against a native `tsgo` built from the pinned
//! submodule. Needs `docs/parity/notes/r6-smallcodes4-top-level-context.diff`.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

fn lines(source: &str) -> Vec<u32> {
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
    assert!(file.diagnostics.is_empty(), "fixture must parse");
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
    let mut lines: Vec<u32> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| d.code().to_string() == "TS1262")
        .map(|(_, d)| u32::try_from(source[..d.span.start as usize].lines().count()).unwrap())
        .collect();
    lines.sort_unstable();
    lines
}

#[test]
fn a_function_declaration_named_await_is_at_the_top_level() {
    // `topLevelAwaitErrors.6`, `exportDefaultAsyncFunction2`.
    assert_eq!(lines("export function await() {}\n"), [1]);
}

#[test]
fn a_namespace_or_a_function_body_is_not_the_top_level() {
    let source =
        "export {};\nnamespace N { var await; }\nfunction f() { var await; }\nvar await;\n";
    assert_eq!(lines(source), [4]);
}
