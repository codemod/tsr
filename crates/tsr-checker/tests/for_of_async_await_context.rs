//! TS1106 — `checkGrammarForInOrForOfStatement`'s `async` arm
//! (`grammarchecks.go:1251`) reads the statement's `NodeFlagsAwaitContext`,
//! so a class static block (an await context) is silent and an `async`
//! function's own body is too. Expectations were checked against a native
//! `tsgo` built from the pinned submodule (r7-grammar lane note §56).

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
    let root: NodeId = file.source_file.node_id().expect("a parsed file has an id");
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
        .filter(|(_, d)| d.code() == "TS1106")
        .map(|(_, d)| {
            let start = d.span.start as usize;
            u32::try_from(source[..start].matches('\n').count() + 1).unwrap_or(0)
        })
        .collect();
    lines.sort_unstable();
    lines
}

#[test]
fn async_left_hand_side_outside_an_await_context() {
    let source = "declare var xs: any[];
let async: any;
class C { static { for (async of xs) {} } }
async function f() { for (async of xs) {} }
function g() { for (async of xs) {} }
for (async of xs) {}
";
    assert_eq!(lines(source), vec![5, 6]);
}
