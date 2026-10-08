//! Which symbol "Cannot find name 'x'. Did you mean 'y'?" names.
//!
//! Native `resolveNameForSymbolSuggestion` is the ordinary scope walk with a
//! spelling lookup per table: each arm keeps its own meaning mask and its
//! own acceptance rules, and the first table yielding a near-miss wins. The
//! native controls (pinned tsgo) are recorded beside each fixture.

use tsr_ast::NodeId;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

/// Every reported message, in report order.
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
    checker.diagnostics().iter().map(|(_, d)| d.text()).collect()
}

/// The class arm looks up its members at `meaning & Type`, so a property is
/// never a suggestion for a value. Native: `TS2304 Cannot find name 'NodeType'`
/// (`conformance/parserRealSource11`, 56 sites).
#[test]
fn a_class_property_is_not_a_value_suggestion() {
    assert_eq!(
        messages("class C { nodeType = 1; m() { NodeType; } }"),
        ["Cannot find name 'NodeType'."]
    );
}

/// The control: a near-miss in an enclosing scope's own table is suggested.
#[test]
fn an_enclosing_local_is_a_value_suggestion() {
    assert_eq!(
        messages("function f(nodeType: number) { NodeType; }"),
        ["Cannot find name 'NodeType'. Did you mean 'nodeType'?"]
    );
}

/// The first table with a near-miss wins over a closer name further out.
/// Native: `Did you mean 'abcdx'?` although `abcde` is one case-change away.
#[test]
fn an_inner_near_miss_shadows_a_closer_outer_one() {
    assert_eq!(
        messages("var Abcde = 1;\nfunction f() { var abcdx = 1; abcde; }"),
        ["Cannot find name 'abcde'. Did you mean 'abcdx'?"]
    );
}

/// A `declare global` block's symbol has no spellable name natively, so
/// `global` is not an exact hit and the walk suggests `globals`
/// (`compiler/spellingSuggestionGlobal2`).
#[test]
fn a_global_augmentation_is_not_a_candidate() {
    assert_eq!(
        messages(
            "export {}\ndeclare global { const x: any }\nconst globals = { x: true }\nglobal.x"
        ),
        ["Cannot find name 'global'. Did you mean 'globals'?"]
    );
}
